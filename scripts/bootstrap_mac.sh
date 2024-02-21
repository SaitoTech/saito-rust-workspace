#!/usr/bin/env bash

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

ask_permission() {
  while true; do
    read -p "$1 [y/n]: " yn
    case $yn in
      [Yy]* ) return 0;;
      [Nn]* ) 
        echo "Aborting. The following installations were pending: ${pending_installations[*]}"
        exit 1
        ;;
      * ) echo "Please answer yes or no.";;
    esac
  done
}


pending_installations=()







if ! command_exists brew; then
  ask_permission "Homebrew is not installed. Install Homebrew?"
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" || exit 1
  pending_installations=("${pending_installations[@]/Homebrew}")
fi


# Install Rust if not present
if ! command_exists rustc || ! command_exists cargo; then
  ask_permission "Rust is not installed. Install Rust?"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y || exit 1
  source "$HOME/.cargo/env"
else 
  echo "Rustup is already installed"
fi

# Update Homebrew and install necessary packages
ask_permission "Update Homebrew and install necessary packages (llvm, clang, pkg-config, node, npm, python3)?"
brew update || exit 1
for package in llvm clang pkg-config node npm python3; do
  if ! command_exists $package; then
    brew install $package || exit 1
  else
    echo "Package $package is already installed."
  fi
done


# Install wasm-pack if not present
if ! command_exists wasm-pack; then
  ask_permission "wasm-pack is not installed. Install wasm-pack?"
  cargo install wasm-pack || exit 1
  else
    echo "Package wasm-pack is already installed."
fi


 ask_permission "Build?"
cargo build || exit 1

echo "Setup completed successfully."
