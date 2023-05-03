#!/usr/bin/env bash

# Script to setup the basic requirements for running a saito rust node
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
sudo apt update
sudo NEEDRESTART_MODE=a apt install -y build-essential libssl-dev pkg-config nodejs npm clang gcc-multilib python-is-python3
bash
cargo install flamegraph
cargo install wasm-pack
bash
cargo build || exit

# setup the saito-rust/config.json file from the template and run `cargo run`
