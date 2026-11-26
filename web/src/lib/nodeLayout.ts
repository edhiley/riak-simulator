// Node view: the clients on the left, the load balancer in the middle of the
// nodes, and the nodes as cards in two rows. Each node's partitions (vnodes)
// are tiles inside its card. On the card edge that faces the corridor, a
// strip shows the node's ports and the FSMs that coordinate its requests.
import { vnodeOf, type Endpoint, type RingInfo } from "./sim.ts";

type Pt = [number, number];

export interface Card {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface NodeGeometry {
  viewBox: string;
  centre: Pt;
  client: Pt;
  balancer: Pt;
  boxW: number;
  boxH: number;
  tile: { w: number; h: number };
  card: (n: number) => Card;
  nodeCentre: (n: number) => Pt;
  /** The node's strip on the card edge that faces the corridor: the HTTP
   *  listener, the FSMs that coordinate requests, and the handoff listener. */
  strip: (n: number) => Card;
  /** Where the FSMs sit: a node endpoint is the FSM that coordinates. */
  nodePort: (n: number) => Pt;
  /** HTTP listener, port 8098: client requests arrive here. */
  httpPort: (n: number) => Pt;
  /** Handoff listener, port 8099: partitions move over their own connection. */
  handoffPort: (n: number) => Pt;
  /** Erlang distribution: commands from other nodes reach the vnodes here. */
  distPort: (n: number) => Pt;
  /** Width of the port labels at each end of the strip. */
  portW: number;
  /** Which way the corridor is from the node: 1 below it, -1 above it. */
  facing: (n: number) => 1 | -1;
  /** Centre of a vnode's tile, for a usual or a fallback vnode id. */
  tileCentre: (id: number) => Pt;
  /** Free space at the top left, above the clients, for page controls. */
  corner: Card;
}

const TILE_W = 48;
const TILE_H = 36;
/** Live mode: taller tiles, with room for a dot per stored key. */
const TILE_H_TALL = 76;
const GAP = 6;
const PAD = 14;
const HEADER = 50;

/** Fallback vnode ids go after the node's own partitions in its card.
 *  With `fit`, the size of the space the drawing goes in, the cards get the
 *  number of tile columns that makes the drawing largest in that space: wide
 *  and short cards for a wide space, narrow and tall ones for a tall space. */
export function nodeGeometry(ring: RingInfo, fallbacks: number[] = [], tall = false, fit?: { w: number; h: number }): NodeGeometry {
  const counts = Array.from({ length: ring.max_nodes }, (_, n) => ring.owners.filter((o) => o === n).length);
  const perNode = new Array(ring.max_nodes).fill(0);
  for (const id of fallbacks) perNode[vnodeOf(id, ring).node]++;
  const most = Math.max(1, ...ring.members.map((n) => counts[n] + perNode[n]));
  const usual = most <= 8 ? 4 : most <= 15 ? 5 : 8;
  if (!fit || fit.w <= 0 || fit.h <= 0) return layout(ring, fallbacks, tall, usual);
  let best = layout(ring, fallbacks, tall, usual);
  let bestScale = 0;
  // From 4 columns (narrower cards crowd the port strip) up to one row of
  // tiles; more columns than tiles only wastes room.
  for (let cols = Math.min(4, most); cols <= most; cols++) {
    const g = layout(ring, fallbacks, tall, cols);
    const [, , vw, vh] = g.viewBox.split(" ").map(Number);
    const scale = Math.min(fit.w / vw, fit.h / vh);
    // Prefer fewer columns when the gain is under 2%, so the cards do not
    // change shape for a tiny gain.
    if (scale > bestScale * 1.02) {
      best = g;
      bestScale = scale;
    }
  }
  return best;
}

function layout(ring: RingInfo, fallbacks: number[], tall: boolean, cols: number): NodeGeometry {
  const TILE_H_NOW = tall ? TILE_H_TALL : TILE_H;
  // Counts are indexed by node number, which can have gaps after a leave.
  const counts = Array.from({ length: ring.max_nodes }, (_, n) => ring.owners.filter((o) => o === n).length);
  const extra = new Map<number, number>();
  const perNode = new Array(ring.max_nodes).fill(0);
  for (const id of [...fallbacks].sort((a, b) => a - b)) {
    const host = vnodeOf(id, ring).node;
    extra.set(id, counts[host] + perNode[host]++);
  }
  const most = Math.max(1, ...ring.members.map((n) => counts[n] + perNode[n]));
  const rows = Math.ceil(most / cols);
  const w = cols * TILE_W + (cols - 1) * GAP + 2 * PAD;
  const h = HEADER + rows * TILE_H_NOW + (rows - 1) * GAP + PAD;

  // Two even rows with a corridor between them. Nodes go clockwise: left
  // to right along the top row, then right to left along the bottom row.
  // Messages meet a card at the edge that faces the corridor, so a line
  // between two cards never crosses a third.
  const n = ring.members.length;
  // Position of a node in the layout: its place among the members.
  const place = (node: number) => Math.max(0, ring.members.indexOf(node));
  const top = Math.ceil(n / 2);
  const bottom = n - top;
  const GAP_X = 48;
  // The strips stand out into the corridor, so it is wider than before.
  const STRIP_H = 40;
  const CORRIDOR = 130 + 2 * STRIP_H;
  const left = 160;
  const rowWidth = (k: number) => k * w + Math.max(0, k - 1) * GAP_X;
  const inner = rowWidth(top);
  const y0 = 24;
  const y1 = y0 + h + CORRIDOR;
  const cx = left + inner / 2;
  const cy = bottom ? y0 + h + CORRIDOR / 2 : y0 + h + CORRIDOR / 2;
  const width = left + inner + 24;
  const height = (bottom ? y1 + h : y0 + h + CORRIDOR) + 24;

  const inTop = (node: number) => place(node) < top;
  const card = (node: number): Card => {
    const i = place(node);
    if (i < top) return { x: left + i * (w + GAP_X), y: y0, w, h };
    // Bottom row, right to left, centred under the top row.
    const j = i - top;
    const start = left + (inner - rowWidth(bottom)) / 2;
    return { x: start + (bottom - 1 - j) * (w + GAP_X), y: y1, w, h };
  };
  const nodeCentre = (i: number): Pt => {
    const c = card(i);
    return [c.x + w / 2, c.y + h / 2];
  };

  // The strip runs along the corridor edge, outside the card, and leaves room
  // at the right end for the distribution notch. Its text is large because
  // the whole drawing is scaled down to fit.
  const PORT_W = 66;
  const DIST_ROOM = 28;
  const strip = (node: number): Card => {
    const c = card(node);
    const w2 = c.w - DIST_ROOM;
    return { x: c.x, y: inTop(node) ? c.y + c.h : c.y - STRIP_H, w: w2, h: STRIP_H };
  };
  const stripMid = (node: number) => {
    const s = strip(node);
    return s.y + s.h / 2;
  };

  // Each node's partitions in ring order, laid out row by row in its card.
  const slot = new Map<number, number>();
  const seen = new Array(ring.max_nodes).fill(0);
  ring.owners.forEach((owner, p) => slot.set(p, seen[owner]++));
  const tileCentre = (id: number): Pt => {
    const c = card(vnodeOf(id, ring).node);
    const i = id < ring.size ? slot.get(id)! : (extra.get(id) ?? 0);
    const col = i % cols;
    const row = Math.floor(i / cols);
    return [c.x + PAD + col * (TILE_W + GAP) + TILE_W / 2, c.y + HEADER + row * (TILE_H_NOW + GAP) + TILE_H_NOW / 2];
  };

  return {
    viewBox: `0 0 ${Math.round(width)} ${Math.round(height)}`,
    centre: [cx, cy],
    // The clients wait on the left; the load balancer sits in the middle of
    // the nodes, so its line to each node is short and even.
    client: [75, cy],
    balancer: [cx, cy],
    boxW: 120,
    boxH: 56,
    tile: { w: TILE_W, h: TILE_H_NOW },
    card,
    nodeCentre,
    strip,
    portW: PORT_W,
    facing: (i: number) => (inTop(i) ? 1 : -1),
    nodePort: (i: number) => {
      const s = strip(i);
      return [s.x + s.w / 2, stripMid(i)];
    },
    httpPort: (i: number) => [strip(i).x + PORT_W / 2, stripMid(i)],
    handoffPort: (i: number) => {
      const s = strip(i);
      return [s.x + s.w - PORT_W / 2, stripMid(i)];
    },
    distPort: (i: number) => {
      const c = card(i);
      return [c.x + c.w - DIST_ROOM / 2, inTop(i) ? c.y + c.h : c.y];
    },
    tileCentre,
    corner: { x: 8, y: 8, w: left - 16, h: Math.max(0, cy - 60 - 8) },
  };
}

export function endpointPos(e: Endpoint, g: NodeGeometry): Pt {
  switch (e.kind) {
    case "client":
      return g.client;
    case "balancer":
      return g.balancer;
    case "node":
      return g.nodePort(e.id);
    case "vnode":
      return g.tileCentre(e.id);
  }
}

export interface Route {
  d: string;
  at: (t: number) => Pt;
}

/** A path through the given points, travelled at an even speed. */
function polyline(points: Pt[]): Route {
  const lengths = points.slice(1).map((p, i) => Math.hypot(p[0] - points[i][0], p[1] - points[i][1]));
  const total = lengths.reduce((a, b) => a + b, 0) || 1;
  return {
    d: points.map((p, i) => `${i ? "L" : "M"}${p[0]},${p[1]}`).join(""),
    at: (t) => {
      let d = Math.max(0, Math.min(1, t)) * total;
      for (let i = 0; i < lengths.length; i++) {
        if (d <= lengths[i] || i === lengths.length - 1) {
          const f = lengths[i] ? Math.min(1, d / lengths[i]) : 1;
          const [a, b] = [points[i], points[i + 1]];
          return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
        }
        d -= lengths[i];
      }
      return points[points.length - 1];
    },
  };
}

const outside = (e: Endpoint) => e.kind === "client" || e.kind === "balancer";

/** A handoff message, which travels over the handoff connection. */
export const isHandoff = (label: string) => label.startsWith("handoff");

/** The path a message takes, as in Riak:
 *  - A client request arrives at the node's HTTP listener (8098), and the
 *    reply leaves from it.
 *  - An FSM's command to a vnode on another node goes over Erlang
 *    distribution, addressed to that vnode's proxy; it enters the card at
 *    the distribution notch.
 *  - Handoff leaves through the sender's handoff side and arrives at the
 *    receiver's handoff listener (8099), a connection of its own. */
export function route(from: Endpoint, to: Endpoint, ring: RingInfo, g: NodeGeometry, label = ""): Route {
  const a = endpointPos(from, g);
  const b = endpointPos(to, g);
  if (outside(from) && to.kind === "node") return polyline([a, g.httpPort(to.id), b]);
  if (from.kind === "node" && outside(to)) return polyline([a, g.httpPort(from.id), b]);
  // Between two FSMs in the same row: out into the corridor and back, so the
  // line does not run along the strips.
  if (from.kind === "node" && to.kind === "node" && g.facing(from.id) === g.facing(to.id)) {
    const d = 44 * g.facing(from.id);
    return polyline([a, [a[0], a[1] + d], [b[0], b[1] + d], b]);
  }
  if (from.kind === "vnode" && to.kind === "vnode" && isHandoff(label)) {
    const na = vnodeOf(from.id, ring).node;
    const nb = vnodeOf(to.id, ring).node;
    if (na !== nb) return polyline([a, g.handoffPort(na), g.handoffPort(nb), b]);
  }
  if (from.kind === "node" && to.kind === "vnode" && vnodeOf(to.id, ring).node !== from.id) {
    return polyline([a, g.distPort(vnodeOf(to.id, ring).node), b]);
  }
  if (from.kind === "vnode" && to.kind === "node" && vnodeOf(from.id, ring).node !== to.id) {
    return polyline([a, g.distPort(vnodeOf(from.id, ring).node), b]);
  }
  return polyline([a, b]);
}
