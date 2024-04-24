use saito_core::core::defs::Timestamp;
use saito_core::core::process::keep_time::KeepTime;

pub struct WasmTimeKeeper {}

impl KeepTime for WasmTimeKeeper {
    fn get_timestamp_in_ms(&self) -> Timestamp {
        let date = js_sys::Date::new_0();

        date.get_time() as Timestamp
    }
}

pub struct WasmHastenedTimeKeeper {
    start_time: Timestamp,
    time_multiplier: u64,
}

impl WasmHastenedTimeKeeper {
    pub fn new(start_time: Timestamp, time_multiplier: u64) -> WasmHastenedTimeKeeper {
        WasmHastenedTimeKeeper {
            start_time,
            time_multiplier,
        }
    }
}

impl KeepTime for WasmHastenedTimeKeeper {
    fn get_timestamp_in_ms(&self) -> Timestamp {
        let date = js_sys::Date::new_0();

        let current_time = date.get_time() as Timestamp;
        let time_since_start = current_time - self.start_time;

        self.start_time + time_since_start * self.time_multiplier
    }
}
