//! The request paths, checked through the event trace.

use riak_sim_core::ring::BucketRef;
use riak_sim_core::sim::{Payload, Sim};
use riak_sim_core::key_info;

fn idle(sim: &mut Sim) {
    sim.run_until_idle(100_000);
    assert!(sim.next_time().is_none(), "simulation did not go idle");
}

fn text(sim: &mut Sim) -> Vec<String> {
    sim.take_events().into_iter().map(|e| e.text).collect()
}

/// A node that owns no replica of users/alice in a 5-node, 32-partition ring.
fn outsider(sim: &Sim) -> usize {
    let info = key_info(sim.ring(), &BucketRef::new(None, "users"), "alice", 3);
    (0..sim.nodes()).find(|n| info.preflist.iter().all(|r| r.node != *n)).expect("a node without a replica")
}

fn put(sim: &mut Sim, node: usize, query: &str) -> u64 {
    sim.submit(
        node,
        "PUT",
        &format!("/buckets/users/keys/alice{query}"),
        vec![("Content-Type".into(), "text/plain".into())],
        b"Alice".to_vec(),
    )
}

#[test]
fn put_is_forwarded_and_answered_then_read_back() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let node = outsider(&sim);
    let id = put(&mut sim, node, "");
    idle(&mut sim);
    let log = text(&mut sim);
    assert!(log.iter().any(|t| t.contains("It sends the write to node")), "{log:#?}");
    assert!(log.iter().any(|t| t.contains("gets now_executing")), "{log:#?}");
    let responses = sim.take_responses();
    assert_eq!(responses.len(), 1);
    assert_eq!((responses[0].req, responses[0].status), (id, 204));

    // Every replica stored it.
    let stored: usize = sim.snapshot().vnodes.iter().map(|v| v.keys).sum();
    assert_eq!(stored, 3);

    let get = sim.submit((node + 1) % 5, "GET", "/buckets/users/keys/alice", vec![], vec![]);
    idle(&mut sim);
    let r = sim.take_responses();
    assert_eq!((r[0].req, r[0].status, r[0].body.as_str()), (get, 200, "Alice"));
    assert!(text(&mut sim).iter().any(|t| t.contains("sends HEAD to")));
}

#[test]
fn missing_key_is_404() {
    let mut sim = Sim::new(3, 8, 1).unwrap();
    sim.submit(0, "GET", "/buckets/b/keys/nothing", vec![], vec![]);
    idle(&mut sim);
    assert_eq!(sim.take_responses()[0].status, 404);
}

/// Run until a `put` to another replica is in flight, then hold it.
fn hold_one_remote_put(sim: &mut Sim) -> u64 {
    loop {
        let found = sim.snapshot().messages.iter().find(|m| matches!(m.payload, Payload::Put)).map(|m| m.id);
        if let Some(id) = found {
            assert!(sim.hold(id));
            return id;
        }
        assert!(sim.step(), "no remote put was sent");
    }
}

#[test]
fn reply_needs_w_acks() {
    // W = 2: one replica held back, the other two are enough.
    let mut sim = Sim::new(5, 32, 1).unwrap();
    put(&mut sim, 0, "");
    hold_one_remote_put(&mut sim);
    idle(&mut sim);
    assert_eq!(sim.take_responses()[0].status, 204);

    // W = 3: the held replica blocks the reply until it is released.
    let mut sim = Sim::new(5, 32, 1).unwrap();
    put(&mut sim, 0, "?w=3&dw=3");
    let held = hold_one_remote_put(&mut sim);
    idle(&mut sim);
    assert!(sim.take_responses().is_empty());
    assert!(sim.release(held));
    idle(&mut sim);
    assert_eq!(sim.take_responses()[0].status, 204);
}

