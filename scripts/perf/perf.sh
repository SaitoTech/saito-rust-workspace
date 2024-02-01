
#!/bin/bash

SCRIPT_DIR=$(dirname "$0")

cd "$SCRIPT_DIR"

config_file="./perf_config.json"

get_config_value() {
    jq -r ".$1.$2" "$config_file"
}




# Main Node Configurations
main_node_dir=$(get_config_value 'main_node' 'dir')
main_node_ip=$(get_config_value 'main_node' 'ip')
main_node_port=$(get_config_value 'main_node' 'port')
main_node_ssh_dir=$(get_config_value 'main_node' 'ssh_directory')

echo $main_node_ip $main_node_ssh_dir

# Spammer Node Configurations
spammer_node_dir=$(get_config_value 'spammer_node' 'dir')
spammer_node_ip=$(get_config_value 'spammer_node' 'ip')
spammer_node_port=$(get_config_value 'spammer_node' 'port')
spammer_node_ssh_dir=$(get_config_value 'spammer_node' 'ssh_directory')

 intermediate_host="deployment.saito.network"


echo "tx_rate_from_spammer,tx_payload_size,verification_thread_count,max_tx_rate_at_network_thread,max_tx_rate_at_verification_threads,total_txs,block_count,longest_chain_length,total_block_size,average_block_size,time_to_load_blocks,time_to_fetch_blocks,ram_after_initial_run,ram_after_loading_blocks,ram_after_fetching_blocks" > "$output_csv"



is_process_running_remote() {
    local target_ip="$1"
    local service_name="$2"
    local intermediate_host="deployment.saito.network"

    echo "Checking if $service_name is running on $target_ip via $intermediate_host"

    ssh -t "root@$intermediate_host" "ssh -t root@$target_ip 'pgrep -f $service_name > /dev/null 2>&1'"
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

clear_blocks_directory_remote() {
    local intermediate_host="deployment.saito.network"
    local target_ip="$1"
    local ssh_directory="$2"

    echo "Clearing blocks directory on remote node: $target_ip via $intermediate_host"

    ssh -t "root@$intermediate_host" "ssh -t root@$target_ip 'bash -l -c \"cd \\\"\$HOME/$ssh_directory\\\" && if [ -d \\\"data/blocks\\\" ]; then rm -rf data/blocks/*; echo Cleared all contents in data/blocks; else echo Directory data/blocks does not exist.; fi\"'"
}


get_latest_block_name() {
    ls -Art "$1/data/blocks" | tail -n 1
}

update_config_file_remote() {
    local intermediate_host="deployment.saito.network"
    local target_ip="$1"
    local ssh_directory="$2"
    local config_file="$3"
    local jq_script="$4"

    echo "Updating config file on remote node: $target_ip via $intermediate_host"

    ssh -t "root@$intermediate_host" "ssh -t root@$target_ip 'bash -l -c \"cd \\\"\$HOME/$ssh_directory\\\" && jq \\\"$jq_script\\\" $config_file > temp.json && mv temp.json $config_file\"'"
}




start_pm2_service_remote() {
    local target_ip="$1"
    local service_name="$2"
    local ssh_directory="$3"


    local log_directory="${ssh_directory}"

    local stdout_log="${log_directory}/${service_name}_out.log"
    local stderr_log="${log_directory}/${service_name}_err.log"

    echo "Starting PM2 service on remote node: $target_ip"

    ssh -t root@$target_ip "bash -l -c 'mkdir -p \"\$HOME/$log_directory\" && cd \"\$HOME/$ssh_directory\" && pm2 start \"RUST_LOG=debug cargo run\" --name \"$service_name\" --output \"\$HOME/$stdout_log\" --error \"\$HOME/$stderr_log\" --no-autorestart'"
}




stop_pm2_service_remote() {
    local intermediate_host="deployment.saito.network"
    local target_ip="$1"
    local service_name="$2"

    echo "Stopping PM2 service $service_name on remote node: $target_ip via $intermediate_host"

    ssh -t "root@$intermediate_host" "ssh -t root@$target_ip 'pm2 stop $service_name'"
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


copy_remote_stats_file() {
    local intermediate_host="deployment.saito.network"
    local target_ip="$1"
    local remote_directory="$2"
    local local_directory="$3"  # Local directory where the file will be saved
    local stats_file_path="$remote_directory/data/saito.stats"
    local temp_file="temp_saito_stats"

    echo "Copying stats file from remote node: $target_ip to intermediate host $intermediate_host"

    ssh "root@$intermediate_host" "scp root@$target_ip:$stats_file_path ~/$temp_file"

    echo "Copying stats file from intermediate host $intermediate_host to local machine"

    scp "root@$intermediate_host:~/$temp_file" "$local_directory"

    ssh "root@$intermediate_host" "rm -f ~/$temp_file"
}



loader() {
    local intermediate_host="deployment.saito.network"
    local target_ip="$1"  # The IP of the server where the spammer_node is running
    local service_name="spammer_node"
    local i=0
    local max_dots=30
    local delay=0.5 

    while true; do
        # Execute pm2 show command on the remote server to get the status
        STATUS=$(ssh -t "root@$intermediate_host" "ssh -t root@$target_ip 'pm2 show $service_name | grep \"status\" | awk \"{print \$4}\"'")
        
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
    verification_threads=$(echo "$config" | jq '.verification_threads')
    burst_count=$(echo "$config" | jq '.burst_count')
    tx_payload_size=$(echo "$config" | jq '.tx_payload_size')

    clear_blocks_directory_remote "$main_node_ip" "$main_node_ssh_dir"
    clear_blocks_directory_remote "$spammer_node_ip" "$spammer_node_ssh_dir"

    update_config_file_remote "$main_node_ip" "$main_node_ssh_dir" "configs/config.json" ".server.verification_threads = $verification_threads"
    update_config_file_remote "$spammer_node_ip" "$spammer_node_ssh_dir" "configs/config.json" ".spammer.burst_count = $burst_count | .spammer.tx_payload_size = $tx_payload_size | .server.verification_threads = $verification_threads"

    echo $main_node_ip
    start_pm2_service_remote $main_node_ip "main_node" $main_node_ssh_dir
    until is_process_running_remote $main_node_ip "main_node"; do
        sleep 1
    done
    echo "Main node is running."

    start_pm2_service_remote $spammer_node_ip "spammer_node" $spammer_node_ssh_dir

    while true; do
        output_csv="./perf_result.csv"
        echo "processing"

        intermediate_host="deployment.saito.network"
 STATUS=$(ssh -t "root@$intermediate_host" "ssh -t root@$spammer_node_ip 'pm2 show spammer_node | grep -m1 \"│ status\" | awk -F \"│\" \"{gsub(/ /, \\\"\\\", \\\$3); print \\\$3}\"'" | grep -o 'online\|stopped' | tr -d '[:space:]')




        echo "$STATUS" 

        if [ "$STATUS" != "online" ]; then
            echo "Spammer process has stopped."

            stop_pm2_service_remote "$main_node_ip" "main_node"
            copy_remote_stats_file "$main_node_ip" "$main_node_ssh_dir" "."

            stats_file="./temp_saito_stats"
            max_tx_rate_network_thread=$(grep "network::incoming_msgs" "$stats_file" | awk '{print $5}' | tr -d ',' | sort -nr | head -n 1)
            max_tx_rate_verification_threads=$(grep "verification_.*::processed_txs" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)
            total_txs=$(grep "routing::incoming_msgs" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)
            block_count=$(grep "blockchain::state" "$stats_file" | awk '{print $8}' | tr -d ',' | sort -nr | head -n 1)
            longest_chain_length=$(grep "blockchain::state" "$stats_file" | awk '{print $11}' | tr -d ',' | sort -nr | head -n 1)

            total_block_size=$(ssh -t "root@$intermediate_host" "ssh -t root@$target_ip 'du -ck \\\"\$HOME/$ssh_directory/data/blocks/*\\\" | grep \"total\" | awk \"{print \$1}\"'")
            num_block_files=$(ssh -t "root@$intermediate_host" "ssh -t root@$target_ip 'find \\\"\$HOME/$ssh_directory/data/blocks/\\\" -type f | wc -l'")

            start_pm2_service_remote $main_node_ip "main_node" $main_node_ssh_dir

     ssh root@$main_node_ip "tail -f $HOME/$main_node_ssh_dir/main_node_output.log" | grep --line-buffered -m1 "0 blocks remaining to be loaded"

     ssh root@$main_node_ip "
case \$(uname) in
    Linux) free -m | awk '/Mem:/ {print \$3 \" MB\"}' ;;
    Darwin) vm_stat | awk -F': ' '/Pages active|Pages inactive|Pages wired down/ {used+=\$2} END {print used * 4096 / 1048576 \" MB\"}' ;;
    *) echo \"Unsupported OS\" ;;
