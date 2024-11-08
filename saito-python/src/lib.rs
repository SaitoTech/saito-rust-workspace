pub mod saitowasm;
pub mod wasm_balance_snapshot;
pub mod wasm_block;
pub mod wasm_blockchain;
pub mod wasm_configuration;
pub mod wasm_consensus_values;
pub mod wasm_hop;
pub mod wasm_io_handler;
pub mod wasm_peer;
pub mod wasm_peer_service;
pub mod wasm_slip;
pub mod wasm_task_runner;
pub mod wasm_time_keeper;
pub mod wasm_transaction;
pub mod wasm_wallet;

use pyo3::prelude::*;

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

#[pyfunction]
async fn initialize() {
    println!("222");
    crate::saitowasm::initialize().await;
}

/// A Python module implemented in Rust.
#[pymodule]
fn saito_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_function(wrap_pyfunction!(initialize, m)?)?;
    Ok(())
}
