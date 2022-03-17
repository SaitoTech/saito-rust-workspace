use std::future::Future;
use std::pin::Pin;
use std::process::Output;

pub type RunnableTask = Pin<Box<dyn Fn() -> () + Send + 'static>>;

pub trait RunTask {
    // fn run(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);
    fn run(&self, task: RunnableTask);
}
