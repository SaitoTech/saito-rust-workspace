use saito_core::core::util::balance_snapshot::BalanceSnapshot;

pub struct PyBalanceSnapshot {
    snapshot: BalanceSnapshot,
}

impl PyBalanceSnapshot {
    pub fn new(snapshot: BalanceSnapshot) -> PyBalanceSnapshot {
        PyBalanceSnapshot { snapshot }
    }
    pub fn get_snapshot(self) -> BalanceSnapshot {
        self.snapshot
    }
}

impl PyBalanceSnapshot {
    pub fn get_file_name(&self) -> String {
        self.snapshot.get_file_name().into()
    }
    // pub fn get_entries(&self) -> Array {
    //     let rows = self.snapshot.get_rows();
    //     let array = js_sys::Array::new_with_length(rows.len() as u32);
    //     for (index, row) in rows.iter().enumerate() {
    //         let entry: String = row.to_string().into();
    //         array.set(index as u32, JsValue::from(entry));
    //     }
    //     array
    // }
    // pub fn from_string(str: String) -> Result<WasmBalanceSnapshot, JsValue> {
    //     let str: String = str.into();
    //     let result = str.try_into();
    //     if result.is_err() {
    //         // log::info!("str = {:?}", str);
    //         return Err(JsValue::from("failed converting string to snapshot"));
    //     }
    //     let snapshot: BalanceSnapshot = result.unwrap();
    //     let snapshot = WasmBalanceSnapshot::new(snapshot);
    //
    //     Ok(snapshot)
    // }

    pub fn to_string(&self) -> String {
        self.snapshot.to_string().into()
    }
}
