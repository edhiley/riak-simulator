// Writes public/THIRD-PARTY-LICENSES.txt, and copies LICENSE and NOTICE into
// public/, so that the built page ships with every licence it needs.
// Run with `npm run licenses` (the build runs it too).
import { readFileSync, writeFileSync, copyFileSync, readdirSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const web = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(web, "..");
const pkg = JSON.parse(readFileSync(join(web, "package.json"), "utf8"));

// npm packages that end up in the page: the runtime dependencies, their own
// dependencies, and Svelte's runtime.
const seen = new Map();
function walk(name, follow = true) {
  if (seen.has(name)) return;
  const dir = join(web, "node_modules", name);
  const meta = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  const file = readdirSync(dir).find((f) => /^licen[cs]e/i.test(f));
  if (!file) throw new Error(`${name}: no licence file`);
  seen.set(name, { version: meta.version, license: meta.license, text: readFileSync(join(dir, file), "utf8").trim() });
  if (follow) for (const dep of Object.keys(meta.dependencies ?? {})) walk(dep);
}
for (const name of Object.keys(pkg.dependencies)) walk(name);
// Svelte's own dependencies are compile-time tools, not part of the page.
walk("svelte", false);

// Rust crates compiled into the WebAssembly module. Where a crate offers
// "MIT OR Apache-2.0", this project uses it under Apache-2.0 (text in LICENSE).
const crates = [
  ["wasm-bindgen", "MIT OR Apache-2.0"],
  ["wasm-bindgen-shared", "MIT OR Apache-2.0"],
  ["serde", "MIT OR Apache-2.0"],
  ["serde_core", "MIT OR Apache-2.0"],
  ["serde_json", "MIT OR Apache-2.0"],
  ["itoa", "MIT OR Apache-2.0"],
  ["cfg-if", "MIT OR Apache-2.0"],
  ["once_cell", "MIT OR Apache-2.0"],
  ["memchr", "Unlicense OR MIT", "memchr-LICENSE-MIT"],
  ["zmij", "MIT", "zmij-LICENSE-MIT"],
];

const rule = "=".repeat(72);
const out = [
  "Third-party software in the Riak Simulator page",
  "",
  "The page includes the software below. Each part is under its own licence.",
  "",
  rule,
  "Rust crates (in the WebAssembly module)",
  rule,
  "",
  "These crates are used under the Apache License, Version 2.0, the text of",
  "which is in LICENSE, unless a licence text is given below.",
  "",
];
for (const [name, license, file] of crates) {
  out.push(`${name} (${license})${file ? "" : ": used under Apache-2.0"}`);
  if (file) out.push("", readFileSync(join(root, "licenses", file), "utf8").trim(), "");
}
out.push("", rule, "npm packages (in the JavaScript, CSS and fonts)", rule, "");
for (const [name, p] of [...seen].sort()) {
  out.push(`--- ${name} ${p.version} (${p.license}) ---`, "", p.text, "");
}

const pub = join(web, "public");
mkdirSync(pub, { recursive: true });
writeFileSync(join(pub, "THIRD-PARTY-LICENSES.txt"), out.join("\n"));
copyFileSync(join(root, "LICENSE"), join(pub, "LICENSE.txt"));
copyFileSync(join(root, "NOTICE"), join(pub, "NOTICE.txt"));
console.log(`licences: ${crates.length} crates, ${seen.size} npm packages`);
