use log::debug;

pub struct Wallet {}

impl Wallet {
    pub fn new() -> Wallet {
        Wallet {}
    }
    pub fn init(&mut self) {
        debug!("wallet.init");
    }
}
