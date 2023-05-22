pub mod saitowasm;
mod wasm_block;
mod wasm_blockchain;
mod wasm_configuration;
mod wasm_io_handler;
mod wasm_peer;
mod wasm_peer_service;
mod wasm_slip;
mod wasm_task_runner;
mod wasm_time_keeper;
mod wasm_transaction;
mod wasm_wallet;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
