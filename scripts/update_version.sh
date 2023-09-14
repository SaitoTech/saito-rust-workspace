#!/bin/bash

VERSION=$(cat ../VERSION | tr -d '\n')

update_cargo() {
    local cargo_file="$1"
    if [[ -f $cargo_file ]]; then
        OS=$(uname)
        if [[ "$OS" == "Darwin" ]]; then 
            sed -i "" "s/^version = .*/version = \"$VERSION\"/" $cargo_file
        fi
    fi
}


update_package_json() {
    local json_file="$1"
    if [[ -f $json_file ]]; then
        if jq .version "$json_file" > /dev/null 2>&1; then
            jq ".version = \"$VERSION\"" "$json_file" | sponge "$json_file"
        fi
    fi
}

members=("../saito-core" "../saito-wasm" "../saito-rust" "../saito-spammer" "../saito-js")

for member in "${members[@]}"; do
    echo "Checking $member..."

    update_cargo "$member/Cargo.toml"
    
    update_package_json "$member/package.json"
done

echo "Version update complete!"
