use saito_core::core::defs::Timestamp;
use saito_core::core::process::keep_time::KeepTime;

pub struct PyTimeKeeper {}

impl KeepTime for PyTimeKeeper {
    fn get_timestamp_in_ms(&self) -> Timestamp {
        // js_sys::Date::now() as Timestamp
        0
    }
}
