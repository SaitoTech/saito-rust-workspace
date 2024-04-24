use crate::core::defs::Timestamp;

/// Provides the current time in a implementation agnostic way into the core logic library. Since the core logic lib can be used on rust
/// application as well as WASM, it needs to get the time via this trait implementation.

pub trait KeepTime {
    fn get_timestamp_in_ms(&self) -> Timestamp;
}

pub trait ClockFactory {
    type Output: KeepTime + Clone;

    fn create() -> Self::Output;
}

// impl<T: KeepTime> ClockFactory<T> {
//     pub fn create(&self) -> T {
//         T::new()
//     }
// }
//
// impl<T: KeepTime> Default for ClockFactory<T> {
//     fn default() -> Self {}
// }
