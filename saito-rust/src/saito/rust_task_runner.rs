use std::future::Future;
use std::pin::Pin;

use saito_core::common::run_task::RunTask;

pub struct RustTaskRunner {}

impl RunTask for RustTaskRunner {
    fn run(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        tokio::spawn(async move {
            task.await;
        });
    }
}
