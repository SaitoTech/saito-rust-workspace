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

run_test_case() {
  verification_thread_count=$1
  txs_rate_from_spammer=$2
  tx_payload_size=$3
  

  # connect to remote machine

  # terminate currently running saito-rust and saito-spammer processes
  ssh_server "pkill -f saito-rust"
  ssh_spammer "pkill -f saito-spammer"

  # clean data/blocks directories
ssh_server "rm -rf $SERVER_DIR/saito-rust/data/blocks/*"
  ssh_spammer "rm -rf $SPAMMER_DIR/saito-spammer/data/blocks/*"

  # start saito-rust process
  ssh_server "$SERVER_DIR/target/debug/saito-rust&"

  # start saito-spammer process
  ssh_spammer "$SPAMMER_DIR/target/debug/saito-spammer-new&"

  # wait till spammer dies or timeout expires

  # stop the spammer

  # check the rust node's memory usage

  # stop the rust node

  # find transaction rate at the network thread

  # find the average transaction rate at verification thread

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

test_case_count=0
test_cases_ver_thread_count=()
test_cases_tx_rate_from_spammer=()
test_cases_tx_payload_size=()

read_test_cases() {
  echo "loading test cases..."

  while read line; do
    verification_thread_count=$(echo $line | cut -d, -f 1)
    spammer_tx_rate=$(echo $line | cut -d, -f 2)
    payload_size=$(echo $line | cut -d, -f 3)

    test_cases_ver_thread_count+=(verification_thread_count)
    test_cases_tx_rate_from_spammer+=(spammer_tx_rate)
    test_cases_tx_payload_size+=(payload_size)
    test_case_count=$((test_case_count + 1))
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
  i=test_case_count
  echo "running $test_case_count test cases..."
  while [[ $i -gt 0 ]]; do
    i=$i-1
    run_test_case "${test_cases_ver_thread_count[$i]}" "${test_cases_tx_rate_from_spammer[$i]}" "${test_cases_tx_payload_size[$i]}"
  done

  echo "finished running test cases"

  # write collected data to a csv file
  echo "writing performance data to csv file..."

  echo "exiting script"
}

run_perf_test $1 $2
