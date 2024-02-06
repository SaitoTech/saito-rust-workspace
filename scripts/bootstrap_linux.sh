#!/usr/bin/env bash

# Script to setup the basic requirements for running a saito rust node
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
sudo apt update
sudo NEEDRESTART_MODE=a apt install -y build-essential libssl-dev pkg-config nodejs npm clang gcc-multilib python-is-python3 cargo
tset
#cargo install flamegraph
sudo npm install -g pm2
cargo install wasm-pack
cargo build
