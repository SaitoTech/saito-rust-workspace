# saito-rust-workspace
<todo: introduction>

# How to run the node

## Prerequisites

1. Install Cargo/Rust [from here](https://www.rust-lang.org/tools/install)
1. Install wasm-pack [from here](https://rustwasm.github.io/wasm-pack/installer/)
1. Install build tools /
   On ubuntu: sudo apt-get update && sudo apt install build-essential pkg-config libssl-dev
   
## Get the code

git clone git@github.com:SaitoTech/saito-rust-workspace.git

> Use https for deployment.

## Running rust node

1. Navigate into the directory: `cd saito-rust-workspace/`
1. Run `cp configs/saito.config.template.json configs/saito.config.json` and do the necessary changes in saito.config.json.
1. run `RUST_LOG=debug GEN_TX=1 cargo run`

#### Environment Variables

- RUST_LOG - `error,warn,info,debug,trace` Log level of the node
- GEN_TX - If this is "1" will generate test transactions within the node

## Compiling WASM code

1. Go to saito-wasm directory
2. Execute `wasm-pack build --debug --target browser`

