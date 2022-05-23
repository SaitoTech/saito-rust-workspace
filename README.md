# saito-rust-workspace
<todo: introduction>

# How to run the node

## Prerequisites

1. Install Cargo/Rust [from here](https://www.rust-lang.org/tools/install)
2. Install wasm-pack [from here](https://rustwasm.github.io/wasm-pack/installer/)

## Running rust node

1. Copy saito-rust/configs/saito.config.template.json into saito.config.json and do the necessary changes.
2. Go into saito-rust/ directory
3. run cargo run

## Compiling WASM code

1. Go to saito-wasm directory
2. Execute "wasm-pack build --debug --target browser"

