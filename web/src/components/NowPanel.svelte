<script lang="ts">
  // The steps of the current request, one at a time. Each step is a short
  // paragraph that says what happened and why.
  //
  // With pause at each step on, the simulation stops when a step happens.
  // If one action caused several steps, the box starts at the first of
  // them. Next walks through them; on the newest step, Continue runs the
  // simulation to the next step. When the request is finished, a result
  // sums up what it did.
  import Button from "./Button.svelte";
  import Terms from "./Terms.svelte";
  import type { SimEvent } from "../lib/sim.ts";
  import { partLabel } from "../lessons/index.ts";

  let {
    events,
    req,
    pauseEachStep,
    paused,
    oncontinue,
    highlight = null,
    intro = null,
    oncap = () => {},
  }: {
    events: SimEvent[];
    req: number | null;
    pauseEachStep: boolean;
    paused: boolean;
    oncontinue: () => void;
    highlight?: string | null;
    /** The lesson step: its reliability story is shown before anything runs. */
    intro?: import("../lessons/index.ts").LessonStep | null;
    oncap?: () => void;
  } = $props();

  let steps = $derived(events.filter((e) => e.req === req && !e.summary));
  let result = $derived(events.find((e) => e.req === req && e.summary) ?? null);
  // null: show the newest step.
  let chosen = $state<number | null>(null);
  let seen = 0;

  // A new request starts from its first step.
  $effect(() => {
    void req;
    chosen = null;
    seen = 0;
  });

  // When several steps arrive at once, start at the first new one.
  $effect(() => {
    const n = steps.length;
    if (pauseEachStep && n > seen + 1) chosen = seen;
    seen = n;
  });

  let index = $derived(chosen === null ? steps.length - 1 : Math.min(chosen, steps.length - 1));
  let step = $derived(index >= 0 ? steps[index] : null);
  let atNewest = $derived(index === steps.length - 1);
  let finished = $derived(result !== null);

  function previous() {
    chosen = Math.max(0, index - 1);
  }

  function next() {
    chosen = index + 1 >= steps.length - 1 ? null : index + 1;
  }

</script>

<section aria-labelledby="now-title">
  <h2 id="now-title">What is happening</h2>
  {#if step}
    <div class="nav">
      <Button variant="text" disabled={index === 0} onclick={previous}>Previous</Button>
      <span class="count">Step {index + 1}{finished ? ` of ${steps.length}` : ""}</span>
      {#if atNewest && paused && !finished}
        <Button variant="go" onclick={oncontinue}>Continue</Button>
      {:else}
        <Button variant="text" disabled={atNewest} onclick={next}>Next</Button>
      {/if}
    </div>
    <p class="block" aria-live="polite"><Terms text={step.text} {highlight} /></p>
    {#if result && atNewest}
      <div class="result">
        <h3>Result</h3>
        <p><Terms text={result.text} {highlight} /></p>
      </div>
    {/if}
  {:else if intro}
    <h3 class="intro-title">{partLabel(intro)}</h3>
    {#each intro.intro as paragraph, i (i)}
      <p class="intro"><Terms text={paragraph} /></p>
    {/each}
    {#if intro.cap}
      <div><Button variant="text" onclick={oncap}>The CAP theorem</Button></div>
    {/if}
    <p class="empty">Select Run in the lesson to start.</p>
  {:else}
    <p class="empty">Select Run in the lesson to start.</p>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    border-radius: var(--shape-large);
    background: var(--md-sys-color-surface-container);
  }
  h2 {
    margin: 0;
    font: var(--type-title-medium);
    letter-spacing: var(--tracking-title-medium);
  }
  h3 {
    margin: 0 0 var(--space-1);
    font: var(--type-title-small);
  }
  p {
    margin: 0;
  }
  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .count {
    font: var(--type-label-medium);
    letter-spacing: var(--tracking-label);
    color: var(--md-sys-color-on-surface-variant);
  }
  .block {
    padding: var(--space-3);
    border-radius: var(--shape-medium);
    background: var(--md-sys-color-surface-container-lowest);
    font: var(--type-body-large);
  }
  .result {
    padding: var(--space-3);
    border-radius: var(--shape-medium);
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
    font: var(--type-body-medium);
  }
  .intro-title {
    margin: 0;
  }
  .intro {
    font: var(--type-body-large);
  }
  .empty {
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
