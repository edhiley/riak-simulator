// Typed access to the WebAssembly simulation core. The page draws what the
// core reports and never runs simulation logic itself.
import init, { Sim } from "../wasm/pkg/riak_sim_wasm.js";
import type { ParsedRequest } from "./curl.ts";

export type Endpoint = { kind: "client" } | { kind: "balancer" } | { kind: "node"; id: number } | { kind: "vnode"; id: number };

export interface RingInfo {
  /** How many nodes are in the cluster. */
  nodes: number;
  /** The nodes in the cluster, by number from 0. */
  members: number[];
  /** The most nodes a cluster can have; vnode ids depend on it. */
  max_nodes: number;
  size: number;
  owners: number[];
}

export interface ReplicaInfo {
  partition: number;
  index: string;
  node: number;
}

export interface KeyInfo {
  bucket: { bucket_type: string | null; bucket: string };
  key: string;
  hash: string;
  position: number;
  preflist: ReplicaInfo[];
}

export interface Message {
  id: number;
  req: number;
  from: Endpoint;
  to: Endpoint;
  payload: { type: string };
  label: string;
  start: number;
  progress: number;
  deliver: number | null;
}

export interface MailItem {
  id: number;
  req: number;
  label: string;
}

export interface VnodeView {
  /** Vnode id: the partition for a usual vnode; see vnodeOf for a fallback. */
  id: number;
  partition: number;
  /** The node the vnode runs on. */
  owner: number;
  /** Keeps copies for a stopped node. */
  fallback: boolean;
  /** The new owner's copy of a partition that is moving to this node. */
  moving: boolean;
  /** A key fold (list keys) is running in its worker pool. */
  folding: boolean;
  mailbox: MailItem[];
  busy: MailItem | null;
  keys: number;
  /** Objects stored, not counting deleted ones. */
  objects: { bucket: string; key: string; value: string; siblings: number }[];
}

export interface RequestView {
  id: number;
  op: "put" | "delete" | "get" | "ping" | "invalid" | "handoff";
  method: string;
  path: string;
  origin: number;
  via_balancer: boolean;
  arrived: boolean;
  coordinator: number | null;
  bucket: { bucket_type: string | null; bucket: string } | null;
  key: string | null;
  preflist: number[];
  w: number;
  dw: number;
  r: number;
  num_w: number;
  num_dw: number;
  num_r: number;
  status: number | null;
  done: boolean;
}

export interface Snapshot {
  now: number;
  messages: Message[];
  vnodes: VnodeView[];
  requests: RequestView[];
  idle: boolean;
  /** Stopped nodes. */
  down: number[];
  /** Nodes in the cluster, and those leaving it. */
  members: number[];
  leaving: number[];
  /** Changes when owners or members change: read the ring again. */
  ring_version: number;
}

/** One step of work inside a vnode's leveled store. */
export interface LEvent {
  at: number;
  who: "bookie" | "inker" | "iclerk" | "penciller" | "pclerk";
  kind: "journal" | "cache" | "push" | "l0" | "merge" | "roll" | "compact" | "head" | "get" | "fold";
  text: string;
  /** Why leveled does this step this way. */
  why: string;
  level: number | null;
}

/** A vnode's leveled store. Counts are the real defaults divided by scale. */
export interface LeveledView {
  scale: number;
  seq: number;
  cache: string[];
  cache_size: number;
  pmem: number;
  pmem_size: number;
  sst_keys: number;
  journal_objects: number;
  level_files: number[];
  levels: { id: number; first: string; last: string; keys: number }[][];
  journal: { id: number; objects: number; live: number; active: boolean }[];
  merges: number;
  compactions: number;
  events: LEvent[];
}

export interface Hop {
  req: number;
  from: Endpoint;
  to: Endpoint;
  label: string;
  sent: number;
  arrived: number;
}

export interface SimEvent {
  at: number;
  req: number;
  text: string;
  why: string;
  summary: boolean;
}

export interface HttpResponse {
  req: number;
  status: number;
  reason: string;
  headers: [string, string][];
  body: string;
}

/** Position of a message along its path at simulation time `now`, 0 to 1. */
export function progressAt(m: Message, now: number): number {
  if (m.deliver === null) return m.progress;
  if (m.deliver <= m.start) return 1;
  return Math.min(1, m.progress + ((1 - m.progress) * (now - m.start)) / (m.deliver - m.start));
}

