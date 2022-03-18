use saito_core::common::command::InterfaceEvent;

pub struct IoEvent {
    pub controller_id: u8,
    pub event: InterfaceEvent,
}