#[test]
fn slowing_a_message_keeps_its_position() {
    let mut sim = Sim::new(3, 8, 1).unwrap();
    sim.submit(0, "GET", "/ping", vec![], vec![]);
    let id = sim.snapshot().messages[0].id;
    sim.advance_to(250, 100); // half way across a 0.5 ms hop
    assert!(sim.slow(id, 4));
    let m = sim.snapshot().messages[0].clone();
    assert!((m.progress - 0.5).abs() < 1e-9);
    assert_eq!(m.deliver, Some(250 + 250 * 4));
}

#[test]
fn same_inputs_same_trace() {
    let run = || {
        let mut sim = Sim::new(5, 32, 7).unwrap();
        put(&mut sim, 2, "");
        sim.advance_to(1_500, 1000);
        sim.submit(4, "GET", "/buckets/users/keys/alice", vec![], vec![]);
        idle(&mut sim);
        sim.take_events().into_iter().map(|e| (e.at, e.text)).collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn realistic_timing_makes_local_messages_faster() {
    use riak_sim_core::sim::{Endpoint, Timing};
    let mut sim = Sim::new(5, 32, 1).unwrap();
    sim.set_timing(Timing::REALISTIC);
    put(&mut sim, 0, "");
    // Run until node1 has sent the write to its own vnode.
    while !sim.snapshot().messages.iter().any(|m| matches!(m.payload, Payload::CoordPut)) {
        assert!(sim.step());
    }
    let m = sim.snapshot().messages.iter().find(|m| matches!(m.payload, Payload::CoordPut)).map(|m| (*m).clone()).unwrap();
    assert!(matches!(m.to, Endpoint::Vnode(_)));
    assert_eq!(m.deliver.unwrap() - m.start, Timing::REALISTIC.local);
}

#[test]
fn balancer_sends_to_nodes_in_turn_and_returns_the_reply() {
    let mut sim = Sim::new(3, 8, 1).unwrap();
    let a = sim.submit_to(None, "GET", "/ping", vec![], vec![]);
    let b = sim.submit_to(None, "GET", "/ping", vec![], vec![]);
    sim.run_until_idle(10_000);
    let log: Vec<String> = sim.take_events().into_iter().map(|e| e.text).collect();
    let first = log.iter().find_map(|t| t.split("It sends the request to node").nth(1)).expect("a balancer event");
    let n: usize = first[..1].parse().unwrap();
    let next = format!("It sends the request to node{}", n % 3 + 1);
    assert!(log.iter().any(|t| t.contains(&next)), "{log:#?}");
    let r = sim.take_responses();
    assert_eq!(r.iter().map(|r| (r.req, r.status)).collect::<Vec<_>>(), vec![(a, 200), (b, 200)]);
}

#[test]
fn rejects_bad_quorum_values() {
    let mut sim = Sim::new(3, 8, 1).unwrap();
    put(&mut sim, 0, "?w=4");
    idle(&mut sim);
    let r = sim.take_responses();
    assert_eq!(r[0].status, 400);
    assert!(r[0].body.contains("w must be"));
}

/// riak_core_apl:get_apl_ann/3 worked by hand: the usual vnodes on running
/// nodes, then a fallback vnode for each partition on a stopped node, hosted
/// by the first running owner after the first N partitions in ring order.
#[test]
fn stopped_node_uses_a_fallback_vnode_and_the_write_succeeds() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let info = key_info(sim.ring(), &BucketRef::new(None, "users"), "alice", 32);
    let order: Vec<(usize, usize)> = info.preflist.iter().map(|r| (r.partition, r.node)).collect();
    let (stopped_p, stopped) = order[2];
    // The fallback host: the first owner after the first 3 that is running.
    let host = order[3..].iter().map(|&(_, n)| n).find(|&n| n != stopped).unwrap();
    assert!(sim.stop_node(stopped));
    let coordinator = order[0].1;
    let id = put(&mut sim, coordinator, "");
    idle(&mut sim);
    let snap = sim.snapshot();
    let fallback = snap.vnodes.iter().find(|v| v.fallback).expect("a fallback vnode");
    assert_eq!((fallback.partition, fallback.owner), (stopped_p, host));
    assert_eq!(fallback.keys, 1);
    drop(snap);
    let log = text(&mut sim);
    assert!(log.iter().any(|t| t.contains(&format!("node{} is stopped", stopped + 1)) && t.contains("fallback vnode")), "{log:#?}");
    let reply = sim.take_responses().into_iter().find(|r| r.req == id).unwrap();
    assert_eq!(reply.status, 204);
}

#[test]
fn load_balancer_skips_a_stopped_node_and_a_direct_request_is_refused() {
    let mut sim = Sim::new(3, 16, 1).unwrap();
    assert!(sim.stop_node(1));
    for _ in 0..3 {
        sim.submit_to(None, "GET", "/ping", vec![], vec![]);
        idle(&mut sim);
    }
    let reqs: Vec<String> = text(&mut sim);
    assert!(reqs.iter().all(|t| !t.contains("sends the request to node2")), "{reqs:#?}");
    assert!(reqs.iter().any(|t| t.contains("node2 is stopped, so the load balancer skips it")), "{reqs:#?}");
    let id = sim.submit(1, "GET", "/ping", vec![], vec![]);
    let r = sim.take_responses().into_iter().find(|r| r.req == id).unwrap();
    assert_eq!(r.status, 0);
    // A node can stop while a request runs; the request still finishes.
    // The last running node cannot stop.
    let id = sim.submit_to(None, "GET", "/ping", vec![], vec![]);
    assert!(sim.stop_node(0));
    idle(&mut sim);
    assert!(sim.take_responses().iter().any(|r| r.req == id && r.status == 200));
    assert!(!sim.stop_node(2));
    assert!(sim.start_node(1) && sim.start_node(0));
}

#[test]
fn starting_a_node_hands_copies_back_from_its_fallback_vnode() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let info = key_info(sim.ring(), &BucketRef::new(None, "users"), "alice", 3);
    let (stopped_p, stopped) = (info.preflist[2].partition, info.preflist[2].node);
    assert!(sim.stop_node(stopped));
    put(&mut sim, info.preflist[0].node, "");
    idle(&mut sim);
    text(&mut sim);
    assert!(sim.start_node(stopped));
    let handoff = sim.last_handoff().expect("a handoff");
    idle(&mut sim);
    let snap = sim.snapshot();
    assert!(snap.vnodes.iter().all(|v| !v.fallback), "fallback vnode still there");
    let usual = snap.vnodes.iter().find(|v| v.id == stopped_p).unwrap();
    assert_eq!(usual.keys, 1);
    drop(snap);
    let events = sim.take_events();
    assert!(events.iter().any(|e| e.req == handoff && e.summary && e.text.contains("Hinted handoff is complete")), "{events:#?}");
    // The copy is the new one: a read through the usual vnodes finds it.
    sim.submit(stopped, "GET", "/buckets/users/keys/alice", vec![], vec![]);
    idle(&mut sim);
    let r = sim.take_responses().pop().unwrap();
    assert_eq!((r.status, r.body.as_str()), (200, "Alice"));
}

