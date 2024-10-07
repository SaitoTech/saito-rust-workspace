use crate::core::defs::Timestamp;
use crate::core::io::interface_io::InterfaceIO;
use std::io::Error;
use std::net::IpAddr;
use std::time::Duration;

const PEER_STATE_FILENAME: &str = "./data/peer_state.txt";

pub(crate) const PEER_STATE_WRITE_PERIOD: Timestamp =
    Duration::from_secs(5).as_millis() as Timestamp;

#[derive(Clone, Debug, Default)]
pub(crate) struct PeerStateWriter {
    lines: Vec<String>,
}

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
        &mut self,
        io_handler: &mut Box<dyn InterfaceIO + Send + Sync>,
    ) -> Result<(), Error> {
        io_handler.write_value(PEER_STATE_FILENAME, &[]).await?;

        for line in self.lines.drain(..) {
            io_handler
                .append_value(PEER_STATE_FILENAME, (line + "\r\n").as_bytes())
                .await?
        }

        io_handler.flush_data(PEER_STATE_FILENAME).await?;

        Ok(())
    }

    pub(crate) fn add_line(&mut self, line: String) {
        self.lines.push(line);
    }
}
