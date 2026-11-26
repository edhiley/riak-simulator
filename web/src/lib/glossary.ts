// Short definitions for terms in events and lesson text. Text follows the
// writing rules in DESIGN.md. Patterns are tried in order; the first match
// at a position wins, so longer terms come first.

export const terms: { pattern: RegExp; definition: string }[] = [
  { pattern: /\bnow_executing\b/, definition: "An ACK from a coordinator to the node that forwarded the request to it. It means: I have started the request." },
  { pattern: /\bnotfound_ok\b/, definition: "Bucket property. If true (the default), a not-found reply from a vnode counts towards R." },
  { pattern: /\bCRDTs?\b|\bdata types?\b/, definition: "Conflict-free replicated data type. A value, such as a counter or a set, whose copies Riak can always merge without losing an update. So it never has siblings." },
  { pattern: /\bCAP theorem\b/, definition: "A rule about distributed databases. When the network splits, a database cannot give both consistency and availability. It must choose one." },
  { pattern: /\bfallback vnodes?\b/, definition: "A vnode that keeps a copy for a while, when the usual vnode for that copy cannot be reached. It sends the copy back later." },
  { pattern: /\bhinted handoff\b/, definition: "Riak sends copies from fallback vnodes back to their usual vnodes, when those vnodes can be reached again." },
  { pattern: /\beventual(?:ly)? consisten(?:cy|t)\b/, definition: "All copies become the same after a time, when no new writes arrive. A read can get an older version before then." },
  { pattern: /\bnetwork split\b|\bnetwork splits\b/, definition: "A network fault that stops some nodes from sending messages to other nodes. Each group of nodes can still get requests." },
  { pattern: /\ballow_mult\b/, definition: "Bucket property. If true, Riak keeps conflicting versions as siblings. If false, Riak keeps only one version." },
  { pattern: /\bpr\b|\bpw\b/, definition: "Primary read and primary write. The number of copies in their usual vnodes, not fallback vnodes, that a request must reach. The default is 0." },
  { pattern: /\bring_size\b/, definition: "The number of partitions in the ring. It must be a power of 2. The default is 64." },
  { pattern: /\bn_val\b/, definition: "Bucket property. The number of copies of each object. The default is 3." },
  { pattern: /\bdurable-write acknowledgements?\b/, definition: "Short form: dw ACK. A vnode sends it when it has saved the copy in its backend. DW is the number of these that the coordinator needs before it replies." },
  { pattern: /\bwrite acknowledgements?\b/, definition: "Short form: w ACK. A vnode sends it when the write arrives, before it saves the copy. W is the number of these that the coordinator needs before it replies." },
  { pattern: /\bdw ACKs?\b|\bdw\b(?= from)|(?<=\()dw(?=\))/, definition: "dw means durable write. A dw ACK tells the coordinator that a vnode has stored the object in its backend." },
  { pattern: /\bw ACKs?\b|\bw\b(?= from)|(?<=\()w(?=\))/, definition: "w means write. A w ACK tells the coordinator that a vnode has accepted the write. The vnode has not stored the object yet." },
  { pattern: /\bDW\b/, definition: "Durable write quorum. The number of durable-write acknowledgements (dw ACKs) the coordinator waits for before it replies to a write. Each one means a saved copy. The default is 2 of 3." },
  { pattern: /\bW\b/, definition: "Write quorum. The number of write acknowledgements (w ACKs) the coordinator waits for before it replies to a write. The default is 2 of 3." },
  { pattern: /\bR\b/, definition: "Read quorum. The number of answers the coordinator waits for before it replies to a read. The default is 2 of 3." },
  { pattern: /\bvclocks?\b/, definition: "Vector clock. A version record that shows which writes an object has seen. Riak uses it to find the newest version and to detect conflicting writes." },
  { pattern: /\bcopy\b|\bcopies\b/, definition: "One saved copy of an object. Riak keeps 3 copies by default, each in a different vnode. The number of copies is n_val." },
  { pattern: /\bvnodes?\b/, definition: "Virtual node. One process that owns one partition and stores the data for it. Each node runs many vnodes." },
  { pattern: /\bcoordinat(?:or|es|ing)\b/, definition: "The coordinator is the node that runs a request. It sends messages to the vnodes that hold the copies, counts their answers and decides the reply." },
  { pattern: /\bpartitions?\b/, definition: "One of the equal parts of the ring. Each partition is owned by one vnode." },
  { pattern: /\breplicas?\b/, definition: "Another word for a copy of an object." },
  { pattern: /\bpreflist\b/, definition: "Preference list. The vnodes that store a key, in ring order." },
  { pattern: /\bHEAD\b/, definition: "A request for an object's vclock and metadata, without the value." },
  { pattern: /\bmailbox(?:es)?\b/, definition: "The queue of messages that waits for a vnode. A vnode processes one message at a time." },
  { pattern: /\bsoft limit\b/, definition: "A mailbox size above which Riak prefers a different vnode to coordinate a write." },
  { pattern: /\bbackend\b/, definition: "The storage engine in each vnode. The simulator uses leveled; select a partition to see inside it." },
  { pattern: /\btombstones?\b/, definition: "A marker that says an object is deleted. Riak saves it like a new version, so a copy that missed the delete cannot bring the object back." },
  { pattern: /\bsiblings\b/, definition: "Two or more versions of an object that Riak keeps because neither vclock includes the other." },
  { pattern: /\bACKs?\b/, definition: "Acknowledgement. A short reply that confirms a step is done." },
  { pattern: /\bhash\b/, definition: "Riak calculates the SHA-1 hash of the bucket and key. The 160-bit result is a position on the ring." },
  { pattern: /\bring\b/, definition: "All possible hash values (0 to 2^160 − 1), arranged in a circle and divided into partitions." },
  { pattern: /\bnode\d+\b|\bnodes?\b/, definition: "A server that runs Riak. Each node owns some of the partitions." },
];

export interface Segment {
  text: string;
  definition?: string;
}

/** Split text into plain parts and defined terms. */
export function markup(text: string): Segment[] {
  const out: Segment[] = [];
  let rest = text;
  while (rest.length) {
    let best: { index: number; length: number; definition: string } | null = null;
    for (const { pattern, definition } of terms) {
      const m = pattern.exec(rest);
      if (m && (best === null || m.index < best.index)) best = { index: m.index, length: m[0].length, definition };
    }
    if (!best) {
      out.push({ text: rest });
      break;
    }
    if (best.index > 0) out.push({ text: rest.slice(0, best.index) });
    out.push({ text: rest.slice(best.index, best.index + best.length), definition: best.definition });
    rest = rest.slice(best.index + best.length);
  }
  return out;
}
