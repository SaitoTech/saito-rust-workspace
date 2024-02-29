#!/usr/bin/env bash



command_exists() {
  command -v "$1" >/dev/null 2>&1
}

linux_package_installed() {
  dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q "ok installed"
}

ask_permission() {
  while true; do
    read -p "$1 [Y/n]: " yn
    case $yn in
      [Yy]* | "" ) return 0;;  # Treat empty input as Yes
      [Nn]* ) echo "Aborting. The following installations were pending: ${pending_installations[*]}"
              exit 1;;
      * ) echo "Please answer yes (default) or no.";;
    esac
  done
}


pending_installations=()


! command_exists rustc && ! command_exists cargo && pending_installations+=("Rust")
! command_exists llvm && pending_installations+=("build-essential")
# ! command_exists clang && pending_installations+=("libssl-dev")
# ! command_exists pkg-config && pending_installations+=("pkg-config")
# ! command_exists node && pending_installations+=("nodejs")
# ! command_exists npm && pending_installations+=("npm")
# ! command_exists clang && pending_installations+=("clang")
# ! command_exists gcc-multilib && pending_installations+=("gcc-multilib")
# ! command_exists python-is-python3 && pending_installations+=("python-is-python3")





sudo apt update

export PATH="$HOME/.cargo/bin:$PATH"
# Install Rust if not present
if ! command_exists rustc || ! command_exists cargo; then
  ask_permission "Rust is not installed. Install Rust?"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
    echo "Sourced $HOME/.cargo/env successfully."
  else
    echo "$HOME/.cargo/env does not exist. Attempting to directly update PATH."
  fi

  # Directly update PATH as a fallback
  export PATH="$HOME/.cargo/bin:$PATH"
  echo "Updated PATH: $PATH"
  pending_installations=("${pending_installations[@]/Rust}")

else 
  echo "Rustup is already installed"
fi


missing_packages=()
for package in libssl-dev pkg-config nodejs npm clang gcc-multilib python-is-python3; do
  if ! command_exists $package && ! linux_package_installed $package; then
    missing_packages+=("$package")
  fi
done



if [ ${#missing_packages[@]} -ne 0 ]; then
  ask_permission "Install necessary packages (${missing_packages[*]})?"
  for package in "${missing_packages[@]}"; do
      sudo NEEDRESTART_MODE=a apt install -y $package || exit 1
       missing_packages=("${missing_packages[@]/$package}")
  done
else
  echo "All necessary packages are already installed."
fi



# Build project
 ask_permission "Build Project?"
if cargo build; then
  echo "Setup completed successfully."
else
  echo "Cargo build failed."
  exit 1
fi
