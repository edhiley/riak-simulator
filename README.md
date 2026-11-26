# Riak Simulator

A browser simulator of an OpenRiak KV cluster. It shows where each key is saved, how requests travel between nodes and vnodes, what happens when nodes stop, join or leave, and how the leveled backend inside a vnode stores data.

**Try it: https://edhiley.github.io/riak-simulator/**

No Riak runs in the page. A model of the cluster, written in Rust and compiled to WebAssembly, runs in the browser, and the page draws every message it sends. The model follows the OpenRiak 3.4.1 source and contains no Riak code. Select **How it works** in the page for a diagram and the sources.

## What you can do

- Send batches of writes, reads, deletes and pings (1 to 1,000 at a time), steady traffic with a read and write mix, or one of three scenarios.
- Use the data types (a counter and a set), make and resolve siblings, and list the keys in a bucket.
- Type `curl` commands in the console. The console is a floating panel.
- Stop and start nodes, add a node, and make a node leave. Partitions move by handoff, one message at a time.
- Point at a key's dot to see its other copies in the cluster.
- Select a partition tile to open its leveled store. The Bookie, the Inker, the Penciller and their clerks are pixel characters. Each step they do says why.
- Select **What happened?** to pause and read each step of the last command, from the side of the node that got the message.
- Change R, W, DW, PR and PW, the number of nodes and partitions, the speed, and the timing, in Settings.
- Read the explainers: History, Partitions, CAP theorem and How it works.

The plan, the decisions and the learning objectives are in [`PLAN.md`](PLAN.md). The visual and writing rules are in [`web/DESIGN.md`](web/DESIGN.md).

## Layout

| Path | What |
|---|---|
| `core/` | The simulation in Rust. It has no I/O, no real clock and a seeded random generator, so the same inputs always give the same run. `cargo run -p riak-sim-core --example trace` prints the events of one PUT and one GET |
| `wasm/` | The WebAssembly wrapper (`wasm-bindgen`), and the Dockerfile that builds it |
| `web/` | The page: Svelte 5, TypeScript, Vite, Lucide icons, D3's geometry modules |
| `validation/` | Scripts that check the core against a real OpenRiak node |
| `licenses/` | Licence texts of Rust crates that are not under Apache-2.0 |

## Build and run

Rust builds run in Docker, so you do not need a Rust toolchain. The page needs Node 24 or later.

Build the WebAssembly package into `web/src/wasm/pkg`. The first build downloads the WebAssembly target and `wasm-bindgen`:

```bash
cd web && npm run wasm
```

Run the page:

```bash
cd web && npm install && npm run dev
```

Make a production build. This also runs the type check and the design lint, and writes the licence files:

```bash
cd web && npm run build
```

## Publishing

Every push to `main` runs [`.github/workflows/pages.yml`](.github/workflows/pages.yml). It runs the tests, builds the WebAssembly package and the page, and publishes `web/dist` to GitHub Pages.

## Tests

The core tests, including the real-node fixture:

```bash
docker run --rm -v "$PWD":/src -v riakemu-cargo:/usr/local/cargo/registry -v riakemu-target:/target -e CARGO_TARGET_DIR=/target/riak-sim-v3 -w /src rust:1-slim cargo test -p riak-sim-core
```

The page tests (the `curl` parser):

```bash
cd web && npm test
```

The design lint finds raw colours, pixel sizes, fonts, shadows and gradients outside the token file:

```bash
cd web && npm run lint:design
```

## How key placement is checked

`core/tests/fixtures/hash-real-3.4.1.tsv` comes from a real OpenRiak 3.4.1 node (`validation/make-hash-fixture.sh`). The node calculated the hash and the first three partitions at ring sizes 8 and 64, for 1,530 bucket and key pairs: plain buckets, typed buckets, and empty, long, Unicode and binary keys. The core gives the same hash and preference list for each one.

The same script then asks the node for each key's preference list over Protocol Buffers and compares. All 1,525 keys that are not empty agree.

One finding: the client APIs treat bucket type `default` as no type. A request for `/types/default/buckets/b/keys/k` is placed by the hash of `b` alone, not of `{<<"default">>, <<"b">>}`. The core does the same.

Which node owns which partition is not checked. The simulator gives partitions to nodes in turn. A new cluster usually looks like this, but Riak's claim algorithm can give a different result. The page says so.

## Console

- The console accepts a subset of `curl`: `-X`, `-H`, `-d`, `--data`, `--data-raw`, `--data-binary`, `-i`, `-s`, and one URL.
- It also accepts a method and a path, for example `GET /ping` or `PUT node2/buckets/b/keys/k hello`.
- The host `localhost` is the load balancer. It sends each request to the next running node in turn. The hosts `node1` to `node8` are the nodes. The port must be 8098, or left out.
- `node3$ riak stop` and `node3$ riak start` stop and start a node. `riak admin cluster join`, `leave`, `plan` and `commit` change the cluster, as in Riak.
- Anything else gets an error that names the part that is not supported.

## Licence

Apache License 2.0: see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE). The simulator contains no Riak code. It follows the behaviour of OpenRiak 3.4.1, and `NOTICE` credits those repositories. It is not part of OpenRiak, and OpenRiak does not endorse it.

`npm run licenses` runs as part of `npm run build`. It writes `web/public/THIRD-PARTY-LICENSES.txt` from the installed packages and the crate texts in [`licenses/`](licenses), and copies `LICENSE` and `NOTICE` next to it, so the built page includes all of them. If you add a Rust crate to the WebAssembly module, add it to the list in `web/scripts/third-party.mjs`.