esac
"


# Execute the script remotely and capture the output
time_loading_output=$(ssh -t root@$main_node_ip "bash -l -c '$remote_script'")

# Display the captured output, which includes the time taken
echo "$time_loading_output"

# Execute the script on the main_node_ip and capture the output
time_loading_output=$(ssh -t root@$main_node_ip "bash -l -c '$remote_script_formatted'")

# Display the captured output, which includes the time taken
echo "$time_loading_output"


            # time_to_load_blocks=$(echo "$remote_output" | grep "Time to Load Blocks" | awk '{print $5}')
            # echo "Time to load blocks: $time_to_load_blocks seconds"

            # ram_after_loading_blocks=$(echo "$remote_output" | grep "RAM After Loading Blocks" | awk -F': ' '{print $2}')
            # echo "RAM after loading blocks: $ram_after_loading_blocks"

            # ram_after_initial_run=$(echo "$remote_output" | grep "RAM After Initial Run" | awk -F': ' '{print $2}')
            # echo "RAM after initial run: $ram_after_initial_run"

            # echo "All blocks loaded. Time taken: $time_to_load_blocks seconds."
            # echo "RAM usage before initial run: $ram_after_initial_run"
            # echo "Time to fetch blocks : $time_to_fetch_blocks"
            # echo "RAM usage after fetching blocks blocks: $ram_after_fetching_blocks"

            echo "$burst_count,$tx_payload_size,$verification_threads,$max_tx_rate_network_thread,$max_tx_rate_verification_threads,$total_txs,$block_count,$longest_chain_length,$total_block_size,$average_block_size,$time_to_load_blocks,$time_to_fetch_blocks,$ram_after_initial_run,$ram_after_loading_blocks,$ram_after_fetching_blocks" >> "$output_csv"

            echo "CSV file created at $output_csv"
            pm2 "kill"
            break
        fi
        sleep 1
    done

    pm2 list
done
