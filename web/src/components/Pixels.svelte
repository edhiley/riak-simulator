<script lang="ts">
  // Draws a pixel-art sprite in an SVG, with hard pixel edges.
  import type { Sprite } from "../lib/pixels.ts";

  let {
    sprite,
    x,
    y,
    pixel = 2,
    ink = "currentColor",
    accent = ink,
    palette = undefined,
  }: {
    sprite: Sprite;
    x: number;
    y: number;
    pixel?: number;
    ink?: string;
    accent?: string;
    /** Letter to colour, for full-colour sprites; replaces ink and accent. */
    palette?: Record<string, string>;
  } = $props();

  let cells = $derived(
    sprite.flatMap((row, r) =>
      [...row].flatMap((c, col) =>
        c === "." ? [] : [{ x: x + col * pixel, y: y + r * pixel, fill: palette ? (palette[c] ?? ink) : c === "o" ? accent : ink }],
      ),
    ),
  );
</script>

<g class="pixels" aria-hidden="true">
  {#each cells as c, i (i)}
    <!-- A little wider than a pixel, so no seams show when scaled. -->
    <rect x={c.x} y={c.y} width={pixel * 1.04} height={pixel * 1.04} style:fill={c.fill} />
  {/each}
</g>

<style>
  .pixels {
    shape-rendering: crispEdges;
    pointer-events: none;
  }
</style>
