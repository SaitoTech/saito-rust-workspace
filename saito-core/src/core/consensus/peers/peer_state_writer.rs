use crate::core::defs::{PrintForLog, SaitoPublicKey, Timestamp};
use crate::core::io::interface_io::InterfaceIO;
use ahash::HashMap;
use std::io::Error;
use std::time::Duration;

const PEER_STATE_FILENAME: &str = "./data/peer_state.txt";

pub(crate) const PEER_STATE_WRITE_PERIOD: Timestamp =
    Duration::from_secs(1).as_millis() as Timestamp;

#[derive(Default, Debug, Clone)]
pub(crate) struct PeerStateEntry {
    pub msg_limit_exceeded: bool,
    pub invalid_blocks_received: bool,
    pub same_depth_blocks_received: bool,
    pub too_far_blocks_received: bool,
    pub limited_till: Option<Timestamp>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PeerStateWriter {}

impl PeerStateWriter {
    /// Writes peer state data to the file and clears collected state
    ///
    /// # Arguments
    ///
    /// * `io_handler`:
    ///
    /// returns: Result<(), Error>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub(crate) async fn write_state(
        &self,
        data: HashMap<SaitoPublicKey, PeerStateEntry>,
        io_handler: &mut Box<dyn InterfaceIO + Send + Sync>,
    ) -> Result<(), Error> {
        let line =
            "key,limited_till,msg_limit,invalid_blocks_limit,same_depth_limit,too_far_block_limit\r\n"
                .to_string();
        io_handler
            .write_value(PEER_STATE_FILENAME, line.as_bytes())
            .await?;

        for (public_key, data) in data.iter() {
            let line = format!(
                "{:?},{:?},{:?},{:?},{:?},{:?}\r\n",
                public_key.to_base58(),
                data.limited_till.unwrap_or(0),
                data.msg_limit_exceeded,
                data.invalid_blocks_received,
                data.same_depth_blocks_received,
                data.too_far_blocks_received,
            );
            io_handler
                .append_value(PEER_STATE_FILENAME, line.as_bytes())
                .await?
        }

        if !data.is_empty() {
            io_handler.flush_data(PEER_STATE_FILENAME).await?;
        }

        Ok(())
    }
}
