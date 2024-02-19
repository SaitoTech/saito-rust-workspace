#!/usr/bin/env bash

# Script to setup the basic requirements for running a saito rust node on macOS  

if ! command -v brew &> /dev/null
then
    echo "Homebrew not found. Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
fi


curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

brew update

brew install llvm clang pkg-config node npm python3

cargo install wasm-pack

cargo build


 