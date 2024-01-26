#!/bin/bash

config_file="./scripts/perf/perf_config.json"

main_node_dir=$(jq -r '.main_node_dir' "$config_file")
spammer_node_dir=$(jq -r '.spammer_node_dir' "$config_file")
verification_threads=$(jq -r '.verification_threads' "$config_file")
burst_count=$(jq -r '.burst_count' "$config_file")
tx_size=$(jq -r '.tx_size' "$config_file")

is_process_running() {
    pm2 show "$1" > /dev/null 2>&1
    return $?
}

clear_blocks_directory() {
    dir_path="$1/data/blocks"
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
jq ".server.verification_threads = $verification_threads" "$main_node_config" > temp.json && mv temp.json "$main_node_config"

spammer_node_config="$spammer_node_dir/configs/config.json"
jq ".spammer.burst_count = $burst_count | .spammer.tx_size = $tx_size" "$spammer_node_config" > temp.json && mv temp.json "$spammer_node_config"

cd "$main_node_dir"
pm2 start "RUST_LOG=debug cargo run" --name "main_node" --no-autorestart
until is_process_running "main_node"; do
    sleep 1
done
echo "Main node is running."

cd "$spammer_node_dir"
pm2 start "RUST_LOG=debug cargo run" --name "spammer_node" --no-autorestart
echo "Spammer node is started."

while true; do
    stats_file="$main_node_dir/data/saito.stats"
    output_csv="$main_node_dir/perf_result.csv"

    STATUS=$(pm2 show spammer_node | grep "status" | awk '{print $4}')
    echo "$STATUS"

    if [ "$STATUS" != "online" ]; then
        echo "Spammer process has stopped."
        echo "Stopping main node."
        pm2 stop main_node


        max_tx_rate_network_thread=$(grep "network::incoming_msgs" "$stats_file" | awk '{print $5}' | tr -d ',' | sort -nr | head -n 1)
        max_tx_rate_verification_threads=$(grep "verification_.*::processed_txs" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)
        total_txs=$(grep "routing::incoming_msgs" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)
        block_count=$(grep "blockchain::state" "$stats_file" | awk '{print $8}' | tr -d ',' | sort -nr | head -n 1)
        longest_chain_length=$(grep "blockchain::state" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)


        total_block_size=$(du -ck "$main_node_dir/data/blocks/"* | grep "total" | awk '{print $1}')
        num_block_files=$(find "$main_node_dir/data/blocks/" -type f | wc -l)
        average_block_size=$(echo "$total_block_size $num_block_files" | awk '{print int($1/$2)}')


        
        echo "Restarting main node and monitoring for block loading."
        pm2 start "main_node"
        start_time=$(date +%s)



        output_file="$main_node_dir/main_node_output.log"
        pm2 logs main_node > "$output_file" 2>&1 &

   
        while ! grep -m1 "0 blocks remaining to be loaded" "$output_file" > /dev/null; do
            sleep 5 
        done
        end_time=$(date +%s)
        time_to_load_blocks=$((end_time - start_time))

        time_to_fetch_blocks="N/A"
        ram_after_initial_run="N/A"
        ram_after_loading_blocks="N/A"
        ram_after_fetching_blocks="N/A"








echo "tx_rate_from_spammer,tx_size,verification_thread_count,max_tx_rate_at_network_thread,max_tx_rate_at_verification_threads,total_txs,block_count,longest_chain_length,total_block_size,average_block_size,time_to_load_blocks,time_to_fetch_blocks,ram_after_initial_run,ram_after_loading_blocks,ram_after_fetching_blocks" > "$output_csv"

# Write the data in snake_case format to the CSV file
echo "$burst_count,$tx_size,$verification_threads,$max_tx_rate_network_thread,$max_tx_rate_verification_threads,$total_txs,$block_count,$longest_chain_length,$total_block_size,$average_block_size,$time_to_load_blocks,$time_to_fetch_blocks,$ram_after_initial_run,$ram_after_loading_blocks,$ram_after_fetching_blocks" >> "$output_csv"


        echo "CSV file created at $output_csv"

        break
    fi
    sleep 5
done

pm2 list
