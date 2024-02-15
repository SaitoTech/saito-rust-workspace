#!/usr/bin/env bash

ssh_execute() {
  echo "Executing: ssh $1 $2"
 ssh "$1" "$2"

}



ssh_server() {
  # printf "$REMOTE_SERVER_IP"

  output=$(ssh_execute root@$REMOTE_SERVER_IP "$1" "ls -l")
  echo "$output"


}
ssh_spammer() {
  ssh_execute root@$REMOTE_SPAMMER_IP "$1"
}

configure_server(){
 config_path="$SERVER_DIR/saito-rust/configs/config.json"
  update_cmd="sed -i '/\"verification_threads\":/c\\    \"verification_threads\": "$verification_thread_count",' $config_path"
  ssh_server "$update_cmd"
}

configure_spammer(){
 spammer_config_path="$SPAMMER_DIR/saito-spammer/configs/config.json"
  ssh_spammer "sed -i '/\"burst_count\":/c\\    \"burst_count\": $txs_rate_from_spammer,' $spammer_config_path; sed -i '/\"tx_size\":/c\\    \"tx_size\": $tx_payload_size,' $spammer_config_path; sed -i '/\"verification_threads\":/c\\    \"verification_threads\": $verification_thread_count,' $spammer_config_path"

}

run_test_case() {
  verification_thread_count=$1
  txs_rate_from_spammer=$2
  tx_payload_size=$3



#   # terminate currently running saito-rust and saito-spammer processes
  ssh_server "pkill -f saito-rust"
  ssh_spammer "pkill -f saito-spammer"

#   # clean data/blocks directories
  ssh_server "rm -rf $SERVER_DIR/saito-rust/data/blocks/*"
  ssh_spammer "rm -rf $SPAMMER_DIR/saito-spammer/data/blocks/*"


 #configure saito-rust server
  configure_server

 #Configure saito-spammer
  configure_spammer




# For saito-rust process on the rust server
ssh_server "cd $SERVER_DIR/saito-rust && nohup sh -c 'export RUST_LOG=debug; ~/.cargo/bin/cargo run --release > $SERVER_DIR/saito-rust.log 2>&1 &'"

# For saito-rust process on the spammer server
ssh_spammer "cd $SPAMMER_DIR/saito-spammer && nohup sh -c 'export RUST_LOG=debug; ~/.cargo/bin/cargo run --release > $SPAMMER_DIR/saito-spammer.log 2>&1 &'"

  # Wait till spammer dies or timeout expires
    echo "Waiting for spammer to terminate..."
    while ssh_spammer "pgrep -f saito-spammer" > /dev/null; do
      echo "Spammer is still running. Checking again in 10 seconds..."
      sleep 10
    done
    echo "Spammer has terminated."

    # stop the spammer
    ssh_spammer "pkill -f saito-spammer"

    # check the rust node's memory usage
    local memory_usage=$(ssh_server "ps aux | grep saito-rust | grep -v grep | awk '{print \$4}'")

    # stop the rust node
    ssh_server "pkill -f saito-rust"

    local stats_file="$SERVER_DIR/saito-rust/data/saito.stats"
    local blocks_dir="$SERVER_DIR/saito-rust/data/blocks"

    # find transaction rate at the network thread
    local max_tx_rate_network_thread=$(ssh_server "grep 'network::incoming_msgs' $stats_file | awk '{print \$5}' | tr -d ',' | sort -nr | head -n 1")


    # find the average transaction rate at verification thread
    local max_tx_rate_verification_threads=$(ssh_server "grep 'verification_.*::processed_txs' $stats_file | awk '{print \$11}' | tr -d ',' | sort -nr | head -n 1")

    # find the average size of mempool

    # find the max size of mempool

    # find total block size in disk

    # find the block count in disk
  

    # find the longest chain length
  






  # restart the rust node

  # find the time to load the total blocks from disk

  # find the memory usage after loading blocks from disk

  # connect another rust node from another environment

  # find the time to sync the blockchain

  # find the memory usage of the fetching node

  # find the total transaction count sent

}

test_case_count=-1
test_cases_ver_thread_count=()
test_cases_tx_rate_from_spammer=()
test_cases_tx_payload_size=()

read_test_cases() {
  echo "loading test cases..."

  while read line; do
    verification_thread_count=$(echo $line | cut -d, -f 1)
    spammer_tx_rate=$(echo $line | cut -d, -f 2)
    payload_size=$(echo $line | cut -d, -f 3)

    # echo $verification_thread_count $spammer_tx_rate $payload_size

    test_cases_ver_thread_count+=($verification_thread_count)
    test_cases_tx_rate_from_spammer+=($spammer_tx_rate)
    test_cases_tx_payload_size+=($payload_size)
    test_case_count=$((test_case_count + 1))

    # echo  $test_cases_ver_thread_count $test_cases_tx_rate_from_spammer $test_cases_tx_payload_size $test_case_count
  done < <(tail -n +2 "$test_cases_file")

  echo "$test_case_count test cases loaded"
}

run_perf_test() {
  config_file=$1
  test_cases_file=$2

  echo "Running perf test script..."
  echo "Config File : $config_file"
  echo "Test Cases : $test_cases_file"

  # read the config file
  source $config_file

  echo "REMOTE_SERVER_IP : $REMOTE_SERVER_IP"
  echo "REMOTE_SPAMMER_IP : $REMOTE_SPAMMER_IP"

  # read the test cases
  read_test_cases

  # per each test case run perf tests
  i=$((test_case_count -1))
  echo "running $test_case_count test cases..."
  while [[ $i -ge 0 ]]; do 
    run_test_case "${test_cases_ver_thread_count[$i]}" "${test_cases_tx_rate_from_spammer[$i]}" "${test_cases_tx_payload_size[$i]}"
    sleep 700 
    ((i--))
  done

  echo "finished running test cases"

  # write collected data to a csv file
  echo "writing performance data to csv file..."

  echo "exiting script"
}

run_perf_test $1 $2
