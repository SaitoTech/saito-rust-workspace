use std::pin::Pin;

pub type RunnableTask = Pin<Box<dyn Fn() -> () + Send + 'static>>;

pub trait RunTask {
    // fn run(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);
    fn run(&self, task: RunnableTask);
}
