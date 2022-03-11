pub mod saitowasm;
mod wasm_io_handler;
mod wasm_task_runner;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
