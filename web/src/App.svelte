<script lang="ts">
  // v3: the simulator only. Left: make traffic. Right: the cluster. A
  // floating console takes commands; "What happened" explains the last one.
  import { untrack } from "svelte";
  import NodeView from "./components/NodeView.svelte";
  import KeyPlacement from "./components/KeyPlacement.svelte";
  import InfoDialog from "./components/InfoDialog.svelte";
  import CapFigure from "./components/CapFigure.svelte";
  import AboutFigure from "./components/AboutFigure.svelte";
  import ControlPanel, { type BatchOp } from "./components/ControlPanel.svelte";
  import WhatHappened from "./components/WhatHappened.svelte";
  import Repl from "./components/Repl.svelte";
  import Segmented from "./components/Segmented.svelte";
  import Button from "./components/Button.svelte";
  import Tooltip from "./components/Tooltip.svelte";
  import Resizer from "./components/Resizer.svelte";
  import LeveledPanel from "./components/LeveledPanel.svelte";
  import PanelLeftClose from "@lucide/svelte/icons/panel-left-close";
  import PanelLeftOpen from "@lucide/svelte/icons/panel-left-open";
  import Settings from "@lucide/svelte/icons/settings";
  import Play from "@lucide/svelte/icons/play";
  import Pause from "@lucide/svelte/icons/pause";
  import { brand } from "./lib/brand.ts";
  import { about, cap, history, nodeCount, ringSize as ringSizeInfo } from "./lib/explainers.ts";
  import type { Scenario } from "./lib/scenarios.ts";
  import type { ParsedRequest } from "./lib/curl.ts";
  import { Cluster, type Hop, type HttpResponse, type RingInfo, type SimEvent, type Snapshot } from "./lib/sim.ts";

  const NODE_OPTIONS = [1, 2, 3, 4, 5, 6, 7, 8];
  const RING_OPTIONS = [8, 16, 32, 64];
  // Speed as a fraction of real time. At 1/1000 a 0.5 ms hop takes half a
  // second, fast enough to feel live and slow enough to follow.
  const SPEEDS = ["1/4000", "1/1000", "1/250", "1/100", "1×"] as const;
  const FRACTION: Record<(typeof SPEEDS)[number], number> = {
    "1/4000": 1 / 4000,
    "1/1000": 1 / 1000,
    "1/250": 1 / 250,
    "1/100": 1 / 100,
    "1×": 1,
  };
  const SIM_US_PER_WALL_MS = 1000;
  const MAX_ACTIONS_PER_FRAME = 400;
  const THEMES = ["system", "light", "dark"] as const;
  const TIMINGS = ["Even", "Realistic"] as const;
  const HOP_FADE_MS = 900;

  let nodes = $state(5);
  let ringSize = $state(32);
  let cluster = $state.raw<Cluster | null>(null);
  let ring = $state.raw<RingInfo | null>(null);
  let snapshot = $state.raw<Snapshot | null>(null);
  let now = $state(0);
  let playing = $state(true);
  let speed = $state<(typeof SPEEDS)[number]>("1/1000");
  let timing = $state<(typeof TIMINGS)[number]>("Realistic");
  // Hide the left column, to give a running simulation the whole width.
  const COLLAPSED_KEY = "riak-sim-v3-left-hidden";
  let collapsed = $state(
    (() => {
      try {
        return localStorage.getItem(COLLAPSED_KEY) === "true";
      } catch {
        return false;
      }
    })(),
  );
  $effect(() => {
    const v = String(collapsed);
    try {
      localStorage.setItem(COLLAPSED_KEY, v);
    } catch {
      /* no storage */
    }
  });
  // The vnode whose leveled store is open, if any. It follows the
  // partition: after a move, the partition's store is on its new owner.
  let zoom = $state<number | null>(null);
  // Read again with every new snapshot, so the panel follows the simulation.
  let zoomView = $derived.by(() => {
    if (zoom === null || !snapshot || !cluster) return null;
    void snapshot.now;
    return cluster.leveled(zoom);
  });
  // The tab the left column shows; a tab icon in the hidden column opens it.
  let tab = $state<"requests" | "traffic" | "data" | "scenarios">("requests");
  // A wide window: the page fills the window and the drawing fits the space.
  const WIDE = "(min-width: 52rem)";
  let wide = $state(typeof matchMedia === "function" && matchMedia(WIDE).matches);
  $effect(() => {
    const m = matchMedia(WIDE);
    const on = () => (wide = m.matches);
    m.addEventListener("change", on);
    return () => m.removeEventListener("change", on);
  });
  let consoleOpen = $state(false);
  // Where the console sits when moved: its top left corner, in pixels.
  // null keeps it at the bottom right.
  let consolePos = $state<{ x: number; y: number } | null>(null);
  let dragFrom: { x: number; y: number; px: number; py: number } | null = null;
  let consoleEl: HTMLElement | undefined = $state();

  function dragStart(e: PointerEvent) {
    if (!consoleEl || (e.target as HTMLElement).closest("button")) return;
    const r = consoleEl.getBoundingClientRect();
    dragFrom = { x: e.clientX, y: e.clientY, px: r.left, py: r.top };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function dragMove(e: PointerEvent) {
    if (!dragFrom || !consoleEl) return;
    const r = consoleEl.getBoundingClientRect();
    const x = Math.min(Math.max(0, dragFrom.px + e.clientX - dragFrom.x), window.innerWidth - r.width);
    const y = Math.min(Math.max(0, dragFrom.py + e.clientY - dragFrom.y), window.innerHeight - r.height);
    consolePos = { x, y };
  }
  let theme = $state<(typeof THEMES)[number]>("system");
  let error = $state("");
  let ringVersion = 0;
  let hops = $state.raw<{ hop: Hop; at: number }[]>([]);
  let wall = $state(0);

  // Quorum defaults for every request that does not set its own.
  let quorum = $state({ r: 2, w: 2, dw: 2, pr: 0, pw: 0 });

  // Popups.
  let placement: { open: () => void } | undefined = $state();
  let capInfo: { open: () => void } | undefined = $state();
  let nodesInfo: { open: () => void } | undefined = $state();
  let ringInfo: { open: () => void } | undefined = $state();
  let historyInfo: { open: () => void } | undefined = $state();
  let aboutInfo: { open: () => void } | undefined = $state();
  let what: { open: () => void } | undefined = $state();
  let repl:
    | {
        receive: (r: HttpResponse) => void;
        retire: () => void;
        runCommands: (c: string[]) => void;
        summary: (label: string) => { sent: (n?: number) => void; got: (status: number) => void };
      }
    | undefined = $state();

  // Column width, kept in this browser only.
  const WIDTH_KEY = "riak-sim-v3-left-width";
  let leftWidth = $state<number | null>(
    (() => {
      try {
        return JSON.parse(localStorage.getItem(WIDTH_KEY) ?? "null");
      } catch {
        return null;
      }
    })(),
  );
  $effect(() => {
    const v = JSON.stringify(leftWidth);
    try {
      localStorage.setItem(WIDTH_KEY, v);
    } catch {
      /* no storage */
    }
  });

  // ---- Requests this page sent, and what to do with their replies ----

  type Tracked = {
    tally?: { got: (s: number) => void; note?: (text: string) => void };
    /** A list keys request: count the keys in the reply. */
    listKeys?: boolean;
    /** A data type read: its value goes in the panel. */
    crdt?: "counter" | "set";
    /** A data type update: read the value back when it succeeds. */
    readBack?: string;
    /** Part of the running traffic. */
    traffic?: boolean;
    /** A read of the cart: show its values; then write them back as one. */
    cartRead?: "show" | "resolve";
    /** A write to the cart: read it back to show its values. */
    readBackCart?: boolean;
  };
  const tracked = new Map<number, Tracked>();
  // Keys written and not deleted, for reads and deletes.
  let stored = $state.raw<string[]>([]);
  const storedSet = new Set<string>();
  let counter = $state<number | null>(null);
  let cart = $state<string[] | null>(null);
  const CART = "/types/carts/buckets/carts/keys/alice";
  let basket = $state<string[] | null>(null);

  // "What happened": the last command and its steps.
  let explainId: number | null = null;
  let explainCommand = $state<string | null>(null);
  let explainEvents = $state.raw<SimEvent[]>([]);
  let wasPlaying = true;

  function explain(id: number | null, command: string | null) {
    explainId = id;
    explainCommand = command;
    explainEvents = [];
  }

  const randomId = () => Math.random().toString(36).slice(2, 8);
  const pick = <T,>(a: T[]) => a[Math.floor(Math.random() * a.length)];

  const plain = (method: string, path: string, body = ""): ParsedRequest => ({
    node: null,
    method,
    path,
    headers: body ? [["Content-Type", method === "POST" ? "application/json" : "text/plain"]] : [],
    body,
    include: false,
  });
  const write = (key: string, value = `value-${randomId()}`) => plain("PUT", `/buckets/data/keys/${key}`, value);
  const read = (key: string) => plain("GET", `/buckets/data/keys/${key}`);
  const del = (key: string) => plain("DELETE", `/buckets/data/keys/${key}`);
  const COUNTER = "/types/counters/buckets/stats/datatypes";
  const SET = "/types/sets/buckets/fruit/datatypes";
  const FRUIT = ["apple", "pear", "plum", "fig", "lime", "kiwi", "mango", "peach", "cherry", "grape", "lemon", "melon", "date", "guava", "papaya", "quince"];
  let fruit = 0;
  /** An element not in the basket yet, so each add changes the set. */
  function nextFruit(): string {
    const have = new Set(basket ?? []);
    for (let i = 0; i < FRUIT.length; i++) {
      const f = FRUIT[(fruit + i) % FRUIT.length];
      if (!have.has(f)) {
        fruit = (fruit + i + 1) % FRUIT.length;
        return f;
      }
    }
    return `fruit-${have.size + 1}`;
  }

  /** The curl command a person would type for a request. */
  function curl(r: ParsedRequest): string {
    const url = `localhost:8098${r.path}`;
    const method = r.method === "GET" ? "" : `-X ${r.method} `;
    const headers = r.headers.map(([k, v]) => `-H '${k}: ${v}' `).join("");
    const body = r.body ? `-d '${r.body}'` : "";
    return `curl ${method}${url.includes("?") ? `'${url}'` : url} ${headers}${body}`.trim();
  }

  /** The values in a reply: one body, or the parts of a multipart body. */
  function values(r: HttpResponse): string[] {
    if (r.status === 404) return [];
    const type = r.headers.find(([k]) => k.toLowerCase() === "content-type")?.[1] ?? "";
    const boundary = /boundary=([^;]+)/.exec(type)?.[1];
    if (!boundary) return [r.body];
    return r.body
      .split(`--${boundary}`)
      .map((part) => part.split("\r\n\r\n").slice(1).join("\r\n\r\n").replace(/\r\n$/, ""))
      .filter((v) => v !== "");
  }

  /** The cart's siblings card. */
  function sibling(action: "writeA" | "writeB" | "read" | "resolve") {
    let req: ParsedRequest;
    let t: Tracked;
    switch (action) {
      case "writeA":
      case "writeB":
        req = plain("PUT", CART, action === "writeA" ? "milk" : "bread");
        t = { readBackCart: true };
        break;
      case "read":
      case "resolve":
        req = { ...plain("GET", CART), headers: [["Accept", "multipart/mixed"]] };
        t = { cartRead: action === "read" ? "show" : "resolve" };
        break;
    }
    const tally = repl?.summary(curl(req));
    const id = send(req, { ...t, tally });
    tally?.sent();
    if (id !== null) explain(id, curl(req));
    playing = true;
  }

  /** List every key in bucket data: a coverage query across the cluster. */
  function listKeys() {
    const req = plain("GET", "/buckets/data/keys?keys=true");
    const tally = repl?.summary(curl(req));
    const id = send(req, { tally, listKeys: true });
    tally?.sent();
    if (id !== null) explain(id, curl(req));
    playing = true;
  }

  function example(op: BatchOp | "counter" | "set" | "siblings" | "keys"): string {
    const key = stored.length ? pick(stored) : `k-${randomId()}`;
    switch (op) {
      case "write":
        return curl(write(`k-${randomId()}`));
      case "read":
        return curl(read(key));
      case "delete":
        return curl(del(key));
      case "ping":
        return "curl localhost:8098/ping";
      case "counter":
        return curl(plain("POST", `${COUNTER}/visits`, '{"increment": 1}'));
      case "set":
        return curl(plain("POST", `${SET}/basket`, '{"add": "apple"}'));
      case "keys":
        return "curl 'localhost:8098/buckets/data/keys?keys=true'";
      case "siblings":
        return `curl -i -H 'Accept: multipart/mixed' localhost:8098${CART}`;
    }
  }

  /** Send a request, now or after `delay` microseconds of simulated time.
   *  Writes and deletes update the stored keys at once. */
  function send(r: ParsedRequest, t: Tracked = {}, delay = 0): number | null {
    if (!cluster) return null;
    const id = delay > 0 ? cluster.submitAfter(delay, r) : cluster.submit(r);
    tracked.set(id, t);
    const m = /^\/buckets\/data\/keys\/(.+)$/.exec(r.path);
    if (m && r.method === "PUT" && !storedSet.has(m[1])) {
      storedSet.add(m[1]);
      stored = [...stored, m[1]];
    } else if (m && r.method === "DELETE" && storedSet.delete(m[1])) {
      stored = stored.filter((k) => k !== m[1]);
    }
    return id;
  }

  // ---- Batches: many clients, each sending at its own moment ----

  // A batch arrives over about this long on screen, whatever the speed, at
  // random times: independent clients, not one queue emptied at once.
  const BATCH_SPREAD_WALL_MS = 2000;
  const spreadUs = () => BATCH_SPREAD_WALL_MS * SIM_US_PER_WALL_MS * FRACTION[speed];

  function sendSpread(reqs: ParsedRequest[], label: string) {
    const tally = repl?.summary(label);
    if (!tally) return;
    const spread = spreadUs();
    const delays = reqs.map(() => Math.random() * spread);
    const lastAt = Math.max(...delays);
    reqs.forEach((req, i) => {
      const id = send(req, { tally }, delays[i]);
      tally.sent();
      if (delays[i] === lastAt && id !== null) explain(id, `${curl(req)}    # the last of ${label}`);
    });
    playing = true;
  }

  function batch(op: BatchOp, n: number) {
    let keys: string[] = [];
    if (op === "read" || op === "delete") {
      keys = [...stored].sort(() => Math.random() - 0.5).slice(0, n);
    }
    const reqs = Array.from({ length: n }, (_, i) =>
      op === "write" ? write(`k-${randomId()}`) : op === "read" ? read(keys[i]) : op === "delete" ? del(keys[i]) : plain("GET", "/ping"),
    );
    sendSpread(reqs, `${op} ×${n.toLocaleString()}`);
  }

  // ---- Data types ----

  function crdt(action: "count1" | "count10" | "readCounter" | "add" | "remove" | "readSet") {
    let req: ParsedRequest;
    let kind: "counter" | "set" | undefined;
    let readBack: string | undefined;
    switch (action) {
      case "count1":
      case "count10":
        req = plain("POST", `${COUNTER}/visits`, `{"increment": ${action === "count1" ? 1 : 10}}`);
        readBack = `${COUNTER}/visits`;
        break;
      case "readCounter":
        req = plain("GET", `${COUNTER}/visits`);
        kind = "counter";
        break;
      case "add":
        req = plain("POST", `${SET}/basket`, `{"add": "${nextFruit()}"}`);
        readBack = `${SET}/basket`;
        break;
      case "remove":
        req = plain("POST", `${SET}/basket`, `{"remove": "${basket?.[0] ?? FRUIT[0]}"}`);
        readBack = `${SET}/basket`;
        break;
      case "readSet":
        req = plain("GET", `${SET}/basket`);
        kind = "set";
        break;
    }
    const tally = repl?.summary(curl(req));
    const id = send(req, { tally, crdt: kind, readBack });
    tally?.sent();
    if (id !== null) explain(id, curl(req));
    playing = true;
  }

  // ---- Traffic and scenarios ----

  let rate = $state(400);
  let readShare = $state(80);
  let trafficOn = $state(false);
  let trafficTally: { sent: (n?: number) => void; got: (s: number) => void } | null = null;
  let carry = 0;
  let scenario = $state.raw<Scenario | null>(null);
  let scenarioStart = 0;
  let scenarioDone = new Set<number>();

  function setTraffic(on: boolean) {
    trafficOn = on;
    carry = 0;
    trafficTally = on ? (repl?.summary(scenario ? `scenario: ${scenario.title}` : `traffic ${rate.toLocaleString()}/s, ${readShare}% reads`) ?? null) : null;
    if (on) playing = true;
  }

  function setScenario(s: Scenario | null) {
    if (scenario) {
      // Start any node the scenario stopped.
      for (const e of scenario.events) if (e.action === "stop" && snapshot?.down.includes(e.node)) repl?.runCommands([`node${e.node + 1}$ riak start`]);
    }
    scenario = s;
    setTraffic(false);
    if (!s) return;
    rate = s.rate;
    readShare = Math.round(s.mix.read * 100);
    // Fast enough that a busy workload looks busy: about 5 requests a
    // second on screen.
    speed = "1/250";
    scenarioStart = cluster?.now ?? 0;
    scenarioDone = new Set();
    if (s.preload) {
      sendSpread(Array.from({ length: s.preload }, (_, i) => write(`${s.id}-${i}`)), `preload ${s.preload} keys`);
    }
    setTraffic(true);
  }

  /** One request of the running traffic or scenario. */
  function nextRequest(): ParsedRequest {
    const s = scenario;
    if (!s) {
      if (stored.length && Math.random() * 100 < readShare) return read(pick(stored));
      // Writes: mostly new keys until there are a few hundred, then overwrites.
      return write(stored.length < 300 || Math.random() < 0.3 ? `k-${randomId()}` : pick(stored));
    }
    const key = (prefix: string) => `${prefix}-${Math.random() < s.hot ? Math.floor(Math.random() * Math.min(5, s.keys)) : Math.floor(Math.random() * s.keys)}`;
    let x = Math.random();
    if ((x -= s.mix.read) < 0) return s.mix.write > 0 ? read(key(s.id)) : plain("GET", `${COUNTER}/${key("likes")}`);
    if ((x -= s.mix.write) < 0) return write(key(s.id));
    if ((x -= s.mix.counter) < 0) return plain("POST", `${COUNTER}/${key(s.id === "social" ? "likes" : "stakes")}`, '{"increment": 1}');
    return plain("POST", `${SET}/${key("followers")}`, `{"add": "user-${Math.floor(Math.random() * 500)}"}`);
  }

  // ---- The cluster ----

  async function rebuild() {
    try {
      cluster?.free();
      repl?.retire();
      tracked.clear();
      stored = [];
      storedSet.clear();
      counter = null;
      basket = null;
      cart = null;
      scenario = null;
      trafficOn = false;
      explain(null, null);
      const c = await Cluster.create(nodes, ringSize, Math.floor(Math.random() * 2 ** 31));
      cluster = c;
      c.setRealisticTiming(timing === "Realistic");
      applyQuorum();
      ring = c.ring();
      ringVersion = 0;
      hops = [];
      refresh();
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  function applyQuorum() {
    const e = cluster?.setQuorum(quorum.r, quorum.w, quorum.dw, quorum.pr, quorum.pw);
    if (e) error = e;
  }

  $effect(() => {
    void nodes;
    void ringSize;
    untrack(rebuild);
  });

  $effect(() => {
    if (theme === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  });

  /** Read the core's state after it changed. */
  function refresh() {
    if (!cluster) return;
    snapshot = cluster.snapshot();
    now = cluster.now;
    if (snapshot.ring_version !== ringVersion) {
      ringVersion = snapshot.ring_version;
      ring = cluster.ring();
    }
    const fresh = cluster.takeEvents();
    if (explainId !== null) {
      const mine = fresh.filter((e) => e.req === explainId);
      if (mine.length) explainEvents = [...explainEvents, ...mine];
    }
    const arrived = cluster.takeHops();
    wall = performance.now();
    if (arrived.length) hops = [...hops, ...arrived.map((hop) => ({ hop, at: wall }))].slice(-600);
    for (const r of cluster.takeResponses()) {
      const t = tracked.get(r.req);
      tracked.delete(r.req);
      if (t?.crdt && r.status === 200) {
        try {
          const v = JSON.parse(r.body);
          if (t.crdt === "counter") counter = v.value;
          else basket = v.value;
        } catch {
          /* not JSON */
        }
      }
      if (t?.cartRead) {
        cart = values(r);
        const vclock = r.headers.find(([k]) => k.toLowerCase() === "x-riak-vclock")?.[1];
        // Resolve: write every value back as one, with the vclock just read.
        if (t.cartRead === "resolve" && vclock && cart.length) {
          const merged = [...new Set(cart.flatMap((v) => v.split(", ")))].sort().join(", ");
          const req = { ...plain("PUT", CART, merged), headers: [["Content-Type", "text/plain"], ["X-Riak-Vclock", vclock]] as [string, string][] };
          const tally = repl?.summary(curl(req));
          const id = send(req, { tally, readBackCart: true });
          tally?.sent();
          if (id !== null) explain(id, curl(req));
        }
      }
      if (t?.readBackCart && r.status < 300) {
        send({ ...plain("GET", CART), headers: [["Accept", "multipart/mixed"]] }, { cartRead: "show" });
      }
      if (t?.readBack && r.status < 300) {
        // Show the new value: read it back, as a client would.
        send(plain("GET", t.readBack), { crdt: t.readBack.startsWith(COUNTER) ? "counter" : "set" });
      }
      if (t?.listKeys && r.status === 200) {
        try {
          t.tally?.note?.(`${(JSON.parse(r.body).keys as string[]).length.toLocaleString()} keys`);
        } catch {
          /* not JSON */
        }
      }
      if (t?.tally) t.tally.got(r.status);
      else if (t?.traffic) trafficTally?.got(r.status);
      else if (!t) repl?.receive(r);
    }
  }

  // The frame loop: wall time drives simulation time while playing.
  $effect(() => {
    let last = performance.now();
    let frame = requestAnimationFrame(function tick(t) {
      const dt = Math.min(t - last, 250);
      last = t;
      if (playing && cluster) {
        const target = cluster.now + dt * SIM_US_PER_WALL_MS * FRACTION[speed];
        // Traffic: requests due in this slice of simulated time, each at a
        // random moment in the slice, so they flow rather than arrive in
        // one lump per frame.
        if (trafficOn) {
          const slice = target - cluster.now;
          carry += (rate * slice) / 1e6;
          const n = Math.min(Math.floor(carry), 80);
          carry -= n;
          for (let i = 0; i < n; i++) {
            send(nextRequest(), { traffic: true }, Math.random() * slice);
            trafficTally?.sent();
          }
        }
        // Scenario events, at set times after the start.
        if (scenario) {
          scenario.events.forEach((e, i) => {
            if (!scenarioDone.has(i) && cluster && cluster.now - scenarioStart >= e.at * 1000) {
              scenarioDone.add(i);
              repl?.runCommands([`node${e.node + 1}$ riak ${e.action}`]);
            }
          });
        }
        if (cluster.nextTime !== null || trafficOn) {
          cluster.advanceTo(target, MAX_ACTIONS_PER_FRAME);
        }
        refresh();
      }
      if (hops.length) {
        wall = performance.now();
        if (hops[0].at < wall - HOP_FADE_MS) hops = hops.filter((h) => h.at >= wall - HOP_FADE_MS);
      }
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  });

  /** "nodeN$ riak stop|start" from the console or a node card. */
  function power(n: number, stop: boolean): string | null {
    if (!cluster || !snapshot) return "The cluster is not ready.";
    const down = snapshot.down.includes(n);
    if (stop) {
      if (down) return `node${n + 1} is already stopped.`;
      if (!cluster.stopNode(n)) return `node${n + 1} is the last running node, so it cannot stop.`;
    } else {
      if (!down) return `node${n + 1} is already running.`;
      const handoff = cluster.startNode(n);
      if (handoff > 0) explain(handoff, `node${n + 1}$ riak start`);
    }
    refresh();
    return null;
  }

  function admin(n: number, action: string, target: number | null) {
    if (!cluster) return { ok: false, text: "The cluster is not ready." };
    const r = cluster.cluster(n, action, target);
    if (r.id) explain(r.id, `node${n + 1}$ riak admin cluster commit`);
    refresh();
    return r;
  }

  const adminCmd = (n: number, rest: string) => `node${n + 1}$ riak admin cluster ${rest}`;
  let freeNode = $derived(ring ? (Array.from({ length: ring.max_nodes }, (_, n) => n).find((n) => !ring!.members.includes(n)) ?? null) : null);

  function addNode() {
    if (freeNode === null || !ring) return;
    const to = ring.members[0];
    repl?.runCommands([adminCmd(freeNode, `join node${to + 1}`), adminCmd(to, "plan"), adminCmd(to, "commit")]);
  }
</script>

<Tooltip />

<header>
  <button
    type="button"
    class="icon-button"
    aria-pressed={collapsed}
    title={collapsed ? "Show the controls" : "Hide the controls"}
    aria-label={collapsed ? "Show the controls" : "Hide the controls"}
    onclick={() => (collapsed = !collapsed)}
  >
    {#if collapsed}<PanelLeftOpen />{:else}<PanelLeftClose />{/if}
  </button>
  <h1>{brand.product}</h1>
  <div class="playback">
    <button type="button" class="play" aria-label={playing ? "Pause" : "Play"} title={playing ? "Pause" : "Play"} onclick={() => (playing = !playing)}>
      {#if playing}<Pause />{:else}<Play />{/if}
    </button>
    <span class="clock mono" aria-live="off">{(now / 1000).toFixed(1)} ms</span>
    <Segmented label="Speed" hideLabel options={[...SPEEDS]} value={speed} onchange={(v) => (speed = v)} />
  </div>
  <span class="spacer"></span>
  <nav class="links" aria-label="Explainers">
    <Button variant="text" onclick={() => historyInfo?.open()}>History</Button>
    <span class="sep" aria-hidden="true">|</span>
    <Button variant="text" onclick={() => placement?.open()}>Partitions</Button>
    <span class="sep" aria-hidden="true">|</span>
    <Button variant="text" onclick={() => capInfo?.open()}>CAP theorem</Button>
    <span class="sep" aria-hidden="true">|</span>
    <Button variant="text" onclick={() => aboutInfo?.open()}>How it works</Button>
  </nav>
  <button type="button" class="icon-button" popovertarget="settings" title="Settings" aria-label="Settings"><Settings /></button>
  <Button variant="text" onclick={() => (theme = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length])}>Theme: {theme}</Button>
</header>

<!-- Settings: a popover, so opening it does not push the cluster down. -->
<div class="settings" id="settings" popover="auto">
    <div class="group">
      <div class="row">
        <div class="with-help">
          <Segmented label="Nodes" options={NODE_OPTIONS} value={nodes} onchange={(v) => (nodes = v)} />
          <Button variant="text" onclick={() => nodesInfo?.open()}>How to choose</Button>
        </div>
        <div class="with-help">
          <Segmented label="Partitions (ring_size)" options={RING_OPTIONS} value={ringSize} onchange={(v) => (ringSize = v)} />
          <Button variant="text" onclick={() => ringInfo?.open()}>How to choose</Button>
        </div>
      </div>
      <p class="supporting">A change to Nodes or Partitions starts a new cluster with no data.</p>
    </div>
    <div class="group">
      <div class="row">
        <Segmented label="R (read quorum)" options={[1, 2, 3]} value={quorum.r} onchange={(v) => { quorum.r = v; applyQuorum(); }} />
        <Segmented label="W (write quorum)" options={[1, 2, 3]} value={quorum.w} onchange={(v) => { quorum.w = v; applyQuorum(); }} />
        <Segmented label="DW (durable write)" options={[1, 2, 3]} value={quorum.dw} onchange={(v) => { quorum.dw = v; applyQuorum(); }} />
        <Segmented label="PR (primary read)" options={[0, 1, 2, 3]} value={quorum.pr} onchange={(v) => { quorum.pr = v; applyQuorum(); }} />
        <Segmented label="PW (primary write)" options={[0, 1, 2, 3]} value={quorum.pw} onchange={(v) => { quorum.pw = v; applyQuorum(); }} />
      </div>
      <p class="supporting">Quorum defaults for every request. A request can still set its own, for example <code>?r=1</code>. N (copies) is 3. PR and PW count only replies from primary vnodes (the usual vnodes, not fallback vnodes); 0 turns the check off.</p>
    </div>
    <div class="group">
      <Segmented label="Timing" options={[...TIMINGS]} value={timing} onchange={(v) => { timing = v; cluster?.setRealisticTiming(v === "Realistic"); }} />
    </div>
</div>

<main class:collapsed class:zoomed={zoom !== null} style:--col-left={leftWidth === null ? undefined : `${leftWidth}px`}>
  <aside>
    {#if !collapsed}<Resizer label="Left column width" edge="right" bind:width={leftWidth} min={280} max={560} />{/if}
    <ControlPanel
      {collapsed}
      bind:tab
      onexpand={(t) => {
        tab = t;
        collapsed = false;
      }}
      stored={stored.length}
      onbatch={batch}
      onlistkeys={listKeys}
      {example}
      bind:rate
      bind:readShare
      onScreen={rate * FRACTION[speed]}
      {trafficOn}
      ontraffic={setTraffic}
      {counter}
      {basket}
      {cart}
      onsibling={sibling}
      oncrdt={crdt}
      {scenario}
      onscenario={setScenario}
    />
  </aside>
  {#if error}
    <p class="error">{error}</p>
  {:else if ring && snapshot}
    <section class="cluster" aria-label="The cluster">
      <NodeView
        {ring}
        {snapshot}
        {now}
        key={null}
        request={null}
        selected={null}
        onselect={() => {}}
        live
        {hops}
        {wall}
        hopFade={HOP_FADE_MS}
        onpower={(n, stop) => repl?.runCommands([`node${n + 1}$ riak ${stop ? "stop" : "start"}`])}
        onleave={(n) => repl?.runCommands([adminCmd(n, "leave"), adminCmd(n, "plan"), adminCmd(n, "commit")])}
        fit={wide}
        zoomed={zoom}
        onzoom={(p) => (zoom = zoom === p ? null : p)}
      >
        {#snippet corner()}
          <Button variant="tonal" disabled={freeNode === null} onclick={addNode}>Add a node</Button>
          <Button variant="text" onclick={() => what?.open()}>What happened?</Button>
          <span class="summary">{ring?.members.length} nodes · {ring?.size} partitions<br />N=3, R={quorum.r}, W={quorum.w}</span>
        {/snippet}
      </NodeView>
    </section>
    {#if zoom !== null && zoomView}
      {@const zv = snapshot.vnodes.find((v) => v.id === zoom)}
      <LeveledPanel
        title="Partition {zoom} on node{ring.owners[zoom] + 1}"
        view={zoomView}
        {wall}
        mailbox={zv?.mailbox.length ?? 0}
        busy={zv?.busy != null}
        onclose={() => (zoom = null)}
      />
    {/if}
    <KeyPlacement bind:this={placement} {ring} key={null} />
    <InfoDialog bind:this={capInfo} explainer={cap}>
      {#snippet figure()}<CapFigure />{/snippet}
    </InfoDialog>
    <InfoDialog bind:this={nodesInfo} explainer={nodeCount} />
    <InfoDialog bind:this={ringInfo} explainer={ringSizeInfo} />
    <InfoDialog bind:this={historyInfo} explainer={history} />
    <InfoDialog bind:this={aboutInfo} explainer={about}>
      {#snippet figure()}<AboutFigure />{/snippet}
    </InfoDialog>
    <WhatHappened
      bind:this={what}
      command={explainCommand}
      events={explainEvents}
      onopen={() => {
        wasPlaying = playing;
        playing = false;
      }}
      onclose={() => (playing = wasPlaying)}
    />
  {/if}
</main>

<!-- The console floats over the page, so it is always to hand. -->
{#if cluster && ring}
  <!-- The button and the panel are separate, so the button always sits
       at the bottom right, wherever the panel was moved to. -->
  {#if !consoleOpen}
    <div class="console-button">
      <Button variant="tonal" onclick={() => (consoleOpen = true)}>Console</Button>
    </div>
  {/if}
  <div
    class="console"
    class:open={consoleOpen}
    class:moved={consolePos !== null}
    bind:this={consoleEl}
    style:left={consolePos ? `${consolePos.x}px` : undefined}
    style:top={consolePos ? `${consolePos.y}px` : undefined}
  >
    {#if consoleOpen}
      <!-- Drag the title bar to move the console; drag the corner to resize it. -->
      <div
        class="console-head"
        role="toolbar"
        aria-label="Console. Drag to move."
        tabindex="-1"
        onpointerdown={dragStart}
        onpointermove={dragMove}
        onpointerup={() => (dragFrom = null)}
        onpointercancel={() => (dragFrom = null)}
      >
        <span class="console-title">Console</span>
        <span class="supporting">Drag to move · corner to resize</span>
        <span class="spacer"></span>
        <Button variant="text" onclick={() => (consolePos = null)}>Dock</Button>
        <Button variant="text" onclick={() => (consoleOpen = false)}>Close</Button>
      </div>
    {/if}
    <div class="console-body" class:hidden={!consoleOpen}>
      <Repl
        bind:this={repl}
        bind:open={consoleOpen}
        onpower={power}
        onadmin={admin}
        onreset={() => void rebuild()}
        {cluster}
        nodes={ring.max_nodes}
        lesson={null}
        onstep={() => {}}
        onsubmit={(id, request) => {
          tracked.delete(id);
          explain(id, curl(request));
          playing = true;
          refresh();
        }}
      />
    </div>
  </div>
{/if}

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    min-height: var(--size-target);
    padding: var(--space-2) var(--space-4);
    gap: var(--space-2) var(--space-3);
  }
  .icon-button,
  .play {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--size-target);
    height: var(--size-target);
    padding: 0;
    border: none;
    border-radius: var(--shape-full);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .icon-button:hover {
    background: var(--md-sys-color-surface-container-highest);
  }
  .play {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .play:hover {
    outline: var(--stroke-thick) solid currentColor;
    outline-offset: calc(-1 * var(--stroke-thick));
  }
  header :global(.lucide) {
    width: var(--size-icon);
    height: var(--size-icon);
  }
  h1 {
    margin: 0;
    font: var(--type-title-large);
  }
  .spacer {
    flex: 1;
  }
  .links {
    display: flex;
    align-items: center;
  }
  .sep {
    color: var(--md-sys-color-outline);
  }
  .playback {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
  }
  .clock {
    min-width: var(--size-clock);
  }
  /* A popover under the header, on the right. */
  .settings {
    inset: auto;
    top: calc(var(--size-target) + var(--space-4));
    right: var(--space-4);
    max-width: min(var(--size-dialog), calc(100vw - var(--space-8)));
    margin: 0;
    padding: var(--space-4);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline-variant);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }
  .settings:popover-open {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4) var(--space-8);
  }
  .summary {
    font: var(--type-body-medium);
    color: var(--md-sys-color-on-surface-variant);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3) var(--space-5);
  }
  .with-help {
    display: flex;
    align-items: flex-end;
    gap: var(--space-1);
  }
  .supporting {
    margin: 0;
    font: var(--type-body-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  .error {
    color: var(--md-sys-color-error);
  }
  main {
    display: grid;
    grid-template-columns: var(--col-left, var(--size-column)) minmax(0, 1fr);
    gap: var(--space-4);
    padding: 0 var(--space-4) var(--space-6);
    align-items: start;
  }
  /* A partition's store is open on the right: the drawing makes room. */
  @media (min-width: 52rem) {
    main.zoomed {
      padding-right: calc(var(--size-column) + var(--space-12) + var(--space-8));
    }
  }
  /* The hidden left column keeps a rail of tab icons. */
  main.collapsed {
    grid-template-columns: auto minmax(0, 1fr);
  }
  /* On a wide window the page fills the window and does not scroll: the
     cluster drawing takes the height that is left, and the left column
     scrolls on its own if it is too long. */
  @media (min-width: 52rem) {
    :global(#app) {
      display: flex;
      flex-direction: column;
      height: 100dvh;
    }
    main {
      flex: 1;
      min-height: 0;
      align-items: stretch;
      padding-bottom: var(--space-4);
    }
    aside {
      align-self: start;
      max-height: 100%;
      overflow-y: auto;
    }
    .cluster {
      min-height: var(--size-dialog-short);
    }
  }
  aside {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container);
    min-width: 0;
  }
  .cluster {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container);
    min-width: 0;
  }
  @media (max-width: 52rem) {
    main {
      grid-template-columns: minmax(0, 1fr);
    }
    aside > :global(.resizer) {
      display: none;
    }
  }
  /* The floating console, bottom right. Hidden, not removed, when closed,
     so its log and size are kept. */
  .console-button {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 5;
  }
  .console:not(.open) {
    display: none;
  }
  .console {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 5;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .console.open {
    width: calc(var(--size-column) * 1.5);
    height: var(--size-dialog-short);
    min-width: var(--size-column);
    min-height: var(--size-log-max);
    max-width: calc(100vw - var(--space-8));
    max-height: calc(100vh - var(--space-8));
    padding: var(--space-2) var(--space-3) var(--space-3);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container-high);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline);
    align-items: stretch;
    resize: both;
    overflow: hidden;
  }
  .console.moved {
    right: auto;
    bottom: auto;
  }
  .console-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    cursor: move;
    touch-action: none;
    user-select: none;
  }
  /* The log fills the console, so resizing it shows more lines. */
  .console-body {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }
  .console-body :global(.repl) {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }
  .console-body :global(.log) {
    flex: 1;
    max-height: none;
  }
  .console-title {
    font: var(--type-title-small);
  }
  .console-body.hidden {
    display: none;
  }
</style>
