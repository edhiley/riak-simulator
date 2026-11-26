<script lang="ts">
  // "What happened?": the steps of the last command, each told from the
  // side of the node or vnode that got the message, and why. The page pauses
  // the simulation while this is open.
  import Button from "./Button.svelte";
  import Terms from "./Terms.svelte";
  import type { SimEvent } from "../lib/sim.ts";

  let {
    command,
    events,
    onopen,
    onclose,
  }: {
    /** The last command, as typed or as a button would type it. */
    command: string | null;
    events: SimEvent[];
    onopen: () => void;
    onclose: () => void;
  } = $props();

  let dialog: HTMLDialogElement | undefined = $state();
  let steps = $derived(events.filter((e) => !e.summary));
  let result = $derived(events.find((e) => e.summary) ?? null);

  export function open() {
    onopen();
    dialog?.showModal();
  }
</script>

<dialog bind:this={dialog} aria-labelledby="what-title" onclose={onclose} onclick={(e) => e.target === dialog && dialog?.close()}>
  <div class="content">
    <h2 id="what-title">What happened</h2>
    {#if command}
      <pre class="command">$ {command}</pre>
    {/if}
    {#if steps.length === 0}
      <p class="empty">Run a command first. This shows each step of the last one.</p>
    {:else}
      <ol>
        {#each steps as s, i (i)}
          <li><Terms text={s.text} /></li>
        {/each}
      </ol>
      {#if result}
        <div class="result"><h3>Result</h3><p><Terms text={result.text} /></p></div>
      {:else}
        <p class="empty">The command is still running. The simulation is paused while this is open.</p>
      {/if}
    {/if}
    <div class="actions">
      <Button onclick={() => dialog?.close()}>Close and continue</Button>
    </div>
  </div>
</dialog>

<style>
  dialog {
    width: min(var(--size-dialog), calc(100vw - var(--space-8)));
    max-height: calc(100vh - var(--space-8));
    padding: 0;
    border: none;
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }
  dialog::backdrop {
    background: var(--md-sys-color-scrim);
    opacity: 0.32;
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-6);
  }
  h2 {
    margin: 0;
    font: var(--type-headline-small);
  }
  h3 {
    margin: 0 0 var(--space-1);
    font: var(--type-title-small);
  }
  .command {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--shape-small);
    background: var(--md-sys-color-surface-container-lowest);
    font: var(--type-code);
    white-space: pre-wrap;
    word-break: break-all;
  }
  ol {
    margin: 0;
    padding-left: var(--space-6);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    font: var(--type-body-large);
  }
  .result {
    padding: var(--space-3);
    border-radius: var(--shape-medium);
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  .result p,
  .empty {
    margin: 0;
  }
  .empty {
    color: var(--md-sys-color-on-surface-variant);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
