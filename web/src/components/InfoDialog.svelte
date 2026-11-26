<script lang="ts">
  // A popup that explains one idea: an optional picture and numbered points.
  // Same frame as "What are partitions?".
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";
  import Terms from "./Terms.svelte";
  import type { Explainer } from "../lib/explainers.ts";

  let { explainer, figure }: { explainer: Explainer; figure?: Snippet } = $props();

  let dialog: HTMLDialogElement | undefined = $state();

  export function open() {
    dialog?.showModal();
  }
</script>

<dialog bind:this={dialog} aria-labelledby="info-{explainer.id}" onclick={(e) => e.target === dialog && dialog?.close()}>
  <div class="content">
    <h2 id="info-{explainer.id}">{explainer.title}</h2>
    <div class="body" class:with-figure={figure}>
      {#if figure}
        <div class="figure">{@render figure()}</div>
      {/if}
      <div class="text">
        <ol>
          {#each explainer.points as point, i (i)}
            <!-- plain: text that uses a word in another sense than the glossary. -->
            <li>{#if point.plain}{point.text}{:else}<Terms text={point.text} />{/if}</li>
          {/each}
        </ol>
        {#each explainer.links ?? [] as group, g (group.heading)}
          <section aria-labelledby="links-{explainer.id}-{g}">
            <h3 id="links-{explainer.id}-{g}">{group.heading}</h3>
            <ul>
              {#each group.items as link (link.href)}
                <li>
                  <a href={link.href} target="_blank" rel="noopener noreferrer">{link.label}</a>{#if link.note}<span class="note">: {link.note}</span>{/if}
                </li>
              {/each}
            </ul>
          </section>
        {/each}
        {#if explainer.footnote}
          <p class="footnote">{explainer.footnote}</p>
        {/if}
      </div>
    </div>
    <div class="actions">
      <Button onclick={() => dialog?.close()}>Close</Button>
    </div>
  </div>
</dialog>

<style>
  dialog {
    max-width: min(var(--size-dialog), calc(100vw - var(--space-8)));
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
    gap: var(--space-4);
    padding: var(--space-6);
  }
  h2 {
    margin: 0;
    font: var(--type-headline-small);
  }
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-6);
    align-items: start;
  }
  .body.with-figure {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }
  @media (max-width: 52rem) {
    .body.with-figure {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  ol {
    margin: 0;
    padding-left: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    font: var(--type-body-large);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  h3 {
    margin: 0 0 var(--space-2);
    font: var(--type-title-medium);
  }
  ul {
    margin: 0;
    padding-left: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font: var(--type-body-large);
  }
  a {
    color: var(--md-sys-color-primary);
  }
  .note {
    color: var(--md-sys-color-on-surface-variant);
  }
  .footnote {
    margin: 0;
    font: var(--type-body-medium);
    color: var(--md-sys-color-on-surface-variant);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