fn put_value(sim: &mut Sim, node: Option<usize>, key: &str, value: &str) -> u64 {
    sim.submit_to(node, "PUT", &format!("/buckets/users/keys/{key}"), vec![("Content-Type".into(), "text/plain".into())], value.as_bytes().to_vec())
}

fn read(sim: &mut Sim, node: usize, key: &str) -> (u16, String) {
    let id = sim.submit(node, "GET", &format!("/buckets/users/keys/{key}"), vec![], vec![]);
    idle(sim);
    let r = sim.take_responses().into_iter().find(|r| r.req == id).unwrap();
    (r.status, r.body)
}

/// A fallback vnode does not hand off while its own node is stopped. It
/// hands off when both nodes run.
#[test]
fn handoff_waits_for_the_fallback_host() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let info = key_info(sim.ring(), &BucketRef::new(None, "users"), "alice", 32);
    let stopped = info.preflist[2].node;
    let host = info.preflist[3..].iter().map(|r| r.node).find(|&n| n != stopped).unwrap();
    assert!(sim.stop_node(stopped));
    put_value(&mut sim, Some(info.preflist[0].node), "alice", "new");
    idle(&mut sim);
    assert!(sim.stop_node(host));
    assert!(sim.start_node(stopped));
    assert_eq!(sim.last_handoff(), None, "handoff from a stopped host");
    idle(&mut sim);
    assert!(sim.snapshot().vnodes.iter().any(|v| v.fallback && v.keys == 1));
    assert!(sim.start_node(host));
    assert!(sim.last_handoff().is_some());
    idle(&mut sim);
    assert!(sim.snapshot().vnodes.iter().all(|v| !v.fallback));
    assert_eq!(read(&mut sim, stopped, "alice"), (200, "new".into()));
}

