<script lang="ts">
  // Every step so far, in order, newest at the bottom.
  import Terms from "./Terms.svelte";
  import type { SimEvent } from "../lib/sim.ts";

  let { events }: { events: SimEvent[] } = $props();
  let el: HTMLElement | undefined = $state();

  $effect(() => {
    void events.length;
    el?.scrollTo({ top: el.scrollHeight });
  });

  const ms = (t: number) => `${(t / 1000).toFixed(1)} ms`;
</script>

<section aria-labelledby="log-title">
  <h2 id="log-title">Log</h2>
  <ol bind:this={el} aria-live="polite">
    {#each events as e, i (i)}
      <li class:summary={e.summary}>
        <span class="mono time">{ms(e.at)}</span><span><Terms text={e.summary ? `Result: ${e.text}` : e.text} /></span>
      </li>
    {:else}
      <li class="empty">No steps yet. Run a command.</li>
    {/each}
  </ol>
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  h2 {
    margin: 0;
    font: var(--type-title-medium);
    letter-spacing: var(--tracking-title-medium);
  }
  ol {
    list-style: none;
    margin: 0;
    padding: var(--space-3);
    max-height: var(--size-log-max);
    overflow-y: auto;
    border-radius: var(--shape-small);
    background: var(--md-sys-color-surface-container-lowest);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  li {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-3);
    font: var(--type-body-small);
  }
  .summary {
    font-weight: 500;
  }
  .time {
    font: var(--type-code-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  .empty {
    display: block;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
