<script lang="ts">
  // Messages in flight, with controls to hold, release or slow each one.
  import Button from "./Button.svelte";
  import { vnodeOf, type Endpoint, type Message, type RingInfo } from "../lib/sim.ts";

  let {
    messages,
    ring,
    now,
    selected,
    onselect,
    onhold,
    onrelease,
    onslow,
  }: {
    messages: Message[];
    ring: RingInfo;
    now: number;
    selected: number | null;
    onselect: (id: number) => void;
    onhold: (id: number) => void;
    onrelease: (id: number) => void;
    onslow: (id: number) => void;
  } = $props();

  const name = (e: Endpoint) =>
    e.kind === "client"
      ? "client"
      : e.kind === "balancer"
        ? "load balancer"
        : e.kind === "node"
          ? `node${e.id + 1}`
          : `${vnodeOf(e.id, ring).fallback ? "fallback vnode" : "vnode"} ${vnodeOf(e.id, ring).partition} on node${vnodeOf(e.id, ring).node + 1}`;
  const left = (m: Message) => (m.deliver === null ? "Held" : `Arrives in ${((m.deliver - now) / 1000).toFixed(2)} ms`);
  const owner = (e: Endpoint) => (e.kind === "node" ? e.id : e.kind === "vnode" ? vnodeOf(e.id, ring).node : -1);
  const hop = (m: Message) =>
    owner(m.from) >= 0 && owner(m.from) === owner(m.to)
      ? "Inside one node"
      : m.from.kind === "vnode" || m.to.kind === "vnode"
        ? `Network, through node${owner(m.from.kind === "vnode" ? m.from : m.to) + 1}`
        : "Network";
</script>

<section aria-labelledby="inflight-title">
  <h2 id="inflight-title">Messages in flight</h2>
  {#if messages.length === 0}
    <p class="empty">No messages in flight.</p>
  {:else}
    <ul>
      {#each messages as m (m.id)}
        <li class:selected={selected === m.id} class:held={m.deliver === null}>
          <button type="button" class="summary" onclick={() => onselect(m.id)} aria-pressed={selected === m.id}>
            <span class="label mono">{m.label}</span>
            <span class="route">{name(m.from)} → {name(m.to)}</span>
            <span class="meta">{hop(m)}. {left(m)}.</span>
          </button>
          <div class="actions">
            {#if m.deliver === null}
              <Button variant="text" onclick={() => onrelease(m.id)}>Release</Button>
            {:else}
              <Button variant="text" onclick={() => onhold(m.id)}>Hold</Button>
              <Button variant="text" onclick={() => onslow(m.id)}>Slow ×4</Button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container);
    min-width: 0;
  }
  h2 {
    margin: 0;
    font: var(--type-title-medium);
    letter-spacing: var(--tracking-title-medium);
  }
  .empty {
    margin: 0;
    color: var(--md-sys-color-on-surface-variant);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  li {
    border-radius: var(--shape-medium);
    background: var(--md-sys-color-surface-container-lowest);
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-3);
  }
  li.selected {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  li.held {
    outline: var(--stroke-thick) solid var(--md-sys-color-error);
  }
  .summary {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    width: 100%;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .label {
    font: var(--type-code-large);
  }
  .route {
    font: var(--type-body-medium);
  }
  .meta {
    font: var(--type-body-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
