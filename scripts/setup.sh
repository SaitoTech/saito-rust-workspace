#!/bin/bash

# Get the directory where the script is located
SCRIPT_DIR=$(dirname "$0")

BASE_PATH="$SCRIPT_DIR/../saito-rust" 

# setup config
if [ ! -f "$BASE_PATH/configs/config.json" ]; then
  cp "$BASE_PATH/configs/config.template.json" "$BASE_PATH/configs/config.json"
  echo "config.json has been created from config.template.json."
else
  echo "config.json already exists. No changes made."
fi

# Create blocks folder
if [ ! -d "$BASE_PATH/data/blocks" ]; then
  mkdir -p "$BASE_PATH/data/blocks"
  echo "blocks folder has been created."
else
  echo "blocks folder already exists. No changes made."
fi

# setup issuance
if [ ! -f "$BASE_PATH/data/issuance" ]; then
  cp "$BASE_PATH/data/issuance/issuance.template" "$BASE_PATH/data/issuance/issuance"
  echo "issuance file has been created from issuance.template."
else
  echo "issuance file already exists. No changes made."
fi


# install packages
chmod +x "$SCRIPT_DIR/bootstrap_mac.sh"
chmod +x "$SCRIPT_DIR/bootstrap_linux.sh"
OS="$(uname)"
case "$OS" in
  Darwin)
    echo "Running bootstrap_mac.sh for macOS"
    "$SCRIPT_DIR/bootstrap_mac.sh"
    ;;
  Linux)
    echo "Running bootstrap_linux.sh for Linux"
    "$SCRIPT_DIR/bootstrap_linux.sh"
    ;;
  *)
    echo "Unsupported operating system: $OS"
    ;;
esac

source "$HOME/.cargo/env"


# Start node
cd "$BASE_PATH"

echo "Running 'RUST_LOG=debug cargo run' in $BASE_PATH"
env RUST_LOG=debug cargo run
