// Fails if any style outside src/styles/tokens.css uses a raw value instead of
// a design token. See DESIGN.md. A line may opt out with "design-lint-ignore"
// and a reason, which review should question.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("../src", import.meta.url).pathname;
const SKIP = new Set(["styles/tokens.css", "wasm"]);

const RULES = [
  [/#[0-9a-fA-F]{3,8}\b(?![-\w])/, "raw hex colour", (line) => /[:("'\s,]#[0-9a-fA-F]{3,8}\b/.test(line)],
  [/\b(?:rgba?|hsla?|oklch|lab|lch)\(/, "raw colour function"],
  [/(?<![\w-])\d*\.?\d+px\b/, "pixel value; use a space, size or stroke token"],
  [/font-family\s*:/, "font-family; use var(--type-…)"],
  [/(?<![\w-])font\s*:(?!\s*(?:var\(--type-|inherit\b))/, "font shorthand other than var(--type-…)"],
  [/(?:linear|radial|conic)-gradient\(/, "gradient"],
  [/box-shadow\s*:/, "box-shadow; use a surface container role"],
  [/backdrop-filter|filter\s*:\s*blur/, "blur or glass effect"],
  [/(?:^|[\s:;])(?:color|background(?:-color)?|fill|stroke|border(?:-\w+)?)\s*:\s*(?:white|black|red|blue|green|gray|grey|purple|orange|yellow|pink)\b/, "named colour"],
  [/\p{Extended_Pictographic}/u, "emoji"],
];

function* files(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    const rel = relative(root, path);
    if (SKIP.has(rel)) continue;
    if (statSync(path).isDirectory()) yield* files(path);
    else if (/\.(svelte|css|ts)$/.test(name) && !name.endsWith(".test.ts")) yield [path, rel];
  }
}

let failures = 0;
for (const [path, rel] of files(root)) {
  readFileSync(path, "utf8").split("\n").forEach((line, i) => {
    if (line.includes("design-lint-ignore")) return;
    for (const [pattern, message, guard] of RULES) {
      if (pattern.test(line) && (!guard || guard(line))) {
        console.error(`src/${rel}:${i + 1}: ${message}\n    ${line.trim()}`);
        failures++;
      }
    }
  });
}
if (failures) {
  console.error(`\n${failures} design-lint failure(s). See DESIGN.md.`);
  process.exit(1);
}
console.log("design lint: clean");
