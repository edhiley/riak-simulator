<script lang="ts">
  // The left column: make traffic. Four tabs, so each set of controls fits
  // without scrolling and the cluster stays in view. Requests sends a number
  // of one request; Traffic sends a steady rate until stopped; Data updates
  // counters, sets and siblings; Scenarios run a whole workload. A dot on a
  // tab shows that its traffic is still running.
  import { scenarios, type Scenario } from "../lib/scenarios.ts";
  import Send from "@lucide/svelte/icons/send";
  import Gauge from "@lucide/svelte/icons/gauge";
  import Shapes from "@lucide/svelte/icons/shapes";
  import Clapperboard from "@lucide/svelte/icons/clapperboard";
  import PenLine from "@lucide/svelte/icons/pen-line";
  import BookOpen from "@lucide/svelte/icons/book-open";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Radio from "@lucide/svelte/icons/radio";
  import ListTree from "@lucide/svelte/icons/list-tree";
  import Copy from "@lucide/svelte/icons/copy";
  import Check from "@lucide/svelte/icons/check";
  import Play from "@lucide/svelte/icons/play";
  import Square from "@lucide/svelte/icons/square";
  import Hash from "@lucide/svelte/icons/hash";
  import Blend from "@lucide/svelte/icons/blend";
  import GitFork from "@lucide/svelte/icons/git-fork";
  import GitMerge from "@lucide/svelte/icons/git-merge";
  import Milk from "@lucide/svelte/icons/milk";
  import Wheat from "@lucide/svelte/icons/wheat";
  import Plus from "@lucide/svelte/icons/plus";
  import Minus from "@lucide/svelte/icons/minus";
  import ShoppingBag from "@lucide/svelte/icons/shopping-bag";
  import Trophy from "@lucide/svelte/icons/trophy";
  import MessageCircle from "@lucide/svelte/icons/message-circle";
  import type { Component } from "svelte";

  export type BatchOp = "write" | "read" | "delete" | "ping";
  export type Tab = "requests" | "traffic" | "data" | "scenarios";

  let {
    stored,
    onbatch,
    onlistkeys,
    example,
    rate = $bindable(),
    readShare = $bindable(),
    onScreen,
    trafficOn,
    ontraffic,
    counter,
    basket,
    cart,
    onsibling,
    oncrdt,
    scenario,
    onscenario,
    collapsed = false,
    tab = $bindable("requests"),
    onexpand,
  }: {
    /** Hidden: only a rail of tab icons shows. */
    collapsed?: boolean;
    tab?: Tab;
    /** Show the column again, at this tab. */
    onexpand: (t: Tab) => void;
    /** Keys written and not deleted. */
    stored: number;
    onbatch: (op: BatchOp, n: number) => void;
    onlistkeys: () => void;
    /** One example command, for the copy button. */
    example: (op: BatchOp | "counter" | "set" | "siblings" | "keys") => string;
    rate: number;
    readShare: number;
    /** Requests a second on screen, at the current speed. */
    onScreen: number;
    trafficOn: boolean;
    ontraffic: (on: boolean) => void;
    /** The last value read, or null if not read yet. */
    counter: number | null;
    basket: string[] | null;
    /** The cart's values from the last read: one, or several siblings. */
    cart: string[] | null;
    onsibling: (action: "writeA" | "writeB" | "read" | "resolve") => void;
    oncrdt: (action: "count1" | "count10" | "readCounter" | "add" | "remove" | "readSet") => void;
    scenario: Scenario | null;
    onscenario: (s: Scenario | null) => void;
  } = $props();

  const TABS: { id: Tab; label: string; icon: Component }[] = [
    { id: "requests", label: "Requests", icon: Send },
    { id: "traffic", label: "Traffic", icon: Gauge },
    { id: "data", label: "Data", icon: Shapes },
    { id: "scenarios", label: "Scenarios", icon: Clapperboard },
  ];
  function choose(t: Tab) {
    tab = t;
  }
  const running = (t: Tab) => (t === "traffic" && trafficOn) || (t === "scenarios" && scenario !== null);
  let tabButtons: HTMLButtonElement[] = $state([]);
  function onTabKey(e: KeyboardEvent, i: number) {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const j = (i + step + TABS.length) % TABS.length;
    choose(TABS[j].id);
    tabButtons[j]?.focus();
  }

  const COUNTS = [1, 10, 100, 1000];
  // Each operation has its own colour role, the same as the mix bar: reads
  // are secondary, writes tertiary, deletes error, pings primary.
  const OPS: { op: BatchOp; icon: Component }[] = [
    { op: "write", icon: PenLine },
    { op: "read", icon: BookOpen },
    { op: "delete", icon: Trash2 },
    { op: "ping", icon: Radio },
  ];
  // Reads and deletes need that many stored keys.
  const ready = (op: BatchOp, n: number) => op === "write" || op === "ping" || stored >= n;
  const label = (n: number) => (n >= 1000 ? `${n / 1000}k` : String(n));

  const SCENARIO_ICONS: Record<string, Component> = { shop: ShoppingBag, betting: Trophy, social: MessageCircle };

  let copied = $state<string | null>(null);
  async function copy(what: BatchOp | "counter" | "set" | "siblings" | "keys") {
    try {
      await navigator.clipboard.writeText(example(what));
      copied = what;
      setTimeout(() => (copied = copied === what ? null : copied), 1200);
    } catch {
      copied = null;
    }
  }
