#!/bin/bash

config_file="./scripts/configs/perf_config.json"

main_node_dir=$(jq -r '.main_node_dir' $config_file)
spammer_node_dir=$(jq -r '.spammer_node_dir' $config_file)
verification_threads=$(jq -r '.verification_threads' $config_file)
burst_count=$(jq -r '.burst_count' $config_file)
tx_size=$(jq -r '.tx_size' $config_file)

is_process_running() {
    pm2 show $1 > /dev/null 2>&1
    return $?
}


clear_blocks_directory() {
    dir_path=$1/data/blocks
    if [ -d "$dir_path" ]; then
        rm -rf "$dir_path"/*
        echo "Cleared all contents in $dir_path"
    else
        echo "Directory $dir_path does not exist."
    fi
}

clear_blocks_directory "$main_node_dir"


clear_blocks_directory "$spammer_node_dir"


main_node_config="$main_node_dir/configs/config.json"
jq ".server.verification_threads = $verification_threads" $main_node_config > temp.json && mv temp.json $main_node_config


spammer_node_config="$spammer_node_dir/configs/config.json"
jq ".spammer.burst_count = $burst_count | .spammer.tx_size = $tx_size" $spammer_node_config > temp.json && mv temp.json $spammer_node_config


cd $main_node_dir
pm2 start "RUST_LOG=debug cargo run" --name "main_node" --no-autorestart
until is_process_running "main_node"; do
    sleep 1
done
echo "Main node is running."


cd $spammer_node_dir
pm2 start "RUST_LOG=debug cargo run" --name "spammer_node" --no-autorestart
echo "Spammer node is started."

# create csv performance metrics

# 


pm2 list
