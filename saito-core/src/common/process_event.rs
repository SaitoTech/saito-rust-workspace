use std::time::Duration;

use crate::common::command::{InterfaceEvent, SaitoEvent};

pub trait ProcessEvent<T> {
    fn process_saito_event(&mut self, event: SaitoEvent) -> Option<()>;
    fn process_interface_event(&mut self, event: InterfaceEvent) -> Option<()>;
    fn process_timer_event(&mut self, duration: Duration) -> Option<()>;
    fn process_event(&mut self, event: T) -> Option<()>;
}