/// A write that reaches a fallback vnode during its handoff is not lost: it
/// stays, and a second handoff takes it back.
#[test]
fn a_write_during_handoff_is_handed_off_later() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let info = key_info(sim.ring(), &BucketRef::new(None, "users"), "alice", 3);
    let stopped = info.preflist[2].node;
    let coord = info.preflist[0].node;
    assert!(sim.stop_node(stopped));
    put_value(&mut sim, Some(coord), "alice", "one");
    idle(&mut sim);
    // A second write: start the node while its put to the fallback is in flight.
    put_value(&mut sim, Some(coord), "alice", "two");
    loop {
        let to_fallback = sim.snapshot().messages.iter().any(|m| matches!(m.payload, Payload::Put) && matches!(m.to, riak_sim_core::sim::Endpoint::Vnode(id) if id >= 32));
        if to_fallback {
            break;
        }
        assert!(sim.step());
    }
    assert!(sim.start_node(stopped));
    idle(&mut sim);
    assert!(sim.snapshot().vnodes.iter().all(|v| !v.fallback), "a copy was left on the fallback vnode");
    assert_eq!(read(&mut sim, stopped, "alice"), (200, "two".into()));
}

/// Many requests while nodes stop and start: everything finishes, finished
/// requests are forgotten, and every copy ends on its usual vnode.
#[test]
fn traffic_with_nodes_stopping_and_starting() {
    let mut sim = Sim::new(5, 32, 7).unwrap();
    let mut rng = 12345u64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut sent = 0;
    for round in 0..400 {
        let key = format!("k{}", next() % 20);
        if next() % 3 == 0 {
            put_value(&mut sim, None, &key, &format!("v{round}"));
        } else {
            sim.submit_to(None, "GET", &format!("/buckets/users/keys/{key}"), vec![], vec![]);
        }
        sent += 1;
        if round % 37 == 0 {
            let n = (next() % 5) as usize;
            if sim.is_down(n) {
                sim.start_node(n);
            } else {
                sim.stop_node(n);
            }
        }
        for _ in 0..(next() % 12) {
            sim.step();
        }
    }
    for n in 0..5 {
        sim.start_node(n);
    }
    idle(&mut sim);
    let replies = sim.take_responses();
    assert_eq!(replies.len(), sent, "every request got a reply");
    let snap = sim.snapshot();
    assert!(snap.requests.is_empty(), "{} requests left", snap.requests.len());
    assert!(snap.vnodes.iter().all(|v| !v.fallback), "copies left on fallback vnodes");
}

fn post(sim: &mut Sim, node: Option<usize>, path: &str, body: &str) -> u64 {
    sim.submit_to(node, "POST", path, vec![("Content-Type".into(), "application/json".into())], body.as_bytes().to_vec())
}

fn get_body(sim: &mut Sim, path: &str) -> (u16, String) {
    let id = sim.submit_to(None, "GET", path, vec![], vec![]);
    idle(sim);
    let r = sim.take_responses().into_iter().find(|r| r.req == id).unwrap();
    (r.status, r.body)
}

