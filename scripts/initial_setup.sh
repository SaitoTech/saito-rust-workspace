#!/bin/bash

# Get the directory where the script is located
SCRIPT_DIR=$(dirname "$0")

BASE_PATH="$SCRIPT_DIR/../saito-rust"

# Setup config
if [ ! -f "$BASE_PATH/configs/config.json" ]; then
  cp "$BASE_PATH/configs/config.template.json" "$BASE_PATH/configs/config.json"
  echo "config.json has been created from config.template.json."


  echo "Do you want to setup an isolated node (i) or a connected node (c)?"
read -p "[i/c]: " node_type

CONFIG_PATH="$BASE_PATH/configs/config.json"

if [ "$node_type" == "i" ]; then
  if grep -q '"peers": \[' "$CONFIG_PATH"; then
    awk '
    BEGIN {print_mode=1}
    /"peers": \[/ {print_mode=0; print "\"peers\": []"; next}
    /]/ {if (print_mode == 0) {print_mode=1; next}}
    {if (print_mode == 1) print}
    ' "$CONFIG_PATH" > temp && mv temp "$CONFIG_PATH"
    echo "Configured as an isolated node with empty peers array."
  fi
elif [ "$node_type" == "c" ]; then
  echo "Configured as a connected node. No changes to peers array."
else
  echo "Invalid input. No changes made to peers configuration."
fi
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