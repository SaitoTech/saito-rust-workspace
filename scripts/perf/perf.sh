#!/bin/bash

SCRIPT_DIR=$(dirname "$0")
cd "$SCRIPT_DIR"
config_file="./perf_config.json"

get_config_value() {
    jq -r ".$1" "$config_file"
}

main_node_dir=$(get_config_value 'main_node_dir')
spammer_node_dir=$(get_config_value 'spammer_node_dir')
output_csv="./perf_result.csv"

echo "tx_rate_from_spammer,tx_payload_size,verification_thread_count,max_tx_rate_at_network_thread,max_tx_rate_at_verification_threads,total_txs,block_count,longest_chain_length,total_block_size,average_block_size,transaction_throughput,data_throughput" > "$output_csv"

is_process_running() {
    pm2 show "$1" > /dev/null 2>&1
    return $?
}

# Function to check if a command exists
command_exists() {
    command -v "$@" > /dev/null 2>&1
}

install_pm2() {
    echo "Checking for PM2 installation..."

    if command_exists pm2; then
        echo "PM2 is already installed."
    else
        echo "PM2 not found. Installing PM2..."

        if command_exists node && command_exists npm; then
            echo "Node.js and npm are already installed."
        else
            echo "Installing Node.js and npm..."

            curl -sL https://deb.nodesource.com/setup_14.x | sudo -E bash -
            sudo apt-get install -y nodejs
        fi

        sudo npm install -g pm2

        echo "PM2 has been installed."
    fi
}

get_ram_usage() {
    os_name=$(uname -s)
    case "$os_name" in
        Darwin)
            echo $(vm_stat | awk -F': ' '/Pages (active|inactive|wired down)/ {used+=$2} END {print used * 4096 / 1048576 " MB"}')
            ;;
        Linux)
            echo $(free -m | awk '/Mem:/ {print $3 " MB"}')
            ;;
        *)
            echo "Unsupported OS: $os_name"
            ;;
    esac
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

    (
        cd "$dir" || exit
        pm2 start "RUST_LOG=debug cargo run" --name "$service_name" --no-autorestart
    )
}

create_or_append_issuance() {
    local dir="$1/data/issuance"
    local file="$dir/issuance"

    mkdir -p "$dir"

    if [ -f "$file" ]; then
        echo "100000	v4S4wiwJQVP3fNZkpYLG59PFNMXmHnNNpE4kREfCeJic	Normal" >> "$file"
    else
        echo "100000	v4S4wiwJQVP3fNZkpYLG59PFNMXmHnNNpE4kREfCeJic	Normal" > "$file"
    fi
}

loader() {
    local i=0
    local max_dots=30
    local delay=0.5 

    while true; do
        STATUS=$(pm2 show spammer_node | grep "status" | awk '{print $4}')
        if [ "$STATUS" = "online" ]; then
            printf "\rProcessing " 
            for (( j=0; j<i; j++ )); do
                printf "."
            done

            ((i=(i+1)%max_dots))

            sleep $delay
        else
            break
        fi
    done

    printf "\r%s\n" "$(for (( j=0; j<max_dots; j++ )); do printf " "; done)" 
}

create_or_append_issuance "$main_node_dir"
install_pm2
test_configs=$(jq -c '.perf_tests[]' "$config_file")

for config in $test_configs; do
    echo "Running test with configuration: $config"

    verification_threads=$(echo "$config" | jq '.verification_threads')
    burst_count=$(echo "$config" | jq '.burst_count')
    tx_payload_size=$(echo "$config" | jq '.tx_payload_size')

    clear_blocks_directory "$main_node_dir"
    clear_blocks_directory "$spammer_node_dir"

    update_config_file "$main_node_dir/configs/config.json" ".server.verification_threads = $verification_threads"
    update_config_file "$spammer_node_dir/configs/config.json" ".spammer.burst_count = $burst_count | .spammer.tx_size = $tx_payload_size"

    start_pm2_service "$main_node_dir" "main_node"

    until is_process_running "main_node"; do
        sleep 1
    done
    echo "Main node is running."

    start_pm2_service "$spammer_node_dir" "spammer_node"
    echo "Spammer node is started."

    while true; do
        stats_file="$main_node_dir/data/saito.stats"
        output_csv="./perf_result.csv"

        echo "Waiting for 5 minutes to gather data..."
        sleep 10  # Sleep for 300 seconds (5 minutes)

        echo "Spammer process has stopped."
        echo "Stopping main node and spammer node"
        pm2 stop spammer_node
        pm2 stop main_node

        max_tx_rate_network_thread=$(grep "network::incoming_msgs" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)
        max_tx_rate_verification_threads=$(grep "verification_.*::processed_txs" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)
        total_txs=$(grep "routing::incoming_msgs" "$stats_file" | awk '{print $5}' | tr -d ',' | sort -nr | head -n 1)
        block_count=$(grep "blockchain::state" "$stats_file" | awk '{print $8}' | tr-d ',' | sort -nr | head -n 1)
        longest_chain_length=$(grep "blockchain::state" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)
        txs_in_blocks=$(grep "blockchain::state" "$stats_file" | awk '{print $17}' | tr -d ',' | sort -nr | head -n 1)

        total_block_size=$(du -ck "$main_node_dir/data/blocks/"* | grep "total" | awk '{print $1}')
        num_block_files=$(find "$main_node_dir/data/blocks/" -type f | wc -l)
        average_block_size=$(echo "$total_block_size $num_block_files" | awk '{print int($1/$2)}')

        transaction_throughput=$(echo "scale=2; $txs_in_blocks / 300" | bc)
        data_throughput=$(echo "scale=2; $tx_payload_size * $txs_in_blocks / 300" | bc)

        echo "Transaction Throughput (tx/s): $transaction_throughput"
        echo "Data Throughput (bytes/s): $data_throughput"

        echo "$burst_count,$tx_payload_size,$verification_threads,$max_tx_rate_network_thread,$max_tx_rate_verification_threads,$total_txs,$block_count,$longest_chain_length,$total_block_size,$average_block_size,$transaction_throughput,$data_throughput" >> "$output_csv"

        break
        sleep 1
    done

    pm2 list
done
