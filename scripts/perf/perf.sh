
#!/bin/bash

config_file="./scripts/perf/perf_config.json"

get_config_value() {
    jq -r ".$1" "$config_file"
}

main_node_dir=$(get_config_value 'main_node_dir')
spammer_node_dir=$(get_config_value 'spammer_node_dir')
output_csv="$main_node_dir/perf_result.csv"


echo "tx_rate_from_spammer,tx_payload_size,verification_thread_count,max_tx_rate_at_network_thread,max_tx_rate_at_verification_threads,total_txs,block_count,longest_chain_length,total_block_size,average_block_size,time_to_load_blocks,time_to_fetch_blocks,ram_after_initial_run,ram_after_loading_blocks,ram_after_fetching_blocks" > "$output_csv"



is_process_running() {
    pm2 show "$1" > /dev/null 2>&1
    return $?
}


clear_blocks_directory() {
    echo "$1"
    dir_path="$1/data/blocks"
    if [ -d "$dir_path" ]; then
        rm -rf "$dir_path"/*
        echo "Cleared all contents in $dir_path"
    else
        echo "Directory $dir_path does not exist."
    fi
}

get_latest_block_name() {
    ls -Art "$1/data/blocks" | tail -n 1
}

update_config_file() {
    local config_path="$1"
    local jq_script="$2"
    jq "$jq_script" "$config_path" > temp.json && mv temp.json "$config_path"
}

start_pm2_service() {
    
    local dir="$1"
    local service_name="$2"
    echo $dir
    cd "$dir"
    pm2 start "RUST_LOG=debug cargo run" --name "$service_name" --no-autorestart
}
# Iterate over each test configuration
test_configs=$(jq -c '.perf_tests[]' "$config_file")
for config in $test_configs; do
    echo "Running test with configuration: $config"

    verification_threads=$(echo "$config" | jq '.verification_threads')
    burst_count=$(echo "$config" | jq '.burst_count')
    tx_payload_size=$(echo "$config" | jq '.tx_payload_size')

    # Clear directories, update config files, and start services
    clear_blocks_directory "$main_node_dir"
    clear_blocks_directory "$spammer_node_dir"

    update_config_file "$main_node_dir/configs/config.json" ".server.verification_threads = $verification_threads"
    update_config_file "$spammer_node_dir/configs/config.json" ".spammer.burst_count = $burst_count | .spammer.tx_payload_size = $tx_payload_size"

    start_pm2_service "$main_node_dir" "main_node"
    until is_process_running "main_node"; do sleep 1; done
    echo "Main node is running."

    start_pm2_service "$spammer_node_dir" "spammer_node"
    echo "Spammer node is started."


    while true; do
    stats_file="$main_node_dir/data/saito.stats"
    output_csv="$main_node_dir/perf_result.csv"

    STATUS=$(pm2 show spammer_node | grep "status" | awk '{print $4}')
    echo "processing"

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


                 # Restart main node
                echo "Restarting main node and monitoring for block loading."
                pm2 start "main_node"
                start_time=$(date +%s)

                output_file="$main_node_dir/main_node_output.log"
                pm2 logs main_node > "$output_file" 2>&1 &

                output_file_spammer_node="$spammer_node_dir/spammer_node_output.log"
               pm2 logs spammer_node > "$output_file_spammer_node" 2>&1 &  

                # Time taken to load blocks
                while ! grep -m1 "0 blocks remaining to be loaded" "$output_file" > /dev/null; do
                    sleep 1 
                done
                end_time=$(date +%s)
                time_to_load_blocks=$((end_time - start_time))

                # Check RAM usage after loading blocks
                ram_after_loading_blocks=$(vm_stat | awk -F': ' '/Pages (active|inactive|wired down)/ {used+=$2} END {print used * 4096 / 1048576 " MB"}')

                # Wait for 'starting websocket server' to measure initial RAM usage
                while ! grep -m1 "starting websocket server" "$output_file" > /dev/null; do
                    sleep 1
                done
                ram_after_initial_run=$(vm_stat | awk -F': ' '/Pages (active|inactive|wired down)/ {used+=$2} END {print used * 4096 / 1048576 " MB"}')

                # check if main node has properly started
                until is_process_running "main_node"; do
                sleep 1
                done

                # clear blocks in spammer node directory
                clear_blocks_directory "$spammer_node_dir"

                # Start spammer node and monitor for fetching blocks
                echo "Starting spammer node and monitoring for fetching blocks."
                pm2 start "spammer_node"
                fetch_start_time=$(date +%s)

                latest_block_name=$(get_latest_block_name "$main_node_dir")
                if [ -z "$latest_block_name" ]; then
                    echo "No block files found in $spammer_node_dir/data/blocks"
                    exit 1
                fi

                block_identifier=$(echo "$latest_block_name" | grep -oE '[^-]*\.sai$' | sed 's/\.sai$//')
                if [ -z "$block_identifier" ]; then
                    echo "Unable to extract block identifier from $latest_block_name"
                    exit 1
                fi
                search_phrase="fetching block : .*\/$block_identifier"
                echo "Latest block name: $latest_block_name"
                echo "Searching for phrase: $search_phrase"
                while ! grep -m1 "$search_phrase" "$output_file_spammer_node" > /dev/null; do
                    sleep 1 
                done

                fetch_end_time=$(date +%s)
                time_to_fetch_blocks=$((fetch_end_time - fetch_start_time))
                ram_after_fetching_blocks=$(vm_stat | awk -F': ' '/Pages (active|inactive|wired down)/ {used+=$2} END {print used * 4096 / 1048576 " MB"}')



                echo "All blocks loaded. Time taken: $time_to_load_blocks seconds."
                echo "RAM usage before initial run: $ram_after_initial_run"
                echo "Time to fetch blocks : $time_to_fetch_blocks"
                echo "RAM usage after fetching blocks blocks: $ram_after_fetching_blocks"




                echo "$burst_count,$tx_payload_size,$verification_threads,$max_tx_rate_network_thread,$max_tx_rate_verification_threads,$total_txs,$block_count,$longest_chain_length,$total_block_size,$average_block_size,$time_to_load_blocks,$time_to_fetch_blocks,$ram_after_initial_run,$ram_after_loading_blocks,$ram_after_fetching_blocks" >> "$output_csv"


                echo "CSV file created at $output_csv"

                pm2 "kill"

        break
    fi
    sleep 1
done


    pm2 list
done



