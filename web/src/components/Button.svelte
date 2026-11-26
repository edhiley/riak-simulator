<script lang="ts">
  // Material 3 filled and text buttons.
  import type { Snippet } from "svelte";

  let {
    variant = "filled",
    disabled = false,
    title,
    onclick,
    children,
  }: { variant?: "filled" | "text" | "tonal" | "go"; disabled?: boolean; title?: string; onclick?: () => void; children: Snippet } = $props();
</script>

<button class={variant} {disabled} {title} {onclick}>{@render children()}</button>

<style>
  button {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: var(--size-target);
    padding: 0 var(--space-6);
    border: none;
    border-radius: var(--shape-full);
    font: var(--type-label-large);
    letter-spacing: var(--tracking-label);
    cursor: pointer;
    isolation: isolate;
    overflow: hidden;
  }
  /* State layer: the content colour at Material's state opacities. */
  button::after {
    content: "";
    position: absolute;
    inset: 0;
    background: currentColor;
    opacity: 0;
    transition: opacity var(--duration-short) var(--motion-standard);
    z-index: -1;
  }
  button:hover::after {
    opacity: var(--state-hover);
  }
  button:active::after {
    opacity: var(--state-pressed);
  }
  .filled {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  .tonal {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  /* The next thing to do. */
  .go {
    background: var(--color-go);
    color: var(--color-on-go);
  }
  .text {
    background: none;
    color: var(--md-sys-color-primary);
    padding: 0 var(--space-3);
  }
  button:disabled {
    cursor: default;
    color: var(--md-sys-color-on-surface);
    opacity: var(--state-disabled-content);
  }
  button.filled:disabled,
  button.tonal:disabled {
    background: var(--md-sys-color-on-surface);
    color: var(--md-sys-color-surface);
  }
</style>
