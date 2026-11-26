<script lang="ts">
  // Ring view. Partitions are arcs coloured by owning node; the nodes and
  // the client are a row below the ring. Every message in flight is drawn
  // at its position for the current simulation time.
  import { arc } from "d3-shape";
  import { arcStart, pointAt, ringGeometry, route } from "../lib/layout.ts";
  import { progressAt, type KeyInfo, type RequestView, type RingInfo, type Snapshot } from "../lib/sim.ts";

  let {
    ring,
    snapshot,
    now,
    key,
    request,
    selected,
    onselect,
    ringOnly = false,
  }: {
    ring: RingInfo;
    snapshot: Snapshot;
    now: number;
    key: KeyInfo | null;
    request: RequestView | null;
    selected: number | null;
    onselect: (id: number) => void;
    /** Draw only the ring and the key, for the explainer. */
    ringOnly?: boolean;
  } = $props();

  const TAU = Math.PI * 2;
  let g = $derived(ringGeometry(ring));
  let replicas = $derived(new Set(key?.preflist.map((r) => r.partition) ?? []));

  let arcs = $derived.by(() => {
    const step = TAU / ring.size;
    const shape = arc<number>()
      .innerRadius(g.rIn)
      .outerRadius(g.rOut)
      .padAngle(ring.size > 64 ? 0.002 : 0.006)
      .startAngle((p) => arcStart(p, ring.size))
      .endAngle((p) => arcStart(p, ring.size) + step);
    return ring.owners.map((owner, p) => ({
      p,
      owner,
      d: shape(p)!,
      label: pointAt(arcStart(p, ring.size) + step / 2, (g.rIn + g.rOut) / 2),
    }));
  });

  let walk = $derived.by(() => {
    if (!key) return null;
    const start = key.position * TAU;
    const last = key.preflist[key.preflist.length - 1].partition;
    let end = arcStart(last, ring.size) + TAU / ring.size;
    while (end < start) end += TAU;
    // A stroked arc, clockwise, so it can end in an arrowhead.
    const r = g.rOut + 16;
    const [x1, y1] = pointAt(start, r);
    const [x2, y2] = pointAt(end, r);
    const large = end - start > Math.PI ? 1 : 0;
    return `M${x1},${y1}A${r},${r} 0 ${large} 1 ${x2},${y2}`;
  });
  let tick = $derived(key ? [pointAt(key.position * TAU, g.rIn - 8), pointAt(key.position * TAU, g.rOut + 16)] : null);
  let hashLabel = $derived(key ? pointAt(key.position * TAU, g.rOut + 38) : null);

  // Mailbox items stack outward from each partition.
  let mail = $derived(
    snapshot.vnodes.flatMap((v) => {
      const angle = arcStart(v.partition, ring.size) + Math.PI / ring.size;
      return v.mailbox.map((m, i) => ({ ...m, pos: pointAt(angle, g.rOut + 30 + i * 12) }));
    }),
  );
  let busy = $derived(new Set(snapshot.vnodes.filter((v) => v.busy).map((v) => v.partition)));

  let dots = $derived(
    snapshot.messages.map((m) => {
      const r = route(m.from, m.to, ring, g);
      const [x, y] = r.at(progressAt(m, now));
      const owner = (e: typeof m.from) => (e.kind === "node" ? e.id : e.kind === "vnode" ? ring.owners[e.id] : -1);
      const local = owner(m.from) >= 0 && owner(m.from) === owner(m.to);
      return { m, x, y, path: r.d, held: m.deliver === null, local };
    }),
  );

  const pillWidth = (label: string) => 32 + label.length * 13.5;
</script>

