# Builds the WebAssembly package into web/src/wasm/pkg. Run from riak-sim/:
#   docker build -f wasm/Dockerfile -o web/src/wasm/pkg .
FROM rust:1-slim AS build
# Must match the wasm-bindgen version pinned in wasm/Cargo.toml.
ARG WASM_BINDGEN=0.2.100
RUN rustup target add wasm32-unknown-unknown \
 && cargo install --locked wasm-bindgen-cli --version "$WASM_BINDGEN"
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY core ./core
COPY wasm ./wasm
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,id=riak-sim-v3-target,target=/src/target \
    cargo build --release --locked -p riak-sim-wasm --target wasm32-unknown-unknown \
 && wasm-bindgen --target web --out-dir /pkg target/wasm32-unknown-unknown/release/riak_sim_wasm.wasm

FROM scratch
COPY --from=build /pkg /
