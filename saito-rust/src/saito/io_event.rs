use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

use lazy_static::lazy_static;
use log::debug;

use saito_core::common::command::InterfaceEvent;

pub struct IoEvent {
    // TODO : remove controller id if not used
    pub controller_id: u8,
    pub event_id: u64,
    pub event: InterfaceEvent,
}

lazy_static! {
    static ref EVENT_COUNTER: Mutex<u64> = Mutex::new(0);
}

impl IoEvent {
    pub fn new(event: InterfaceEvent) -> IoEvent {
        let mut value = EVENT_COUNTER.lock().unwrap();
        *value = *value + 1;
        assert_ne!(*value, 0);
        debug!("new event created : {:?}", *value);
        IoEvent {
            controller_id: 0,
            event_id: value.clone(),
            event,
        }
    }
}
