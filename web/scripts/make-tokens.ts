// Generates src/styles/tokens.css: every colour, size, font and timing the
// page may use. Run with `npm run tokens` after changing anything here, and
// commit the output. DESIGN.md explains the choices; the design lint rejects
// raw values anywhere else.
import { writeFileSync } from "node:fs";
import {
  argbFromHex,
  hexFromArgb,
  Hct,
  MaterialDynamicColors,
  SchemeTonalSpot,
} from "@material/material-color-utilities";

// Chrome colours come from one seed through Material 3's Tonal Spot scheme:
// soft tints, so the controls have some colour, while strong, solid hue on
// the page still means "which node". The seed is a violet, a hue that no
// node colour uses, so no control looks like a node.
const SEED = "#6750a4";

const ROLES = [
  "primary", "onPrimary", "primaryContainer", "onPrimaryContainer",
  "secondary", "onSecondary", "secondaryContainer", "onSecondaryContainer",
  "tertiary", "onTertiary", "tertiaryContainer", "onTertiaryContainer",
  "error", "onError", "errorContainer", "onErrorContainer",
  "surface", "onSurface", "onSurfaceVariant",
  "surfaceContainerLowest", "surfaceContainerLow", "surfaceContainer",
  "surfaceContainerHigh", "surfaceContainerHighest",
  "inverseSurface", "inverseOnSurface", "inversePrimary",
  "outline", "outlineVariant", "scrim",
] as const;

// Node colours: the Okabe–Ito palette, distinguishable with the common forms
// of colour blindness. Its black becomes light grey on a dark surface.
const NODES_LIGHT = ["#e69f00", "#56b4e9", "#009e73", "#f0e442", "#0072b2", "#d55e00", "#cc79a7", "#000000"];
const NODES_DARK = ["#e69f00", "#56b4e9", "#009e73", "#f0e442", "#0072b2", "#d55e00", "#cc79a7", "#bbbbbb"];

// "Go" colour: green, for the one control that is the next thing to do.
// Material 3 has no success role, so it is a custom colour: the primary
// container of a tonal scheme from a green seed.
const GO_SEED = "#2e7d32";

const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

// Text on a node colour: whichever of black or white has more contrast.
const onNode = (hex: string) => ((luminance(hex) + 0.05) / 0.05 > 1.05 / (luminance(hex) + 0.05) ? "#000000" : "#ffffff");

function colours(dark: boolean): string {
  const scheme = new SchemeTonalSpot(Hct.fromInt(argbFromHex(SEED)), dark, 0);
  const lines = ROLES.map((role) => `  --md-sys-color-${kebab(role)}: ${hexFromArgb(MaterialDynamicColors[role].getArgb(scheme))};`);
  const nodes = dark ? NODES_DARK : NODES_LIGHT;
  nodes.forEach((hex, i) => {
    lines.push(`  --node-${i}: ${hex};`);
    lines.push(`  --node-${i}-on: ${onNode(hex)};`);
  });
  const go = new SchemeTonalSpot(Hct.fromInt(argbFromHex(GO_SEED)), dark, 0);
  lines.push(`  --color-go: ${hexFromArgb(MaterialDynamicColors.primaryContainer.getArgb(go))};`);
  lines.push(`  --color-on-go: ${hexFromArgb(MaterialDynamicColors.onPrimaryContainer.getArgb(go))};`);
  lines.push(`  color-scheme: ${dark ? "dark" : "light"};`);
  return lines.join("\n");
}

