<script lang="ts">
  // Two pictures. First, the classic "pick two" triangle: Consistency,
  // Availability and Partition tolerance, with Riak on the AP side.
  // Second, a network split: 3 nodes on one side, 2 on the other. A client on
  // each side writes alice, so the two sides have different versions.
  const left = [0, 1, 2];
  const right = [3, 4];
  const W = 120;
  const H = 52;
  const nodeY = (i: number) => 120 + i * 74;

  // Triangle corners.
  const C = { x: 280, y: 72 };
  const A = { x: 110, y: 256 };
  const P = { x: 450, y: 256 };
  const R = 34;
</script>

<div class="figures">
  <svg
    viewBox="0 0 560 330"
    role="img"
    aria-label="The CAP triangle. Consistency at the top, Availability at the bottom left, Partition tolerance at the bottom right. A database can pick two. CA needs a network that never splits, so only one server, such as PostgreSQL. CP: CockroachDB, etcd and Google Spanner. AP: Riak, Cassandra and ScyllaDB."
  >
    <!-- Edges: each one is a pair you can pick. Riak's pair is solid. -->
    <line x1={C.x} y1={C.y} x2={A.x} y2={A.y} class="edge" />
    <line x1={C.x} y1={C.y} x2={P.x} y2={P.y} class="edge" />
    <line x1={A.x} y1={A.y} x2={P.x} y2={P.y} class="edge riak" />

    <text x="168" y="112" class="pair end">CA</text>
    <text x="168" y="134" class="caption end">one server only,</text>
    <text x="168" y="154" class="caption end">such as PostgreSQL</text>
    <text x="392" y="112" class="pair start">CP</text>
    <text x="392" y="134" class="caption start">CockroachDB, etcd,</text>
    <text x="392" y="154" class="caption start">Google Spanner</text>
    <text x="280" y="212" class="pair riak-text">AP: Riak</text>
    <text x="280" y="234" class="caption">Cassandra, ScyllaDB</text>

    <!-- Corners -->
    <circle cx={C.x} cy={C.y} r={R} class="corner" />
    <text x={C.x} y={C.y} class="letter">C</text>
    <text x={C.x} y={C.y - R - 14} class="word">Consistency</text>

    <circle cx={A.x} cy={A.y} r={R} class="corner picked" />
    <text x={A.x} y={A.y} class="letter picked-text">A</text>
    <text x={A.x} y={A.y + R + 18} class="word">Availability</text>

    <circle cx={P.x} cy={P.y} r={R} class="corner picked" />
    <text x={P.x} y={P.y} class="letter picked-text">P</text>
    <text x={P.x} y={P.y + R + 18} class="word">Partition tolerance</text>

    <text x="280" y="158" class="caption">pick two</text>
  </svg>

  <svg viewBox="0 0 560 380" role="img" aria-label="A network split. node1, node2 and node3 are on one side, node4 and node5 on the other. A client on each side writes a different value of alice.">
    <!-- The split -->
    <line x1="300" y1="20" x2="300" y2="360" class="split" />
    <text x="300" y="370" class="caption">network split</text>

    <!-- Clients -->
    <rect x="80" y="20" width={W} height="44" rx="12" class="client" />
    <text x="140" y="42">client A</text>
    <rect x="380" y="20" width={W} height="44" rx="12" class="client" />
    <text x="440" y="42">client B</text>
    <text x="140" y="88" class="write">alice = Alice</text>
    <text x="440" y="88" class="write">alice = Al</text>

    {#each left as n, i (n)}
      <rect x="80" y={nodeY(i)} width={W} height={H} rx="14" style:fill="var(--node-{n})" />
      <text x="140" y={nodeY(i) + H / 2} style:fill="var(--node-{n}-on)" class="node">node{n + 1}</text>
    {/each}
    {#each right as n, i (n)}
      <rect x="380" y={nodeY(i)} width={W} height={H} rx="14" style:fill="var(--node-{n})" />
      <text x="440" y={nodeY(i) + H / 2} style:fill="var(--node-{n}-on)" class="node">node{n + 1}</text>
    {/each}
    <text x="440" y={nodeY(2) + H / 2} class="caption">Both sides reply.</text>
    <text x="440" y={nodeY(2) + H / 2 + 22} class="caption">Riak fixes it later.</text>
  </svg>
</div>

<style>
  .figures {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  svg {
    width: 100%;
    height: auto;
    display: block;
  }
  text {
    text-anchor: middle;
    dominant-baseline: central;
    font: var(--type-title-medium);
    fill: var(--md-sys-color-on-surface);
  }
  .client {
    fill: var(--md-sys-color-surface-container-highest);
  }
  .split {
    stroke: var(--md-sys-color-error);
    stroke-width: var(--stroke-thick);
    stroke-dasharray: 8 6;
  }
  .caption {
    font: var(--type-label-large);
    fill: var(--md-sys-color-on-surface-variant);
  }
  .write {
    font: var(--type-code-xl);
  }
  /* The triangle */
  .edge {
    stroke: var(--md-sys-color-outline);
    stroke-width: var(--stroke-thick);
    stroke-dasharray: 6 5;
  }
  .edge.riak {
    stroke: var(--md-sys-color-primary);
    stroke-width: var(--stroke-focus);
    stroke-dasharray: none;
  }
  .corner {
    fill: var(--md-sys-color-surface-container-highest);
    stroke: var(--md-sys-color-outline);
    stroke-width: var(--stroke-thick);
  }
  .corner.picked {
    fill: var(--md-sys-color-primary);
    stroke: var(--md-sys-color-primary);
  }
  .letter {
    font: var(--type-headline-small);
  }
  .picked-text {
    fill: var(--md-sys-color-on-primary);
  }
  .word {
    font: var(--type-title-medium);
  }
  .pair {
    font: var(--type-title-medium);
  }
  .end {
    text-anchor: end;
  }
  .start {
    text-anchor: start;
  }
  .riak-text {
    fill: var(--md-sys-color-primary);
  }
</style>
