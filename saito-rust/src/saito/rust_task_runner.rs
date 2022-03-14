use std::future::Future;
use std::pin::Pin;

use log::debug;

use saito_core::common::run_task::RunTask;

pub struct RustTaskRunner {}

impl RunTask for RustTaskRunner {
    fn run(&self, task: Pin<Box<dyn Fn() -> () + Send + 'static>>) {
        let handle = tokio::runtime::Handle::current();
        handle.spawn_blocking(move || {
            debug!("new thread started");
            task();
        });
    }
}
