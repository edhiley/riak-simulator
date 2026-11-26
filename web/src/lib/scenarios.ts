// Autoplay scenarios: three workloads that are clearly different. Each sets
// the traffic and may stop and start a node at set times. Rates are requests
// per second of simulated time.

export interface ScenarioEvent {
  /** Simulated milliseconds after the start. */
  at: number;
  action: "stop" | "start";
  /** Node number, from 0. */
  node: number;
}

export interface Scenario {
  id: string;
  title: string;
  /** One line on the workload. */
  workload: string;
  /** What to look for. */
  watch: string;
  /** Requests per simulated second. */
  rate: number;
  /** Share of each kind of request, adding up to 1. */
  mix: { read: number; write: number; counter: number; setAdd: number };
  /** How many keys the workload uses, and how many to write first. */
  keys: number;
  preload: number;
  /** Most requests go to a few hot keys (a popular product, a live match). */
  hot: number;
  events: ScenarioEvent[];
}

export const scenarios: Scenario[] = [
  {
    id: "shop",
    title: "Online shop at peak",
    workload: "Read-heavy: 90% product page reads, 10% basket writes.",
    watch: "Reads go to every copy but only R must answer. Stop a node: reads still work, from the other 2 copies.",
    rate: 1500,
    mix: { read: 0.9, write: 0.1, counter: 0, setAdd: 0 },
    keys: 120,
    preload: 120,
    hot: 0.6,
    events: [],
  },
  {
    id: "betting",
    title: "Live betting during a match",
    workload: "Write-heavy: 70% bets, 20% stake counters, 10% reads of the odds.",
    watch: "A node fails in the middle of the match. Writes go on to fallback vnodes, and hinted handoff moves them back when the node returns.",
    rate: 1200,
    mix: { read: 0.1, write: 0.7, counter: 0.2, setAdd: 0 },
    keys: 60,
    preload: 30,
    hot: 0.8,
    events: [
      { at: 40, action: "stop", node: 2 },
      { at: 110, action: "start", node: 2 },
    ],
  },
  {
    id: "social",
    title: "Likes and followers",
    workload: "Data types: 60% like counters, 30% follower sets, 10% reads.",
    watch: "Many nodes update the same counters at once. The copies merge, so no like is lost and there are no siblings.",
    rate: 900,
    mix: { read: 0.1, write: 0, counter: 0.6, setAdd: 0.3 },
    keys: 12,
    preload: 0,
    hot: 0.5,
    events: [],
  },
];
