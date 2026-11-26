// Where each endpoint is drawn, and the path a message takes between two
// endpoints. The ring view is one layout; a node view will be a second
// geometry with the same functions.
import type { Endpoint, RingInfo } from "./sim.ts";

export interface Geometry {
  viewBox: string;
  rIn: number;
  rOut: number;
  nodeY: number;
  nodeW: number;
  nodeH: number;
  nodeX: (n: number) => number;
  client: [number, number];
  balancer: [number, number];
  clientW: number;
  balancerW: number;
}

const TAU = Math.PI * 2;

export function ringGeometry(ring: RingInfo): Geometry {
  const left = -295;
  const right = 295;
  const span = (right - left) / Math.max(ring.nodes, 1);
  return {
    viewBox: "-300 -275 600 775",
    rIn: 190,
    rOut: 250,
    nodeY: 325,
    nodeW: Math.min(130, span - 12),
    nodeH: 64,
    nodeX: (n) => left + span * (n + 0.5),
    client: [-180, 465],
    balancer: [70, 465],
    clientW: 120,
    balancerW: 200,
  };
}

/** Start of partition p's arc. Partition p owns the hash range that ends at its index. */
export function arcStart(p: number, size: number): number {
  return (((p - 1 + size) % size) / size) * TAU;
}

export function pointAt(angle: number, r: number): [number, number] {
  return [r * Math.sin(angle), -r * Math.cos(angle)];
}

export function endpointPos(e: Endpoint, ring: RingInfo, g: Geometry): [number, number] {
  switch (e.kind) {
    case "client":
      return g.client;
    case "balancer":
      return g.balancer;
    case "node":
      return [g.nodeX(e.id), g.nodeY];
    case "vnode":
      return pointAt(arcStart(e.id, ring.size) + Math.PI / ring.size, (g.rIn + g.rOut) / 2);
  }
}

export interface Route {
  d: string;
  at: (t: number) => [number, number];
}

type Pt = [number, number];

const straight = (a: Pt, b: Pt): Route => ({
  d: `M${a[0]},${a[1]}L${b[0]},${b[1]}`,
  at: (t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
});

/** Curve between two nodes, below the node row, so it does not pass through
 *  other nodes. */
function between(from: number, to: number, g: Geometry): Route {
  const x1 = g.nodeX(from);
  const x2 = g.nodeX(to);
  const y = g.nodeY + g.nodeH / 2;
  const cx = (x1 + x2) / 2;
  const cy = y + 40 + Math.abs(x2 - x1) * 0.12;
  return {
    d: `M${x1},${y}Q${cx},${cy} ${x2},${y}`,
    at: (t) => {
      const u = 1 - t;
      return [u * u * x1 + 2 * u * t * cx + t * t * x2, u * u * y + 2 * u * t * cy + t * t * y];
    },
  };
}

function length(r: Route): number {
  let total = 0;
  let prev = r.at(0);
  for (let i = 1; i <= 20; i++) {
    const p = r.at(i / 20);
    total += Math.hypot(p[0] - prev[0], p[1] - prev[1]);
    prev = p;
  }
  return total;
}

/** Two routes one after the other. Progress is shared out by length, so the
 *  message moves at an even speed along the whole path. */
function join(a: Route, b: Route): Route {
  const la = length(a);
  const split = la / (la + length(b));
  return {
    d: `${a.d}${b.d.replace(/^M[^LQ]*/, "L")}`.replace(/LL/, "L"),
    at: (t) => (t < split ? a.at(t / split) : b.at((t - split) / (1 - split))),
  };
}

function reverse(r: Route, from: Pt, to: Pt): Route {
  return { d: r.d, at: (t) => (t <= 0 ? from : t >= 1 ? to : r.at(1 - t)) };
}

/** The path between two endpoints. A message between a node and a vnode on
 *  another node goes through the vnode's own node first, as in Riak: the
 *  message crosses the network to that node, which passes it to the vnode. */
export function route(from: Endpoint, to: Endpoint, ring: RingInfo, g: Geometry): Route {
  const a = endpointPos(from, ring, g);
  const b = endpointPos(to, ring, g);
  if (from.kind === "node" && to.kind === "node") return between(from.id, to.id, g);
  if (from.kind === "node" && to.kind === "vnode" && ring.owners[to.id] !== from.id) {
    const owner = ring.owners[to.id];
    return join(between(from.id, owner, g), straight([g.nodeX(owner), g.nodeY + g.nodeH / 2], b));
  }
  if (from.kind === "vnode" && to.kind === "node" && ring.owners[from.id] !== to.id) {
    const owner = ring.owners[from.id];
    const forward = join(between(to.id, owner, g), straight([g.nodeX(owner), g.nodeY + g.nodeH / 2], a));
    return reverse(forward, a, b);
  }
  return straight(a, b);
}
