#!/usr/bin/env bash

# Initialize an array to keep track of pending installations
declare -a pending_installations=("Rust and Cargo" "system packages" "wasm-pack" "project build")


command_exists() {
  command -v "$1" >/dev/null 2>&1
}


ask_permission() {
  while true; do
    read -p "$1 [y/n]: " yn
    case $yn in
      [Yy]* ) return 0;;
      [Nn]* ) echo "Aborting. Remaining installations: ${pending_installations[*]}"
              exit 1;;
      * ) echo "Please answer yes or no.";;
    esac
  done
}


mark_as_installed() {

  for i in "${!pending_installations[@]}"; do
    if [[ "${pending_installations[$i]}" = "$1" ]]; then
      unset 'pending_installations[i]'
    fi
  done
}


if command_exists rustc && command_exists cargo; then
  echo "Rust and Cargo are already installed."
  mark_as_installed "Rust and Cargo"
else
  ask_permission "Rust and Cargo are not installed. Install them?"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  mark_as_installed "Rust and Cargo"
fi


sudo apt update
ask_permission "Install necessary packages (build-essential, libssl-dev, pkg-config, nodejs, npm, clang, gcc-multilib, python-is-python3)?"
sudo NEEDRESTART_MODE=a apt install -y build-essential libssl-dev pkg-config nodejs npm clang gcc-multilib python-is-python3
mark_as_installed "system packages"


ask_permission "Install wasm-pack?"
cargo install wasm-pack
mark_as_installed "wasm-pack"

# Build project
ask_permission "Build project?"
cargo build
mark_as_installed "project build"

echo "Setup completed successfully. All required installations and configurations are done."
