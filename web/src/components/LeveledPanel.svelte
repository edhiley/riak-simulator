<script lang="ts">
  // Inside one vnode: its leveled store, with leveled's processes as
  // characters. Each character works (its second frame, and its station
  // lights up) for a moment after it does a step. Sizes are scaled down, and
  // the panel says by how much.
  import { untrack } from "svelte";
  import Pixels from "./Pixels.svelte";
  import X from "@lucide/svelte/icons/x";
  import { PALETTE, bookie, doorman, iclerk, inker, pclerk, penciller, type Frame } from "../lib/characters.ts";
  import type { LEvent, LeveledView } from "../lib/sim.ts";

  let {
    title,
    view,
    wall,
    mailbox,
    busy,
    onclose,
  }: {
    /** Messages waiting for this vnode, and whether it is working on one. */
    mailbox: number;
    busy: boolean;
    /** Which vnode, in words: "Partition 12 on node3". */
    title: string;
    view: LeveledView;
    /** Wall time now, in ms, for the work animation. */
    wall: number;
    onclose: () => void;
  } = $props();

  type Who = LEvent["who"] | "doorman";
  const SPRITES: Record<Who, [Frame, Frame]> = { bookie, inker, iclerk, penciller, pclerk, doorman };
  // Each character: its module, its job, and why it works that way.
  const NAMES: Record<Who, { name: string; module: string; job: string; why: string }> = {
    doorman: {
      name: "Doorman",
      module: "riak_core_vnode_proxy",
      job: "Every command for this vnode passes the doorman, who counts the queue.",
      why: "Counting lets a put coordinator avoid a busy vnode (above 50 waiting, it looks for another), and protects the vnode: above 10,000 waiting, commands are refused, so one slow vnode cannot run the node out of memory.",
    },
    bookie: {
      name: "Bookie",
      module: "leveled_bookie",
      job: "Takes each request. Sends the value to the Inker and keeps the key in its ledger cache.",
      why: "Values and keys go to different places. Values are big and are written once; keys are small and are sorted again and again. Keeping them apart means the sorting never has to move the values.",
    },
    inker: {
      name: "Inker",
      module: "leveled_inker",
      job: "Appends each object to the journal, in arrival order.",
      why: "An append is a sequential write, the cheapest thing a disk can do, so a write is quick however big the store is.",
    },
    iclerk: {
      name: "Inker's clerk",
      module: "leveled_iclerk",
      job: "Rewrites old journal files without the objects that newer writes replaced.",
      why: "Appending never overwrites, so old values pile up. The clerk only rewrites files that are mostly waste, so space comes back without copying much.",
    },
    penciller: {
      name: "Penciller",
      module: "leveled_penciller",
      job: "Keeps the ledger: every key in order, with its newest number and metadata.",
      why: "A sorted ledger finds a key quickly, answers a HEAD without touching the value, and lists one bucket without reading the others.",
    },
    pclerk: {
      name: "Penciller's clerk",
      module: "leveled_pclerk",
      job: "Merges a file down a level when a level has too many files.",
      why: "Each level is a few times bigger than the one above and its files never overlap. So a read checks a handful of files, not every file ever written.",
    },
  };
  const WORK_MS = 700;

  // When each character last worked, in wall time. New events since the
  // last frame start the animation.
  let lastAt = -1;
  let lastCount = 0;
  let worked = $state<Record<string, number>>({});
  let litLevel = $state<{ level: number; at: number } | null>(null);
  $effect(() => {
    const fresh = view.events.filter((e) => e.at > lastAt || (e.at === lastAt && view.events.length > lastCount));
    if (!fresh.length) return;
    const t = performance.now();
    const next = { ...untrack(() => worked) };
    for (const e of fresh) {
      next[e.who] = t;
      if (e.level !== null) litLevel = { level: e.level, at: t };
    }
    worked = next;
    lastAt = view.events[view.events.length - 1].at;
    lastCount = view.events.length;
  });
  const working = (w: Who) => wall - (worked[w] ?? -Infinity) < WORK_MS;
  const levelLit = (i: number) => litLevel !== null && litLevel.level === i && wall - litLevel.at < WORK_MS;

  let recent = $derived(view.events.slice(-6).reverse());
  let latest = $derived(recent[0] ?? null);
  const SOFT_LIMIT = 50;
  const real = (n: number) => (n * view.scale).toLocaleString();
  const PIXEL = 2.5;
</script>