</script>

{#snippet copyButton(what: BatchOp | "counter" | "set" | "siblings" | "keys")}
  <button type="button" class="copy" title="Copy an example curl command" aria-label="Copy the {what} command" onclick={() => copy(what)}>
    {#if copied === what}<Check />{:else}<Copy />{/if}
  </button>
{/snippet}

{#snippet badge(Icon: Component, role: string)}
  <span class="badge {role}" aria-hidden="true"><Icon /></span>
{/snippet}

{#if collapsed}
  <div class="rail" role="toolbar" aria-label="Controls" aria-orientation="vertical">
    {#each TABS as t (t.id)}
      <button type="button" class="rail-button" title={t.label} aria-label="Show {t.label}" onclick={() => onexpand(t.id)}>
        <t.icon />
        {#if running(t.id)}<span class="running" title="Running"></span>{/if}
      </button>
    {/each}
  </div>
{:else}
<div class="panel">
  <div class="tabs" role="tablist" aria-label="Controls">
    {#each TABS as t, i (t.id)}
      <button
        bind:this={tabButtons[i]}
        type="button"
        role="tab"
        id="tab-{t.id}"
        aria-controls="panel-{t.id}"
        aria-selected={tab === t.id}
        tabindex={tab === t.id ? 0 : -1}
        class="tab"
        class:selected={tab === t.id}
        onclick={() => choose(t.id)}
        onkeydown={(e) => onTabKey(e, i)}
      >
        <span class="tab-icon">
          <t.icon />
          {#if running(t.id)}<span class="running" title="Running"></span>{/if}
        </span>
        <span class="tab-label">{t.label}</span>
      </button>
    {/each}
  </div>

  <div class="body" role="tabpanel" id="panel-{tab}" aria-labelledby="tab-{tab}">
    {#if tab === "requests"}
      <section class="card" aria-labelledby="batch-title">
        <div class="head">
          <h2 id="batch-title">Send a batch</h2>
          <span class="meta mono">{stored.toLocaleString()} keys stored</span>
        </div>
        <div class="grid">
          <span></span>
          {#each COUNTS as n (n)}<span class="col mono">{label(n)}</span>{/each}
          {#each OPS as { op, icon } (op)}
            <span class="op mono">{@render badge(icon, op)}{op}{@render copyButton(op)}</span>
            {#each COUNTS as n (n)}
              <button type="button" class="n mono {op}" disabled={!ready(op, n)} title={ready(op, n) ? `${op} ${n}` : `Write ${n} keys first`} onclick={() => onbatch(op, n)}>{label(n)}</button>
            {/each}
          {/each}
        </div>
        <p class="note">Each request leaves the clients at its own moment. Reads and deletes pick stored keys at random.</p>
      </section>

      <section class="card" aria-labelledby="keys-title">
        <div class="head">
          <h2 id="keys-title">{@render badge(ListTree, "keys")}List keys</h2>
          {@render copyButton("keys")}
        </div>
        <button type="button" class="action keys" title="List every key in bucket data: a coverage query" onclick={onlistkeys}><ListTree />List every key in bucket data</button>
        <p class="note">A coverage query: a vnode on every node reads the bucket's keys from its ledger. Watch the tiles while it runs.</p>
      </section>
    {:else if tab === "traffic"}
      <section class="card" aria-labelledby="traffic-title">
        <div class="head">
          <h2 id="traffic-title">{@render badge(Gauge, "ping")}Steady traffic</h2>
        </div>
        <button type="button" class="action go" class:on={trafficOn} aria-pressed={trafficOn} onclick={() => ontraffic(!trafficOn)}>
          {#if trafficOn}<Square />Stop traffic{:else}<Play />Start traffic{/if}
        </button>
        <label class="slider">
          <span class="mono">rate</span>
          <input type="range" min="50" max="3000" step="50" bind:value={rate} />
          <span class="mono value">{rate.toLocaleString()}/s</span>
        </label>
        <label class="slider">
          <span class="mono">mix</span>
          <input type="range" min="0" max="100" step="5" bind:value={readShare} />
          <span class="mono value">{readShare}% r</span>
        </label>
        <div class="bar mono" aria-hidden="true">
          <span class="reads" style:width="{readShare}%">{#if readShare >= 25}<BookOpen />reads {readShare}%{/if}</span>
          <span class="writes" style:width="{100 - readShare}%">{#if 100 - readShare >= 25}<PenLine />writes {100 - readShare}%{/if}</span>
        </div>
        <p class="note">Requests per second of simulated time. At this speed that is about {onScreen < 1 ? onScreen.toFixed(1) : Math.round(onScreen)} a second on screen.</p>
      </section>
    {:else if tab === "data"}
      <section class="card" aria-labelledby="types-title">
        <div class="head">
          <h2 id="types-title">Data types</h2>
          <span class="meta mono">CRDTs</span>
        </div>
        <div class="grid three">
          <span class="op mono">{@render badge(Hash, "ping")}counter{@render copyButton("counter")}</span>
          <button type="button" class="n mono write" onclick={() => oncrdt("count1")}>+1</button>
          <button type="button" class="n mono write" onclick={() => oncrdt("count10")}>+10</button>
          <button type="button" class="n mono read" onclick={() => oncrdt("readCounter")}>read</button>
          <span class="op mono">{@render badge(Blend, "ping")}set{@render copyButton("set")}</span>
          <button type="button" class="n mono write" title="Add a fruit" onclick={() => oncrdt("add")}><Plus />add</button>
          <button type="button" class="n mono delete" title="Remove a fruit" onclick={() => oncrdt("remove")}><Minus />rm</button>
          <button type="button" class="n mono read" onclick={() => oncrdt("readSet")}>read</button>
        </div>
        <p class="value-line mono">visits = {counter ?? "?"}</p>
        <p class="value-line mono">basket = {basket ? `{${basket.join(", ")}}` : "?"}</p>
      </section>

      <section class="card" aria-labelledby="siblings-title">
        <div class="head">
          <h2 id="siblings-title">{@render badge(GitFork, "delete")}Siblings</h2>
          <span class="meta mono">types/carts{@render copyButton("siblings")}</span>
        </div>
        <div class="grid two">
          <button type="button" class="n mono write" title="Write milk with no vclock" onclick={() => onsibling("writeA")}><Milk />milk</button>
          <button type="button" class="n mono write" title="Write bread with no vclock" onclick={() => onsibling("writeB")}><Wheat />bread</button>
          <button type="button" class="n mono read" onclick={() => onsibling("read")}><BookOpen />read</button>
          <button type="button" class="n mono ping" title="Write all the values back as one, with the vclock from a read" onclick={() => onsibling("resolve")}><GitMerge />resolve</button>
        </div>
        <p class="value-line mono" class:conflict={cart !== null && cart.length > 1}>
          cart = {cart === null ? "?" : cart.length > 1 ? `${cart.length} siblings: ${cart.join(" | ")}` : (cart[0] ?? "not found")}
        </p>
        <p class="note">A write with no vclock keeps the other values: they become siblings. Resolve reads the vclock and writes one value back.</p>
      </section>
    {:else}
      <section class="card" aria-labelledby="scenario-title">
        <div class="head">
          <h2 id="scenario-title">Scenarios</h2>
          {#if scenario}
            <button type="button" class="stop" onclick={() => onscenario(null)}><Square />Stop</button>
          {/if}
        </div>
        {#each scenarios as s (s.id)}
          {@const Icon = SCENARIO_ICONS[s.id] ?? Clapperboard}
          <button type="button" class="scenario {s.id}" class:active={scenario?.id === s.id} aria-pressed={scenario?.id === s.id} onclick={() => onscenario(scenario?.id === s.id ? null : s)}>
            <span class="scenario-icon" aria-hidden="true"><Icon /></span>
            <span class="scenario-text">
              <span class="title">{s.title}</span>
              <span class="workload">{s.workload}</span>
              {#if scenario?.id === s.id}<span class="watch">{s.watch}</span>{/if}
            </span>
          </button>
        {/each}
      </section>
    {/if}
  </div>
</div>

{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .rail {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .rail-button {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--size-target);
    height: var(--size-target);
    border: none;
    border-radius: var(--shape-full);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .rail-button:hover {
    background: var(--md-sys-color-surface-container-highest);
  }
  .rail :global(.lucide),
  .panel :global(.lucide) {
    flex: none;
    width: var(--size-icon);
    height: var(--size-icon);
  }

  /* Material 3 primary tabs: icon over label. */
  .tabs {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    border-bottom: var(--stroke-hairline) solid var(--md-sys-color-outline-variant);
  }
  .tab {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2) 0 var(--space-2);
    border: none;
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    font: var(--type-label-medium);
    cursor: pointer;
  }
  .tab:hover {
    background: var(--md-sys-color-surface-container-highest);
  }
  .tab.selected {
    color: var(--md-sys-color-primary);
  }
  .tab.selected::after {
    content: "";
    position: absolute;
    left: 20%;
    right: 20%;
    bottom: calc(-1 * var(--stroke-hairline));
    height: var(--stroke-focus);
    border-radius: var(--shape-full) var(--shape-full) 0 0;
    background: var(--md-sys-color-primary);
  }
  .tab-icon {
    position: relative;
    display: inline-flex;
    padding: var(--space-1) var(--space-4);
    border-radius: var(--shape-full);
  }
  .tab.selected .tab-icon {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .tab-label {
    white-space: nowrap;
  }
  .running {
    position: absolute;
    top: 0;
    right: var(--space-2);
    width: var(--space-2);
    height: var(--space-2);
    border-radius: var(--shape-full);
    background: var(--color-go);
    outline: var(--stroke-thick) solid var(--md-sys-color-surface);
    animation: beat 1.2s var(--motion-standard) infinite;
  }
  @keyframes beat {
    50% {
      transform: scale(1.4);
    }
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container-lowest);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font: var(--type-title-small);
  }
  .meta,
  .note {
    margin: 0;
    font: var(--type-body-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  .meta {
    display: inline-flex;
    align-items: center;
    font: var(--type-code-small);
  }
  .value-line {
    margin: 0;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--shape-small);
    background: var(--md-sys-color-surface-container);
    font: var(--type-code-small);
    overflow-wrap: anywhere;
  }
  .value-line.conflict {
    background: var(--md-sys-color-error-container);
    color: var(--md-sys-color-on-error-container);
  }

  /* An icon in a tinted circle. The colour roles are shared with the
     buttons of the same operation. */
  .badge {
    display: inline-flex;
    padding: var(--space-1);
    border-radius: var(--shape-full);
  }
  .badge :global(.lucide) {
    width: var(--space-4);
    height: var(--space-4);
  }
  .write {
    --tone: var(--md-sys-color-tertiary-container);
    --on-tone: var(--md-sys-color-on-tertiary-container);
  }
  .read {
    --tone: var(--md-sys-color-secondary-container);
    --on-tone: var(--md-sys-color-on-secondary-container);
  }
  .delete,
  .keys {
    --tone: var(--md-sys-color-error-container);
    --on-tone: var(--md-sys-color-on-error-container);
  }
  .ping {
    --tone: var(--md-sys-color-primary-container);
    --on-tone: var(--md-sys-color-on-primary-container);
  }
  .badge,
  .n {
    background: var(--tone);
    color: var(--on-tone);
  }

  .grid {
    display: grid;
    grid-template-columns: max-content repeat(4, minmax(0, 1fr));
    gap: var(--space-1);
    align-items: center;
  }
  .grid.three {
    grid-template-columns: max-content repeat(3, minmax(0, 1fr));
  }
  .grid.two {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .col {
    text-align: center;
    font: var(--type-label-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  .op {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font: var(--type-code);
  }
  .n {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    min-height: var(--size-target);
    border: none;
    border-radius: var(--shape-small);
    font: var(--type-code);
    cursor: pointer;
    transition: transform var(--duration-short) var(--motion-standard);
  }
  .n :global(.lucide) {
    width: var(--space-4);
    height: var(--space-4);
  }
  .n:hover:not(:disabled) {
    outline: var(--stroke-thick) solid var(--on-tone);
    outline-offset: calc(-1 * var(--stroke-thick));
  }
  .n:active:not(:disabled) {
    transform: scale(0.94);
  }
  .n:disabled {
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-on-surface-variant);
    opacity: var(--dim);
    cursor: default;
  }
  .copy {
    display: inline-flex;
    align-items: center;
    padding: var(--space-1);
    border: none;
    border-radius: var(--shape-full);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .copy :global(.lucide) {
    width: var(--space-4);
    height: var(--space-4);
  }
  .copy:hover {
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-primary);
  }

  /* A full-width tonal button with an icon. */
  .action,
  .stop {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: var(--size-target);
    padding: 0 var(--space-4);
    border: none;
    border-radius: var(--shape-full);
    background: var(--tone, var(--md-sys-color-secondary-container));
    color: var(--on-tone, var(--md-sys-color-on-secondary-container));
    font: var(--type-label-large);
    cursor: pointer;
  }
  .action:hover,
  .stop:hover {
    outline: var(--stroke-thick) solid currentColor;
    outline-offset: calc(-1 * var(--stroke-thick));
  }
  .action.go {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  .action.go.on,
  .stop {
    background: var(--color-go);
    color: var(--color-on-go);
  }
  .stop {
    min-height: calc(var(--size-target) - var(--space-2));
  }

  .slider {
    display: grid;
    grid-template-columns: calc(var(--size-target) + var(--space-2)) minmax(0, 1fr) calc(var(--size-target) * 2);
    align-items: center;
    gap: var(--space-2);
    font: var(--type-code);
  }
  .slider input {
    accent-color: var(--md-sys-color-primary);
  }
  .value {
    text-align: right;
  }
  .bar {
    display: flex;
    height: var(--space-6);
    overflow: hidden;
    border-radius: var(--shape-small);
    font: var(--type-label-small);
  }
  .bar span {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-2);
    white-space: nowrap;
    transition: width var(--duration-short) var(--motion-standard);
  }
  .bar :global(.lucide) {
    width: var(--space-4);
    height: var(--space-4);
  }
  .reads {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  .writes {
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
  }

  .scenario {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline-variant);
    border-radius: var(--shape-medium);
    background: none;
    color: var(--md-sys-color-on-surface);
    text-align: left;
    cursor: pointer;
  }
  .scenario:hover {
    background: var(--md-sys-color-surface-container-highest);
  }
  .scenario.active {
    border-color: transparent;
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  .scenario-icon {
    display: inline-flex;
    padding: var(--space-2);
    border-radius: var(--shape-medium);
  }
  .scenario.shop .scenario-icon {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  .scenario.betting .scenario-icon {
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
  }
  .scenario.social .scenario-icon {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .scenario.active .scenario-icon {
    background: var(--color-go);
    color: var(--color-on-go);
  }
  .scenario-text {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .title {
    font: var(--type-title-small);
  }
  .workload,
  .watch {
    font: var(--type-body-small);
  }
  .watch {
    font-style: italic;
  }

  @media (prefers-reduced-motion: reduce) {
    .running {
      animation: none;
    }
  }
</style>
