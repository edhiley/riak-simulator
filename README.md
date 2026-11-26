# Riak Simulator

A browser simulator of an OpenRiak KV cluster: the ring, where keys live, replication, failures and recovery, handoff, and the leveled backend inside each vnode.

**Try it: https://edhiley.github.io/riak-simulator/**

The plan and the learning objectives are in [`PLAN.md`](PLAN.md). The visual and writing rules are in [`web/DESIGN.md`](web/DESIGN.md).

- Nodes and vnodes are actors. Each vnode has a mailbox. Messages travel on a simulated network on a simulation clock.
- The PUT path and the GET path follow the OpenRiak 3.4.1 source. The simplifications are listed at the top of `core/src/sim.rs`.
- The page draws every message in flight. You can pause, step, change the speed, and hold, release or slow one message.
- Every outside input is recorded with its simulation time (`Sim::inputs`). Replay from this record is not built or tested yet.

## Layout

| Path | What |
|---|---|
| `core/` | The simulation in Rust: no I/O, no real clock, and a seeded generator, so a native run and a browser run of the same inputs are identical. `cargo run -p riak-sim-core --example trace` prints the events of one PUT and one GET |
| `wasm/` | WebAssembly wrapper (`wasm-bindgen`), and the Dockerfile that builds it |
| `web/` | The page: Svelte 5, TypeScript, Vite, D3's geometry modules |
| `validation/` | Scripts that check the core against a real OpenRiak node |

## Build and run

There is no host Rust toolchain; Rust builds run in Docker. Node 24 or later is needed for the page.

Build the WebAssembly package (into `web/src/wasm/pkg`; the first build downloads the WebAssembly target and `wasm-bindgen`):

```bash
cd web && npm run wasm
```

Run the page:

```bash
cd web && npm install && npm run dev
```

Production build, which also runs the type check and the design lint:

```bash
cd web && npm run build
```

## Publishing

Every push to `main` runs [`.github/workflows/pages.yml`](.github/workflows/pages.yml): it runs the tests, builds the WebAssembly package and the page, and publishes `web/dist` to GitHub Pages.

## Tests

Core tests, including the real-node fixture:

```bash
docker run --rm -v "$PWD":/src -v riakemu-cargo:/usr/local/cargo/registry -v riakemu-target:/target -e CARGO_TARGET_DIR=/target/riak-sim-v3 -w /src rust:1-slim cargo test -p riak-sim-core
```

Page tests (the `curl` parser):

```bash
cd web && npm test
```

The design lint (raw colours, pixel sizes, fonts, shadows and gradients outside the token file):

```bash
cd web && npm run lint:design
```

## How key placement is checked

`core/tests/fixtures/hash-real-3.4.1.tsv` comes from a real OpenRiak 3.4.1 node (`validation/make-hash-fixture.sh`). The node computed the hash and the first three partitions at ring sizes 8 and 64, for 1,530 bucket/key pairs: plain buckets, typed buckets, empty, long, Unicode and binary keys. The core reproduces every hash and preflist.

The same script then asks the node for each key's preflist over Protocol Buffers and compares. All 1,525 non-empty keys agree.

One finding: the client APIs treat bucket type `default` as no type. A request for `/types/default/buckets/b/keys/k` is placed by the hash of `b` alone, not of `{<<"default">>, <<"b">>}`. The core does the same.

Which node owns which partition is not checked. It is dealt round-robin, which is how a fresh cluster usually looks but not necessarily what Riak's claim algorithm produces. The page says so.

## REPL

- Accepts a subset of `curl`: `-X`, `-H`, `-d`, `--data`, `--data-raw`, `--data-binary`, `-i`, `-s`, and one URL.
- Also accepts bare lines such as `GET /ping` or `PUT node2/buckets/b/keys/k hello`.
- The host names the node: `localhost` is node1, `node3` is node3. The port must be 8098 or left out.
- Anything else is rejected with an error naming the unsupported part.

Lessons are data in `web/src/lessons/`.

## Branding

The product name and every mention of Riak in the interface come from `web/src/lib/brand.ts`, plus lesson text, so the page can be de-branded by editing those.

## Licence

Apache License 2.0: see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE). The simulator contains no Riak code. It follows the behaviour of OpenRiak 3.4.1, and `NOTICE` credits those repositories. It is not part of OpenRiak and is not endorsed by it.

`npm run licenses` (part of `npm run build`) writes `web/public/THIRD-PARTY-LICENSES.txt` from the installed packages and the crate texts in [`licenses/`](licenses), and copies `LICENSE` and `NOTICE` next to it, so the built page ships with all of them. If you add a Rust crate to the WebAssembly module, add it to the list in `web/scripts/third-party.mjs`.
