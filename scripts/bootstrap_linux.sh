#!/usr/bin/env bash



command_exists() {
  command -v "$1" >/dev/null 2>&1
}


ask_permission() {
  while true; do
    read -p "$1 [y/n]: " yn
    case $yn in
      [Yy]* ) return 0;;
      [Nn]* ) echo "Aborting. The following installations were pending: ${pending_installations[*]}"
              exit 1;;
      * ) echo "Please answer yes or no.";;
    esac
  done
}

pending_installations=()


! command_exists rustc && ! command_exists cargo && pending_installations+=("Rust")
! command_exists llvm && pending_installations+=("build-essential")
! command_exists clang && pending_installations+=("libssl-dev")
! command_exists pkg-config && pending_installations+=("pkg-config")
! command_exists node && pending_installations+=("nodejs")
! command_exists npm && pending_installations+=("npm")
! command_exists clang && pending_installations+=("clang")
! command_exists gcc-multilib && pending_installations+=("gcc-multilib")
! command_exists python-is-python3 && pending_installations+=("python-is-python3")




sudo apt update

# if command_exists rustc && command_exists cargo; then
#   echo "Rust and Cargo are already installed."
# else
#   ask_permission "Rust and Cargo are not installed. Install them?"
#   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
#   source "$HOME/.cargo/env"
#  pending_installations=("${pending_installations[@]/Rust}")
# fi

# Install Rust if not present
if ! command_exists rustc || ! command_exists cargo; then
  ask_permission "Rust is not installed. Install Rust?"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  pending_installations=("${pending_installations[@]/Rust}")

else 
  echo "Rustup is already installed"
fi



ask_permission "Install necessary packages (build-essential, libssl-dev, pkg-config, nodejs, npm, clang, gcc-multilib, python-is-python3)?"
for package in libssl-dev pkg-config nodejs npm clang gcc-multilib python-is-python3; do
  if ! command_exists $package; then
    sudo NEEDRESTART_MODE=a apt install -y $package || exit 1
    pending_installations=("${pending_installations[@]/$package}")
  else
    echo "Package $package is already installed."
  fi
done


# Build project
ask_permission "Build project?"
cargo build

echo "Setup completed successfully. All required installations and configurations are done."
