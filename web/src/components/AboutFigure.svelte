<script lang="ts">
  // How the simulator is built: the OpenRiak source and a real node feed the
  // Rust core; the core runs as WebAssembly; the page drives it and draws it.
  const BOX_W = 240;
  const L = 30;
  const Rx = 290;
  const CORE = { x: 60, y: 168, w: 440, h: 92 };
  const WASM = { x: 60, y: 312, w: 440, h: 52 };
  const PAGE = { x: 60, y: 416, w: 440, h: 92 };
</script>

<svg
  viewBox="0 0 560 530"
  role="img"
  aria-label="How the simulator is built. The OpenRiak 3.4.1 source is read and modelled in the core. A real OpenRiak node checks the core's key placement. The core is written in Rust and compiled to WebAssembly. The page sends your inputs to it and reads a snapshot on each frame."
>
  <defs>
    <marker id="about-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
      <path d="M0 0 L10 5 L0 10 z" class="head" />
    </marker>
  </defs>

  <!-- Sources -->
  <rect x={L} y="16" width={BOX_W} height="76" rx="12" class="source" />
  <text x={L + BOX_W / 2} y="42" class="title on-source">OpenRiak 3.4.1 source</text>
  <text x={L + BOX_W / 2} y="68" class="small on-source">riak_kv · riak_core · leveled</text>

  <rect x={Rx} y="16" width={BOX_W} height="76" rx="12" class="source" />
  <text x={Rx + BOX_W / 2} y="42" class="title on-source">A real OpenRiak node</text>
  <text x={Rx + BOX_W / 2} y="68" class="small on-source">in Docker</text>

  <line x1={L + BOX_W / 2} y1="94" x2={L + BOX_W / 2} y2={CORE.y - 4} class="arrow" marker-end="url(#about-arrow)" />
  <text x={L + BOX_W / 2 + 10} y="130" class="label start">read and modelled</text>
  <line x1={Rx + BOX_W / 2} y1="94" x2={Rx + BOX_W / 2} y2={CORE.y - 4} class="arrow check" marker-end="url(#about-arrow)" />
  <text x={Rx + BOX_W / 2 + 10} y="130" class="label start">checks key placement</text>

  <!-- Core -->
  <rect x={CORE.x} y={CORE.y} width={CORE.w} height={CORE.h} rx="14" class="core" />
  <text x="280" y={CORE.y + 26} class="title on-core">Core (Rust)</text>
  <text x="280" y={CORE.y + 52} class="small on-core">nodes · vnodes · mailboxes · network</text>
  <text x="280" y={CORE.y + 74} class="small on-core">simulation clock · leveled</text>

  <line x1="280" y1={CORE.y + CORE.h + 2} x2="280" y2={WASM.y - 4} class="arrow" marker-end="url(#about-arrow)" />
  <text x="290" y={CORE.y + CORE.h + 26} class="label start">compiled</text>

  <!-- WebAssembly -->
  <rect x={WASM.x} y={WASM.y} width={WASM.w} height={WASM.h} rx="12" class="wasm" />
  <text x="280" y={WASM.y + WASM.h / 2} class="title on-wasm">WebAssembly (wasm-bindgen)</text>

  <!-- Two ways between the page and the core -->
  <line x1="200" y1={PAGE.y - 2} x2="200" y2={WASM.y + WASM.h + 4} class="arrow" marker-end="url(#about-arrow)" />
  <text x="190" y={WASM.y + WASM.h + 26} class="label end">your inputs</text>
  <line x1="360" y1={WASM.y + WASM.h + 2} x2="360" y2={PAGE.y - 4} class="arrow" marker-end="url(#about-arrow)" />
  <text x="370" y={WASM.y + WASM.h + 26} class="label start">a snapshot each frame</text>

  <!-- Page -->
  <rect x={PAGE.x} y={PAGE.y} width={PAGE.w} height={PAGE.h} rx="14" class="page" />
  <text x="280" y={PAGE.y + 26} class="title on-page">Page (Svelte, TypeScript)</text>
  <text x="280" y={PAGE.y + 52} class="small on-page">draws every message in flight</text>
  <text x="280" y={PAGE.y + 74} class="small on-page">requests · scenarios · console</text>
</svg>

<style>
  svg {
    width: 100%;
    height: auto;
    display: block;
  }
  text {
    text-anchor: middle;
    dominant-baseline: central;
    fill: var(--md-sys-color-on-surface);
  }
  .title {
    font: var(--type-title-medium);
  }
  .small,
  .label {
    font: var(--type-label-large);
  }
  .label {
    fill: var(--md-sys-color-on-surface-variant);
  }
  .start {
    text-anchor: start;
  }
  .end {
    text-anchor: end;
  }
  .source {
    fill: var(--md-sys-color-secondary-container);
  }
  .on-source {
    fill: var(--md-sys-color-on-secondary-container);
  }
  .core {
    fill: var(--md-sys-color-primary);
  }
  .on-core {
    fill: var(--md-sys-color-on-primary);
  }
  .wasm {
    fill: var(--md-sys-color-surface-container-highest);
    stroke: var(--md-sys-color-outline);
    stroke-width: var(--stroke-thick);
  }
  .on-wasm {
    fill: var(--md-sys-color-on-surface);
  }
  .page {
    fill: var(--md-sys-color-tertiary-container);
  }
  .on-page {
    fill: var(--md-sys-color-on-tertiary-container);
  }
  .arrow {
    stroke: var(--md-sys-color-outline);
    stroke-width: var(--stroke-thick);
  }
  .arrow.check {
    stroke-dasharray: 6 4;
  }
  .head {
    fill: var(--md-sys-color-outline);
  }
</style>
