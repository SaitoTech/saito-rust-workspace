use std::future::Future;
use std::pin::Pin;
use std::process::Output;

pub trait RunTask {
    fn run(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);
}
