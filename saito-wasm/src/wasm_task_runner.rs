use std::future::Future;
use std::pin::Pin;

use saito_core::common::run_task::RunTask;

pub struct WasmTaskRunner {}

impl RunTask for WasmTaskRunner {
    fn run(&self, task: Pin<Box<dyn Fn() -> () + Send + 'static>>) {
        println!("WasmTaskRunner.run");
        task();
    }
}
