# saito-rust-workspace
<todo: introduction>

# How to run the node

## Prerequisites

1. Install Cargo/Rust [from here](https://www.rust-lang.org/tools/install)
2. Install wasm-pack [from here](https://rustwasm.github.io/wasm-pack/installer/)

## Get the code

git clone git@github.com:SaitoTech/saito-rust-workspace.git

> Use https for deployment.

## Running rust node

1. Run `cp saito-rust/configs/saito.config.template.json saito-rust/configs/saito.config.json` and do the necessary changes in saito.config.json.
2. Go into `cd saito-rust/` directory
3. run `RUST_LOG=debug GEN_TX=1 cargo run`

#### Environment Variables

- RUST_LOG - `error,warn,info,debug,trace` Log level of the node
- GEN_TX - If this is "1" will generate test transactions within the node

## Compiling WASM code

1. Go to saito-wasm directory
2. Execute `wasm-pack build --debug --target browser`