/** Partition and host node of a vnode id. Ids below the ring size are
 *  usual vnodes; a fallback vnode has id ring_size * (1 + host) + partition;
 *  a new owner's copy of a moving partition has id
 *  ring_size * (1 + max_nodes) + ring_size * host + partition. */
export function vnodeOf(id: number, ring: RingInfo): { partition: number; node: number; fallback: boolean; moving: boolean } {
  const movingBase = ring.size * (1 + ring.max_nodes);
  if (id < ring.size) return { partition: id, node: ring.owners[id], fallback: false, moving: false };
  if (id < movingBase) return { partition: id % ring.size, node: Math.floor(id / ring.size) - 1, fallback: true, moving: false };
  return { partition: id % ring.size, node: Math.floor((id - movingBase) / ring.size), fallback: false, moving: true };
}

let ready: Promise<unknown> | undefined;

export class Cluster {
  private constructor(private sim: Sim) {}

  static async create(nodes: number, ringSize: number, seed = 1): Promise<Cluster> {
    ready ??= init();
    await ready;
    return new Cluster(new Sim(nodes, ringSize, seed));
  }

  ring(): RingInfo {
    return JSON.parse(this.sim.ring());
  }

  keyInfo(bucketType: string | null, bucket: string, key: string): KeyInfo {
    return JSON.parse(this.sim.key_info(bucketType ?? "", bucket, key));
  }

  submit(req: ParsedRequest): number {
    return this.sim.submit(req.node ?? -1, req.method, req.path, JSON.stringify(req.headers), new TextEncoder().encode(req.body));
  }

  /** Send a request after `delay` microseconds of simulated time. */
  submitAfter(delay: number, req: ParsedRequest): number {
    return this.sim.submit_after(delay, req.node ?? -1, req.method, req.path, JSON.stringify(req.headers), new TextEncoder().encode(req.body));
  }

  get now(): number {
    return this.sim.now();
  }

  /** Next scheduled time, or null if nothing is scheduled. */
  get nextTime(): number | null {
    const t = this.sim.next_time();
    return t < 0 ? null : t;
  }

  step(): boolean {
    return this.sim.step();
  }

  advanceTo(until: number, maxActions: number): number {
    return this.sim.advance_to(until, maxActions);
  }

  setRealisticTiming(realistic: boolean) {
    this.sim.set_realistic_timing(realistic);
  }

  hold(id: number) {
    this.sim.hold(id);
  }

  release(id: number) {
    this.sim.release(id);
  }

  slow(id: number, factor: number) {
    this.sim.slow(id, factor);
  }

  /** `riak admin cluster join|leave|plan|commit`, run on `node`. */
  cluster(node: number, action: string, target: number | null): { ok: boolean; text: string; id: number | null } {
    return JSON.parse(this.sim.cluster(node, action, target ?? -1));
  }

  /** Set the cluster-wide quorum defaults. Returns an error, or "". */
  setQuorum(r: number, w: number, dw: number, pr: number, pw: number): string {
    return this.sim.set_quorum(r, w, dw, pr, pw);
  }

  /** Stop a node. Only works when no request is running. */
  stopNode(node: number): boolean {
    return this.sim.stop_node(node);
  }

  /** Start a stopped node. Returns the id of the hinted handoff it starts,
   *  0 if there is nothing to hand off, or -1 if it cannot start now. */
  startNode(node: number): number {
    return this.sim.start_node(node);
  }

  snapshot(): Snapshot {
    return JSON.parse(this.sim.snapshot());
  }
  /** The leveled store of one vnode (backend), or null. */
  leveled(vnode: number): LeveledView | null {
    return JSON.parse(this.sim.leveled(vnode));
  }


  takeEvents(): SimEvent[] {
    return JSON.parse(this.sim.take_events());
  }

  /** Messages that arrived since the last call, for drawing after they are gone. */
  takeHops(): Hop[] {
    return JSON.parse(this.sim.take_hops());
  }

  takeResponses(): HttpResponse[] {
    return JSON.parse(this.sim.take_responses());
  }

  free() {
    this.sim.free();
  }
}
