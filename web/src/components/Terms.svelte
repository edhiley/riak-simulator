<script lang="ts">
  // Text with defined terms marked. Hover or focus a term to see its
  // definition. The current key, if given, is shown as a key chip.
  import { markup, type Segment } from "../lib/glossary.ts";
  import { hideTooltip, showTooltip } from "../lib/tooltip.svelte.ts";
  import { shortKey } from "../lib/text.ts";

  let { text, highlight = null }: { text: string; highlight?: string | null } = $props();

  let segments = $derived.by(() => {
    if (!highlight) return markup(text);
    const out: (Segment & { key?: boolean })[] = [];
    const escaped = highlight.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const parts = text.split(new RegExp(`(?<![\\w/-])(${escaped})(?![\\w/-])`));
    parts.forEach((part, i) => {
      if (i % 2 === 1) out.push({ text: part, key: true });
      else if (part) out.push(...markup(part));
    });
    return out;
  });

  // node3 is drawn in the colour of node3's card.
  const nodeIndex = (t: string) => {
    const m = /^node(\d+)$/.exec(t);
    return m ? Number(m[1]) - 1 : null;
  };
</script>

{#each segments as s, i (i)}{#if "key" in s && s.key}<span class="key mono" title={shortKey(s.text) === s.text ? undefined : s.text}>{shortKey(s.text)}</span>{:else if s.definition}{@const n = nodeIndex(s.text)}<button
      type="button"
      class="term"
      class:node={n !== null}
      style:background={n !== null ? `var(--node-${n})` : undefined}
      style:color={n !== null ? `var(--node-${n}-on)` : undefined}
      aria-describedby="term-tooltip"
      onmouseenter={(e) => showTooltip(e.currentTarget, s.definition!)}
      onmouseleave={hideTooltip}
      onfocus={(e) => showTooltip(e.currentTarget, s.definition!)}
      onblur={hideTooltip}>{s.text}</button
    >{:else}{s.text}{/if}{/each}

<style>
  /* The key the current request is about, the same as in the ring centre. */
  .key {
    padding: 0 var(--space-1);
    border-radius: var(--shape-extra-small);
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
    font: var(--type-code);
    font-weight: 500;
  }
  .term {
    display: inline;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    text-decoration: underline dotted;
    text-decoration-color: var(--md-sys-color-outline);
    text-underline-offset: var(--stroke-focus);
    cursor: help;
  }
  /* A node name, in its node's colour. */
  .term.node {
    padding: 0 var(--space-1);
    border-radius: var(--shape-extra-small);
    font-weight: 500;
    text-decoration: none;
  }
  .term.node:hover,
  .term.node:focus-visible {
    outline: var(--stroke-hairline) solid var(--md-sys-color-on-surface);
  }
  .term:hover,
  .term:focus-visible {
    text-decoration-color: var(--md-sys-color-on-surface);
  }
</style>