/// Increments made while a node is stopped land on a fallback vnode. After
/// handoff the counter has every increment: copies merge, none is lost.
#[test]
fn counter_keeps_every_increment_through_a_stopped_node() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let path = "/types/counters/buckets/stats/datatypes/visits";
    assert_eq!(get_body(&mut sim, path), (404, r#"{"type":"counter","error":"notfound"}"#.into()));
    let info = key_info(sim.ring(), &BucketRef::new(Some("counters"), "stats"), "visits", 3);
    for _ in 0..2 {
        post(&mut sim, None, path, r#"{"increment": 1}"#);
        idle(&mut sim);
    }
    assert!(sim.stop_node(info.preflist[1].node));
    for _ in 0..3 {
        post(&mut sim, None, path, r#"{"increment": 2}"#);
        idle(&mut sim);
    }
    post(&mut sim, None, path, r#"{"decrement": 1}"#);
    idle(&mut sim);
    assert!(sim.start_node(info.preflist[1].node));
    idle(&mut sim);
    assert!(sim.snapshot().vnodes.iter().all(|v| !v.fallback));
    let replies = sim.take_responses();
    assert!(replies.iter().all(|r| r.status == 204), "{replies:?}");
    assert_eq!(get_body(&mut sim, path), (200, r#"{"type":"counter","value":7}"#.into()));
    // Each usual vnode now has the whole count: read with r=3.
    assert_eq!(get_body(&mut sim, &format!("{path}?r=3")), (200, r#"{"type":"counter","value":7}"#.into()));
}

#[test]
fn set_add_remove_and_errors() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let path = "/types/sets/buckets/fruit/datatypes/basket";
    post(&mut sim, None, path, r#"{"add_all": ["apple", "pear"]}"#);
    idle(&mut sim);
    post(&mut sim, None, path, r#"{"remove": "apple"}"#);
    idle(&mut sim);
    let (status, body) = get_body(&mut sim, path);
    assert_eq!(status, 200);
    assert!(body.starts_with(r#"{"type":"set","value":["pear"],"context":""#), "{body}");
    // A wrong operation, an unknown bucket type and a plain PUT.
    let bad = post(&mut sim, None, path, r#"{"increment": 1}"#);
    let unknown = post(&mut sim, None, "/types/maps/buckets/b/datatypes/k", r#"{"add": "x"}"#);
    let put = sim.submit_to(None, "PUT", path, vec![], b"{}".to_vec());
    idle(&mut sim);
    let replies = sim.take_responses();
    let status = |id| replies.iter().find(|r| r.req == id).unwrap().status;
    assert_eq!((status(bad), status(unknown), status(put)), (400, 404, 400));
}

/// With one node running, a write still succeeds: every copy goes to a
/// fallback vnode on that node (sloppy quorum). pw refuses it.
#[test]
fn one_running_node_takes_writes_unless_pw_asks_for_primaries() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    for n in 1..5 {
        assert!(sim.stop_node(n));
    }
    let ok = put_value(&mut sim, None, "alice", "only node1");
    idle(&mut sim);
    let pw = sim.submit_to(None, "PUT", "/buckets/users/keys/alice?pw=2", vec![], b"x".to_vec());
    idle(&mut sim);
    let replies = sim.take_responses();
    let get = |id| replies.iter().find(|r| r.req == id).unwrap();
    assert_eq!(get(ok).status, 204);
    assert_eq!((get(pw).status, get(pw).body.as_str()), (503, "PW-value unsatisfied: 1/2\n"));
    let events = sim.take_events();
    assert!(events.iter().any(|e| e.summary && e.text.contains("All 3 copies are on node1")), "{events:#?}");
}

fn put_typed(sim: &mut Sim, value: &str, vclock: Option<&str>) -> u64 {
    let mut headers = vec![("Content-Type".to_string(), "text/plain".to_string())];
    if let Some(v) = vclock {
        headers.push(("X-Riak-Vclock".into(), v.into()));
    }
    sim.submit_to(None, "PUT", "/types/carts/buckets/carts/keys/alice", headers, value.as_bytes().to_vec())
}

fn get_full(sim: &mut Sim, path: &str, accept: Option<&str>) -> riak_sim_core::sim::HttpResponse {
    let headers = accept.map(|a| vec![("Accept".to_string(), a.to_string())]).unwrap_or_default();
    let id = sim.submit_to(None, "GET", path, headers, vec![]);
    idle(sim);
    sim.take_responses().into_iter().find(|r| r.req == id).unwrap()
}

/// In a bucket with allow_mult, two writes with no vclock become siblings;
/// a write with the vclock from the read replaces them.
#[test]
fn siblings_are_created_by_blind_writes_and_resolved_with_the_vclock() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let path = "/types/carts/buckets/carts/keys/alice";
    put_typed(&mut sim, "milk", None);
    idle(&mut sim);
    put_typed(&mut sim, "bread", None);
    idle(&mut sim);
    let r = get_full(&mut sim, path, None);
    assert_eq!(r.status, 300, "{}", r.body);
    assert!(r.body.starts_with("Siblings:\n") && r.body.lines().count() == 3, "{}", r.body);
    let vclock = r.headers.iter().find(|(k, _)| k == "X-Riak-Vclock").unwrap().1.clone();
    // Each sibling by its vtag, and all in one multipart body.
    let vtag = r.body.lines().nth(1).unwrap().to_string();
    let one = get_full(&mut sim, &format!("{path}?vtag={vtag}"), None);
    assert_eq!(one.status, 200);
    assert!(one.body == "milk" || one.body == "bread");
    let multi = get_full(&mut sim, path, Some("multipart/mixed"));
    assert!(multi.body.contains("milk") && multi.body.contains("bread"), "{}", multi.body);
    // Resolve.
    put_typed(&mut sim, "bread, milk", Some(&vclock));
    idle(&mut sim);
    let r = get_full(&mut sim, &format!("{path}?r=3"), None);
    assert_eq!((r.status, r.body.as_str()), (200, "bread, milk"));
    // The default bucket type does not keep siblings.
    sim.submit_to(None, "PUT", "/buckets/plain/keys/k", vec![], b"a".to_vec());
    idle(&mut sim);
    sim.submit_to(None, "PUT", "/buckets/plain/keys/k", vec![], b"b".to_vec());
    idle(&mut sim);
    assert_eq!(get_full(&mut sim, "/buckets/plain/keys/k", None).status, 200);
    // A vclock that this cluster did not send is refused.
    assert_eq!(get_status_of_bad_vclock(&mut sim), 400);
}

fn get_status_of_bad_vclock(sim: &mut Sim) -> u16 {
    let id = put_typed(sim, "x", Some("bm90IGEgdmNsb2Nr"));
    idle(sim);
    sim.take_responses().into_iter().find(|r| r.req == id).unwrap().status
}

/// Writes taken by different copies while a node is stopped become siblings
/// when the copies merge, after the node returns.
#[test]
fn copies_that_took_different_writes_merge_into_siblings() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let path = "/types/carts/buckets/carts/keys/alice";
    put_typed(&mut sim, "milk", None);
    idle(&mut sim);
    let r = get_full(&mut sim, path, None);
    let vclock = r.headers.iter().find(|(k, _)| k == "X-Riak-Vclock").unwrap().1.clone();
    // Two clients both read "milk" and each write their own change.
    put_typed(&mut sim, "milk, eggs", Some(&vclock));
    put_typed(&mut sim, "milk, jam", Some(&vclock));
    idle(&mut sim);
    let r = get_full(&mut sim, &format!("{path}?r=3"), Some("multipart/mixed"));
    assert_eq!(r.status, 300);
    assert!(r.body.contains("milk, eggs") && r.body.contains("milk, jam") && !r.body.contains("\r\n\r\nmilk\r\n"), "{}", r.body);
}

fn list_keys(sim: &mut Sim, bucket: &str) -> (u16, Vec<String>) {
    let id = sim.submit_to(None, "GET", &format!("/buckets/{bucket}/keys?keys=true"), vec![], vec![]);
    idle(sim);
    let r = sim.take_responses().into_iter().find(|r| r.req == id).unwrap();
    let mut keys: Vec<String> = r.body.trim_start_matches("{\"keys\":[").trim_end_matches("]}").split(',').filter(|s| !s.is_empty()).map(|s| s.trim_matches('"').to_string()).collect();
    keys.sort();
    (r.status, keys)
}

/// List keys asks a coverage plan of vnodes and returns every key once.
#[test]
fn list_keys_returns_each_key_once_from_a_coverage_plan() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    let mut want: Vec<String> = (0..60).map(|i| format!("k{i:02}")).collect();
    for k in &want {
        sim.submit_to(None, "PUT", &format!("/buckets/b/keys/{k}"), vec![], b"v".to_vec());
    }
    for i in 0..20 {
        sim.submit_to(None, "PUT", &format!("/buckets/other/keys/o{i}"), vec![], b"v".to_vec());
    }
    sim.submit_to(None, "DELETE", "/buckets/b/keys/k00", vec![], vec![]);
    idle(&mut sim);
    want.remove(0);
    sim.take_events();
    assert_eq!(list_keys(&mut sim, "b"), (200, want.clone()));
    let events = sim.take_events();
    let summary = events.iter().find(|e| e.summary).unwrap();
    assert!(summary.text.contains("asked 11 vnodes on 5 nodes"), "{}", summary.text);
    // With a node stopped, other vnodes cover its ranges.
    assert!(sim.stop_node(2));
    assert_eq!(list_keys(&mut sim, "b"), (200, want));
    // Without ?keys=true it is refused.
    let id = sim.submit_to(None, "GET", "/buckets/b/keys", vec![], vec![]);
    idle(&mut sim);
    assert_eq!(sim.take_responses().into_iter().find(|r| r.req == id).unwrap().status, 400);
}

#[test]
fn list_keys_fails_when_too_many_nodes_are_stopped() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    for n in [1, 2, 3] {
        assert!(sim.stop_node(n));
    }
    assert_eq!(list_keys(&mut sim, "b").0, 503);
}

/// A request sent after a delay leaves the client then, not before.
#[test]
fn submit_after_sends_the_request_later() {
    let mut sim = Sim::new(3, 16, 1).unwrap();
    let id = sim.submit_after(5_000, None, "GET", "/ping", vec![], vec![]);
    assert!(sim.snapshot().messages.is_empty(), "nothing leaves before the delay");
    sim.advance_to(4_999, 1_000);
    assert!(sim.snapshot().messages.is_empty());
    sim.advance_to(5_000, 1_000);
    assert_eq!(sim.snapshot().messages.len(), 1);
    idle(&mut sim);
    assert!(sim.take_responses().iter().any(|r| r.req == id && r.status == 200));
}

#[test]
fn each_vnode_keeps_a_leveled_store() {
    let mut sim = Sim::new(5, 32, 1).unwrap();
    for i in 0..1000 {
        sim.submit_to(None, "PUT", &format!("/buckets/data/keys/k{i}"), vec![("Content-Type".into(), "text/plain".into())], b"v".to_vec());
    }
    sim.run_until_idle(5_000_000);
    // 3000 copies over 32 partitions: about 94 keys a vnode, so the
    // Penciller has written and merged files.
    let views: Vec<_> = (0..32).map(|p| sim.leveled(p).unwrap()).collect();
    let keys: u64 = views.iter().map(|v| v.seq).sum();
    assert_eq!(keys, 3000);
    assert!(views.iter().all(|v| v.merges > 0 && !v.levels[1].is_empty()));
    // A read is recorded as the Bookie's lookup.
    sim.submit_to(None, "GET", "/buckets/data/keys/k1", vec![], vec![]);
    sim.run_until_idle(5_000_000);
    let looked: usize = (0..32).map(|p| sim.leveled(p).unwrap().events.iter().filter(|e| e.kind == "head" || e.kind == "get").count()).sum();
    assert!(looked >= 3, "{looked} lookups");
}
