
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
        Darwin) # 
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
    # echo "$1"
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
    local log_dir="$dir/logs" 
    echo "Service directory: $dir"
    mkdir -p "$log_dir"  


    local stdout_log="$(realpath "$log_dir")/stdout.log"
    local stderr_log="$(realpath "$log_dir")/stderr.log"

    (
        cd "$dir" || exit
        pm2 start "RUST_LOG=debug cargo run" --name "$service_name" --no-autorestart \
            --output "$stdout_log" --error "$stderr_log"
    )
    echo "Logs will be saved in $log_dir"
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

get_last_tx_count() {
    local log_file="$1"
    local search_phrase="$2"
    local count=$(tac "$log_file" | grep -m1 "$search_phrase" | grep -oE '[0-9]+' | tail -1)
    echo "${count:-0}"
}


stats_file="$main_node_dir/data/saito.stats"

restart() {

    local burst_count="$1"
    local verification_threads=10

     pm2 kill;
    update_config_file "$main_node_dir/configs/config.json" ".server.verification_threads = $verification_threads"
    update_config_file "$spammer_node_dir/configs/config.json" ".spammer.burst_count = $burst_count | .spammer.tx_size = $payload_size | .server.verification_threads = $verification_threads"

    clear_blocks_directory "$main_node_dir"
    clear_blocks_directory "$spammer_node_dir"

    if [ -f "$main_node_dir/logs/stdout.log" ]; then
    rm "$main_node_dir/logs/stdout.log"
    echo "Removed $main_node_dir/logs/stdout.log"
    else
    echo "Log file not found: $main_node_dir/logs/stdout.log"
    fi

if [ -f "$spammer_node_dir/logs/stdout.log" ]; then
    rm "$spammer_node_dir/logs/stdout.log"
    echo "Removed $spammer_node_dir/logs/stdout.log"
else
    echo "Log file not found: $spammer_node_dir/logs/stdout.log"
fi

    start_pm2_service "$main_node_dir" "main_node"

    until is_process_running "main_node"; do sleep 1; done
    echo "Main node is running."

    start_pm2_service "$spammer_node_dir" "spammer_node"

    until is_process_running "spammer_node"; do sleep 1; done
    echo "Spammer node is running."
 
}



payload_sizes=(200 1024 512000 1048576 2097152)

test_with_payload_size() {
    local payload_size=$1;
    burst_count=1000
    verification_threads=8
    local payload_size="$1"
    restart "$burst_count" "$verification_threads"
    increase_count=0
    max_allowed_backlog=1000
    prev_diff=0
    prev_txs_per_second=0
    timer=60
    max_burst_count=50000


    output_csv="./performance_metrics-$payload_size.csv"

    while true; do
        

        echo "Current Burst Count: $burst_count"
        echo "payload size $payload_size"
        echo "verification thread $verification_threads"

        echo "Waiting for 'total transactions sent 1' in main_node logs..."
        while ! grep -m1 "total transactions sent 1" "$spammer_node_dir/logs/stdout.log" > /dev/null; do
        sleep 1
        done
        echo "'total transactions sent 1' detected. Starting $timer-second timer..."
        sleep $timer

        main_node_tx_count=$(get_last_tx_count "$main_node_dir/logs/stdout.log" "received transactions consensus")
        spammer_node_tx_count=$(get_last_tx_count "$spammer_node_dir/logs/stdout.log" "total transactions sent")


        echo "Main Node TX Count: $main_node_tx_count"
        echo "Spammer Node TX Count: $spammer_node_tx_count"
        diff=$((spammer_node_tx_count - main_node_tx_count))
        echo "current backlog for transactions $diff"

        if [ "$diff" -gt "$max_allowed_backlog" ]; then
            ((increase_count++))
        fi

        tps=$(bc <<< "scale=2; $main_node_tx_count / $timer")
        tota_payload_size=$(bc <<< "scale=2; $payload_size * $main_node_tx_count / 1024 / 1024")
       dtp=$(bc <<< "scale=2; $tota_payload_size / $timer")
     
        txs_per_second="$tps tx/s"
        data_throughput="$dtp mb/s "


        echo "Tx processed per second: $txs_per_second"
        if [ ! -f "$output_csv" ]; then
        echo "burst_rate, payload_size, processed_txs, txs_per_second, data_throughput" > "$output_csv"
        fi

        echo "$burst_count, $payload_size, $main_node_tx_count, $txs_per_second $data_throughput" >> "$output_csv"


        if [ ! -z "$tps" ] && echo "$tps > $prev_txs_per_second" | bc -l | grep -q 1; then
            echo "Increase in max transaction rate at consensus observed."
            prev_txs_per_second=$tps
        else
            echo "No significant increase in max transaction rate at consensus."
            # pm2 kill
            # break
        fi

        if [ "$increase_count" -ge 1 ]; then
            echo "Load increase detected; main node can't keep up."
            # pm2 kill
            # break
        fi

        burst_count=$((burst_count + 1000)) 

        if [ "$burst_count" -ge "$max_burst_count" ]; then
            echo "Maximum burst count of $max_burst_count reached. Stopping..."
            break
        fi
        restart "$burst_count" "$verification_threads"
    done
}

for size in "${payload_sizes[@]}"; do
    test_with_payload_size "$size"
done