<svg viewBox={ringOnly ? "-300 -300 600 600" : g.viewBox} role="img" aria-label="Ring of {ring.size} partitions over {ring.nodes} nodes">
  <g class="partitions" class:focused={key !== null}>
    {#each arcs as a (a.p)}
      <path d={a.d} style:fill="var(--node-{a.owner})" class:replica={replicas.has(a.p)} class:busy={busy.has(a.p)}>
        <title>Partition {a.p}, owned by node{a.owner + 1}</title>
      </path>
      {#if ring.size <= 32}
        <text x={a.label[0]} y={a.label[1]} class="arc-label" class:replica={replicas.has(a.p)} style:fill="var(--node-{a.owner}-on)">{a.p}</text>
      {/if}
    {/each}
  </g>

  {#if key && walk && tick && hashLabel}
    <defs>
      <marker id="walk-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
        <path d="M0,0L10,5L0,10z" class="arrow" />
      </marker>
    </defs>
    <g class="hash-marker">
      <title>The key's hash is this position on the ring. Riak stores the key on the 3 partitions that follow it, clockwise.</title>
      <line x1={tick[0][0]} y1={tick[0][1]} x2={tick[1][0]} y2={tick[1][1]} class="tick" />
      <path d={walk} class="walk" marker-end="url(#walk-arrow)" />
      <text x={hashLabel[0]} y={hashLabel[1]} class="hash-label">hash</text>
    </g>
  {/if}

  {#each mail as m (m.id)}
    <circle cx={m.pos[0]} cy={m.pos[1]} r="4" class="mail"><title>Waiting in the mailbox: {m.label}</title></circle>
  {/each}

  <g class="centre">
    {#if key}
      <text y="-28" class="centre-label">{key.bucket.bucket_type ? `${key.bucket.bucket_type} / ` : ""}{key.bucket.bucket}</text>
      <rect x={-(key.key.length * 17 + 28) / 2} y="-19" width={key.key.length * 17 + 28} height="46" rx="8" class="key-chip" />
      <text y="4" class="centre-key key-text">{key.key}</text>
      <text y="30" class="centre-hash">{key.hash.slice(0, 8)}…</text>
    {:else}
      <text y="-6" class="centre-key">{ring.size} partitions</text>
      <text y="20" class="centre-label">{ring.nodes} nodes</text>
    {/if}
  </g>

  {#if !ringOnly}
  <g class="row">
    <rect x={g.client[0] - g.clientW / 2} y={g.client[1] - g.nodeH / 2} width={g.clientW} height={g.nodeH} rx="12" class="client" />
    <text x={g.client[0]} y={g.client[1]} class="row-label">client</text>
    <rect x={g.balancer[0] - g.balancerW / 2} y={g.balancer[1] - g.nodeH / 2} width={g.balancerW} height={g.nodeH} rx="12" class="client" />
    <text x={g.balancer[0]} y={g.balancer[1]} class="row-label">load balancer</text>
    {#each ring.members as n (n)}
      <rect
        x={g.nodeX(n) - g.nodeW / 2}
        y={g.nodeY - g.nodeH / 2}
        width={g.nodeW}
        height={g.nodeH}
        rx="12"
        style:fill="var(--node-{n})"
        class:origin={request?.origin === n}
        class:coordinator={request?.coordinator === n}
      />
      <text x={g.nodeX(n)} y={g.nodeY - 9} class="row-label" style:fill="var(--node-{n}-on)">node{n + 1}</text>
      <text x={g.nodeX(n)} y={g.nodeY + 13} class="row-sub" style:fill="var(--node-{n}-on)">{ring.owners.filter((o) => o === n).length} vnodes</text>
      {#if request?.coordinator === n}
        <text x={g.nodeX(n)} y={g.nodeY - g.nodeH / 2 - 10} class="role">coordinator</text>
      {:else if request?.origin === n}
        <text x={g.nodeX(n)} y={g.nodeY - g.nodeH / 2 - 10} class="role">received</text>
      {/if}
    {/each}
  </g>

  {#each dots as d (d.m.id)}
    <path d={d.path} class="trail" class:local={d.local} class:selected={selected === d.m.id} />
  {/each}
  {#each dots as d (d.m.id)}
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
  {/each}
  {/if}
</svg>

<style>
  svg {
    width: 100%;
    height: auto;
    max-width: var(--size-ring-max);
    /* Leave room for the toolbar above, so the load balancer row stays in view. */
    max-height: calc(100vh - var(--space-12) * 3);
    display: block;
    margin: 0 auto;
  }
  path {
    transition: opacity var(--duration-medium) var(--motion-standard);
  }
  .partitions path {
    stroke: var(--md-sys-color-surface);
    stroke-width: var(--stroke-hairline);
  }
  .focused path:not(.replica),
  .focused .arc-label:not(.replica) {
    opacity: var(--dim);
  }
  .partitions path.replica {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-thick);
  }
  .partitions path.busy {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
    stroke-dasharray: 4 3;
  }
  .arc-label {
    font: var(--type-label-large);
    text-anchor: middle;
    dominant-baseline: central;
    pointer-events: none;
  }
  .mail,
  .arrow {
    fill: var(--md-sys-color-on-surface);
  }
  .walk {
    fill: none;
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-thick);
  }
  .hash-label {
    font: var(--type-label-large);
    text-anchor: middle;
    dominant-baseline: central;
    fill: var(--md-sys-color-on-surface);
  }
  .tick {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-thick);
  }
  .centre text {
    text-anchor: middle;
    fill: var(--md-sys-color-on-surface);
  }
  .key-chip {
    fill: var(--md-sys-color-tertiary-container);
  }
  .centre .key-text {
    font: var(--type-key);
    fill: var(--md-sys-color-on-tertiary-container);
  }
  .centre-key {
    font: var(--type-headline-small);
  }
  .centre-label {
    font: var(--type-label-large);
    fill: var(--md-sys-color-on-surface-variant);
  }
  .centre-hash {
    font: var(--type-code);
    fill: var(--md-sys-color-on-surface-variant);
  }
  .client {
    fill: var(--md-sys-color-surface-container-highest);
  }
  .row rect {
    stroke: none;
  }
  .row rect.origin {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-hairline);
  }
  .row rect.coordinator {
    stroke: var(--md-sys-color-on-surface);
    stroke-width: var(--stroke-focus);
  }
  .row-label {
    font: var(--type-title-medium);
    text-anchor: middle;
    dominant-baseline: central;
    fill: var(--md-sys-color-on-surface);
  }
  .row-sub {
    font: var(--type-label-large);
    text-anchor: middle;
    dominant-baseline: central;
  }
  .role {
    font: var(--type-label-large);
    text-anchor: middle;
    fill: var(--md-sys-color-on-surface-variant);
  }
  .trail {
    stroke: var(--md-sys-color-outline-variant);
    stroke-width: var(--stroke-hairline);
    fill: none;
  }
  /* A message inside one node: no network, about ten times faster. */
  .trail.local {
    stroke-dasharray: 2 4;
  }
  .trail.selected {
    stroke: var(--md-sys-color-on-surface);
  }
  .msg {
    cursor: pointer;
  }
  .msg rect {
    fill: var(--md-sys-color-inverse-surface);
  }
  .msg text {
    font: var(--type-code-xl);
    text-anchor: middle;
    dominant-baseline: central;
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
