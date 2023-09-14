#!/bin/bash

VERSION=$(cat ../VERSION | tr -d '\n')

update_cargo() {
    if [[ -f $1 ]]; then
        sed -i "s/^version = .*/version = \"$VERSION\"/" $1
    fi
}

update_package_json() {
    # Check if the file exists
    if [[ -f $1 && $(jq .version $1) ]]; then
        # Update version using jq
        jq ".version = \"$VERSION\"" $1 | sponge $1
    fi
}

members=("../saito-core" "../saito-wasm" "../saito-rust" "../saito-spammer" "../saito-js")

for member in "${members[@]}"; do
    echo "Updating version in $member..."

    # Update cargo.toml
    update_cargo "$member/Cargo.toml"
    
    # Update package.json
    update_package_json "$member/package.json"
done

echo "Version update complete!"