{#snippet character(w: Who)}
  {@const s = SPRITES[w][working(w) ? 1 : 0]}
  <figure class="who" class:working={working(w)} title="{NAMES[w].module}: {NAMES[w].job}">
    <svg viewBox="0 0 {24 * PIXEL} {24 * PIXEL}" width={24 * PIXEL} height={24 * PIXEL} aria-hidden="true">
      <Pixels sprite={s} x={0} y={0} pixel={PIXEL} palette={PALETTE} />
    </svg>
    <figcaption>{NAMES[w].name}</figcaption>
  </figure>
{/snippet}

{#snippet why(w: Who)}
  <details class="why">
    <summary>Why?</summary>
    <p>{NAMES[w].why}</p>
  </details>
{/snippet}

<aside class="zoom" aria-label="{title}: leveled store">
  <div class="head">
    <div>
      <h2>{title}</h2>
      <p class="sub">The leveled backend. Counts are 1/{view.scale.toLocaleString()} of the real defaults.</p>
    </div>
    <button type="button" class="close" aria-label="Close" onclick={onclose}><X /></button>
  </div>

  <section class="station" class:lit={busy || working("doorman")}>
    {@render character("doorman")}
    <div class="what">
      <h3>Vnode proxy</h3>
      <div class="meter queue" title="Soft limit 50; overload at 10,000">
        <span style:width="{Math.min(100, (100 * mailbox) / SOFT_LIMIT)}%" class:over={mailbox > SOFT_LIMIT}></span>
      </div>
      <p class="note">{mailbox} waiting{busy ? ", 1 in progress" : ""}. Soft limit {SOFT_LIMIT}; refused above 10,000. The simulator shows the queue but does not refuse work.</p>
      {@render why("doorman")}
    </div>
  </section>

  <section class="station" class:lit={working("bookie")}>
    {@render character("bookie")}
    <div class="what">
      <h3>Ledger cache</h3>
      <div class="slots" title="leveled.cache_size: {real(view.cache_size)} keys">
        {#each Array.from({ length: view.cache_size }, (_, i) => i) as i (i)}
          <span class="slot" class:full={i < view.cache.length}>{view.cache[i] ?? ""}</span>
        {/each}
      </div>
      <p class="note">{view.seq.toLocaleString()} writes so far. When {view.cache_size} keys wait here (really {real(view.cache_size)}), the Bookie gives them to the Penciller.</p>
      {@render why("bookie")}
    </div>
  </section>

  <section class="station" class:lit={working("inker") || working("iclerk")}>
    <div class="pair">{@render character("inker")}{@render character("iclerk")}</div>
    <div class="what">
      <h3>Journal</h3>
      <div class="files">
        {#each view.journal as j (j.id)}
          <span
            class="jfile"
            class:active={j.active}
            style:--fill="{(100 * j.objects) / view.journal_objects}%"
            style:--live="{j.objects ? (100 * j.live) / j.objects : 100}%"
            title="Journal file {j.id}{j.active ? ' (active)' : ''}: {j.objects} objects, {j.live} still current"
          ><span class="bar"><span class="used"><span class="live"></span></span></span><span class="label mono">{j.id}</span></span>
        {/each}
      </div>
      <p class="note">Values, in arrival order. A file rolls at {view.journal_objects} objects (really {real(view.journal_objects)}). Dark: still current. Light: replaced by a newer write, waiting for compaction. {view.compactions} compaction{view.compactions === 1 ? "" : "s"} so far.</p>
      {@render why("inker")}
      {@render why("iclerk")}
    </div>
  </section>

  <section class="station" class:lit={working("penciller")}>
    {@render character("penciller")}
    <div class="what">
      <h3>Penciller's memory</h3>
      <div class="meter" title="leveled.penciller_cache_size: {real(view.pmem_size)} keys">
        <span style:width="{(100 * view.pmem) / view.pmem_size}%"></span>
      </div>
      <p class="note">{view.pmem} of {view.pmem_size} keys (really {real(view.pmem_size)}). When full, it is written as a level 0 file, in key order.</p>
      {@render why("penciller")}
    </div>
  </section>

  <section class="station" class:lit={working("pclerk")}>
    {@render character("pclerk")}
    <div class="what">
      <h3>Ledger levels</h3>
      {#each view.levels as files, i (i)}
        <div class="level" class:lit={levelLit(i)}>
          <span class="lname mono">L{i}</span>
          <span class="lfiles">
            {#each files as f (f.id)}
              <span class="sst" style:--w="{Math.max(12, (100 * f.keys) / view.sst_keys)}%" title="File {f.id}: {f.keys} keys, {f.first} to {f.last}"></span>
            {/each}
            {#if files.length === 0}<span class="empty">empty</span>{/if}
          </span>
          <span class="limit mono">{i === 0 ? "1 at a time" : `${files.length}/${view.level_files[i] ?? "∞"}`}</span>
        </div>
      {/each}
      <p class="note">Each box is a file of up to {view.sst_keys} keys (really 32,768), labelled with its key range. Files in a level below 0 never overlap. {view.merges} merge{view.merges === 1 ? "" : "s"} so far.</p>
      {@render why("pclerk")}
    </div>
  </section>

  <section class="log" aria-live="off">
    <h3>Last steps</h3>
    {#if latest?.why}
      <p class="latest-why"><strong>Why the last step:</strong> {latest.why}</p>
    {/if}
    {#if recent.length === 0}
      <p class="note">Nothing yet. Write some keys.</p>
    {:else}
      <ol>
        {#each recent as e, i (i)}
          <li>{e.text}</li>
        {/each}
      </ol>
    {/if}
  </section>
</aside>

<style>
  .zoom {
    position: fixed;
    top: calc(var(--size-target) + var(--space-4));
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 2;
    width: min(calc(var(--size-column) + var(--space-12)), calc(100vw - var(--space-8)));
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    overflow-y: auto;
    border: var(--stroke-hairline) solid var(--md-sys-color-outline-variant);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }
  h2 {
    margin: 0;
    font: var(--type-title-large);
  }
  h3 {
    margin: 0;
    font: var(--type-title-small);
  }
  .sub,
  .note {
    margin: 0;
    font: var(--type-body-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  .close {
    display: inline-flex;
    padding: var(--space-2);
    border: none;
    border-radius: var(--shape-full);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .close:hover {
    background: var(--md-sys-color-surface-container-highest);
  }
  .close :global(.lucide) {
    width: var(--size-icon);
    height: var(--size-icon);
  }
  .station {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--shape-medium);
    background: var(--md-sys-color-surface-container-lowest);
    transition: background var(--duration-short) var(--motion-standard);
  }
  .station.lit {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .station.lit .note {
    color: inherit;
  }
  .pair {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .who {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    flex: none;
    width: calc(var(--space-12) + var(--space-6));
  }
  .who figcaption {
    font: var(--type-label-small);
    text-align: center;
  }
  /* The characters stand on a paper stage, the same in both themes. */
  .who svg {
    display: block;
    padding: var(--space-1);
    border-radius: var(--shape-small);
    background: var(--pix-paper);
  }
  .who.working svg {
    outline: var(--stroke-thick) solid var(--pix-gold);
  }
  .why {
    font: var(--type-body-small);
  }
  .why summary {
    cursor: pointer;
    font: var(--type-label-medium);
    color: var(--md-sys-color-primary);
  }
  .station.lit .why summary {
    color: inherit;
  }
  .why p {
    margin: var(--space-1) 0 0;
  }
  .latest-why {
    margin: var(--space-1) 0 0;
    padding: var(--space-2);
    border-radius: var(--shape-small);
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
    font: var(--type-body-small);
  }
  .queue span.over {
    background: var(--md-sys-color-error);
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    flex: 1;
  }
  .slots {
    display: flex;
    gap: var(--space-1);
  }
  .slot {
    flex: 1;
    min-width: 0;
    min-height: var(--space-6);
    padding: 0 var(--space-1);
    overflow: hidden;
    border: var(--stroke-hairline) dashed var(--md-sys-color-outline);
    border-radius: var(--shape-extra-small);
    font: var(--type-code-small);
    line-height: var(--space-6);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .slot.full {
    border-style: solid;
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
  }
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .jfile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--stroke-thick);
    width: var(--space-8);
  }
  .bar {
    position: relative;
    width: 100%;
    height: var(--space-8);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline);
    border-radius: var(--shape-extra-small);
    overflow: hidden;
  }
  .jfile.active .bar {
    border-width: var(--stroke-thick);
    border-color: var(--md-sys-color-primary);
  }
  .used {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: var(--fill);
    background: var(--md-sys-color-secondary-container);
  }
  .live {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: var(--live);
    background: var(--md-sys-color-secondary);
  }
  .label {
    font: var(--type-label-small);
  }
  .meter {
    height: var(--space-3);
    border-radius: var(--shape-full);
    background: var(--md-sys-color-surface-container-highest);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    background: var(--md-sys-color-primary);
    transition: width var(--duration-short) var(--motion-standard);
  }
  .level {
    display: grid;
    grid-template-columns: var(--space-6) minmax(0, 1fr) var(--space-12);
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1);
    border-radius: var(--shape-small);
    transition: background var(--duration-short) var(--motion-standard);
  }
  .level.lit {
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
  }
  .lname,
  .limit {
    font: var(--type-label-medium);
  }
  .limit {
    text-align: right;
  }
  .lfiles {
    display: flex;
    flex-wrap: wrap;
    gap: var(--stroke-thick);
    min-height: var(--space-4);
  }
  .sst {
    width: calc(var(--w) / 4);
    min-width: var(--space-2);
    height: var(--space-4);
    border-radius: var(--shape-extra-small);
    background: var(--md-sys-color-tertiary);
  }
  .empty {
    font: var(--type-body-small);
    color: var(--md-sys-color-outline);
  }
  .log ol {
    margin: var(--space-1) 0 0;
    padding-left: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font: var(--type-body-small);
  }
</style>
