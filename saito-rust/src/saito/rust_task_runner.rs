use tracing::debug;

use saito_core::common::run_task::{RunTask, RunnableTask};

pub struct RustTaskRunner {}

impl RunTask for RustTaskRunner {
    fn run(&self, task: RunnableTask) {
        let handle = tokio::runtime::Handle::current();
        handle.spawn_blocking(move || {
            debug!("new thread started");
            task();
        });
    }
}
