<script lang="ts" generics="T extends string | number">
  // Material 3 segmented button, single select.
  let {
    label,
    options,
    value,
    onchange,
    hideLabel = false,
  }: { label: string; options: T[]; value: T; onchange: (v: T) => void; hideLabel?: boolean } = $props();
</script>

<div class="field">
  <span class="label" class:hidden={hideLabel} id="seg-{label}">{label}</span>
  <div class="group" role="radiogroup" aria-labelledby="seg-{label}">
    {#each options as option (option)}
      <button
        role="radio"
        aria-checked={option === value}
        class:selected={option === value}
        onclick={() => onchange(option)}>{option}</button
      >
    {/each}
  </div>
</div>

<style>
  /* Kept for screen readers when the label is not shown. */
  .hidden {
    position: absolute;
    width: var(--stroke-hairline);
    height: var(--stroke-hairline);
    overflow: hidden;
    clip-path: inset(50%);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .label {
    font: var(--type-label-medium);
    letter-spacing: var(--tracking-label);
    color: var(--md-sys-color-on-surface-variant);
  }
  .group {
    display: inline-flex;
  }
  button {
    position: relative;
    min-width: var(--size-target);
    min-height: var(--size-target);
    padding: 0 var(--space-3);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline);
    margin-left: calc(-1 * var(--stroke-hairline));
    background: none;
    color: var(--md-sys-color-on-surface);
    font: var(--type-label-large);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    isolation: isolate;
  }
  button:first-child {
    margin-left: 0;
    border-radius: var(--shape-full) 0 0 var(--shape-full);
    padding-left: var(--space-4);
  }
  button:last-child {
    border-radius: 0 var(--shape-full) var(--shape-full) 0;
    padding-right: var(--space-4);
  }
  button::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: currentColor;
    opacity: 0;
    z-index: -1;
  }
  button:hover::after {
    opacity: var(--state-hover);
  }
  .selected {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
</style>
