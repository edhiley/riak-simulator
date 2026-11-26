<script lang="ts">
  // The cluster as nodes. Each node is a card; its partitions (vnodes) are
  // tiles in the node's colour. The current key's copies are outlined, and
  // every message in flight is drawn at its position for the current
  // simulation time.
  import { isHandoff, nodeGeometry, route } from "../lib/nodeLayout.ts";
  import { hideTooltip, showTooltip } from "../lib/tooltip.svelte.ts";
  import { shortKey } from "../lib/text.ts";
  import Pixels from "./Pixels.svelte";
  import { laptop, server, splitter, spriteSize } from "../lib/pixels.ts";
  import { PALETTE, movers } from "../lib/characters.ts";
  import { endpointPos } from "../lib/nodeLayout.ts";
  import { vnodeOf, progressAt, type KeyInfo, type RequestView, type RingInfo, type Snapshot } from "../lib/sim.ts";

  let {
    ring,
    snapshot,
    now,
    key,
    value = undefined,
    request,
    selected,
    onselect,
    live = false,
    onpower = undefined,
    onleave = undefined,
    hops = [],
    wall = 0,
    hopFade = 900,
    fit = false,
    corner = undefined,
    zoomed = null,
    onzoom = undefined,
  }: {
    /** The vnode whose store is open, outlined. */
    zoomed?: number | null;
    /** Open a partition's store. */
    onzoom?: (vnode: number) => void;
    /** Choose the card shape that makes the drawing largest in its space. */
    fit?: boolean;
    /** Page controls drawn in the free space at the top left. */
    corner?: import("svelte").Snippet;
    ring: RingInfo;
    snapshot: Snapshot;
    now: number;
    key: KeyInfo | null;
    /** The value the client last saw: undefined if unknown, null if none. */
    value?: string | null;
    request: RequestView | null;
    selected: number | null;
    onselect: (id: number) => void;
    /** Live mode: messages are pulses with no labels, and arrived messages fade. */
    live?: boolean;
    /** Node controls on each card, when given. */
    onpower?: (node: number, stop: boolean) => void;
    onleave?: (node: number) => void;
    hops?: { hop: import("../lib/sim.ts").Hop; at: number }[];
    /** Wall time now, in ms, for the fade. */
    wall?: number;
    hopFade?: number;
  } = $props();

  /** The colour of the node a message comes from. Messages from the clients
   *  and the load balancer use the primary role: soft, and unlike any node. */
  const sourceColour = (e: import("../lib/sim.ts").Endpoint) =>
    e.kind === "node" ? `var(--node-${e.id})` : e.kind === "vnode" ? `var(--node-${vnodeOf(e.id, ring).node})` : "var(--md-sys-color-primary)";

  // Arrived messages: the whole path fades, and a ring grows at the target.
  let streaks = $derived(
    live
      ? hops.map(({ hop, at }) => {
          const age = Math.min(1, Math.max(0, (wall - at) / hopFade));
          const r = route(hop.from, hop.to, ring, g, hop.label);
          const [tx, ty] = r.at(1);
          return {
            d: r.d,
            handoff: isHandoff(hop.label),
            colour: sourceColour(hop.from),
            opacity: 0.6 * (1 - age),
            x: tx,
            y: ty,
            r: 10 + 26 * age,
            flash: 0.9 * (1 - age) ** 2,
          };
        })
      : [],
  );

  // Fallback vnodes, and new owners' copies of moving partitions: both are
  // extra tiles in the card of the node they run on.
  let fallbacks = $derived(snapshot.vnodes.filter((v) => v.fallback || v.moving));
  // The size of the space the drawing has, for fit.
  let boxW = $state(0);
  let boxH = $state(0);
  let g = $derived(nodeGeometry(ring, fallbacks.map((v) => v.id), live, fit ? { w: boxW, h: boxH } : undefined));

  // Live mode: each tile shows a dot for each key it stores. The dots
  // share the space below the partition number and shrink as more arrive.
  const NUMBER_BAND = 26;
  const MAX_DOT = 7;
  // Below this radius the dots are too small to see or point at, so the
  // tile shows one filled block instead.
  const MIN_DOT = 3;
  const MAX_DOTS = 20;
  function pack(n: number, w: number, h: number): { x: number; y: number; r: number }[] {
    if (n === 0) return [];
    const cols = Math.max(1, Math.ceil(Math.sqrt((n * w) / h)));
    const rows = Math.ceil(n / cols);
    const cell = Math.min(w / cols, h / rows);
    const r = Math.min(MAX_DOT, cell * 0.38);
    return Array.from({ length: n }, (_, i) => {
      const row = Math.floor(i / cols);
      const inRow = row === rows - 1 ? n - row * cols : cols;
      const col = i % cols;
      return { x: (col - (inRow - 1) / 2) * cell, y: (row - (rows - 1) / 2) * cell, r };
    });
  }
  // The key under the pointer, so its other copies light up too.
  let hovered = $state<string | null>(null);
  const objectId = (o: { bucket: string; key: string }) => `${o.bucket}/${o.key}`;
  // The vnodes that hold a copy of the key under the pointer. A tile that
  // shows one block (too many keys for dots) is outlined, so its copy shows too.
  let holders = $derived.by(() => {
    const id = hovered;
    if (id === null) return [];
    return snapshot.vnodes.filter((v) => v.objects.some((o) => objectId(o) === id));
  });
  let holderIds = $derived(new Set(holders.map((v) => v.id)));
  function copiesText(o: { bucket: string; key: string; value: string }): string {
    const id = objectId(o);
    const where = snapshot.vnodes
      .filter((v) => v.objects.some((c) => objectId(c) === id))
      .map((v) => (v.fallback || v.moving ? `${v.partition} (${v.moving ? "moving" : "fallback"} on node${v.owner + 1})` : `${v.partition}`));
    return `${id}: ${o.value}. ${where.length} ${where.length === 1 ? "copy" : "copies"}, in partition${where.length === 1 ? "" : "s"} ${where.join(", ")}.`;
  }
  let nodes = $derived(ring.members);
  let leaving = $derived(new Set(snapshot.leaving));
  let down = $derived(new Set(snapshot.down));
  // The vnodes the last request for this key used, which can include a
  // fallback vnode; otherwise the key's usual vnodes.
  let replicas = $derived(
    new Set(
      request && key && request.key === key.key && request.preflist.length
        ? request.preflist
        : (key?.preflist.map((r) => r.partition) ?? []),
    ),
  );
  let vnodes = $derived(new Map(snapshot.vnodes.map((v) => [v.id, v])));

  // The FSMs running on each node: one for each read, write, delete or list
  // keys that has arrived and not finished. A put that arrives at a node
  // outside the key's preference list is forwarded, and its FSM runs on the
  // coordinator. Ping has no FSM; handoff and moves are not requests.
  const FSM_OPS = new Set(["put", "delete", "get", "listkeys"]);
  let fsms = $derived.by(() => {
    const count = new Map<number, number>();
    for (const r of snapshot.requests) {
      if (!r.arrived || r.done || !FSM_OPS.has(r.op)) continue;
      const n = r.coordinator ?? r.origin;
      count.set(n, (count.get(n) ?? 0) + 1);
    }
    return count;
  });
  const MAX_CHIPS = 4;
  // Couriers and movers step: their frame changes every 250 ms of wall time.
  let step = $derived(Math.floor(wall / 250) % 2);
  const PORT_HELP = {
    http: "HTTP listener, port 8098. Client requests arrive here (Protocol Buffers use port 8087). Each read or write starts an FSM on this node.",
    fsm: "FSMs: one short-lived Erlang process for each request this node coordinates (riak_kv_get_fsm, riak_kv_put_fsm), one square each. Why: each request has its own process, so a slow request never holds up another, and the process ends with the reply, so nothing is left over. A put that arrives at a node with no copy of the key is forwarded to a node that has one, and a new FSM starts there.",
    handoff: "Handoff listener, port 8099. The movers bring a partition's data here over a TCP connection of their own. Why: Erlang distribution has one connection between two nodes, and a large transfer on it would hold up every request behind it.",
    dist: "Erlang distribution: one TCP connection to each other node. An FSM on another node sends its commands in here, addressed to each vnode's own proxy process.",
  };

  // The trail covers the last 0.35 of the path. Live mode uses more,
  // shorter segments, so the taper is smooth.
  let WAKE_SEGMENTS = $derived(live ? 24 : 10);
  let WAKE_STEP = $derived(0.35 / WAKE_SEGMENTS);

  let dots = $derived(
    snapshot.messages.map((m) => {
      const r = route(m.from, m.to, ring, g, m.label);
      const t = progressAt(m, now);
      const [x, y] = r.at(t);
      const owner = (e: typeof m.from) => (e.kind === "node" ? e.id : e.kind === "vnode" ? vnodeOf(e.id, ring).node : -1);
      const local = owner(m.from) >= 0 && owner(m.from) === owner(m.to);
      // A trail behind a moving message: short segments that fade out.
      const held = m.deliver === null;
      const wake = held
        ? []
        : Array.from({ length: WAKE_SEGMENTS }, (_, i) => {
            const t1 = Math.max(0, t - i * WAKE_STEP);
            const t0 = Math.max(0, t - (i + 1) * WAKE_STEP);
            const [x0, y0] = r.at(t0);
            const [x1, y1] = r.at(t1);
            const fade = 1 - i / WAKE_SEGMENTS;
            return { x0, y0, x1, y1, opacity: 0.8 * fade, width: 4 + 10 * fade };
          }).filter((s) => s.x0 !== s.x1 || s.y0 !== s.y1);
      return { m, x, y, path: r.d, held, local, wake };
    }),
  );

  const list = (items: number[]) =>
    items.length <= 1 ? items.join("") : `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
  const pillWidth = (label: string) => 32 + label.length * 13.5;
  const chipWidth = (text: string) => text.length * 17 + 28;
  const MAX_VALUE = 200;
  let valueText = $derived(
    value === undefined
      ? "Value not known yet. Read or write the object to see it."
      : value === null
        ? "No value. The object is not found or is deleted."
        : `Value: ${value.length > MAX_VALUE ? `${value.slice(0, MAX_VALUE)}…` : value}`,
  );
  let keyLabel = $derived(key ? `Key: ${shortKey(key.key)}` : "");
</script>

<div class="fit" class:fill={fit} bind:clientWidth={boxW} bind:clientHeight={boxH}>
<svg viewBox={g.viewBox} role="group" aria-label="{ring.nodes} nodes holding {ring.size} partitions">
  {#if corner}
    <foreignObject x={g.corner.x} y={g.corner.y} width={g.corner.w} height={g.corner.h}>
      <div class="corner">{@render corner()}</div>
    </foreignObject>
  {/if}
  <!-- Nodes and their partitions -->
  {#each nodes as n (n)}
    {@const c = g.card(n)}
    <g class="node" class:coordinator={request?.coordinator === n} class:down={down.has(n)}>
      <rect x={c.x} y={c.y} width={c.w} height={c.h} rx="14" class="card" />
      <rect x={c.x} y={c.y} width={c.w} height="38" rx="14" class="band" style:fill="var(--node-{n})" />
      <rect x={c.x} y={c.y + 22} width={c.w} height="16" class="band" style:fill="var(--node-{n})" />
      <Pixels sprite={server} x={c.x + 12} y={c.y + 8} ink="var(--node-{n}-on)" />
      <text x={c.x + 44} y={c.y + 20} class="node-name" style:fill="var(--node-{n}-on)">node{n + 1}</text>
      {#if onpower && onleave}
        {@const off = down.has(n)}
        {@const y = c.y + 19}
        <g class="controls" style:color="var(--node-{n}-on)">
          {#if leaving.has(n)}
            <text x={c.x + c.w - 12} {y} class="role">leaving</text>
          {:else}
            {@render icon(c.x + c.w - 22, y, "leave", `Make node${n + 1} leave the cluster`, () => onleave(n))}
          {/if}
          {@render icon(c.x + c.w - (leaving.has(n) ? 92 : 56), y, off ? "start" : "stop", off ? `Start node${n + 1}` : `Stop node${n + 1}`, () => onpower(n, !off))}
        </g>
      {:else if down.has(n) && leaving.has(n)}
        <text x={c.x + c.w - 12} y={c.y + 20} class="role" style:fill="var(--node-{n}-on)">stopped, leaving</text>
      {:else if leaving.has(n)}
        <text x={c.x + c.w - 12} y={c.y + 20} class="role" style:fill="var(--node-{n}-on)">leaving</text>
      {:else if down.has(n)}
        <text x={c.x + c.w - 12} y={c.y + 20} class="role" style:fill="var(--node-{n}-on)">stopped</text>
      {:else if request?.coordinator === n}
        <text x={c.x + c.w - 12} y={c.y + 20} class="role" style:fill="var(--node-{n}-on)">coordinator</text>
      {:else if request?.arrived && request.origin === n}
        <text x={c.x + c.w - 12} y={c.y + 20} class="role" style:fill="var(--node-{n}-on)">received</text>
      {/if}
    </g>
  {/each}
  <!-- The node's ports and FSMs, on the card edge that faces the corridor. -->
  {#snippet portStrip(n: number)}
    {@const s = g.strip(n)}
    {@const k = fsms.get(n) ?? 0}
    {@const [hx, hy] = g.httpPort(n)}
    {@const [fx, fy] = g.nodePort(n)}
    {@const [ox, oy] = g.handoffPort(n)}
    {@const [dx, dy] = g.distPort(n)}
    <g class="strip">
      <rect x={s.x} y={s.y} width={s.w} height={s.h} rx="8" class="strip-bg" />
      <line x1={s.x + g.portW} x2={s.x + g.portW} y1={s.y + 6} y2={s.y + s.h - 6} class="strip-rule" />
      <line x1={s.x + s.w - g.portW} x2={s.x + s.w - g.portW} y1={s.y + 6} y2={s.y + s.h - 6} class="strip-rule" />
      {@render part(hx, hy, g.portW, s.h, PORT_HELP.http)}
      <text x={hx} y={hy + 1} class="port">:8098</text>
      {@render part(ox, oy, g.portW, s.h, PORT_HELP.handoff)}
      <text x={ox} y={oy + 1} class="port">:8099</text>
      {@render part(fx, fy, s.w - 2 * g.portW, s.h, PORT_HELP.fsm)}
      {#if k === 0}
        <text x={fx} y={fy + 1} class="fsm-none">FSMs</text>
      {:else if k <= MAX_CHIPS}
        {#each Array.from({ length: k }, (_, i) => i) as i (i)}
          <rect x={fx - (k * 18 - 4) / 2 + i * 18} y={fy - 7} width="14" height="14" rx="4" class="fsm" style:fill="var(--node-{n})" />
        {/each}
      {:else}
        <text x={fx} y={fy + 1} class="fsm-count">{k} FSMs</text>
      {/if}
      <g
        class="hover"
        role="img"
        aria-label={PORT_HELP.dist}
        onmouseenter={(e) => showTooltip(e.currentTarget, PORT_HELP.dist)}
        onmouseleave={hideTooltip}
      >
        <circle cx={dx} cy={dy} r="8" class="dist" />
      </g>
    </g>
  {/snippet}
  {#snippet part(x: number, y: number, w: number, h: number, help: string)}
    <rect
      x={x - w / 2}
      y={y - h / 2}
      width={w}
      height={h}
      class="hover part"
      role="img"
      aria-label={help}
      onmouseenter={(e) => showTooltip(e.currentTarget, help)}
      onmouseleave={hideTooltip}
    />
  {/snippet}
  <!-- A small icon button on a node card: stop, start or leave. -->
  {#snippet icon(x: number, y: number, kind: "stop" | "start" | "leave", label: string, act: () => void)}
    <g
      class="icon"
      transform="translate({x},{y})"
      role="button"
      tabindex="0"
      aria-label={label}
      onclick={act}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && act()}
      onmouseenter={(e) => showTooltip(e.currentTarget, label)}
      onmouseleave={hideTooltip}
      onfocus={(e) => showTooltip(e.currentTarget, label)}
      onblur={hideTooltip}
    >
      <circle r="15" class="hit" />
      {#if kind === "stop"}
        <path d="M0,-8 V-1 M-5.5,-5 A7.5 7.5 0 1 0 5.5,-5" class="stroke" />
      {:else if kind === "start"}
        <path d="M-4,-6.5 L6.5,0 L-4,6.5 Z" class="fill" />
      {:else}
        <path d="M-1,-7 H-7 V7 H-1 M-2,0 H7.5 M4,-3.5 L7.5,0 L4,3.5" class="stroke" />
      {/if}
    </g>
  {/snippet}

  <!-- A tile's number; in live mode, also a dot for each stored key. -->
  {#snippet tileBody(tx: number, ty: number, colour: number, p: number, v: import("../lib/sim.ts").VnodeView | undefined)}
    {#if live}
      {@const top = ty - g.tile.h / 2}
      {@const area = { w: g.tile.w - 8, h: g.tile.h - NUMBER_BAND - 6 }}
      {@const cy = top + NUMBER_BAND + 2 + area.h / 2}
      {@const objects = v?.objects ?? []}
      <text x={tx} y={top + NUMBER_BAND / 2 + 1} style:fill="var(--node-{colour}-on)">{p}</text>
      <line x1={tx - g.tile.w / 2 + 5} x2={tx + g.tile.w / 2 - 5} y1={top + NUMBER_BAND} y2={top + NUMBER_BAND} class="divider" style:stroke="var(--node-{colour}-on)" />
      {#if objects.length === 0}
        <circle cx={tx} {cy} r={MAX_DOT} class="empty-dot" style:stroke="var(--node-{colour}-on)" />
      {:else if objects.length > MAX_DOTS || pack(objects.length, area.w, area.h)[0].r < MIN_DOT}
        <rect
          class="full"
          role="img"
          aria-label="{objects.length} keys"
          x={tx - area.w / 2}
          y={cy - area.h / 2}
          width={area.w}
          height={area.h}
          rx="3"
          style:fill="var(--node-{colour}-on)"
          onmouseenter={(e) => showTooltip(e.currentTarget, `${objects.length} keys, including ${objects.slice(0, 3).map(objectId).join(", ")}`)}
          onmouseleave={hideTooltip}
        />
      {:else}
        {@const spots = pack(objects.length, area.w, area.h)}
        <!-- Dots are placed relative to their tile. Their positions animate
             when the tile re-packs; when the tile itself moves to another
             card, the group moves at once and the dots go with it. -->
        <g transform="translate({tx} {cy})">
        {#each objects as o, i (objectId(o))}
          <circle
            class="dot"
            class:same={hovered === objectId(o)}
            class:conflict={o.siblings > 1}
            role="img"
            aria-label="{objectId(o)}: {o.value}"
            style:cx="{spots[i].x}px"
            style:cy="{spots[i].y}px"
            style:r="{spots[i].r}px"
            style:fill="var(--node-{colour}-on)"
            onmouseenter={(e) => {
              hovered = objectId(o);
              showTooltip(e.currentTarget, copiesText(o));
            }}
            onmouseleave={() => {
              hovered = null;
              hideTooltip();
            }}
          />
        {/each}
        </g>
      {/if}
    {:else}
      <text x={tx} y={ty} style:fill="var(--node-{colour}-on)">{p}</text>
    {/if}
  {/snippet}
  <g class="tiles" class:focused={key !== null} class:spotlight={hovered !== null}>
    {#each ring.owners as owner, p (p)}
      {@const [tx, ty] = g.tileCentre(p)}
      {@const v = vnodes.get(p)}
      <g
        class:holds={holderIds.has(p)}
        class:replica={replicas.has(p)}
        class:busy={v?.busy}
        class:off={down.has(owner)}
        class:folding={v?.folding}
        class:zoomable={onzoom !== undefined}
        class:zoomed={zoomed === p}
        role="button"
        tabindex="0"
        aria-label={onzoom ? `Open the store of partition ${p} on node${owner + 1}` : undefined}
        onclick={() => onzoom?.(p)}
        onkeydown={(e) => (e.key === "Enter" || e.key === " ") && onzoom?.(p)}
      >
        <rect x={tx - g.tile.w / 2} y={ty - g.tile.h / 2} width={g.tile.w} height={g.tile.h} rx="6" style:fill="var(--node-{owner})">
          <title>Partition {p} (vnode {p}) on node{owner + 1}{onzoom ? ". Select to see its leveled store." : ""}</title>
        </rect>
        {@render tileBody(tx, ty, owner, p, v)}
        {#if v && v.mailbox.length}
          <circle cx={tx + g.tile.w / 2} cy={ty - g.tile.h / 2} r="10" class="queue" />
          <text x={tx + g.tile.w / 2} y={ty - g.tile.h / 2} class="queue-count">{v.mailbox.length}</text>
        {/if}
      </g>
    {/each}
    <!-- Fallback vnodes: in the colour of the stopped node they stand in for, with a dashed edge. -->
    {#each fallbacks as v (v.id)}
      {@const [tx, ty] = g.tileCentre(v.id)}
      {@const usual = ring.owners[v.partition]}
      <g class="fallback" class:holds={holderIds.has(v.id)} class:replica={replicas.has(v.id)} class:busy={v.busy}>
        <rect x={tx - g.tile.w / 2} y={ty - g.tile.h / 2} width={g.tile.w} height={g.tile.h} rx="6" style:fill="var(--node-{usual})">
          <title
            >{v.moving
              ? `Partition ${v.partition} is moving from node${usual + 1} to node${v.owner + 1}.`
              : `Fallback vnode ${v.partition} on node${v.owner + 1}. It keeps copies for node${usual + 1}.`}</title
          >
        </rect>
        {@render tileBody(tx, ty, usual, v.partition, v)}
        {#if v.mailbox.length}
          <circle cx={tx + g.tile.w / 2} cy={ty - g.tile.h / 2} r="10" class="queue" />
          <text x={tx + g.tile.w / 2} y={ty - g.tile.h / 2} class="queue-count">{v.mailbox.length}</text>
        {/if}
      </g>
    {/each}
  </g>

  <!-- Messages -->
  {#each dots as d (d.m.id)}
    <path d={d.path} class="trail" class:hidden={live} class:local={d.local} class:selected={selected === d.m.id} />
  {/each}
  {#each dots as d (d.m.id)}
    {#each d.wake as w, i (i)}
      <line x1={w.x0} y1={w.y0} x2={w.x1} y2={w.y1} class="wake" stroke-width={w.width} stroke-opacity={w.opacity} style:stroke={live ? sourceColour(d.m.from) : undefined} />
    {/each}
  {/each}
  {#if live}
    {#each streaks as s, i (i)}
      <path d={s.d} class="streak" class:handoff={s.handoff} style:stroke={s.colour} stroke-opacity={s.opacity} />
      <circle cx={s.x} cy={s.y} r={s.r} class="flash" style:stroke={s.colour} stroke-opacity={s.flash} />
    {/each}
  {/if}
  <!-- Clients and load balancer: drawn over the message lines, under the
       pulses, so the balancer in the middle stays readable. -->
  <g class="edge">
    <!-- Many clients: a fan of boxes behind the front one. -->
    <rect x={g.client[0] - g.boxW / 2 + 12} y={g.client[1] - g.boxH / 2 - 12} width={g.boxW} height={g.boxH} rx="12" />
    <rect x={g.client[0] - g.boxW / 2 + 6} y={g.client[1] - g.boxH / 2 - 6} width={g.boxW} height={g.boxH} rx="12" />
    <rect x={g.client[0] - g.boxW / 2} y={g.client[1] - g.boxH / 2} width={g.boxW} height={g.boxH} rx="12" />
    <Pixels sprite={laptop} x={g.client[0] - g.boxW / 2 + 10} y={g.client[1] - spriteSize(laptop).h} ink="var(--md-sys-color-on-surface)" accent="var(--md-sys-color-primary)" />
    <text x={g.client[0] + 16} y={g.client[1]}>clients</text>
    <rect x={g.balancer[0] - g.boxW / 2} y={g.balancer[1] - g.boxH / 2} width={g.boxW} height={g.boxH} rx="12" />
    <Pixels sprite={splitter} x={g.balancer[0] - g.boxW / 2 + 10} y={g.balancer[1] - spriteSize(splitter).h} ink="var(--md-sys-color-on-surface)" accent="var(--md-sys-color-primary)" />
    <text x={g.balancer[0] + 18} y={g.balancer[1] - 11}>load</text>
    <text x={g.balancer[0] + 18} y={g.balancer[1] + 11}>balancer</text>
  </g>

  <!-- The key in the middle of the cluster -->
  <g class="centre">
    {#if key}
      <text x={g.centre[0]} y={g.centre[1] - 58} class="centre-label">{key.bucket.bucket_type ? `${key.bucket.bucket_type} / ` : ""}{key.bucket.bucket}</text>
      <g
        class="hover"
        role="img"
        aria-label="Key {key.key}"
        onmouseenter={(e) => key && key.key !== shortKey(key.key) && showTooltip(e.currentTarget, `Key: ${key.key}`)}
        onmouseleave={hideTooltip}
      >
        <rect x={g.centre[0] - chipWidth(keyLabel) / 2} y={g.centre[1] - 39} width={chipWidth(keyLabel)} height="46" rx="8" class="key-chip" />
        <text x={g.centre[0]} y={g.centre[1] - 16} class="key-text">{keyLabel}</text>
      </g>
      <g
        class="hover"
        role="button"
        tabindex="0"
        aria-label={valueText}
        onmouseenter={(e) => showTooltip(e.currentTarget, valueText)}
        onmouseleave={hideTooltip}
        onfocus={(e) => showTooltip(e.currentTarget, valueText)}
        onblur={hideTooltip}
      >
        <rect x={g.centre[0] - 48} y={g.centre[1] + 14} width="96" height="32" rx="16" class="value-chip" />
        <text x={g.centre[0]} y={g.centre[1] + 30} class="value-text">Value</text>
      </g>
      <text x={g.centre[0]} y={g.centre[1] + 66} class="centre-label">partitions {list(key.preflist.map((r) => r.partition))}</text>
    {/if}
  </g>

  <!-- Ports and FSMs: over the message lines, under the pulses. -->
  {#each nodes as n (n)}
    <g class="strip-wrap" class:down={down.has(n)}>{@render portStrip(n)}</g>
  {/each}
  {#each dots as d (d.m.id)}
    {#if live && d.m.label === "handoff"}
      <!-- Movers carry a partition's data to the new node. -->
      <g class="mover-stage">
        <rect x={d.x - 22} y={d.y - 22} width="44" height="44" rx="8" style:fill="var(--pix-paper)" style:stroke={sourceColour(d.m.from)} />
        <Pixels sprite={movers[step]} x={d.x - 19} y={d.y - 19} pixel={1.6} palette={PALETTE} />
      </g>
    {:else if live}
      <circle cx={d.x} cy={d.y} r="9" class="pulse" style:fill={sourceColour(d.m.from)} />
    {:else}
    <g
      class="msg"
      class:held={d.held}
      class:selected={selected === d.m.id}
      transform="translate({d.x},{d.y})"
      role="button"
      tabindex="0"
      aria-label="Message {d.m.label}. Select to hold or slow it."
      onclick={() => onselect(d.m.id)}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && onselect(d.m.id)}
    >
      <rect x={-pillWidth(d.m.label) / 2} y="-20" width={pillWidth(d.m.label)} height="40" rx="20" />
      <text>{d.m.label}</text>
    </g>
    {/if}
  {/each}
</svg>
</div>

<style>
  /* Fills the height its parent gives it, keeping its shape (the viewBox
     is centred). On a narrow window the parent has no set height, so the
     drawing takes the height that its width gives. */
  .fit {
    min-width: 0;
  }
  .fit.fill {
    position: relative;
    flex: 1 1 0;
    min-height: 0;
  }
  svg {
    width: 100%;
    height: auto;
    display: block;
  }
  .fill svg {
    position: absolute;
    inset: 0;
    height: 100%;
  }
  .corner {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
  }
  text {
    text-anchor: middle;
    dominant-baseline: central;
  }
  /* Outside the cluster: like a node card, so the text has full contrast. */
  .edge > rect {
    fill: var(--md-sys-color-surface-container-lowest);
    stroke: var(--md-sys-color-outline);
    stroke-width: var(--stroke-thick);
  }
  .edge text {
    font: var(--type-title-medium);
    fill: var(--md-sys-color-on-surface);
  }
  .card {
    fill: var(--md-sys-color-surface-container-lowest);
    stroke: var(--md-sys-color-outline-variant);
    stroke-width: var(--stroke-hairline);
  }
  .coordinator .card {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
  }
  .node-name {
    font: var(--type-title-large);
    text-anchor: start;
  }
  /* Icon buttons in the card's band. */
  .icon {
    cursor: pointer;
  }
  .icon .hit {
    fill: currentColor;
    opacity: 0;
  }
  .icon:hover .hit,
  .icon:focus-visible .hit {
    opacity: var(--state-hover);
  }
  .icon:focus-visible {
    outline: none;
  }
  .icon:focus-visible .hit {
    stroke: currentColor;
    stroke-width: var(--stroke-thick);
  }
  .icon .stroke {
    fill: none;
    stroke: currentColor;
    stroke-width: var(--stroke-thick);
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .icon .fill {
    fill: currentColor;
  }
  .controls .role {
    fill: currentColor;
  }
  .role {
    font: var(--type-label-large);
    text-anchor: end;
  }
  .tiles rect {
    transition: opacity var(--duration-medium) var(--motion-standard);
  }
  .tiles text {
    font: var(--type-title-medium);
    pointer-events: none;
  }
  .focused g:not(.replica) rect,
  .focused g:not(.replica) > text {
    opacity: var(--dim);
  }
  /* A stopped node: greyed out. */
  .node.down,
  .strip-wrap.down {
    opacity: var(--dim);
  }
  .tiles .off rect,
  .tiles .off > text {
    opacity: var(--dim);
  }
  .fallback > rect {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-thick);
    stroke-dasharray: 5 3;
  }
  .replica rect {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
  }
  .busy rect {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
    stroke-dasharray: 4 3;
  }
  .queue {
    fill: var(--md-sys-color-inverse-surface);
  }
  .tiles .queue-count {
    font: var(--type-label-medium);
    fill: var(--md-sys-color-inverse-on-surface);
  }
  .centre-label {
    font: var(--type-label-large);
    fill: var(--md-sys-color-on-surface-variant);
    /* A halo in the background colour, so message lines do not cross the text. */
    paint-order: stroke;
    stroke: var(--md-sys-color-surface-container);
    stroke-width: var(--space-1);
    stroke-linejoin: round;
  }
  .key-chip {
    fill: var(--md-sys-color-tertiary-container);
  }
  .hover {
    cursor: help;
  }
  .hover:focus-visible {
    outline: none;
  }
  .hover:focus-visible rect {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
  }
  .value-chip {
    fill: none;
    stroke: var(--md-sys-color-tertiary);
    stroke-width: var(--stroke-hairline);
  }
  .value-text {
    font: var(--type-label-large);
    fill: var(--md-sys-color-tertiary);
    text-decoration: underline dotted;
  }
  .key-text {
    font: var(--type-key);
    fill: var(--md-sys-color-on-tertiary-container);
  }
  .trail {
    stroke: var(--md-sys-color-outline);
    stroke-width: var(--stroke-thick);
    fill: none;
  }
  /* Live mode tiles: a dot for each stored key. */
  .divider {
    stroke-width: var(--stroke-hairline);
    stroke-dasharray: 2 3;
    opacity: 0.6;
  }
  .empty-dot {
    fill: none;
    stroke-width: var(--stroke-hairline);
    stroke-dasharray: 2 2;
    opacity: 0.6;
  }
  .full {
    cursor: help;
  }
  /* A vnode running a key fold (list keys): a moving dashed edge. */
  .folding > rect {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
    stroke-dasharray: 6 4;
    animation: fold var(--duration-long) linear infinite;
  }
  @keyframes fold {
    to {
      stroke-dashoffset: -20;
    }
  }
  /* A key with siblings: a conflict waiting to be resolved. */
  .dot.conflict {
    stroke: var(--md-sys-color-error);
    stroke-width: var(--stroke-thick);
  }
  .dot {
    cursor: help;
    transition:
      cx var(--duration-medium) var(--motion-standard),
      cy var(--duration-medium) var(--motion-standard),
      r var(--duration-medium) var(--motion-standard),
      opacity var(--duration-short) var(--motion-standard);
  }
  /* The same key in other vnodes, while one of its dots is under the pointer:
     the tiles that hold a copy get an outline, and everything else dims. */
  .spotlight g:not(.holds) > rect,
  .spotlight g:not(.holds) > text,
  .spotlight g:not(.holds) .dot,
  .spotlight .holds .dot:not(.same) {
    opacity: var(--dim);
  }
  .spotlight .holds > rect {
    opacity: 1;
    stroke: var(--md-sys-color-primary);
    stroke-width: var(--stroke-focus);
    stroke-dasharray: none;
  }
  .spotlight .holds > text {
    opacity: 1;
  }
  .dot.same {
    stroke: var(--md-sys-color-primary);
    stroke-width: var(--stroke-focus);
  }
  .trail.hidden {
    display: none;
  }
  /* Live mode. */
  .pulse {
    stroke: var(--md-sys-color-surface-container-lowest);
    stroke-width: var(--stroke-thick);
    pointer-events: none;
  }
  .zoomable {
    cursor: pointer;
  }
  .zoomed > rect:first-child {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
  }
  .streak.handoff {
    stroke-dasharray: 10 8;
  }
  .strip-bg {
    fill: var(--md-sys-color-surface-container-lowest);
    stroke: var(--md-sys-color-outline-variant);
    stroke-width: var(--stroke-thick);
  }
  .strip-rule {
    stroke: var(--md-sys-color-outline-variant);
    stroke-width: var(--stroke-hairline);
  }
  .part {
    fill: transparent;
  }
  .port,
  .fsm-none,
  .fsm-count {
    font: var(--type-port);
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
  }
  .port {
    fill: var(--md-sys-color-on-surface-variant);
  }
  .fsm-none {
    fill: var(--md-sys-color-outline);
  }
  .fsm-count {
    fill: var(--md-sys-color-on-surface);
  }
  .fsm {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-hairline);
    pointer-events: none;
  }
  .mover-stage rect {
    stroke-width: var(--stroke-thick);
  }
  .dist {
    fill: var(--md-sys-color-surface-container-lowest);
    stroke: var(--md-sys-color-outline);
    stroke-width: var(--stroke-thick);
  }
  .streak {
    fill: none;
    stroke-width: var(--stroke-focus);
    stroke-linecap: round;
    pointer-events: none;
  }
  .flash {
    fill: none;
    stroke-width: var(--stroke-focus);
    pointer-events: none;
  }
  /* The fading trail behind a moving message. */
  .wake {
    stroke: var(--md-sys-color-inverse-surface);
    stroke-linecap: round;
    pointer-events: none;
  }
  /* A message inside one node: no network. */
  .trail.local {
    stroke-dasharray: 3 6;
  }
  .trail.selected {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
  }
  .msg {
    cursor: pointer;
  }
  .msg rect {
    fill: var(--md-sys-color-inverse-surface);
  }
  .msg text {
    font: var(--type-code-xl);
    fill: var(--md-sys-color-inverse-on-surface);
  }
  .msg.held rect {
    fill: var(--md-sys-color-error-container);
    stroke: var(--md-sys-color-error);
    stroke-width: var(--stroke-thick);
  }
  .msg.held text {
    fill: var(--md-sys-color-on-error-container);
  }
  .msg.selected rect {
    stroke: var(--md-sys-color-primary);
    stroke-width: var(--stroke-focus);
  }
  .msg:focus-visible {
    outline: none;
  }
</style>
