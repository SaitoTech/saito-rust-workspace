#!/usr/bin/env bash

# Script to setup the basic requirements for running a saito rust node
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y || exit
sudo apt update
sudo apt install build-essential libssl-dev pkg-config || exit
bash
cargo build || exit

# setup the saito-rust/saito.config.json file from the template and run `cargo run`