cd $HOME/
start_time=$(date +%s)

get_ram_usage() {
    os_name=$(uname -s)
    case "$os_name" in
        Darwin)
            echo $(vm_stat | awk -F': ' '/Pages (active|inactive|wired down)/ {used+=$2} END {printf "%d MB", used * 4096 / 1048576}')
            ;;
        Linux)
            echo $(free -m | awk '/Mem:/ {print $3 " MB"}')
            ;;
        *)
            echo "Unsupported OS: $os_name"
            ;;
    esac
}

output_file='main_node_output.log'
while ! grep -m1 '0 blocks remaining to be loaded' $output_file > /dev/null; do
    sleep 1
done

end_time=$(date +%s)
time_to_load_blocks=$((end_time - start_time))
echo "Time to Load Blocks: $time_to_load_blocks seconds"

ram_after_loading_blocks=$(get_ram_usage)
echo "RAM After Loading Blocks: $ram_after_loading_blocks"

while ! grep -m1 'starting websocket server' $output_file > /dev/null; do
    sleep 1
done
ram_after_initial_run=$(get_ram_usage)
echo "RAM After Initial Run: $ram_after_initial_run"
