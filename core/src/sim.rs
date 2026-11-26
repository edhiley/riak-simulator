//! The interactive core: nodes, vnodes with mailboxes, and messages on a
//! simulated network, all driven by a simulation clock.
//!
//! The request paths follow OpenRiak 3.4.1's source, simplified:
//!
//! PUT (`riak_kv_put_fsm`, `riak_kv_put_core`, `riak_kv_vnode`):
//! - The node that receives the request coordinates it if it owns a replica
//!   of the key. Otherwise it forwards the request, and the coordinator sends
//!   back a `now_executing` ACK. Riak's mailbox check is on by default: it
//!   asks every remote replica vnode for its mailbox size and forwards to the
//!   first that replies below the soft limit (`check_mailboxes`). The
//!   simulator does not model the check's messages and forwards to the owner
//!   of the first replica.
//! - The coordinator sends the write to its own vnode. That vnode sends a
//!   `w` ACK on receipt, stores the object with a new vclock, and sends a
//!   `dw` ACK with the object.
//! - Then the coordinator sends the object to the other replicas. Each sends
//!   `w` on receipt and `dw` after it stores the object.
//! - The coordinator replies when it has W `w` ACKs and DW `dw` ACKs.
//!
//! GET (`riak_kv_get_fsm`): the receiving node coordinates. It sends HEAD to
//! every replica. When R replies are in, it fetches the body and replies.
//! In Riak, `riak_kv_get_core` returns the list of vnodes to fetch from; how
//! it chooses them has not been checked. The simulator fetches from one
//! replica with the newest vclock. Read repair is not modelled yet.
//!
//! There are no request timeouts. A held message waits until it is released.

use crate::crdt::{self, Crdt, DataType};
use crate::siblings::{self as sibs, Sib};
use crate::ring::{BucketRef, Ring};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

/// Simulation time in microseconds.
pub type Time = u64;

pub const N_VAL: usize = 3;
pub const MAX_NODES: usize = 8;

/// Message and processing times, in microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Timing {
    pub client: Time,
    pub node: Time,
    pub local: Time,
    pub put: Time,
    pub head: Time,
    pub get: Time,
    /// Reading one key during a key fold (list keys).
    pub fold_key: Time,
    /// Saving one object that arrives in a handoff: a backend write, with no
    /// request to coordinate, so much less than a whole put.
    pub handoff_obj: Time,
}

impl Timing {
    /// Every message and every vnode action takes the same time, so each
    /// step is easy to follow. Not how a cluster performs.
    pub const EVEN: Timing = Timing { client: 500, node: 500, local: 500, put: 500, head: 500, get: 500, fold_key: 10, handoff_obj: 20 };

