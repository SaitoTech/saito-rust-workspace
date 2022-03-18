use std::time::Duration;

use crate::common::command::{GlobalEvent, InterfaceEvent};

pub trait ProcessEvent<T>
where
    T: Send,
{
    fn process_global_event(&mut self, event: GlobalEvent) -> Option<()>;
    fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()>;
    fn process_timer_event(&mut self, duration: Duration) -> Option<()>;
    fn process_event(&mut self, event: T) -> Option<()>;
}
