use std::time::Duration;

use async_trait::async_trait;

use crate::common::command::{GlobalEvent, InterfaceEvent};

#[async_trait]
pub trait ProcessEvent<T>
where
    T: Send,
{
    async fn process_global_event(&mut self, event: GlobalEvent) -> Option<()>;
    async fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()>;
    async fn process_timer_event(&mut self, duration: Duration) -> Option<()>;
    async fn process_event(&mut self, event: T) -> Option<()>;
}
