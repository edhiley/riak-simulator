<script lang="ts">
  // Material 3 plain tooltip, placed above the hovered term and kept inside
  // the window.
  import { hideTooltip, tooltip } from "../lib/tooltip.svelte.ts";

  let el: HTMLDivElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);

  // The tooltip is placed once, so hide it when anything scrolls.
  $effect(() => {
    document.addEventListener("scroll", hideTooltip, true);
    return () => document.removeEventListener("scroll", hideTooltip, true);
  });

  // Centre over the term, then move it back inside the window. If there is
  // no room above the term, put it below. It is a popover, so it shows in
  // the top layer, above an open popup (a modal dialog), not behind it.
  $effect(() => {
    if (!tooltip.visible || !el) return;
    try {
      if (!el.matches(":popover-open")) el.showPopover();
    } catch {
      /* no popover support: it still shows outside popups */
    }
    const margin = 8;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    left = Math.min(Math.max(tooltip.x - w / 2, margin), window.innerWidth - w - margin);
    top = tooltip.y - h - margin >= margin ? tooltip.y - h - margin : tooltip.bottom + margin;
  });
</script>

{#if tooltip.visible}
  <div bind:this={el} id="term-tooltip" role="tooltip" popover="manual" style:left="{left}px" style:top="{top}px">{tooltip.text}</div>
{/if}

<style>
  div {
    position: fixed;
    /* Undo the browser's popover placement; left and top come from above. */
    inset: auto;
    margin: 0;
    border: none;
    overflow: visible;
    max-width: var(--size-tooltip);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--shape-extra-small);
    background: var(--md-sys-color-inverse-surface);
    color: var(--md-sys-color-inverse-on-surface);
    font: var(--type-body-small);
    pointer-events: none;
    z-index: 10;
  }
</style>
