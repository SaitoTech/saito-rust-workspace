#!/bin/bash

# Get the directory where the script is located
SCRIPT_DIR=$(dirname "$0")

BASE_PATH="$SCRIPT_DIR/../saito-rust"

# Setup config
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

# Setup issuance
if [ ! -f "$BASE_PATH/data/issuance" ]; then
  cp "$BASE_PATH/data/issuance/issuance.template" "$BASE_PATH/data/issuance/issuance"
  echo "issuance file has been created from issuance.template."
else
  echo "issuance file already exists. No changes made."
fi

# Install packages
chmod +x "$SCRIPT_DIR/bootstrap_mac.sh"
chmod +x "$SCRIPT_DIR/bootstrap_linux.sh"
OS="$(uname)"
case "$OS" in
  Darwin)
    echo "Running bootstrap_mac.sh for macOS"
    "$SCRIPT_DIR/bootstrap_mac.sh" || { echo "Installation aborted by user. Exiting."; exit 1; }
    ;;
  Linux)
    echo "Running bootstrap_linux.sh for Linux"
    "$SCRIPT_DIR/bootstrap_linux.sh" || { echo "Installation aborted by user. Exiting."; exit 1; }
    ;;
  *)
    echo "Unsupported operating system: $OS"
    exit 1
    ;;
esac


source "$HOME/.cargo/env" 2>/dev/null

# Start node
cd "$BASE_PATH"

echo "Running 'RUST_LOG=debug cargo run' in $BASE_PATH"
env RUST_LOG=debug cargo run
