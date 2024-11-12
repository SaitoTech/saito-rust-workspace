pub mod py_balance_snapshot;
pub mod py_block;
pub mod py_blockchain;
pub mod py_configuration;
pub mod py_consensus_values;
pub mod py_hop;
pub mod py_io_handler;
pub mod py_peer;
pub mod py_peer_service;
pub mod py_slip;
pub mod py_task_runner;
pub mod py_time_keeper;
pub mod py_transaction;
pub mod py_wallet;
pub mod saitopython;

use pyo3::prelude::*;

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

#[pyfunction]
async fn initialize() {
    println!("222");
    crate::saitopython::initialize().await;
}

/// A Python module implemented in Rust.
#[pymodule]
fn saito_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_function(wrap_pyfunction!(initialize, m)?)?;
    Ok(())
}