const fixed = `
  /* Type: Material 3 type scale. Roboto Flex for text, Roboto Mono for data. */
  --font-plain: "Roboto Flex Variable", system-ui, sans-serif;
  --font-mono: "Roboto Mono", ui-monospace, monospace;
  --type-headline-small: 400 1.5rem/2rem var(--font-plain);
  --type-title-large: 400 1.375rem/1.75rem var(--font-plain);
  --type-title-medium: 500 1rem/1.5rem var(--font-plain);
  --type-title-small: 500 0.875rem/1.25rem var(--font-plain);
  --type-body-large: 400 1rem/1.5rem var(--font-plain);
  --type-body-medium: 400 0.875rem/1.25rem var(--font-plain);
  --type-body-small: 400 0.75rem/1rem var(--font-plain);
  --type-label-large: 500 0.875rem/1.25rem var(--font-plain);
  --type-label-medium: 500 0.75rem/1rem var(--font-plain);
  --type-label-small: 500 0.6875rem/1rem var(--font-plain);
  --type-code: 400 0.8125rem/1.25rem var(--font-mono);
  --type-code-small: 400 0.75rem/1rem var(--font-mono);
  --type-code-large: 500 1rem/1.5rem var(--font-mono);
  --type-code-xl: 500 1.375rem/1.75rem var(--font-mono);
  /* Port numbers and FSM counts on the node strips, which the SVG scales down. */
  --type-port: 600 1.125rem/1.5rem var(--font-mono);
  --type-key: 500 1.75rem/2rem var(--font-mono);
  --tracking-title-medium: 0.009375rem;
  --tracking-label: 0.03125rem;

  /* Pixel character colours (characters.ts): the same in both themes. */
  --pix-ink: #2a2238;
  --pix-skin: #f3c9a2;
  --pix-skin-shade: #d9a07e;
  --pix-hair: #3b2b2f;
  --pix-white: #fffaf0;
  --pix-blush: #f29b9b;
  --pix-gold: #f4b942;
  --pix-gold-dark: #c8892a;
  --pix-wood: #a06e45;
  --pix-wood-dark: #6b4429;
  --pix-paper: #f7ecd2;
  --pix-paper-shade: #dfcda5;
  --pix-grey: #b3adc2;
  --pix-grey-dark: #6f6880;
  --pix-red: #de5a4f;
  --pix-red-dark: #a63a36;
  --pix-indigo: #4a66b8;
  --pix-indigo-dark: #2f4382;
  --pix-teal: #33a89d;
  --pix-teal-dark: #217a71;
  --pix-plum: #a45fb2;
  --pix-plum-dark: #713f80;
  --pix-ochre: #e8a040;
  --pix-ochre-dark: #b5752b;
  --pix-green: #5fb066;
  --pix-green-dark: #3d7d44;
  --pix-navy: #34416a;
  --pix-navy-dark: #222b48;
  --pix-glass: #a8dcf2;
  --pix-lantern: #ffd36b;

  /* Shape: Material 3 corner scale. */
  --shape-none: 0;
  --shape-extra-small: 4px;
  --shape-small: 8px;
  --shape-medium: 12px;
  --shape-large: 16px;
  --shape-full: 9999px;

  /* Space: a 4px grid. */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-8: 32px;
  --space-12: 48px;

  /* Strokes and sizes. */
  --stroke-hairline: 1px;
  --stroke-thick: 2px;
  --stroke-focus: 3px;
  --size-icon: 20px;
  --size-target: 40px;
  --size-column: 360px;
  --size-column-min: 280px;
  --size-column-max: 640px;
  --size-ring-max: 820px;
  --size-input-min: 72px;
  --size-log-max: 320px;
  --size-tooltip: 280px;
  --size-dialog: 960px;
  --size-dialog-short: 360px;
  --size-clock: 88px;

  /* Material 3 state layers. */
  --state-hover: 0.08;
  --state-focus: 0.1;
  --state-pressed: 0.1;
  --state-disabled-content: 0.38;
  --state-disabled-container: 0.12;
  --dim: 0.28;

  /* Motion: Material 3 easing and duration tokens. Interface chrome only;
     anything in simulation time moves linearly. */
  --motion-standard: cubic-bezier(0.2, 0, 0, 1);
  --motion-emphasized-decelerate: cubic-bezier(0.05, 0.7, 0.1, 1);
  --motion-emphasized-accelerate: cubic-bezier(0.3, 0, 0.8, 0.15);
  --duration-short: 150ms;
  --duration-medium: 300ms;
  --duration-long: 500ms;`;

const css = `/* Generated by scripts/make-tokens.ts. Do not edit by hand. */
:root {${fixed}
${colours(false)}
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
${colours(true).replace(/^/gm, "  ")}
  }
}
:root[data-theme="dark"] {
${colours(true)}
}
`;

writeFileSync(new URL("../src/styles/tokens.css", import.meta.url), css);

// The same values for code that draws in SVG and cannot read CSS directly.
export const NODE_COUNT = NODES_LIGHT.length;
