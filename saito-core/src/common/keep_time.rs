/// Provides the current time in a implementation agnostic way into the core logic library. Since the core logic lib can be used on rust
/// application as well as WASM, it needs to get the time via this trait implementation.  
pub trait KeepTime {
    fn get_timestamp(&self) -> u64;
}
