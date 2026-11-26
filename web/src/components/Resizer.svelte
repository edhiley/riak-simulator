<script lang="ts">
  // A handle on one edge of a column. Drag it, or focus it and use the arrow
  // keys, to change the column width. Double-click to go back to the default.
  // The parent column must have position: relative.
  let {
    label,
    edge,
    width = $bindable(null),
    min,
    max,
  }: {
    label: string;
    /** The edge of the column that the handle is on. */
    edge: "left" | "right";
    /** Width in CSS pixels, or null for the default width. */
    width?: number | null;
    min: number;
    max: number;
  } = $props();

  let handle: HTMLDivElement | undefined = $state();
  let drag: { x: number; w: number } | null = null;
  const STEP = 16;

  const clamp = (w: number) => Math.round(Math.min(max, Math.max(min, w)));
  const current = () => width ?? handle?.parentElement?.getBoundingClientRect().width ?? min;
  // Dragging away from the column makes it wider.
  let sign = $derived(edge === "right" ? 1 : -1);

  function down(e: PointerEvent) {
    drag = { x: e.clientX, w: current() };
    handle?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (drag) width = clamp(drag.w + sign * (e.clientX - drag.x));
  }
  function up() {
    drag = null;
  }
  function key(e: KeyboardEvent) {
    const delta = e.key === "ArrowRight" ? STEP : e.key === "ArrowLeft" ? -STEP : 0;
    if (delta) {
      width = clamp(current() + sign * delta);
      e.preventDefault();
    } else if (e.key === "Home" || e.key === "End") {
      width = e.key === "Home" ? min : max;
      e.preventDefault();
    }
  }
</script>

<!-- A focusable separator is an interactive ARIA widget (window splitter). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={handle}
  class="resizer {edge}"
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={Math.round(current())}
  tabindex="0"
  title="Drag to change the width. Double-click to reset."
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  ondblclick={() => (width = null)}
  onkeydown={key}
></div>

<style>
  /* The hit area fills the gap between columns; the line shows on hover. */
  .resizer {
    position: absolute;
    top: 0;
    bottom: 0;
    width: var(--space-4);
    cursor: col-resize;
    touch-action: none;
    z-index: 1;
  }
  .resizer.right {
    right: calc(-1 * var(--space-4));
  }
  .resizer.left {
    left: calc(-1 * var(--space-4));
  }
  .resizer::after {
    content: "";
    position: absolute;
    top: var(--space-4);
    bottom: var(--space-4);
    left: 50%;
    width: var(--stroke-focus);
    margin-left: calc(-0.5 * var(--stroke-focus));
    border-radius: var(--shape-full);
    background: var(--md-sys-color-primary);
    opacity: 0;
    transition: opacity var(--duration-short) var(--motion-standard);
  }
  .resizer:hover::after,
  .resizer:focus-visible::after,
  .resizer:active::after {
    opacity: 1;
  }
  .resizer:focus-visible {
    outline: none;
  }
</style>