    /// Typical orders of magnitude for one data centre, not measurements. A
    /// message inside a node is an Erlang message between processes and is
    /// about ten times faster than a message between nodes.
    pub const REALISTIC: Timing = Timing { client: 1_000, node: 500, local: 50, put: 1_000, head: 200, get: 300, fold_key: 5, handoff_obj: 20 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum Endpoint {
    Client,
    /// A load balancer in front of the nodes, such as nginx or HAProxy.
    Balancer,
    Node(usize),
    Vnode(usize),
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum Payload {
    /// HTTP request from the client.
    Request,
    /// Request handed to the coordinating node.
    Forward,
    /// `now_executing`: the coordinating node has started the request.
    ForwardAck,
    /// Write to the coordinator's own vnode.
    CoordPut,
    /// Write to another replica.
    Put,
    W { vnode: usize },
    Dw { vnode: usize },
    Head,
    /// The replica's vclock, or none if it does not have the object.
    HeadReply { vnode: usize, vclock: Option<BTreeMap<usize, u64>> },
    Get,
    GetReply { vnode: usize },
    /// Result from the coordinating node to the node that received the request.
    Result,
    /// HTTP response to the client.
    Response { status: u16 },
    /// Hinted handoff: a fallback vnode sends its objects to the usual vnode.
    Handoff { objects: usize },
    /// The usual vnode has taken every handed-off object.
    HandoffAck,
    /// List keys: fold over the vnode's keys for the coordinator. `first`
    /// lists the key ranges (first partitions) this vnode answers for.
    Fold { first: Vec<usize> },
    /// Up to 100 keys from a fold (key_buffer_size); `last` ends the fold.
    Keys { keys: Vec<String>, last: bool },
}

impl Payload {
    pub fn label(&self) -> String {
        match self {
            Payload::Request => "request".into(),
            Payload::Forward => "forward".into(),
            Payload::ForwardAck => "now_executing".into(),
            Payload::CoordPut => "coord put".into(),
            Payload::Put => "put".into(),
            Payload::W { .. } => "w".into(),
            Payload::Dw { .. } => "dw".into(),
            Payload::Head => "head".into(),
            Payload::HeadReply { vclock: Some(_), .. } => "head reply".into(),
            Payload::HeadReply { vclock: None, .. } => "not found".into(),
            Payload::Get => "get".into(),
            Payload::GetReply { .. } => "object".into(),
            Payload::Result => "reply".into(),
            Payload::Response { status } => status.to_string(),
            Payload::Handoff { .. } => "handoff".into(),
            Payload::HandoffAck => "handoff ack".into(),
            Payload::Fold { .. } => "fold".into(),
            Payload::Keys { keys, .. } => format!("{} keys", keys.len()),
        }
    }
}

/// Cluster-wide quorum defaults, used when a request does not set its own
/// (in Riak, the bucket properties r, w, dw, pr and pw).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Quorum {
    pub r: usize,
    pub w: usize,
    pub dw: usize,
    pub pr: usize,
    pub pw: usize,
}

impl Default for Quorum {
    /// Riak's defaults: quorum (2 of 3) for r, w and dw; 0 for pr and pw.
    fn default() -> Self {
        Quorum { r: 2, w: 2, dw: 2, pr: 0, pw: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Join(usize),
    Leave(usize),
}

/// A partition moving from one node to another (ownership handoff).
#[derive(Clone, Debug)]
struct Transfer {
    partition: usize,
    from: usize,
    to: usize,
    /// The commit this move belongs to.
    req: u64,
    /// A round of objects is in flight.
    active: bool,
    /// The last round was acknowledged; the owner changes when nothing new
    /// has arrived and the vnode is idle.
    acked: bool,
    /// The vclock of each object the new owner has had, so a later round
    /// sends only what changed since.
    sent: BTreeMap<(BucketRef, String), BTreeMap<usize, u64>>,
}

/// A message that has arrived, for drawing after it is gone.
#[derive(Clone, Debug, Serialize)]
pub struct Hop {
    pub req: u64,
    pub from: Endpoint,
    pub to: Endpoint,
    pub label: String,
    /// When it was sent and when it arrived, in simulation time.
    pub sent: Time,
    pub arrived: Time,
}

#[derive(Clone, Debug, Serialize)]
pub struct Message {
    pub id: u64,
    pub req: u64,
    pub from: Endpoint,
    pub to: Endpoint,
    pub payload: Payload,
    pub label: String,
    /// Progress along the path (0 to 1) at time `start`. Changes of speed
    /// reset `start` and `progress` so the drawn position does not jump.
    pub start: Time,
    pub progress: f64,
    /// Delivery time, or `None` while held.
    pub deliver: Option<Time>,
    /// Time left in flight when the message was held.
    #[serde(skip)]
    remaining: Time,
}

impl Message {
    pub fn held(&self) -> bool {
        self.deliver.is_none()
    }
}

#[derive(Clone, Debug)]
struct Object {
    value: Vec<u8>,
    content_type: String,
    vclock: BTreeMap<usize, u64>,
    deleted: bool,
    /// A data type's state, in place of a value.
    crdt: Option<Crdt>,
    /// In a bucket with allow_mult: every value no write has replaced. The
    /// fields above then describe the first live one.
    siblings: Vec<Sib>,
}

impl Object {
    /// The values a read returns: live siblings, or the one value.
    fn live(&self) -> usize {
        if self.siblings.is_empty() {
            usize::from(!self.deleted)
        } else {
            self.siblings.iter().filter(|s| !s.deleted).count()
        }
    }

    /// Set value, content type and deleted from the siblings.
    fn from_siblings(mut self) -> Self {
        if self.siblings.is_empty() {
            return self;
        }
        if let Some(s) = self.siblings.iter().find(|s| !s.deleted).or(self.siblings.first()) {
            self.value = s.value.clone();
            self.content_type = s.content_type.clone();
        }
        self.deleted = self.siblings.iter().all(|s| s.deleted);
        self
    }
}

/// A bucket keeps siblings when its bucket type allows them: every created
/// bucket type does, apart from data type buckets; the default type does not.
fn allow_mult(b: &BucketRef) -> bool {
    b.bucket_type.as_deref().is_some_and(|t| DataType::of_bucket_type(t).is_none())
}

fn merge_vclocks(a: &mut BTreeMap<usize, u64>, b: &BTreeMap<usize, u64>) {
    for (actor, n) in b {
        let e = a.entry(*actor).or_default();
        *e = (*e).max(*n);
    }
}

fn descends(a: &BTreeMap<usize, u64>, b: &BTreeMap<usize, u64>) -> bool {
    b.iter().all(|(actor, n)| a.get(actor).is_some_and(|m| m >= n))
}

/// A key as the leveled ledger orders it: bucket (with its type), then key.
fn ledger_key(k: &(BucketRef, String)) -> crate::leveled::Key {
    let b = match &k.0.bucket_type {
        Some(t) => format!("{t}/{}", k.0.bucket),
        None => k.0.bucket.clone(),
    };
    (b, k.1.clone())
}

#[derive(Default)]
struct Vnode {
    mailbox: VecDeque<u64>,
    /// Message being processed and when processing ends.
    busy: Option<(u64, Time)>,
    store: BTreeMap<(BucketRef, String), Object>,
    /// A fallback vnode's hinted handoff in progress: its id and the objects
    /// it sent. Objects written after the send stay until the next handoff.
    handoff: Option<(u64, Vec<((BucketRef, String), Object)>)>,
    /// Key folds running in this vnode's worker pool: request, the keys found
    /// that are still to send, and the chunks still to send.
    folds: Vec<(u64, Vec<String>, usize)>,
    /// The backend: leveled, as in OpenRiak's leveled backend.
    leveled: crate::leveled::Leveled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Put,
    Delete,
    Get,
    Ping,
    Invalid,
    /// Hinted handoff after a stopped node starts again. Not a client request.
    Handoff,
    /// Partitions moving after a cluster change. Not a client request.
    Transfer,
    /// List the keys of a bucket (a coverage query).
    ListKeys,
}

#[derive(Clone, Debug, Serialize)]
pub struct RequestState {
    pub id: u64,
    pub op: Op,
    pub method: String,
    pub path: String,
    /// Node that received the request.
    pub origin: usize,
    /// Sent through the load balancer, not to a node directly.
    pub via_balancer: bool,
    /// The request has reached `origin`.
    pub arrived: bool,
    pub coordinator: Option<usize>,
    pub bucket: Option<BucketRef>,
    pub key: Option<String>,
    /// The data type, for a request on /types/<type>/buckets/<b>/datatypes/<k>.
    pub datatype: Option<DataType>,
    pub preflist: Vec<usize>,
    pub w: usize,
    pub dw: usize,
    pub r: usize,
    /// Copies that must be on their usual (primary) vnodes. 0 by default.
    pub pw: usize,
    pub pr: usize,
    pub num_w: usize,
    pub num_dw: usize,
    pub num_r: usize,
    /// Set when the coordinator has decided the reply.
    pub status: Option<u16>,
    pub done: bool,
    #[serde(skip)]
    body: Vec<u8>,
    /// Hinted handoff: fallback vnodes that have not finished.
    #[serde(skip)]
    handoff_pending: usize,
    /// A data type update.
    #[serde(skip)]
    crdt_op: Option<crdt::Op>,
    /// The vclock the client read (X-Riak-Vclock), its context for a write.
    #[serde(skip)]
    context: Option<sibs::Clock>,
    /// The client asked for all siblings in one multipart body.
    #[serde(skip)]
    multipart: bool,
    /// One sibling, chosen by its vtag (?vtag=).
    #[serde(skip)]
    vtag: Option<String>,
    /// List keys: the keys found so far, and the folds not finished.
    #[serde(skip)]
    keys: std::collections::BTreeSet<String>,
    #[serde(skip)]
    folds_pending: usize,
    /// List keys: keys read in all the folds, in every bucket.
    #[serde(skip)]
    keys_scanned: usize,
    /// A data type read: vnodes asked for their value and not yet answered,
    /// and how many were asked.
    #[serde(skip)]
    fetch_pending: usize,
    #[serde(skip)]
    fetches: usize,
    #[serde(skip)]
    content_type: String,
    #[serde(skip)]
    error: String,
    /// An invalid request that Riak answers with 404, such as an unknown bucket type.
    #[serde(skip)]
    not_found: bool,
    /// Object returned to a client GET.
    #[serde(skip)]
    found: Option<Object>,
    /// HEAD replies: partition and vclock of what it holds.
    #[serde(skip)]
    heads: Vec<(usize, Option<BTreeMap<usize, u64>>)>,
    #[serde(skip)]
    remote_sent: bool,
    /// Vnode a GET fetched the value from.
    #[serde(skip)]
    fetched_from: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub at: Time,
    pub req: u64,
    /// What happened.
    pub text: String,
    /// Why Riak works this way. Empty if there is nothing to add.
    pub why: String,
    /// True for the one event that sums up a finished request.
    pub summary: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HttpResponse {
    pub req: u64,
    pub status: u16,
    pub reason: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// An input from outside the simulation, recorded with the time it was
/// applied. Replaying the inputs from the same seed reproduces the run.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum Input {
    Submit { node: Option<usize>, method: String, path: String, headers: Vec<(String, String)>, body: Vec<u8> },
    SubmitAfter { delay: Time, node: Option<usize>, method: String, path: String, headers: Vec<(String, String)>, body: Vec<u8> },
    Hold { msg: u64 },
    Release { msg: u64 },
    Slow { msg: u64, factor: u32 },
    Timing { timing: Timing },
    Stop { node: usize },
    Start { node: usize },
    Quorum { quorum: Quorum },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Deliver(u64),
    VnodeDone(usize),
    /// A key fold in a vnode's worker sends its next chunk of keys.
    FoldChunk { req: u64, vnode: usize },
    /// A request that waited at its client leaves.
    Launch(u64),
}

pub struct Sim {
    ring: Ring,
    /// Next node the load balancer sends to.
    next_node: usize,
    timing: Timing,
    /// Nodes in the cluster, by number from 0. A node keeps its number (and
    /// colour) when others join or leave.
    members: std::collections::BTreeSet<usize>,
    now: Time,
    seq: u64,
    next_id: u64,
    rng: u64,
    schedule: BTreeMap<(Time, u64), Action>,
    messages: BTreeMap<u64, Message>,
    /// Messages delivered to a vnode and waiting in, or taken from, its mailbox.
    parked: BTreeMap<u64, Message>,
    /// Vnodes by id. Ids below the ring size are the usual (primary) vnodes,
    /// one per partition. A fallback vnode, which keeps a copy for a stopped
    /// node, has the id ring_size * (1 + host node) + partition.
    vnodes: Vec<Vnode>,
    /// Stopped nodes.
    down: std::collections::BTreeSet<usize>,
    /// Quorum defaults for requests that do not set their own.
    quorum: Quorum,
    /// Membership changes staged with `riak admin cluster join|leave`.
    staged: Vec<Change>,
    /// Nodes leaving: they hand their data to others, then go.
    leaving: std::collections::BTreeSet<usize>,
    /// Partitions moving to a new owner.
    transfers: Vec<Transfer>,
    ring_version: u64,
    /// The hinted handoff that the last start_node began, if any.
    last_handoff: Option<u64>,
    requests: BTreeMap<u64, RequestState>,
    /// Short forms already shown in brackets, per request.
    named: std::collections::BTreeSet<(u64, &'static str)>,
    events: Vec<Event>,
    responses: Vec<HttpResponse>,
    inputs: Vec<(Time, Input)>,
    /// Messages that arrived since the last take_hops.
    hops: Vec<Hop>,
    /// Requests waiting at their clients to be sent (submit_after).
    waiting: BTreeMap<u64, (Option<usize>, String, String, Vec<(String, String)>, Vec<u8>)>,
    /// Actions run, for pruning finished requests now and then.
    actions: u64,
}

#[derive(Serialize)]
pub struct VnodeView {
    pub id: usize,
    pub partition: usize,
    /// The node the vnode runs on.
    pub owner: usize,
    /// Keeps copies for a stopped node.
    pub fallback: bool,
    /// The new owner's copy of a partition that is moving to this node.
    pub moving: bool,
    /// A key fold (list keys) is running in its worker pool.
    pub folding: bool,
    /// Messages waiting, oldest first.
    pub mailbox: Vec<MailItem>,
    /// Message being processed.
    pub busy: Option<MailItem>,
    pub keys: usize,
    /// The objects it stores, not counting deleted ones.
    pub objects: Vec<StoredView>,
}

#[derive(Serialize)]
pub struct StoredView {
    /// "users", or "counters/stats" for a bucket in a bucket type.
    pub bucket: String,
    pub key: String,
    /// The value as text, cut to 60 characters; a data type's value; or
    /// the siblings' values.
    pub value: String,
    /// Live siblings: more than 1 means the key has a conflict to resolve.
    pub siblings: usize,
}

#[derive(Serialize)]
pub struct MailItem {
    pub id: u64,
    pub req: u64,
    pub label: String,
}

#[derive(Serialize)]
pub struct Snapshot<'a> {
    pub now: Time,
    pub messages: Vec<&'a Message>,
    pub vnodes: Vec<VnodeView>,
    pub requests: Vec<&'a RequestState>,
    pub idle: bool,
    /// Stopped nodes.
    pub down: Vec<usize>,
    /// Nodes in the cluster, and those leaving it.
    pub members: Vec<usize>,
    pub leaving: Vec<usize>,
    /// Changes when partition owners or members change, so the page can
    /// read the ring again.
    pub ring_version: u64,
}

impl Sim {
    pub fn new(nodes: usize, ring_size: usize, seed: u64) -> Result<Sim, String> {
        if !(1..=MAX_NODES).contains(&nodes) {
            return Err(format!("The simulator supports 1 to {MAX_NODES} nodes, not {nodes}."));
        }
        let ring = Ring::new(ring_size, nodes)?;
        let mut sim = Sim {
            // Usual vnodes, then fallback vnodes, then a new owner's copy of
            // a partition that is moving (see owner()).
            vnodes: (0..ring_size * (1 + 2 * MAX_NODES)).map(|_| Vnode::default()).collect(),
            down: Default::default(),
            quorum: Quorum::default(),
            staged: vec![],
            leaving: Default::default(),
            transfers: vec![],
            ring_version: 0,
            last_handoff: None,
            ring,
            next_node: 0,
            timing: Timing::EVEN,
            members: (0..nodes).collect(),
            now: 0,
            seq: 0,
            next_id: 1,
            rng: seed ^ 0x9E37_79B9_7F4A_7C15,
            schedule: BTreeMap::new(),
            messages: BTreeMap::new(),
            parked: BTreeMap::new(),
            requests: BTreeMap::new(),
            named: Default::default(),
            events: Vec::new(),
            responses: Vec::new(),
            inputs: Vec::new(),
            hops: Vec::new(),
            waiting: BTreeMap::new(),
            actions: 0,
        };
        // The load balancer starts at a node chosen from the seed, then goes round in turn.
        sim.next_node = (sim.random() % nodes as u64) as usize;
        Ok(sim)
    }

    pub fn ring(&self) -> &Ring {
        &self.ring
    }

    /// How many nodes are in the cluster.
    pub fn nodes(&self) -> usize {
        self.members.len()
    }

    /// The nodes in the cluster, by number.
    pub fn members(&self) -> Vec<usize> {
        self.members.iter().copied().collect()
    }

    pub fn now(&self) -> Time {
        self.now
    }

    pub fn inputs(&self) -> &[(Time, Input)] {
        &self.inputs
    }

    /// A sibling's name, as Riak's vtags look: 22 letters and digits.
    fn vtag(&mut self) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        (0..22).map(|_| A[(self.random() % A.len() as u64) as usize] as char).collect()
    }

    /// SplitMix64, so runs are the same everywhere for a seed.
    fn random(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Times for messages sent and vnode actions started from now on.
    pub fn set_timing(&mut self, timing: Timing) {
        self.inputs.push((self.now, Input::Timing { timing }));
        self.timing = timing;
    }

    pub fn timing(&self) -> Timing {
        self.timing
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn take_hops(&mut self) -> Vec<Hop> {
        std::mem::take(&mut self.hops)
    }

    pub fn take_responses(&mut self) -> Vec<HttpResponse> {
        std::mem::take(&mut self.responses)
    }

    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            now: self.now,
            messages: self.messages.values().collect(),
            // Every primary vnode; a fallback vnode only once it has been used.
            vnodes: self
                .vnodes
                .iter()
                .enumerate()
                .filter(|(id, v)| *id < self.ring.size() || !v.store.is_empty() || v.busy.is_some() || !v.mailbox.is_empty() || !v.folds.is_empty())
                .map(|(id, v)| VnodeView {
                    id,
                    partition: self.vpart(id),
                    owner: self.owner(id),
                    fallback: self.is_fallback(id),
                    moving: self.is_moving(id),
                    folding: !v.folds.is_empty(),
                    mailbox: v.mailbox.iter().map(|m| self.mail_item(*m)).collect(),
                    busy: v.busy.map(|(m, _)| self.mail_item(m)),
                    keys: v.store.values().filter(|o| !o.deleted).count(),
                    objects: v
                        .store
                        .iter()
                        .filter(|(_, o)| !o.deleted)
                        .map(|((b, k), o)| StoredView {
                            bucket: match &b.bucket_type {
                                Some(t) => format!("{t}/{}", b.bucket),
                                None => b.bucket.clone(),
                            },
                            key: k.clone(),
                            value: {
                                let cut = |v: &[u8]| {
                                    let text = String::from_utf8_lossy(v);
                                    if text.chars().count() > 60 { format!("{}…", text.chars().take(60).collect::<String>()) } else { text.into_owned() }
                                };
                                match &o.crdt {
                                    Some(c) => c.describe(),
                                    None if o.live() > 1 => format!(
                                        "{} siblings: {}",
                                        o.live(),
                                        o.siblings.iter().filter(|s| !s.deleted).map(|s| cut(&s.value)).collect::<Vec<_>>().join(" | ")
                                    ),
                                    None => cut(&o.value),
                                }
                            },
                            siblings: o.live(),
                        })
                        .collect(),
                })
                .collect(),
            requests: self.requests.values().filter(|r| !r.done || self.messages.values().any(|m| m.req == r.id)).collect(),
            idle: self.schedule.is_empty(),
            down: self.down.iter().copied().collect(),
            members: self.members(),
            leaving: self.leaving.iter().copied().collect(),
            ring_version: self.ring_version,
        }
    }

    fn mail_item(&self, id: u64) -> MailItem {
        let m = &self.parked[&id];
        MailItem { id, req: m.req, label: m.label.clone() }
    }

    /// Time of the next scheduled action, if any. Held messages are not scheduled.
    pub fn next_time(&self) -> Option<Time> {
        self.schedule.keys().next().map(|(t, _)| *t)
    }

    // ---- Inputs ----

    /// Send a request straight to one node.
    pub fn submit(&mut self, node: usize, method: &str, path: &str, headers: Vec<(String, String)>, body: Vec<u8>) -> u64 {
        self.submit_to(Some(node), method, path, headers, body)
    }

    /// Send a request to one node, or to the load balancer if `node` is `None`.
    pub fn submit_to(&mut self, node: Option<usize>, method: &str, path: &str, headers: Vec<(String, String)>, body: Vec<u8>) -> u64 {
        self.inputs.push((
            self.now,
            Input::Submit { node, method: method.into(), path: path.into(), headers: headers.clone(), body: body.clone() },
        ));
        let id = self.id();
        self.launch(id, node, method, path, headers, body);
        id
    }

    /// Send a request `delay` microseconds from now, as one of many clients
    /// would: requests arrive at their own times, not all at once. The id is
    /// returned now; the request starts when it leaves the client.
    pub fn submit_after(&mut self, delay: Time, node: Option<usize>, method: &str, path: &str, headers: Vec<(String, String)>, body: Vec<u8>) -> u64 {
        self.inputs.push((
            self.now,
            Input::SubmitAfter { delay, node, method: method.into(), path: path.into(), headers: headers.clone(), body: body.clone() },
        ));
        let id = self.id();
        if delay == 0 {
            self.launch(id, node, method, path, headers, body);
        } else {
            self.waiting.insert(id, (node, method.into(), path.into(), headers, body));
            self.schedule_at(self.now + delay, Action::Launch(id));
        }
        id
    }

    /// A request that waited at its client leaves now.
    fn launch_waiting(&mut self, id: u64) {
        if let Some((node, method, path, headers, body)) = self.waiting.remove(&id) {
            self.launch(id, node, &method, &path, headers, body);
        }
    }

    fn launch(&mut self, id: u64, node: Option<usize>, method: &str, path: &str, headers: Vec<(String, String)>, body: Vec<u8>) {
        let first = *self.members.iter().next().unwrap();
        let origin = node.filter(|n| self.members.contains(n)).unwrap_or(first);
        let mut req = parse_request(id, origin, method, path, &headers, body, self.quorum);
        req.via_balancer = node.is_none();
        if let Some(n) = node.filter(|n| !self.members.contains(n)) {
            req.op = Op::Invalid;
            req.error = format!("There is no node{} in the cluster.", n + 1);
        }
        if let Some(n) = node.filter(|n| self.down.contains(n)) {
            // Nothing listens on a stopped node's port.
            req.done = true;
            self.requests.insert(id, req);
            self.event(id, format!("The client tries to connect to node{} directly. node{} is stopped, so the connection is refused. No request reaches the cluster.", n + 1, n + 1));
            self.responses.push(HttpResponse { req: id, status: 0, reason: "Connection refused", headers: vec![], body: String::new() });
            let text = format!("node{} is stopped. Send the request to the load balancer, or to a running node.", n + 1);
            self.events.push(Event { at: self.now, req: id, text, why: String::new(), summary: true });
            return;
        }
        let to = if req.via_balancer { Endpoint::Balancer } else { Endpoint::Node(req.origin) };
        self.requests.insert(id, req);
        self.send(id, Endpoint::Client, to, Payload::Request);
        let r = &self.requests[&id];
        let target = match node {
            None => "the load balancer".to_string(),
            Some(n) => format!("node{} directly", n + 1),
        };
        let text = format!("The client sends {} {} to {target}.", r.method, r.path.split('?').next().unwrap_or(""));
        self.event(id, text);
    }

    pub fn hold(&mut self, msg: u64) -> bool {
        let now = self.now;
        let Some(m) = self.messages.get_mut(&msg) else { return false };
        let Some(deliver) = m.deliver else { return false };
        m.progress = progress_at(m, now);
        m.remaining = deliver.saturating_sub(now);
        m.start = now;
        m.deliver = None;
        self.schedule.retain(|_, a| *a != Action::Deliver(msg));
        self.inputs.push((now, Input::Hold { msg }));
        true
    }

    pub fn release(&mut self, msg: u64) -> bool {
        let now = self.now;
        let Some(m) = self.messages.get_mut(&msg) else { return false };
        if m.deliver.is_some() {
            return false;
        }
        let deliver = now + m.remaining.max(1);
        m.start = now;
        m.deliver = Some(deliver);
        self.inputs.push((now, Input::Release { msg }));
        self.schedule_at(deliver, Action::Deliver(msg));
        true
    }

    /// Make a message in flight take `factor` times as long for the rest of its path.
    pub fn slow(&mut self, msg: u64, factor: u32) -> bool {
        let now = self.now;
        let Some(m) = self.messages.get_mut(&msg) else { return false };
        let Some(deliver) = m.deliver else { return false };
        let remaining = deliver.saturating_sub(now).max(1) * factor.max(1) as u64;
        m.progress = progress_at(m, now);
        m.start = now;
        m.deliver = Some(now + remaining);
        self.schedule.retain(|_, a| *a != Action::Deliver(msg));
        self.inputs.push((now, Input::Slow { msg, factor }));
        self.schedule_at(now + remaining, Action::Deliver(msg));
        true
    }

    /// No request is running: nothing in flight, waiting or held.
    pub fn quiet(&self) -> bool {
        self.messages.is_empty() && self.parked.is_empty()
    }

    /// Stop a node, as if it were shut down. From now on it takes no new
    /// work: the load balancer skips it, preflists leave it out, and a direct
    /// connection is refused. Work it already has finishes, so no request
    /// waits for ever (the simulator has no timeouts). A crash in the middle
    /// of a request is not simulated. Its saved copies stay on its disk. At
    /// least one node keeps running.
    pub fn stop_node(&mut self, n: usize) -> bool {
        let running = self.members.iter().filter(|m| !self.down.contains(m)).count();
        if !self.members.contains(&n) || self.down.contains(&n) || running <= 1 {
            return false;
        }
        self.down.insert(n);
        self.inputs.push((self.now, Input::Stop { node: n }));
        true
    }

    /// Start a stopped node again. Its vnodes have the copies they had when
    /// it stopped. Fallback vnodes that can now hand off start to (see
    /// check_handoffs); last_handoff gives that work's id.
    pub fn start_node(&mut self, n: usize) -> bool {
        if !self.down.contains(&n) {
            return false;
        }
        self.down.remove(&n);
        self.inputs.push((self.now, Input::Start { node: n }));
        self.last_handoff = self.check_handoffs(None, Some(n));
        self.start_transfers();
        self.try_flips();
        true
    }

    pub fn last_handoff(&self) -> Option<u64> {
        self.last_handoff
    }

    /// Forget finished requests that no message refers to, so a long run
    /// does not slow down.
    fn prune(&mut self) {
        let live: std::collections::BTreeSet<u64> = self.messages.values().chain(self.parked.values()).map(|m| m.req).collect();
        self.requests.retain(|id, r| !r.done || live.contains(id));
        let requests = &self.requests;
        self.named.retain(|(id, _)| requests.contains_key(id));
    }

    /// Start hinted handoff for each fallback vnode that can: it has
    /// copies, it is idle and not already handing off, and both its own node
    /// and the usual vnode's node are running. As riak_core_vnode does when a
    /// fallback vnode times out as inactive. `only` limits the check to one
    /// vnode; `started` is the node that has just started, for the text.
    ///
    /// In Riak, the vnode must have had no requests for
    /// vnode_inactivity_timeout (60 s by default). Here it starts when idle.
    fn check_handoffs(&mut self, only: Option<usize>, started: Option<usize>) -> Option<u64> {
        let size = self.ring.size();
        let ids: Vec<usize> = match only {
            Some(id) => vec![id],
            None => (size..self.vnodes.len()).collect(),
        };
        let ready: Vec<usize> = ids
            .into_iter()
            .filter(|&id| {
                let v = &self.vnodes[id];
                self.is_fallback(id)
                    && !v.store.is_empty()
                    && v.handoff.is_none()
                    && v.busy.is_none()
                    && v.mailbox.is_empty()
                    && !self.down.contains(&self.owner(id))
                    && !self.down.contains(&self.ring.owners[self.vpart(id)])
            })
            .collect();
        if ready.is_empty() {
            return None;
        }
        let rid = self.id();
        let usual_node = self.ring.owners[self.vpart(ready[0])];
        let mut req = parse_request(rid, usual_node, "HANDOFF", "", &[], vec![], self.quorum);
        // Not a client request: no node is its coordinator.
        req.op = Op::Handoff;
        req.preflist = ready.clone();
        req.handoff_pending = ready.len();
        req.method = String::new();
        self.requests.insert(rid, req);
        let mut lines = vec![];
        for &id in &ready {
            let objects: Vec<_> = self.vnodes[id].store.iter().map(|(k, o)| (k.clone(), o.clone())).collect();
            let count = objects.len();
            let usual = self.vpart(id);
            lines.push(format!(
                "{} has {count} object{} that belong{} on {}",
                self.vnode_name(id),
                if count == 1 { "" } else { "s" },
                if count == 1 { "s" } else { "" },
                self.vnode_name(usual)
            ));
            self.vnodes[id].handoff = Some((rid, objects));
            self.send(rid, Endpoint::Vnode(id), Endpoint::Vnode(usual), Payload::Handoff { objects: count });
        }
        let start = match started {
            Some(n) => format!("node{} starts again. ", n + 1),
            None => String::new(),
        };
        self.event(
            rid,
            format!(
                "{start}{}. Riak sends the copies back to their usual vnodes. This is hinted handoff. Why: the copies belong on their usual vnodes. When they are back, every node holds its share of the data again. In Riak, handoff starts after the fallback vnode has had no requests for 60 seconds.",
                capital(&list(lines)),
            ),
        );
        Some(rid)
    }

    // ---- Cluster membership: riak admin cluster join|leave|plan|commit ----

    /// `riak admin cluster join <node>`, run on a new node: stage it to join.
    pub fn stage_join(&mut self, new: usize) -> Result<String, String> {
        if new >= MAX_NODES {
            return Err(format!("The simulator supports up to {MAX_NODES} nodes."));
        }
        if self.members.contains(&new) {
            return Err(format!("node{} is already in the cluster.", new + 1));
        }
        if self.staged.contains(&Change::Join(new)) {
            return Err(format!("node{} is already staged to join.", new + 1));
        }
        self.staged.push(Change::Join(new));
        Ok(format!("Success: staged join request for 'node{}' to the cluster", new + 1))
    }

    /// `riak admin cluster leave [<node>]`: stage a node to leave.
    pub fn stage_leave(&mut self, n: usize) -> Result<String, String> {
        if !self.members.contains(&n) {
            return Err(format!("node{} is not in the cluster.", n + 1));
        }
        if self.down.contains(&n) {
            return Err(format!("node{} is stopped. A node must be running to hand over its data and leave.", n + 1));
        }
        if self.leaving.contains(&n) || self.staged.contains(&Change::Leave(n)) {
            return Err(format!("node{} is already leaving.", n + 1));
        }
        let staying = self.members.len() - self.leaving.len() - self.staged.iter().filter(|c| matches!(c, Change::Leave(_))).count();
        if staying <= 1 {
            return Err("The last node cannot leave.".into());
        }
        self.staged.push(Change::Leave(n));
        Ok(format!("Success: staged leave request for 'node{}'", n + 1))
    }

    /// The owners after the staged changes, and the members.
    fn planned(&self) -> (Vec<usize>, Vec<usize>) {
        let mut owners = self.ring.owners.clone();
        let mut members: Vec<usize> = self.members().into_iter().filter(|m| !self.leaving.contains(m)).collect();
        for c in &self.staged {
            match *c {
                Change::Join(n) => {
                    members.push(n);
                    members.sort();
                    owners = crate::ring::claim_join(&owners, &members);
                }
                Change::Leave(n) => {
                    members.retain(|&m| m != n);
                    owners = crate::ring::claim_leave(&owners, &members, n);
                }
            }
        }
        (owners, members)
    }

    /// `riak admin cluster plan`: the staged changes and the moves they need.
    pub fn plan(&self) -> String {
        if self.staged.is_empty() {
            return "There are no staged changes".into();
        }
        let (owners, members) = self.planned();
        let mut out = String::from("=============================== Staged Changes ================================\nAction         Details(s)\n-------------------------------------------------------------------------------\n");
        for c in &self.staged {
            match c {
                Change::Join(n) => out.push_str(&format!("join           'node{}'\n", n + 1)),
                Change::Leave(n) => out.push_str(&format!("leave          'node{}'\n", n + 1)),
            }
        }
        out.push_str("-------------------------------------------------------------------------------\n\n");
        let mut moves: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for p in 0..owners.len() {
            if owners[p] != self.ring.owners[p] {
                *moves.entry((self.ring.owners[p], owners[p])).or_default() += 1;
            }
        }
        out.push_str("Partitions owned after the changes:\n");
        for m in &members {
            let n = owners.iter().filter(|&&o| o == *m).count();
            out.push_str(&format!("  node{}: {n}\n", m + 1));
        }
        let total: usize = moves.values().sum();
        out.push_str(&format!("\nTransfers resulting from cluster changes: {total}\n"));
        for ((f, t), n) in moves {
            out.push_str(&format!("  {n} transfers from 'node{}' to 'node{}'\n", f + 1, t + 1));
        }
        out
    }

    /// `riak admin cluster commit`: apply the staged changes. Partitions
    /// then move, at most 2 at a time for each node (transfer_limit). Returns
    /// the id that the moves' steps use.
    pub fn commit(&mut self) -> Result<Option<u64>, String> {
        if self.staged.is_empty() {
            return Err("There are no staged changes to commit".into());
        }
        if !self.transfers.is_empty() {
            return Err("Partitions are still moving after the last change. Commit again when they have finished.".into());
        }
        let (owners, _) = self.planned();
        let staged = std::mem::take(&mut self.staged);
        let mut joined = vec![];
        let mut left = vec![];
        for c in &staged {
            match *c {
                Change::Join(n) => {
                    self.members.insert(n);
                    self.down.remove(&n);
                    joined.push(format!("node{}", n + 1));
                }
                Change::Leave(n) => {
                    self.leaving.insert(n);
                    left.push(format!("node{}", n + 1));
                }
            }
        }
        self.ring_version += 1;
        let rid = self.id();
        let first = *self.members.iter().next().unwrap();
        let mut req = parse_request(rid, first, "TRANSFER", "", &[], vec![], self.quorum);
        req.op = Op::Transfer;
        req.method = String::new();
        self.requests.insert(rid, req);
        let mut moves = vec![];
        for p in 0..owners.len() {
            if owners[p] != self.ring.owners[p] {
                self.transfers.push(Transfer { partition: p, from: self.ring.owners[p], to: owners[p], req: rid, active: false, acked: false, sent: BTreeMap::new() });
                moves.push(p);
            }
        }
        let who = match (joined.is_empty(), left.is_empty()) {
            (false, true) => format!("{} joins the cluster.", list(joined)),
            (true, false) => format!("{} starts to leave the cluster.", list(left)),
            _ => format!("{} joins and {} starts to leave the cluster.", list(joined), list(left)),
        };
        self.event(
            rid,
            format!(
                "{who} Riak works out new owners so that every node has an equal share of the {} partitions. {} partition{} must move. Each moves in turn, at most 2 at a time for each node. Why: moving a few at a time keeps the cluster fast for requests while the data moves.",
                owners.len(),
                moves.len(),
                if moves.len() == 1 { "" } else { "s" }
            ),
        );
        self.start_transfers();
        self.finish_leaves();
        Ok(Some(rid))
    }

    /// Start moves that can start: both nodes running, and fewer than 2
    /// moves in progress at each (riak_core transfer_limit, default 2).
    fn start_transfers(&mut self) {
        const LIMIT: usize = 2;
        for i in 0..self.transfers.len() {
            let t = self.transfers[i].clone();
            if t.active || t.acked || self.down.contains(&t.from) || self.down.contains(&t.to) {
                continue;
            }
            let busy = |n: usize| self.transfers.iter().filter(|x| (x.active || x.acked) && (x.from == n || x.to == n)).count();
            if busy(t.from) >= LIMIT || busy(t.to) >= LIMIT {
                continue;
            }
            self.send_round(i);
        }
    }

    /// Send the partition's objects, as they are now, to the new owner.
    fn send_round(&mut self, i: usize) {
        let t = self.transfers[i].clone();
        let p = t.partition;
        // The first round sends every object; a later round only those
        // written since the last one.
        let objects: Vec<_> = self.vnodes[p].store.iter().filter(|(k, o)| t.sent.get(*k) != Some(&o.vclock)).map(|(k, o)| (k.clone(), o.clone())).collect();
        let count = objects.len();
        for (k, o) in &objects {
            self.transfers[i].sent.insert(k.clone(), o.vclock.clone());
        }
        let again = !t.sent.is_empty();
        self.vnodes[p].handoff = Some((t.req, objects));
        self.transfers[i].active = true;
        self.transfers[i].acked = false;
        let to = self.moving_id(p, t.to);
        self.send(t.req, Endpoint::Vnode(p), Endpoint::Vnode(to), Payload::Handoff { objects: count });
        self.event(
            t.req,
            format!(
                "{} sends {}{count} object{} to node{}, which will own partition {p}. It keeps answering requests until the move is complete.",
                capital(&self.vnode_name(p)),
                if again { "the " } else { "its " },
                if count == 1 { "" } else { "s" },
                t.to + 1
            ),
        );
    }

    /// The new owner has the objects. If nothing new arrived at the old
    /// owner since, and no message is on its way to or from either vnode,
    /// the partition changes owner; otherwise send what is new.
    fn try_flips(&mut self) {
        let mut i = 0;
        while i < self.transfers.len() {
            let t = self.transfers[i].clone();
            if !t.acked || self.down.contains(&t.from) || self.down.contains(&t.to) {
                i += 1;
                continue;
            }
            let p = t.partition;
            let to = self.moving_id(p, t.to);
            let idle = |s: &Sim, id: usize| {
                s.vnodes[id].busy.is_none()
                    && s.vnodes[id].mailbox.is_empty()
                    && !s.messages.values().any(|m| m.from == Endpoint::Vnode(id) || m.to == Endpoint::Vnode(id))
            };
            if !idle(self, p) || !idle(self, to) {
                i += 1;
                continue;
            }
            let changed = self.vnodes[p].store.iter().filter(|(k, o)| t.sent.get(*k) != Some(&o.vclock)).count();
            if changed > 0 {
                self.event(t.req, format!("{} has taken writes since it sent its objects, so it sends the {changed} object{} that changed.", capital(&self.vnode_name(p)), if changed == 1 { "" } else { "s" }));
                self.send_round(i);
                i += 1;
                continue;
            }
            // The new owner's copy becomes the usual vnode of the partition.
            let store = std::mem::take(&mut self.vnodes[to].store);
            self.vnodes[p].store = store;
            // The new owner's backend becomes the partition's; the old one is dropped.
            self.vnodes[p].leveled = std::mem::take(&mut self.vnodes[to].leveled);
            self.vnodes[p].handoff = None;
            self.ring.owners[p] = t.to;
            self.ring_version += 1;
            self.transfers.remove(i);
            self.event(t.req, format!("Partition {p} now belongs to node{}. node{} deletes its copy. Requests for partition {p} now go to node{}.", t.to + 1, t.from + 1, t.to + 1));
            if !self.transfers.iter().any(|x| x.req == t.req) {
                let r = self.requests.get_mut(&t.req).unwrap();
                r.done = true;
                r.status = Some(200);
                let counts = self.members().iter().map(|m| format!("node{} {}", m + 1, self.ring.owners.iter().filter(|&&o| o == *m).count())).collect::<Vec<_>>();
                let text = format!("Every partition has moved. Partitions now owned: {}.", list(counts));
                self.events.push(Event { at: self.now, req: t.req, text, why: String::new(), summary: true });
            }
            self.start_transfers();
        }
        self.finish_leaves();
    }

    /// A leaving node goes once it owns no partitions and holds no copies
    /// for stopped nodes.
    fn finish_leaves(&mut self) {
        let size = self.ring.size();
        let gone: Vec<usize> = self
            .leaving
            .iter()
            .copied()
            .filter(|&n| {
                !self.ring.owners.contains(&n)
                    && (0..size).all(|p| self.vnodes[size * (1 + n) + p].store.is_empty())
                    && !self.transfers.iter().any(|t| t.from == n || t.to == n)
            })
            .collect();
        for n in gone {
            self.leaving.remove(&n);
            self.members.remove(&n);
            self.down.remove(&n);
            self.ring_version += 1;
            if let Some(rid) = self.requests.values().filter(|r| r.op == Op::Transfer).map(|r| r.id).max() {
                self.event(rid, format!("node{} has handed over all its data. It leaves the cluster and shuts down.", n + 1));
            }
        }
    }

    /// Set the quorum defaults. r, w and dw are 1 to 3; pr and pw 0 to 3.
    pub fn set_quorum(&mut self, q: Quorum) -> Result<(), String> {
        for (name, v, low) in [("r", q.r, 1), ("w", q.w, 1), ("dw", q.dw, 1), ("pr", q.pr, 0), ("pw", q.pw, 0)] {
            if !(low..=N_VAL).contains(&v) {
                return Err(format!("{name} must be {low} to {N_VAL}, not {v}."));
            }
        }
        self.inputs.push((self.now, Input::Quorum { quorum: q }));
        self.quorum = q;
        Ok(())
    }

    pub fn quorum(&self) -> Quorum {
        self.quorum
    }

    pub fn is_down(&self, n: usize) -> bool {
        self.down.contains(&n)
    }

    // ---- Running ----

    /// Run the next scheduled action. Returns false if there is none.
    pub fn step(&mut self) -> bool {
        let Some((&(t, seq), &action)) = self.schedule.iter().next() else { return false };
        self.schedule.remove(&(t, seq));
        self.now = self.now.max(t);
        match action {
            Action::Deliver(id) => self.deliver(id),
            Action::VnodeDone(p) => self.vnode_done(p),
            Action::FoldChunk { req, vnode } => self.fold_chunk(req, vnode),
            Action::Launch(id) => self.launch_waiting(id),
        }
        self.actions += 1;
        if !self.transfers.is_empty() || !self.leaving.is_empty() {
            self.try_flips();
        }
        if self.actions % 256 == 0 {
            self.prune();
        }
        true
    }

    /// Run every action due by `until`, at most `max_actions`, then set the
    /// clock to `until` if all of them ran. Returns the number run.
    pub fn advance_to(&mut self, until: Time, max_actions: usize) -> usize {
        let mut ran = 0;
        while ran < max_actions {
            match self.next_time() {
                Some(t) if t <= until => {
                    self.step();
                    ran += 1;
                }
                _ => {
                    self.now = self.now.max(until);
                    break;
                }
            }
        }
        ran
    }

    /// Run actions until none are scheduled, at most `max_actions`. The
    /// clock stops at the last action. Held messages stay held.
    pub fn run_until_idle(&mut self, max_actions: usize) -> usize {
        let mut ran = 0;
        while ran < max_actions && self.step() {
            ran += 1;
        }
        ran
    }

    // ---- Internals ----

    fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn schedule_at(&mut self, t: Time, action: Action) {
        self.seq += 1;
        self.schedule.insert((t, self.seq), action);
    }

    fn event(&mut self, req: u64, text: String) {
        let text = self.name_short_forms(req, text);
        self.events.push(Event { at: self.now, req, text, why: String::new(), summary: false });
    }

    /// Steps use full words. The first time a request's steps use one, the
    /// short form follows in brackets: "write acknowledgement (w ACK)".
    fn name_short_forms(&mut self, req: u64, mut text: String) -> String {
        const FORMS: [(&str, &str); 2] = [("durable-write acknowledgement", "dw ACK"), ("write acknowledgement", "w ACK")];
        for (long, short) in FORMS {
            if self.named.contains(&(req, short)) {
                continue;
            }
            // "write acknowledgement" must not match inside "durable-write acknowledgement".
            let found = text.match_indices(long).map(|(i, _)| i).find(|&i| !text[..i].ends_with("durable-"));
            if let Some(i) = found {
                let mut end = i + long.len();
                let plural = text[end..].starts_with('s');
                if plural {
                    end += 1;
                }
                text.insert_str(end, &format!(" ({short}{})", if plural { "s" } else { "" }));
                self.named.insert((req, short));
            }
        }
        text
    }

    /// The node a vnode runs on. Ids: below ring_size, the usual vnode of
    /// that partition; then ring_size * (1 + host) + partition for a fallback
    /// vnode; then a block of the same shape for a new owner's copy of a
    /// partition that is moving to it.
    fn owner(&self, id: usize) -> usize {
        let size = self.ring.size();
        if id < size {
            self.ring.owners[id]
        } else if id < self.moving_base() {
            id / size - 1
        } else {
            (id - self.moving_base()) / size
        }
    }

    fn moving_base(&self) -> usize {
        self.ring.size() * (1 + MAX_NODES)
    }

    /// The new owner's copy of partition `p` on node `to`.
    fn moving_id(&self, p: usize, to: usize) -> usize {
        self.moving_base() + self.ring.size() * to + p
    }

    fn is_moving(&self, id: usize) -> bool {
        id >= self.moving_base()
    }

    /// The partition a vnode is for.
    fn vpart(&self, id: usize) -> usize {
        id % self.ring.size()
    }

    fn is_fallback(&self, id: usize) -> bool {
        id >= self.ring.size() && id < self.moving_base()
    }

    /// A vnode named with the node it runs on, for steps.
    fn vnode_name(&self, id: usize) -> String {
        let kind = if self.is_fallback(id) {
            "fallback vnode"
        } else if self.is_moving(id) {
            "new vnode"
        } else {
            "vnode"
        };
        format!("{kind} {} on node{}", self.vpart(id), self.owner(id) + 1)
    }

    fn latency(&self, from: Endpoint, to: Endpoint) -> Time {
        let node = |e: Endpoint| match e {
            Endpoint::Node(n) => Some(n),
            Endpoint::Vnode(p) => Some(self.owner(p)),
            Endpoint::Client | Endpoint::Balancer => None,
        };
        if from == Endpoint::Client || to == Endpoint::Client {
            return self.timing.client;
        }
        match (node(from), node(to)) {
            (Some(a), Some(b)) if a == b => self.timing.local,
            _ => self.timing.node,
        }
    }

    fn send(&mut self, req: u64, from: Endpoint, to: Endpoint, payload: Payload) {
        let id = self.id();
        let deliver = self.now + self.latency(from, to);
        let label = payload.label();
        self.messages.insert(
            id,
            Message { id, req, from, to, payload, label, start: self.now, progress: 0.0, deliver: Some(deliver), remaining: 0 },
        );
        self.schedule_at(deliver, Action::Deliver(id));
    }

    fn deliver(&mut self, id: u64) {
        let Some(m) = self.messages.get(&id).cloned() else { return };
        // Only the last few seconds matter to the page; keep the list short
        // if nobody takes it.
        if self.hops.len() > 4096 {
            self.hops.drain(..2048);
        }
        self.hops.push(Hop { req: m.req, from: m.from, to: m.to, label: m.label.clone(), sent: m.start, arrived: self.now });
        match m.to {
            Endpoint::Vnode(p) => {
                // Stays visible in the mailbox until processed.
                self.messages.remove(&id);
                self.vnodes[p].mailbox.push_back(id);
                self.parked.insert(id, m);
                self.vnode_next(p);
            }
            Endpoint::Node(n) => {
                self.messages.remove(&id);
                self.at_node(n, m);
            }
            Endpoint::Client => {
                self.messages.remove(&id);
                if let Payload::Response { .. } = m.payload {
                    self.finish(m.req);
                }
            }
            Endpoint::Balancer => {
                self.messages.remove(&id);
                self.at_balancer(m);
            }
        }
    }
}

// Messages waiting in vnode mailboxes, kept apart from those in flight.
impl Sim {
    fn vnode_next(&mut self, p: usize) {
        if self.vnodes[p].busy.is_some() {
            return;
        }
        let Some(id) = self.vnodes[p].mailbox.pop_front() else { return };
        let m = self.parked[&id].clone();
        let coord = self.ename(m.from);
        let me = self.vnode_name(p);
        let waiting = self.vnodes[p].mailbox.len();
        let queue = if waiting > 0 { format!(" {waiting} more messages wait in its mailbox.") } else { String::new() };
        let req = self.requests[&m.req].clone();
        let delete = req.op == Op::Delete;
        let has_copy = match (&req.bucket, &req.key) {
            (Some(b), Some(k)) => self.vnodes[p].store.get(&(b.clone(), k.clone())).is_some_and(|o| !o.deleted),
            _ => false,
        };
        let key = req.key.clone().unwrap_or_default();
        if let (Payload::Head | Payload::Get, Some(b), Some(k)) = (&m.payload, &req.bucket, &req.key) {
            let get = matches!(m.payload, Payload::Get);
            let now = self.now;
            self.vnodes[p].leveled.read(now, &ledger_key(&(b.clone(), k.clone())), get);
        }
        let (service, text) = match m.payload {
            Payload::CoordPut => {
                // riak_kv_vnode replies `w` as soon as it takes the write.
                self.send(m.req, Endpoint::Vnode(p), m.from, Payload::W { vnode: p });
                let text = if let (Some(op), Some(dt)) = (&req.crdt_op, req.datatype) {
                    let change = match op {
                        crdt::Op::Count(n) if *n >= 0 => format!("adds {n} to its own entry in the counter"),
                        crdt::Op::Count(n) => format!("takes {} away in its own entry in the counter", n.unsigned_abs()),
                        crdt::Op::Add(e) => format!("adds {} to the set, tagged with its own id", list(e.clone())),
                        crdt::Op::Remove(e) => format!("removes {} from the set", list(e.clone())),
                    };
                    let why = match dt {
                        DataType::Counter => "Why: each vnode keeps its own count. When copies merge, Riak keeps the larger count for each vnode, so no update is lost and there are no siblings.",
                        DataType::Set => "Why: each element carries the ids of the adds that put it there. When copies merge, an element stays unless a copy that saw those adds removed it.",
                    };
                    format!("{me} gets the update from {coord}. It sends a write acknowledgement back at once. Then it {change}, makes a new vclock and saves the {}.{queue} {why}", dt.name())
                } else if !delete && req.bucket.as_ref().is_some_and(allow_mult) {
                    let seen = if req.context.is_some() {
                        "The client sent the vclock it read, so this value replaces the values that vclock has seen. Values it has not seen stay, as siblings."
                    } else {
                        "The client sent no vclock, so this value replaces nothing: any value already saved stays, and the two become siblings."
                    };
                    format!("{me} gets the write from {coord}. It sends a write acknowledgement back at once. Then it makes a new vclock and saves the value. {seen}{queue} Why: this bucket type keeps siblings (allow_mult), so Riak never drops a write that did not see another one.")
                } else if delete {
                    format!("{me} gets the delete from {coord}. It sends a write acknowledgement back at once. Then it makes a new vclock and saves a tombstone, a marker that says the object is deleted.{queue} Why: a copy that misses the delete cannot bring the object back later, because the tombstone has the newer vclock.")
                } else {
                    format!("{me} gets the write from {coord}. It sends a write acknowledgement back at once. Then it makes a new vclock and saves the copy.{queue} Why: the write acknowledgement says the write has arrived. The durable-write acknowledgement, later, says the copy is saved. A saved copy is not lost if the node restarts.")
                };
                (self.timing.put, text)
            }
            Payload::Put => {
                self.send(m.req, Endpoint::Vnode(p), m.from, Payload::W { vnode: p });
                let text = if let Some(dt) = req.datatype {
                    format!("{me} gets the update from {coord}. It sends a write acknowledgement back at once and merges the {} with its own copy.{queue}", dt.name())
                } else if !delete && req.bucket.as_ref().is_some_and(allow_mult) {
                    format!("{me} gets the write from {coord}. It sends a write acknowledgement back at once and merges it with its own copy: it keeps every value that the other has not seen.{queue}")
                } else {
                    let what = if delete { "the delete" } else { "the write" };
                    let saves = if delete { "the tombstone" } else { "the copy" };
                    format!("{me} gets {what} from {coord}. It sends a write acknowledgement back at once and saves {saves}.{queue}")
                };
                (self.timing.put, text)
            }
            Payload::Handoff { objects } => {
                let plural = if objects == 1 { "" } else { "s" };
                let text = format!("{me} gets {objects} object{plural} from {coord}. For each object, it keeps the version with the newer vclock, or merges a data type with its own copy, and saves it.{queue}");
                (self.timing.handoff_obj * objects.max(1) as u64, text)
            }
            Payload::Fold { ref first } => {
                let (bucket, scanned, found) = self.fold_scan(p, &req, first);
                let text = format!("{me} gets the list keys request from {coord}. It starts a key fold in its worker pool: it reads the {scanned} keys of bucket {bucket} and keeps the {found} that this vnode answers for. It sends them in chunks of up to 100.{queue} Why: the leveled ledger is in bucket and key order, so the fold reads only this bucket's range. It still runs on every vnode the coverage plan names, so a big bucket is still costly. The fold runs beside the vnode, so the vnode still answers other requests.");
                (self.timing.local, text)
            }
            Payload::HandoffAck if req.op == Op::Transfer => {
                let text = format!("{me} gets the handoff acknowledgement from {coord}. When no new write has arrived since, partition {} changes owner.{queue}", self.vpart(p));
                (self.timing.head, text)
            }
            Payload::HandoffAck => {
                let text = format!("{me} gets the handoff acknowledgement from {coord}. Every object is now on its usual vnode, so {me} deletes its copies and stops. Why: a fallback vnode only keeps copies while the usual vnode cannot.{queue}");
                (self.timing.head, text)
            }
            Payload::Head => {
                let text = if has_copy {
                    format!("{me} gets the HEAD request from {coord}. It has a copy of {key}, so it sends back the vclock of its copy.{queue}")
                } else {
                    format!("{me} gets the HEAD request from {coord}. It has no copy of {key}, so it sends back not found.{queue}")
                };
                (self.timing.head, text)
            }
            _ => (self.timing.get, format!("{me} gets the request for the value from {coord}. It reads its copy and sends it back.{queue}")),
        };
        self.event(m.req, text);
        let done = self.now + service;
        self.vnodes[p].busy = Some((id, done));
        self.schedule_at(done, Action::VnodeDone(p));
    }

    fn vnode_done(&mut self, p: usize) {
        let Some((id, _)) = self.vnodes[p].busy.take() else { return };
        let m = self.parked.remove(&id).expect("parked message");
        let Some(req) = self.requests.get(&m.req).cloned() else { return };
        if let Payload::Fold { first } = &m.payload {
            self.start_fold(p, m.req, first);
            self.vnode_next(p);
            return;
        }
        if let Payload::Handoff { .. } | Payload::HandoffAck = m.payload {
            self.handoff_done(p, &m);
            self.vnode_next(p);
            self.check_handoffs(Some(p), None);
            return;
        }
        let k = (req.bucket.clone().unwrap(), req.key.clone().unwrap());
        let me = Endpoint::Vnode(p);
        match m.payload {
            Payload::CoordPut => {
                // The coordinating vnode starts from its own copy and adds one
                // to its own vclock entry.
                let existing = self.vnodes[p].store.get(&k).cloned();
                let mut vclock = existing.as_ref().map(|o| o.vclock.clone()).unwrap_or_default();
                *vclock.entry(p).or_default() += 1;
                // A data type update: this vnode is the actor and applies
                // the change to its own copy (riak_kv_crdt:update/3).
                let crdt = match (&req.crdt_op, req.datatype) {
                    (Some(op), Some(dt)) => {
                        let mut c = existing.as_ref().and_then(|o| o.crdt.clone()).filter(|c| c.data_type() == dt).unwrap_or_else(|| Crdt::new(dt));
                        c.apply(p, op);
                        Some(c)
                    }
                    _ => None,
                };
                let body = if crdt.is_some() { vec![] } else { req.body.clone() };
                // allow_mult: keep the values the client's context has not
                // seen, and add this one with a new dot (riak-emu's rule).
                let mut siblings = vec![];
                if crdt.is_none() && req.bucket.as_ref().is_some_and(allow_mult) {
                    if let Some(c) = &req.context {
                        merge_vclocks(&mut vclock, c);
                    }
                    let dot = (p, vclock[&p]);
                    siblings = sibs::unseen(existing.as_ref().map(|o| o.siblings.as_slice()).unwrap_or(&[]), req.context.as_ref());
                    let vtag = self.vtag();
                    siblings.push(Sib { dot, value: body.clone(), content_type: req.content_type.clone(), vtag, deleted: req.op == Op::Delete });
                    siblings.sort_by(|x, y| x.value.cmp(&y.value).then(x.dot.cmp(&y.dot)));
                }
                let obj = Object { value: body, content_type: req.content_type.clone(), vclock, deleted: req.op == Op::Delete, crdt, siblings }.from_siblings();
                self.backend_put(p, &k, obj.deleted);
                self.vnodes[p].store.insert(k, obj.clone());
                self.requests.get_mut(&m.req).unwrap().found = Some(obj);
                self.send(m.req, me, m.from, Payload::Dw { vnode: p });
            }
            Payload::Put => {
                let obj = req.found.clone().expect("coordinated object");
                self.store_merge(p, k, obj);
                self.send(m.req, me, m.from, Payload::Dw { vnode: p });
            }
            Payload::Head => {
                let vclock = self.vnodes[p].store.get(&k).map(|o| o.vclock.clone());
                self.send(m.req, me, m.from, Payload::HeadReply { vnode: p, vclock });
            }
            Payload::Get => {
                // The object travels in the reply; it is kept on the request.
                // Data type values from several vnodes are merged.
                let obj = self.vnodes[p].store.get(&k).cloned();
                let r = self.requests.get_mut(&m.req).unwrap();
                r.found = match (r.found.take(), obj) {
                    (Some(mut a), Some(b)) if a.crdt.is_some() && b.crdt.is_some() => {
                        a.crdt.as_mut().unwrap().merge(b.crdt.as_ref().unwrap());
                        merge_vclocks(&mut a.vclock, &b.vclock);
                        Some(a)
                    }
                    (Some(mut a), Some(b)) if !a.siblings.is_empty() && !b.siblings.is_empty() => {
                        a.siblings = sibs::merge(&a.siblings, &a.vclock, &b.siblings, &b.vclock);
                        merge_vclocks(&mut a.vclock, &b.vclock);
                        Some(a.from_siblings())
                    }
                    (_, b) => b,
                };
                self.send(m.req, me, m.from, Payload::GetReply { vnode: p });
            }
            _ => {}
        }
        self.vnode_next(p);
        // A fallback vnode that is now idle may hand off.
        self.check_handoffs(Some(p), None);
    }

    /// Save an object that another vnode sent. A data type merges with the
    /// copy here. Otherwise the newer version stays; when neither vclock
    /// includes the other, Riak keeps both as siblings and the simulator
    /// keeps the one that arrived.
    fn store_merge(&mut self, p: usize, k: (BucketRef, String), obj: Object) {
        let tombstone = obj.deleted;
        if self.store_merge_inner(p, k.clone(), obj) {
            self.backend_put(p, &k, tombstone);
        }
    }

    /// Returns whether the stored object changed.
    fn store_merge_inner(&mut self, p: usize, k: (BucketRef, String), obj: Object) -> bool {
        let store = &mut self.vnodes[p].store;
        match store.get_mut(&k) {
            Some(here) if here.crdt.is_some() && obj.crdt.is_some() => {
                here.crdt.as_mut().unwrap().merge(obj.crdt.as_ref().unwrap());
                merge_vclocks(&mut here.vclock, &obj.vclock);
            }
            Some(here) if !here.siblings.is_empty() && !obj.siblings.is_empty() => {
                let merged = sibs::merge(&here.siblings, &here.vclock, &obj.siblings, &obj.vclock);
                here.siblings = merged;
                merge_vclocks(&mut here.vclock, &obj.vclock);
                *here = here.clone().from_siblings();
            }
            Some(here) if descends(&here.vclock, &obj.vclock) => return false,
            _ => {
                store.insert(k, obj);
            }
        }
        true
    }

    /// Write to the vnode's leveled backend.
    fn backend_put(&mut self, p: usize, k: &(BucketRef, String), tombstone: bool) {
        let mut r = self.random();
        let now = self.now;
        self.vnodes[p].leveled.put(now, ledger_key(k), tombstone, move |n| {
            r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((r >> 33) % n as u64) as usize
        });
    }

    /// The leveled store of a vnode, for the zoom view.
    pub fn leveled(&self, id: usize) -> Option<crate::leveled::LeveledView> {
        self.vnodes.get(id).map(|v| v.leveled.view())
    }

    /// A vnode has finished one handoff message.
    fn handoff_done(&mut self, p: usize, m: &Message) {
        let Endpoint::Vnode(from) = m.from else { return };
        match m.payload {
            Payload::Handoff { .. } => {
                // riak_kv_vnode:do_handoff_put: keep the newer version. When
                // neither vclock includes the other, Riak merges them; the
                // simulator keeps the handed-off one.
                let incoming: Vec<_> = self.vnodes[from].handoff.as_ref().map(|(_, objs)| objs.clone()).unwrap_or_default();
                for (k, obj) in incoming {
                    self.store_merge(p, k, obj);
                }
                self.send(m.req, Endpoint::Vnode(p), m.from, Payload::HandoffAck);
            }
            Payload::HandoffAck if self.requests.get(&m.req).is_some_and(|r| r.op == Op::Transfer) => {
                // A partition move: the old owner keeps its copy until the
                // partition changes owner (try_flips).
                if let Some(t) = self.transfers.iter_mut().find(|t| t.partition == p && t.req == m.req) {
                    t.active = false;
                    t.acked = true;
                }
            }
            Payload::HandoffAck => {
                // Delete what was sent, but not a newer copy written since.
                if let Some((_, sent)) = self.vnodes[p].handoff.take() {
                    for (k, obj) in sent {
                        if self.vnodes[p].store.get(&k).is_some_and(|o| o.vclock == obj.vclock) {
                            self.vnodes[p].store.remove(&k);
                            self.backend_put(p, &k, true);
                        }
                    }
                }
                let r = self.requests.get_mut(&m.req).unwrap();
                r.handoff_pending = r.handoff_pending.saturating_sub(1);
                if r.handoff_pending == 0 {
                    r.done = true;
                    r.status = Some(200);
                    let pl = r.preflist.clone();
                    let moved = list(pl.iter().map(|&id| self.vnode_name(id)).collect());
                    let to = list(pl.iter().map(|&id| self.vnode_name(self.vpart(id))).collect());
                    let text = format!("Hinted handoff is complete. {} sent {} copies back to {to}.", capital(&moved), if pl.len() == 1 { "its" } else { "their" });
                    self.events.push(Event { at: self.now, req: m.req, text, why: String::new(), summary: true });
                }
            }
            _ => {}
        }
    }

    /// How a step names an endpoint, with the node of a vnode.
    fn ename(&self, e: Endpoint) -> String {
        match e {
            Endpoint::Vnode(id) => self.vnode_name(id),
            other => name(other),
        }
    }

    // Every message that arrives is one step, told from the receiver's side:
    // who gets what from whom, what it does next, and, where it helps, why.
    // The one exception is the first step, where the client sends.

    /// "node1 now has 1 write acknowledgement. It needs 2, because W is 2."
    /// and what that means. `ack` is the full name, `quorum` the short one.
    fn tally(&self, rid: u64, n1: usize, ack: &str, got: usize, need: usize, quorum: &str) -> String {
        let r = &self.requests[&rid];
        let plural = if got == 1 { "" } else { "s" };
        let mut text = format!("node{n1} now has {got} {ack}{plural}.");
        if got > need {
            text.push_str(&format!(" It needed only {need}, so this one changes nothing."));
        } else if got == need {
            text.push_str(&format!(" This is enough, because {quorum} is {need}."));
        } else {
            text.push_str(&format!(" It needs {need}, because {quorum} is {need}."));
            if need == r.preflist.len() && r.remote_sent {
                text.push_str(&format!(" node{n1} cannot reply until every copy answers, because you asked for all {need} ({}={need}).", quorum.to_lowercase()));
            }
        }
        text
    }

    fn at_node(&mut self, n: usize, m: Message) {
        let rid = m.req;
        let Some(req) = self.requests.get(&rid).cloned() else { return };
        let me = Endpoint::Node(n);
        let n1 = n + 1;
        let from = name(m.from);
        let what = req.what();
        match m.payload {
            Payload::Request => self.start_request(n, req, &from),
            Payload::Forward => {
                let p = self.coord_put(n, &req);
                self.send(rid, me, Endpoint::Node(req.origin), Payload::ForwardAck);
                let vn = self.vnode_name(p);
                self.event(
                    rid,
                    format!(
                        "node{n1} gets the {what} from {from}. node{n1} is now the coordinator. It sends now_executing back to {from}. Then it sends the {what} to {vn} first. Why: {from} must know that the {what} has started. If {from} does not hear this in time, it tries another node, so one failed node does not lose the {what}."
                    ),
                );
            }
            Payload::ForwardAck => {
                self.event(
                    rid,
                    format!("node{n1} gets now_executing from {from}. It waits for the reply from {from}. Why: the client is connected to node{n1}, so node{n1} sends the reply to the client."),
                );
            }
            Payload::W { vnode: partition } => {
                let vn = self.vnode_name(partition);
                let r = self.requests.get_mut(&rid).unwrap();
                r.num_w += 1;
                let (late, got, need) = (r.status.is_some(), r.num_w, r.w);
                let mut text = if late {
                    format!("node{n1} gets a write acknowledgement from {vn}. The reply has already gone, so this changes nothing.")
                } else {
                    format!("node{n1} gets a write acknowledgement from {vn}. {}", self.tally(rid, n1, "write acknowledgement", got, need, "W"))
                };
                if let Some(more) = self.check_put(n, rid) {
                    text = format!("{text} {more}");
                }
                self.event(rid, text);
            }
            Payload::Dw { vnode: partition } => {
                let vn = self.vnode_name(partition);
                let r = self.requests.get_mut(&rid).unwrap();
                r.num_dw += 1;
                let first = !r.remote_sent;
                let (late, got, need) = (r.status.is_some(), r.num_dw, r.dw);
                let mut text = if late {
                    format!("node{n1} gets a durable-write acknowledgement from {vn}. The reply has already gone, so this changes nothing. The {} is saved all the same.", if req.op == Op::Delete { "tombstone" } else { "copy" })
                } else {
                    format!("node{n1} gets a durable-write acknowledgement from {vn}. {}", self.tally(rid, n1, "durable-write acknowledgement", got, need, "DW"))
                };
                if first {
                    // The coordinator's own vnode has stored the object. Now
                    // send the object, with its new vclock, to the other replicas.
                    let r = self.requests.get_mut(&rid).unwrap();
                    r.remote_sent = true;
                    let pl = r.preflist.clone();
                    let coord_p = pl.iter().copied().find(|&p| self.owner(p) == n).expect("coordinator vnode");
                    let others: Vec<usize> = pl.iter().copied().filter(|&p| p != coord_p).collect();
                    let list = others.iter().map(|&p| self.vnode_name(p)).collect::<Vec<_>>().join(" and ");
                    text = format!("{text} The new vclock is ready. node{n1} sends the {what} to {list}. Why: all copies get the same vclock, so Riak does not make two versions of one {what}.");
                    for p in others {
                        self.send(rid, me, Endpoint::Vnode(p), Payload::Put);
                    }
                }
                if let Some(more) = self.check_put(n, rid) {
                    text = format!("{text} {more}");
                }
                self.event(rid, text);
            }
            Payload::HeadReply { vnode: partition, vclock } => {
                let vn = self.vnode_name(partition);
                let found = vclock.is_some();
                let r = self.requests.get_mut(&rid).unwrap();
                r.heads.push((partition, vclock));
                r.num_r += 1;
                let late = r.status.is_some() || r.num_r > r.r;
                let ready = r.num_r == r.r && r.status.is_none();
                let (got, need) = (r.num_r, r.r);
                let answer = if found { "a vclock" } else { "not found" };
                let mut text = if late {
                    format!("node{n1} gets {answer} from {vn}. node{n1} already has enough answers, so this changes nothing.")
                } else {
                    let plural = if got == 1 { "" } else { "s" };
                    format!("node{n1} gets {answer} from {vn}. node{n1} now has {got} answer{plural}. It needs {need}, because R is {need}.")
                };
                if ready {
                    text = format!("{text} {}", self.after_heads(n, rid));
                }
                self.event(rid, text);
            }
            Payload::GetReply { vnode: partition } => {
                let vn = self.vnode_name(partition);
                if self.requests[&rid].status.is_some() {
                    return;
                }
                let r = self.requests.get_mut(&rid).unwrap();
                r.fetch_pending = r.fetch_pending.saturating_sub(1);
                if r.fetch_pending > 0 {
                    let left = r.fetch_pending;
                    self.event(rid, format!("node{n1} gets the value from {vn}. It waits for {left} more."));
                    return;
                }
                let req = self.requests[&rid].clone();
                let live = req.found.as_ref().map(|o| o.live()).unwrap_or(0);
                let status = match &req.vtag {
                    Some(t) => {
                        if req.found.as_ref().is_some_and(|o| o.siblings.iter().any(|s| &s.vtag == t && !s.deleted)) { 200 } else { 404 }
                    }
                    None if live > 1 => 300,
                    None if live == 1 => 200,
                    None => 404,
                };
                let to = self.reply_target(n, rid);
                let several = req.fetches > 1;
                if status == 300 {
                    let merged = if several { "It merges the values. " } else { "" };
                    self.event(rid, format!("node{n1} gets the value from {vn}. {merged}The key has {live} siblings: values that no write has replaced. node{n1} sends 300 Multiple Choices to {to}, with the vclock. Why: Riak does not choose between writes that did not see each other. The client chooses, then writes with this vclock to replace them."));
                    self.reply(n, rid, status);
                    return;
                }
                let text = match req.found.as_ref().and_then(|o| o.crdt.as_ref()) {
                    Some(c) if several => format!("node{n1} gets the value from {vn}. It merges the values into one {}: {}. It sends {status} to {to}.", c.data_type().name(), c.describe()),
                    Some(c) => format!("node{n1} gets the {} from {vn}: {}. It sends {status} with it to {to}.", c.data_type().name(), c.describe()),
                    None => format!("node{n1} gets the value from {vn}. It sends {status} with the value to {to}."),
                };
                self.event(rid, text);
                self.reply(n, rid, status);
            }
            Payload::Keys { keys, last } => {
                let vn = self.ename(m.from);
                let r = self.requests.get_mut(&rid).unwrap();
                let count = keys.len();
                r.keys.extend(keys);
                if last {
                    r.folds_pending = r.folds_pending.saturating_sub(1);
                }
                let (left, total) = (r.folds_pending, r.keys.len());
                if last && left == 0 {
                    let to = self.reply_target(n, rid);
                    self.event(rid, format!("node{n1} gets the last {count} keys from {vn}. Every fold has finished: node{n1} has {total} keys. It sends 200 with the list to {to}."));
                    self.reply(n, rid, 200);
                } else {
                    let tail = if last { format!(" That fold has finished; {left} still run.") } else { String::new() };
                    self.event(rid, format!("node{n1} gets {count} keys from {vn}. It has {total} keys so far.{tail}"));
                }
            }
            Payload::Result => {
                let status = req.status.unwrap_or(500);
                let to = if req.via_balancer { Endpoint::Balancer } else { Endpoint::Client };
                self.event(rid, format!("node{n1} gets the reply ({status}) from {from}. It sends the reply to {}.", name(to)));
                self.send(rid, me, to, Payload::Response { status });
            }
            _ => {}
        }
    }

    /// The load balancer sends each new request to the next node in turn
    /// and passes each response back to the client.
    fn at_balancer(&mut self, m: Message) {
        let rid = m.req;
        match m.payload {
            Payload::Request => {
                // The load balancer's health check finds stopped nodes and skips them.
                let members = self.members();
                let mut skipped = vec![];
                let mut n = members[self.next_node % members.len()];
                self.next_node += 1;
                while self.down.contains(&n) {
                    skipped.push(format!("node{}", n + 1));
                    n = members[self.next_node % members.len()];
                    self.next_node += 1;
                }
                self.requests.get_mut(&rid).unwrap().origin = n;
                let text = if skipped.is_empty() {
                    format!("The load balancer gets the request from the client. It sends the request to node{}, the next node in its list. Why: Riak has no leader node, so every node can take every request. If one node fails, the load balancer sends requests to the other nodes.", n + 1)
                } else {
                    let s = list(skipped);
                    format!("The load balancer gets the request from the client. {s} is stopped, so the load balancer skips it and sends the request to node{}. Why: Riak has no leader node, so any running node can take the request.", n + 1)
                };
                self.event(rid, text);
                self.send(rid, Endpoint::Balancer, Endpoint::Node(n), Payload::Request);
            }
            Payload::Response { status } => {
                self.event(rid, format!("The load balancer gets the reply ({status}) from {}. It sends the reply to the client.", name(m.from)));
                self.send(rid, Endpoint::Balancer, Endpoint::Client, Payload::Response { status });
            }
            _ => {}
        }
    }

    fn start_request(&mut self, n: usize, req: RequestState, from: &str) {
        let rid = req.id;
        self.requests.get_mut(&rid).unwrap().arrived = true;
        let me = Endpoint::Node(n);
        let n1 = n + 1;
        let key = req.key.clone().unwrap_or_default();
        match req.op {
            Op::Ping => {
                let to = self.reply_target(n, rid);
                self.event(rid, format!("node{n1} gets the ping from {from}. It sends OK to {to}."));
                self.reply(n, rid, 200)
            }
            Op::Invalid => {
                let unsupported = req.error.starts_with("The simulator does not support");
                let status = if unsupported { 501 } else if req.not_found { 404 } else { 400 };
                let to = self.reply_target(n, rid);
                let reason = if unsupported { req.error.split(" Supported:").next().unwrap_or(&req.error).to_string() } else { format!("The request is not valid. {}", req.error) };
                self.event(rid, format!("node{n1} gets the request from {from}. {reason} node{n1} sends {status} to {to}."));
                self.reply(n, rid, status)
            }
            Op::Put | Op::Delete => {
                let what = req.what();
                let pl = self.preflist(&req);
                let parts = self.parts(&req);
                let stand_in = self.stand_in(&pl);
                self.requests.get_mut(&rid).unwrap().preflist = pl.clone();
                let primaries = pl.iter().filter(|&&id| !self.is_fallback(id)).count();
                if req.pw > primaries {
                    // riak_kv_put_fsm: PW > NumPrimaries.
                    let to = self.reply_target(n, rid);
                    self.requests.get_mut(&rid).unwrap().error = format!("PW-value unsatisfied: {primaries}/{}", req.pw);
                    self.event(rid, format!("node{n1} gets the {what} for {key} from {from}.{stand_in} Only {primaries} of the 3 usual vnodes for {key} are running, and pw is {}. So node{n1} sends 503 to {to}. Why: pw asks for copies on their usual vnodes. Without it, Riak takes the {what} even when every copy is on a fallback vnode.", req.pw));
                    self.reply(n, rid, 503);
                } else if pl.is_empty() {
                    let to = self.reply_target(n, rid);
                    self.event(rid, format!("node{n1} gets the {what} for {key} from {from}. Every node with a copy, and every node that could keep a copy for them, is stopped. node{n1} sends 503 to {to}."));
                    self.reply(n, rid, 503);
                } else if pl.iter().any(|&p| self.owner(p) == n) {
                    let p = self.coord_put(n, &self.requests[&rid].clone());
                    let vn = self.vnode_name(p);
                    let holds = if self.is_fallback(p) { format!("node{n1} runs {vn}") } else { format!("node{n1} holds partition {}", self.vpart(p)) };
                    self.event(
                        rid,
                        format!(
                            "node{n1} gets the {what} for {key} from {from}. The key {key} belongs on partitions {parts}.{stand_in} {holds}, so node{n1} is the coordinator. It sends the {what} to {vn} first. Why: only a node with a copy can coordinate the {what}, because its vnode makes the new vclock."
                        ),
                    );
                } else {
                    let c = self.owner(pl[0]);
                    self.requests.get_mut(&rid).unwrap().coordinator = Some(c);
                    let vnodes = list(pl.iter().map(|&p| self.vnode_name(p)).collect());
                    self.event(
                        rid,
                        format!(
                            "node{n1} gets the {what} for {key} from {from}. The key {key} belongs on partitions {parts}, and node{n1} holds none of them.{stand_in} node{n1} asks {vnodes} how many messages wait in each mailbox. It sends the {what} to node{c1}, the node of the first vnode with a short mailbox. Why: only a node with a copy can coordinate the {what}, because its vnode makes the new vclock. Choosing a vnode with a short mailbox keeps a busy node from slowing the {what}.",
                            c1 = c + 1
                        ),
                    );
                    self.send(rid, me, Endpoint::Node(c), Payload::Forward);
                }
            }
            // Handoff and transfers start inside the cluster, not from a request.
            Op::Handoff | Op::Transfer => {}
            Op::ListKeys => self.start_list_keys(n, req, &from),
            Op::Get if req.pr > self.preflist(&req).iter().filter(|&&id| !self.is_fallback(id)).count() => {
                // riak_kv_get_fsm validate_quorum: PR > NumPrimaries.
                let pl = self.preflist(&req);
                let primaries = pl.iter().filter(|&&id| !self.is_fallback(id)).count();
                let stand_in = self.stand_in(&pl);
                let to = self.reply_target(n, rid);
                self.requests.get_mut(&rid).unwrap().error = format!("PR-value unsatisfied: {primaries}/{}", req.pr);
                self.event(rid, format!("node{n1} gets the read for {key} from {from}.{stand_in} Only {primaries} of the 3 usual vnodes for {key} are running, and pr is {}. So node{n1} sends 503 to {to}.", req.pr));
                self.reply(n, rid, 503);
            }
            Op::Get => {
                let pl = self.preflist(&req);
                let vnodes = list(pl.iter().map(|&p| self.vnode_name(p)).collect());
                let stand_in = self.stand_in(&pl);
                let r = self.requests.get_mut(&rid).unwrap();
                r.preflist = pl.clone();
                r.coordinator = Some(n);
                self.event(
                    rid,
                    format!(
                        "node{n1} gets the read for {key} from {from}. node{n1} is the coordinator.{stand_in} node{n1} sends HEAD to {vnodes}. HEAD asks for the vclock only. Why: a read makes no new vclock, so any node can coordinate it. A HEAD answer is small, because it has no value."
                    ),
                );
                for p in pl {
                    self.send(rid, me, Endpoint::Vnode(p), Payload::Head);
                }
            }
        }
    }

    /// The first partition of a key: the start of its preflist.
    fn first_partition(&self, b: &BucketRef, key: &str) -> usize {
        self.ring.preflist(&b.hash(key), 1)[0]
    }

    /// riak_core_coverage_plan: enough usual vnodes on running nodes that
    /// every key is answered once. Every Nth vnode first, from an offset set
    /// by the request, then the others where a node is stopped. Each chosen
    /// vnode answers for the key ranges (first partitions) not yet covered.
    fn coverage_plan(&self, rid: u64) -> Option<Vec<(usize, Vec<usize>)>> {
        let size = self.ring.size();
        let start = rid as usize % size;
        let mut uncovered: std::collections::BTreeSet<usize> = (0..size).collect();
        let mut plan = vec![];
        for off in 0..N_VAL {
            for i in 0..size.div_ceil(N_VAL) {
                let p = (start + off + i * N_VAL) % size;
                if uncovered.is_empty() {
                    return Some(plan);
                }
                if self.down.contains(&self.owner(p)) || plan.iter().any(|(q, _)| *q == p) {
                    continue;
                }
                // Vnode p keeps keys whose preflist starts at p, p-1 or p-2.
                let covers: Vec<usize> = (0..N_VAL).map(|d| (p + size - d) % size).filter(|q| uncovered.contains(q)).collect();
                if covers.is_empty() {
                    continue;
                }
                for q in &covers {
                    uncovered.remove(q);
                }
                plan.push((p, covers));
            }
        }
        uncovered.is_empty().then_some(plan)
    }

    fn start_list_keys(&mut self, n: usize, req: RequestState, from: &str) {
        let rid = req.id;
        let n1 = n + 1;
        let bucket = req.bucket.as_ref().map(|b| b.bucket.clone()).unwrap_or_default();
        let r = self.requests.get_mut(&rid).unwrap();
        r.coordinator = Some(n);
        let Some(plan) = self.coverage_plan(rid) else {
            let to = self.reply_target(n, rid);
            self.requests.get_mut(&rid).unwrap().error = "Error: insufficient_vnodes_available".into();
            self.event(rid, format!("node{n1} gets the list keys request for bucket {bucket} from {from}. Too many nodes are stopped to cover every key range, so node{n1} sends 503 to {to}."));
            self.reply(n, rid, 503);
            return;
        };
        let nodes: std::collections::BTreeSet<usize> = plan.iter().map(|(p, _)| self.owner(*p)).collect();
        let r = self.requests.get_mut(&rid).unwrap();
        r.folds_pending = plan.len();
        r.preflist = plan.iter().map(|(p, _)| *p).collect();
        let size = self.ring.size();
        self.event(
            rid,
            format!(
                "node{n1} gets the list keys request for bucket {bucket} from {from}. Riak keeps no list of the keys in a bucket, so node{n1} makes a coverage plan: {} of the {size} vnodes, on {} nodes, that together hold a copy of every key. It asks each of them to fold over its keys. Why: a read of one key asks 3 vnodes. Listing keys asks vnodes on every node, and each reads every key it holds. Riak's documentation warns not to list keys on a busy production cluster.",
                plan.len(),
                nodes.len()
            ),
        );
        for (p, first) in plan {
            self.send(rid, Endpoint::Node(n), Endpoint::Vnode(p), Payload::Fold { first });
        }
    }

    /// What a fold in vnode `p` reads and finds: the bucket name, keys read
    /// (leveled reads only the bucket's range of its ledger) and keys found.
    fn fold_scan(&self, p: usize, req: &RequestState, first: &[usize]) -> (String, usize, usize) {
        let b = req.bucket.clone().unwrap();
        let scanned = self.vnodes[p].leveled.bucket_keys(&ledger_key(&(b.clone(), String::new())).0);
        let found = self.vnodes[p]
            .store
            .iter()
            .filter(|((kb, k), o)| *kb == b && !o.deleted && first.contains(&self.first_partition(kb, k)))
            .count();
        (b.bucket, scanned, found)
    }

    /// The fold runs in the vnode's worker pool: it reads every key, and
    /// sends a chunk each time 100 keys are found, spread over its time.
    fn start_fold(&mut self, p: usize, rid: u64, first: &[usize]) {
        let Some(req) = self.requests.get(&rid).cloned() else { return };
        let b = req.bucket.clone().unwrap();
        let name = ledger_key(&(b.clone(), String::new())).0;
        let scanned = self.vnodes[p].leveled.bucket_keys(&name);
        let all = self.vnodes[p].store.len();
        let now = self.now;
        self.vnodes[p].leveled.fold(now, &name, scanned, all);
        let found: Vec<String> = self.vnodes[p]
            .store
            .iter()
            .filter(|((kb, k), o)| *kb == b && !o.deleted && first.contains(&self.first_partition(kb, k)))
            .map(|((_, k), _)| k.clone())
            .collect();
        self.requests.get_mut(&rid).unwrap().keys_scanned += scanned;
        let chunks = found.len().div_ceil(100).max(1);
        let duration = (scanned as u64 * self.timing.fold_key).max(1);
        self.vnodes[p].folds.push((rid, found, chunks));
        for k in 1..=chunks {
            self.schedule_at(self.now + duration * k as u64 / chunks as u64, Action::FoldChunk { req: rid, vnode: p });
        }
    }

    fn fold_chunk(&mut self, rid: u64, p: usize) {
        let Some(i) = self.vnodes[p].folds.iter().position(|f| f.0 == rid) else { return };
        let (_, keys, chunks) = &mut self.vnodes[p].folds[i];
        let take = keys.len().min(100);
        let chunk: Vec<String> = keys.drain(..take).collect();
        *chunks -= 1;
        let last = *chunks == 0;
        if last {
            self.vnodes[p].folds.remove(i);
        }
        let Some(coord) = self.requests.get(&rid).and_then(|r| r.coordinator) else { return };
        self.send(rid, Endpoint::Vnode(p), Endpoint::Node(coord), Payload::Keys { keys: chunk, last });
    }

    /// Who the coordinator's reply goes to.
    fn reply_target(&self, n: usize, rid: u64) -> String {
        let r = &self.requests[&rid];
        if n != r.origin {
            format!("node{}", r.origin + 1)
        } else if r.via_balancer {
            "the load balancer".into()
        } else {
            "the client".into()
        }
    }

    /// The vnodes a request uses, as riak_core_apl:get_apl_ann/3 finds them:
    /// the key's usual vnodes on running nodes, then, for each usual vnode on
    /// a stopped node, a fallback vnode for the same partition. Fallback
    /// nodes are the running owners of the partitions after the first N, in
    /// ring order.
    fn preflist(&self, req: &RequestState) -> Vec<usize> {
        let hash = req.bucket.as_ref().unwrap().hash(req.key.as_ref().unwrap());
        let size = self.ring.size();
        let all = self.ring.preflist(&hash, size);
        let (primaries, rest) = all.split_at(N_VAL.min(size));
        let mut up: Vec<usize> = primaries.iter().copied().filter(|&p| !self.down.contains(&self.owner(p))).collect();
        let pangs: Vec<usize> = primaries.iter().copied().filter(|&p| self.down.contains(&self.owner(p))).collect();
        let mut hosts = rest.iter().map(|&p| self.ring.owners[p]).filter(|h| !self.down.contains(h));
        for p in pangs {
            match hosts.next() {
                Some(h) => up.push(size * (1 + h) + p),
                None => break,
            }
        }
        up
    }

    /// The key's partitions, as "15, 16 and 17".
    fn parts(&self, req: &RequestState) -> String {
        let hash = req.bucket.as_ref().unwrap().hash(req.key.as_ref().unwrap());
        list(self.ring.preflist(&hash, N_VAL).iter().map(|p| p.to_string()).collect())
    }

    /// A sentence on fallback vnodes in a preflist, or nothing.
    fn stand_in(&self, pl: &[usize]) -> String {
        let fallbacks: Vec<usize> = pl.iter().copied().filter(|&id| self.is_fallback(id)).collect();
        if fallbacks.is_empty() {
            return String::new();
        }
        let usual = list(fallbacks.iter().map(|&id| self.vnode_name(self.vpart(id))).collect());
        let mut stopped: Vec<usize> = fallbacks.iter().map(|&id| self.ring.owners[self.vpart(id)]).collect();
        stopped.sort();
        stopped.dedup();
        let are = if stopped.len() == 1 { "is" } else { "are" };
        let stopped = list(stopped.iter().map(|n| format!("node{}", n + 1)).collect());
        let instead = list(fallbacks.iter().map(|&id| self.vnode_name(id)).collect());
        format!(" {stopped} {are} stopped, so {usual} cannot be used. Riak uses {instead} instead. Why: Riak still keeps 3 copies. Riak picks a fallback vnode on the next running node around the ring. It keeps the copy until the stopped node is back.")
    }

    /// Send the write to the coordinator's own vnode. Returns its partition.
    fn coord_put(&mut self, n: usize, req: &RequestState) -> usize {
        let p = req.preflist.iter().copied().find(|&p| self.owner(p) == n).expect("local replica");
        self.requests.get_mut(&req.id).unwrap().coordinator = Some(n);
        self.send(req.id, Endpoint::Node(n), Endpoint::Vnode(p), Payload::CoordPut);
        p
    }

    /// riak_kv_put_core:enough/1, without PW and node_confirms. Checked only
    /// after the write to the other replicas has started, as in the source.
    /// If the coordinator can reply, sends the reply and returns a sentence
    /// that says so.
    fn check_put(&mut self, n: usize, rid: u64) -> Option<String> {
        let r = &self.requests[&rid];
        if r.status.is_some() || !r.remote_sent || r.num_w < r.w || r.num_dw < r.dw {
            return None;
        }
        let all = r.preflist.len();
        let waits_for_all = r.w >= all && r.dw >= all;
        let what = r.what();
        let to = self.reply_target(n, rid);
        self.reply(n, rid, 204);
        Some(if waits_for_all {
            format!("So node{} sends 204 to {to}. All {all} copies are saved.", n + 1)
        } else {
            format!("node{} now has enough write and durable-write acknowledgements, so it sends 204 to {to}. Why: Riak does not wait for every copy. So the {what} succeeds even when one copy is slow or its node is down.", n + 1)
        })
    }

    /// Called when R HEAD replies are in. Sends the next message and returns
    /// a sentence that says what happens.
    fn after_heads(&mut self, n: usize, rid: u64) -> String {
        let r = &self.requests[&rid];
        let rr = r.r;
        // Newest copy among the replies: one whose vclock descends from all others.
        let found: Vec<&(usize, Option<BTreeMap<usize, u64>>)> = r.heads.iter().filter(|(_, v)| v.is_some()).collect();
        let same = found.windows(2).all(|w| w[0].1 == w[1].1);
        let several = found.len() > 1;
        let best = found
            .iter()
            .find(|(_, a)| found.iter().all(|(_, b)| descends(a.as_ref().unwrap(), b.as_ref().unwrap())))
            .or(found.first())
            .map(|(p, _)| *p);
        drop(found);
        // A data type: ask every vnode whose vclock no other answer includes,
        // and merge their values (riak_kv_get_core head_merge, then fetch).
        if (r.datatype.is_some() || r.bucket.as_ref().is_some_and(allow_mult)) && best.is_some() {
            let heads = r.heads.clone();
            let r_is_crdt = r.datatype.is_some();
            let found: Vec<&(usize, Option<BTreeMap<usize, u64>>)> = heads.iter().filter(|(_, v)| v.is_some()).collect();
            let mut frontier: Vec<(usize, BTreeMap<usize, u64>)> = vec![];
            for (p, v) in &found {
                let v = v.clone().unwrap();
                let covered = found.iter().any(|(_, o)| {
                    let o = o.as_ref().unwrap();
                    descends(o, &v) && *o != v
                });
                if !covered && !frontier.iter().any(|(_, f)| *f == v) {
                    frontier.push((*p, v));
                }
            }
            if frontier.len() > 1 {
                let ids: Vec<usize> = frontier.iter().map(|(p, _)| *p).collect();
                let r = self.requests.get_mut(&rid).unwrap();
                r.fetch_pending = ids.len();
                r.fetches = ids.len();
                r.fetched_from = Some(ids[0]);
                for &p in &ids {
                    self.send(rid, Endpoint::Node(n), Endpoint::Vnode(p), Payload::Get);
                }
                let names = list(ids.iter().map(|&p| self.vnode_name(p)).collect());
                let why = if r_is_crdt { "Why: a data type merges without losing an update, so Riak needs no siblings." } else { "Why: Riak keeps every value that no write replaced. The client sees them as siblings and chooses." };
                return format!("This is enough answers. The vclocks do not include each other, so the copies took different writes. node{} asks {names} for their values, to merge them. {why}", n + 1);
            }
        }
        match best {
            None => {
                let to = self.reply_target(n, rid);
                self.reply(n, rid, 404);
                format!("This is enough answers, and every answer is not found. node{} sends 404 to {to}. Why: an answer of not found counts as an answer (notfound_ok).", n + 1)
            }
            Some(p) => {
                let r = self.requests.get_mut(&rid).unwrap();
                r.fetched_from = Some(p);
                r.fetch_pending = 1;
                r.fetches = 1;
                self.send(rid, Endpoint::Node(n), Endpoint::Vnode(p), Payload::Get);
                let which = if same && several {
                    format!("The answers have the same vclock. node{} asks {} for the value.", n + 1, self.vnode_name(p))
                } else {
                    format!("{} has the newest vclock. node{} asks it for the value.", self.vnode_name(p), n + 1)
                };
                let w = self.quorum.w;
                let why = if rr + w > 3 {
                    format!("Why: R is {rr}. With W of {w}, {rr} + {w} is more than the 3 copies. So when no node has failed, at least one answer comes from a copy with the newest write. When one node is down, the read still works with the other copies.")
                } else {
                    format!("Why: R is {rr}, so the read is fast. But with W of {w}, {rr} + {w} is not more than the 3 copies, so the answer can come from a copy that missed the newest write.")
                };
                format!("This is enough answers. {which} {why}")
            }
        }
    }

    /// Decide the reply. It goes to the client from the node that received
    /// the request, through that node if another node coordinated.
    fn reply(&mut self, n: usize, rid: u64, status: u16) {
        let r = self.requests.get_mut(&rid).unwrap();
        r.status = Some(status);
        let origin = r.origin;
        if n == origin {
            let to = if r.via_balancer { Endpoint::Balancer } else { Endpoint::Client };
            self.send(rid, Endpoint::Node(n), to, Payload::Response { status });
        } else {
            self.send(rid, Endpoint::Node(n), Endpoint::Node(origin), Payload::Result);
        }
    }

    fn finish(&mut self, rid: u64) {
        let r = self.requests.get_mut(&rid).unwrap();
        r.done = true;
        let status = r.status.unwrap_or(500);
        let mut headers = vec![("Server".to_string(), "MochiWeb/1.1 WebMachine/1.10.9 (riak-sim)".to_string())];
        let body = match (r.op, status) {
            (Op::Ping, _) => "OK".to_string(),
            (Op::Get, 300) => {
                let o = r.found.as_ref().unwrap();
                headers.push(("X-Riak-Vclock".into(), sibs::encode(&o.vclock)));
                let live: Vec<&Sib> = o.siblings.iter().filter(|s| !s.deleted).collect();
                if r.multipart {
                    let boundary = "sibling";
                    headers.push(("Content-Type".into(), format!("multipart/mixed; boundary={boundary}")));
                    let mut body = String::new();
                    for s in live {
                        body.push_str(&format!("\r\n--{boundary}\r\nContent-Type: {}\r\nEtag: {}\r\n\r\n{}", s.content_type, s.vtag, String::from_utf8_lossy(&s.value)));
                    }
                    body.push_str(&format!("\r\n--{boundary}--\r\n"));
                    body
                } else {
                    headers.push(("Content-Type".into(), "text/plain".into()));
                    let mut body = String::from("Siblings:\n");
                    for s in live {
                        body.push_str(&s.vtag);
                        body.push('\n');
                    }
                    body
                }
            }
            (Op::Get, 200) => {
                let o = r.found.as_ref().unwrap();
                if o.crdt.is_none() {
                    headers.push(("X-Riak-Vclock".into(), sibs::encode(&o.vclock)));
                }
                // ?vtag= picks one sibling.
                let chosen = r.vtag.as_ref().and_then(|t| o.siblings.iter().find(|s| &s.vtag == t));
                if let Some(s) = chosen {
                    headers.push(("Content-Type".into(), s.content_type.clone()));
                    String::from_utf8_lossy(&s.value).into_owned()
                } else { match &o.crdt {
                    Some(c) => {
                        headers.push(("Content-Type".into(), "application/json".into()));
                        c.fetch_json()
                    }
                    None => {
                        headers.push(("Content-Type".into(), o.content_type.clone()));
                        String::from_utf8_lossy(&o.value).into_owned()
                    }
                } }
            }
            (Op::Get, 404) => match r.datatype {
                Some(dt) => format!("{{\"type\":\"{}\",\"error\":\"notfound\"}}", dt.name()),
                None => "not found\n".into(),
            },
            (Op::ListKeys, 200) => {
                headers.push(("Content-Type".into(), "application/json".into()));
                let keys: Vec<String> = r.keys.iter().map(|k| format!("\"{}\"", k.replace('"', "\\\""))).collect();
                format!("{{\"keys\":[{}]}}", keys.join(","))
            }
            (Op::Invalid, _) => format!("{}\n", r.error),
            (_, 503) if !r.error.is_empty() => format!("{}\n", r.error),
            _ => String::new(),
        };
        self.responses.push(HttpResponse { req: rid, status, reason: reason(status), headers, body });
        let from = if self.requests[&rid].via_balancer { "the load balancer".to_string() } else { format!("node{}", self.requests[&rid].origin + 1) };
        // Work for this request that is still in flight or in a vnode.
        let pending = self.messages.values().any(|m| m.req == rid) || self.parked.values().any(|m| m.req == rid);
        let tail = if pending { " Some copies have not finished yet. They finish after this." } else { "" };
        self.event(rid, format!("The client gets {status} {} from {from}. The request is complete.{tail}", reason(status)));
        let text = self.summary(rid);
        self.events.push(Event { at: self.now, req: rid, text, why: String::new(), summary: true });
    }

    /// One or two sentences on what a finished request did.
    fn summary(&self, rid: u64) -> String {
        let r = &self.requests[&rid];
        let status = r.status.unwrap_or(500);
        let parts = list(r.preflist.iter().map(|&p| self.vpart(p).to_string()).collect());
        let owners: Vec<usize> = r.preflist.iter().map(|&p| self.owner(p)).collect();
        let fallbacks: Vec<String> = r.preflist.iter().filter(|&&p| self.is_fallback(p)).map(|&p| self.vnode_name(p)).collect();
        let stand_in = if fallbacks.is_empty() { String::new() } else { {
            let names = list(fallbacks.clone());
            let mut c = names.chars();
            let names = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
            format!(" {names} kept a copy for a stopped node.")
        } };
        let mut distinct = owners.clone();
        distinct.sort();
        distinct.dedup();
        let nodes = list(owners.iter().map(|n| format!("node{}", n + 1)).collect());
        let shared = if distinct.len() < owners.len() && fallbacks.is_empty() {
            " Some copies are on the same node, because the cluster has too few nodes to spread them.".to_string()
        } else if distinct.len() < owners.len() {
            let on = list(distinct.iter().map(|n| format!("node{}", n + 1)).collect());
            let all = if distinct.len() == 1 { format!("All {} copies are on {on}", owners.len()) } else { format!("The {} copies are on only {on}", owners.len()) };
            format!(" {all}, because other nodes are stopped. The data is only as safe as {on} until the other nodes are back. To refuse a write like this, use pw.")
        } else {
            String::new()
        };
        let key = r.key.as_deref().unwrap_or("");
        let bucket = r.bucket.as_ref().map(|b| b.bucket.as_str()).unwrap_or("");
        let coord = r.coordinator.map(|c| format!("node{}", c + 1)).unwrap_or_default();
        let n = r.preflist.len();
        match r.op {
            Op::Get if status == 300 => format!(
                "Read {key} from bucket {bucket}: {} siblings. To resolve them, write the value you want with the vclock from this read (X-Riak-Vclock).",
                r.found.as_ref().map(|o| o.live()).unwrap_or(0)
            ),
            Op::Put | Op::Delete | Op::Get if status == 503 && !r.error.is_empty() => format!(
                "Riak refused the request with 503: {}. Too few of the usual vnodes for {key} are running.",
                r.error
            ),
            Op::Put if r.datatype.is_some() => format!(
                "Updated the {dt} {key} in bucket {bucket} (bucket type {t}).{stand_in} {coord} applied the change in its own vnode, and the other copies merged it. It replied {status} when {w} copies had the update and {dw} had saved it.",
                dt = r.datatype.unwrap().name(),
                t = r.bucket.as_ref().and_then(|b| b.bucket_type.clone()).unwrap_or_default(),
                w = r.w,
                dw = r.dw,
            ),
            Op::Get if status == 200 && r.datatype.is_some() => {
                let c = r.found.as_ref().and_then(|o| o.crdt.as_ref());
                format!(
                    "Read the {} {key} from bucket {bucket}: {}. {coord} sent HEAD to {n} vnodes and waited for {rr} answers.",
                    r.datatype.unwrap().name(),
                    c.map(|c| c.describe()).unwrap_or_default(),
                    rr = r.r,
                )
            }
            Op::Put => format!(
                "Saved {key} in bucket {bucket} on partitions {parts}, on {nodes}.{shared}{stand_in} {coord} coordinated the write. It replied {status} when {w} copies had the write and {dw} had saved it. Now {got} of {n} copies are saved.",
                w = r.w,
                dw = r.dw,
                got = r.num_dw,
            ),
            Op::Delete => format!(
                "Deleted {key} from bucket {bucket}. Riak saved a tombstone on partitions {parts}, on {nodes}.{shared}{stand_in} Now {got} of {n} tombstones are saved.",
                got = r.num_dw,
            ),
            Op::Get if status == 200 => format!(
                "Read {key} from bucket {bucket}. {coord} sent HEAD to {n} vnodes, waited for {rr} answers, and got the value from {v}.",
                rr = r.r,
                v = r.fetched_from.map(|p| self.vnode_name(p)).unwrap_or_default(),
            ),
            Op::Get => format!("The key {key} is not in bucket {bucket}. {coord} got {rr} answers, and every one was not found.", rr = r.r),
            Op::Ping => format!("node{} answered the ping.", r.origin + 1),
            Op::Invalid => format!("The request was rejected. {}", r.error),
            Op::Handoff | Op::Transfer => String::new(),
            Op::ListKeys if status == 200 => {
                let nodes: std::collections::BTreeSet<usize> = r.preflist.iter().map(|&p| self.owner(p)).collect();
                format!(
                    "Listed {} keys in bucket {bucket}. {coord} asked {} vnodes on {} nodes. Their folds read {} keys in all, in every bucket, to find them. A read of one key asks 3 vnodes.",
                    r.keys.len(),
                    r.preflist.len(),
                    nodes.len(),
                    r.keys_scanned
                )
            }
            Op::ListKeys => format!("Riak could not list the keys: {}.", r.error),
        }
    }
}

/// The text with its first letter in capitals, for a name at the start of a sentence.
fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// "a, b and c".
fn list(items: Vec<String>) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// How a step names an endpoint.
fn name(e: Endpoint) -> String {
    match e {
        Endpoint::Client => "the client".into(),
        Endpoint::Balancer => "the load balancer".into(),
        Endpoint::Node(n) => format!("node{}", n + 1),
        Endpoint::Vnode(p) => format!("vnode {p}"),
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        300 => "Multiple Choices",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Object Not Found",
        405 => "Method Not Allowed",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

fn progress_at(m: &Message, now: Time) -> f64 {
    match m.deliver {
        Some(d) if d > m.start => (m.progress + (1.0 - m.progress) * (now - m.start) as f64 / (d - m.start) as f64).min(1.0),
        Some(_) => 1.0,
        None => m.progress,
    }
}

impl RequestState {
    /// How steps name a change: write, delete, or a data type update.
    fn what(&self) -> &'static str {
        match self.op {
            Op::Delete => "delete",
            _ if self.datatype.is_some() => "update",
            _ => "write",
        }
    }
}

/// Parse an HTTP request into the request the cluster will run.
fn parse_request(id: u64, origin: usize, method: &str, path: &str, headers: &[(String, String)], body: Vec<u8>, q: Quorum) -> RequestState {
    let method = method.to_ascii_uppercase();
    let (path_only, query) = path.split_once('?').unwrap_or((path, ""));
    let segs: Vec<String> = path_only.split('/').skip(1).map(percent_decode).collect();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    let mut req = RequestState {
        id,
        op: Op::Invalid,
        method: method.clone(),
        path: path.into(),
        origin,
        via_balancer: false,
        arrived: false,
        coordinator: None,
        bucket: None,
        key: None,
        datatype: None,
        preflist: vec![],
        w: q.w,
        dw: q.dw,
        r: q.r,
        pw: q.pw,
        pr: q.pr,
        num_w: 0,
        num_dw: 0,
        num_r: 0,
        status: None,
        done: false,
        body,
        content_type: headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "application/octet-stream".into()),
        error: String::new(),
        not_found: false,
        found: None,
        heads: vec![],
        handoff_pending: 0,
        crdt_op: None,
        context: None,
        multipart: false,
        vtag: None,
        fetch_pending: 0,
        fetches: 0,
        keys: Default::default(),
        folds_pending: 0,
        keys_scanned: 0,
        remote_sent: false,
        fetched_from: None,
    };
    let (bucket, key) = match segs.as_slice() {
        ["ping"] if method == "GET" => {
            req.op = Op::Ping;
            return req;
        }
        ["buckets", b, "keys"] | ["types", _, "buckets", b, "keys"] if method == "GET" => {
            let t = if segs.len() == 5 { Some(segs[1]) } else { None };
            let flag = query.split('&').find_map(|p| p.strip_prefix("keys="));
            if !matches!(flag, Some("true") | Some("stream")) {
                req.error = "List keys needs ?keys=true or ?keys=stream.".into();
                return req;
            }
            req.op = Op::ListKeys;
            req.bucket = Some(BucketRef::new(t, b));
            req.content_type = "application/json".into();
            return req;
        }
        ["buckets", b, "keys", k] => (BucketRef::new(None, b), *k),
        ["types", t, "buckets", b, "keys", k] => (BucketRef::new(Some(t), b), *k),
        ["types", t, "buckets", b, "datatypes", k] => {
            let Some(dt) = DataType::of_bucket_type(t) else {
                req.error = format!("Unknown bucket type: {t}. The simulator has the bucket types counters and sets.");
                req.not_found = true;
                return req;
            };
            if b.is_empty() || k.is_empty() {
                req.error = "The bucket and the key must not be empty.".into();
                return req;
            }
            req.op = match method.as_str() {
                "POST" => match crdt::parse_op(dt, &String::from_utf8_lossy(&req.body)) {
                    Ok(op) => {
                        req.crdt_op = Some(op);
                        Op::Put
                    }
                    Err(e) => {
                        req.error = e;
                        return req;
                    }
                },
                "GET" => Op::Get,
                _ => {
                    req.error = format!("The method {method} is not allowed on a data type. Use GET to read it and POST to update it.");
                    return req;
                }
            };
            req.datatype = Some(dt);
            req.bucket = Some(BucketRef::new(Some(t), b));
            req.key = Some((*k).to_owned());
            req.content_type = "application/json".into();
            return parse_quorum(req, query);
        }
        _ => {
            req.error = format!(
                "The simulator does not support {method} {path_only} yet. Supported: GET /ping, and GET, PUT, POST and DELETE on /buckets/<bucket>/keys/<key>."
            );
            return req;
        }
    };
    if bucket.bucket.is_empty() || key.is_empty() {
        req.error = "The bucket and the key must not be empty.".into();
        return req;
    }
    req.op = match method.as_str() {
        "PUT" | "POST" => Op::Put,
        "GET" => Op::Get,
        "DELETE" => Op::Delete,
        _ => {
            req.error = format!("The method {method} is not allowed on a key.");
            return req;
        }
    };
    let mut req = parse_quorum(req, query);
    if req.op == Op::Invalid {
        return req;
    }
    let header = |name: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
    if let Some(v) = header("x-riak-vclock") {
        match sibs::decode(&v) {
            Some(c) => req.context = Some(c),
            None => {
                req.op = Op::Invalid;
                req.error = "The X-Riak-Vclock header is not a vclock that this cluster sent.".into();
                return req;
            }
        }
    }
    req.multipart = header("accept").is_some_and(|a| a.contains("multipart/mixed"));
    req.vtag = query.split('&').find_map(|p| p.strip_prefix("vtag=")).map(str::to_owned);
    req.bucket = Some(bucket);
    req.key = Some(key.to_owned());
    req
}

/// The w, dw and r query parameters.
fn parse_quorum(mut req: RequestState, query: &str) -> RequestState {
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let value = match v {
            "one" => Some(1),
            "quorum" | "default" => Some(N_VAL / 2 + 1),
            "all" => Some(N_VAL),
            n => n.parse::<usize>().ok(),
        };
        // pw and pr may be 0 (the default): no copy has to be primary.
        let low = if k == "pw" || k == "pr" { 0 } else { 1 };
        let slot = match k {
            "w" => &mut req.w,
            "dw" => &mut req.dw,
            "r" => &mut req.r,
            "pw" => &mut req.pw,
            "pr" => &mut req.pr,
            _ => continue,
        };
        match value {
            Some(v) if (low..=N_VAL).contains(&v) => *slot = v,
            _ => {
                req.op = Op::Invalid;
                req.error = format!("{k} must be one, quorum, all or 1 to {N_VAL}, not \"{v}\".");
                return req;
            }
        }
    }
    req
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
