<script lang="ts">
  // "What are partitions?": the ring model, shown on request. The main view
  // shows nodes; this ties it back to the hash ring.
  import Button from "./Button.svelte";
  import Ring from "./Ring.svelte";
  import Terms from "./Terms.svelte";
  import type { KeyInfo, RingInfo } from "../lib/sim.ts";

  let { ring, key }: { ring: RingInfo; key: KeyInfo | null } = $props();

  let dialog: HTMLDialogElement | undefined = $state();
  const empty = { now: 0, messages: [], vnodes: [], requests: [], idle: true, down: [], members: [], leaving: [], ring_version: 0 };

  export function open() {
    dialog?.showModal();
  }

  const list = (items: string[]) =>
    items.length <= 1 ? items.join("") : `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
</script>

<dialog bind:this={dialog} aria-labelledby="placement-title" onclick={(e) => e.target === dialog && dialog?.close()}>
  <div class="content">
    <h2 id="placement-title">What are partitions?</h2>
    <div class="body">
      <div class="ring">
        <Ring {ring} snapshot={empty} now={0} {key} request={null} selected={null} onselect={() => {}} ringOnly />
      </div>
      <ol>
        <li><Terms text="Riak calculates the SHA-1 hash of the bucket and the key. The hash is a number from 0 to 2^160 − 1." /></li>
        <li><Terms text={`Riak arranges these numbers in a circle, called the ring. The ring has ${ring.size} equal partitions.`} /></li>
        <li><Terms text="The hash is one position on the ring. Riak keeps a copy of the object in each of the 3 partitions that follow that position, clockwise. The number of copies is n_val." /></li>
        <li><Terms text="Each partition is a vnode, and each vnode runs on one node. The colours show which node. Riak gives each node an equal share of the partitions, and puts neighbouring partitions on different nodes. So the 3 copies are on 3 different nodes." /></li>
        {#if key}
          <li>
            <Terms
              highlight={key.key}
              text={`For ${key.key}, the partitions are ${list(key.preflist.map((r) => String(r.partition)))}, on ${list([...new Set(key.preflist.map((r) => `node${r.node + 1}`))])}.`}
            />
          </li>
        {/if}
      </ol>
    </div>
    <div class="actions">
      <Button onclick={() => dialog?.close()}>Close</Button>
    </div>
  </div>
</dialog>

<style>
  dialog {
    max-width: min(var(--size-dialog), calc(100vw - var(--space-8)));
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
    gap: var(--space-4);
    padding: var(--space-6);
  }
  h2 {
    margin: 0;
    font: var(--type-headline-small);
  }
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-6);
    align-items: center;
  }
  @media (max-width: 52rem) {
    .body {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .ring :global(svg) {
    max-height: none;
  }
  ol {
    margin: 0;
    padding-left: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    font: var(--type-body-large);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
