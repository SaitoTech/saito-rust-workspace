use std::sync::Arc;

use log::{debug, trace};
use tokio::sync::RwLock;

use crate::common::interface_io::InterfaceIO;
use crate::core::data::block::Block;
use crate::core::data::msg::message::Message;
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::transaction::Transaction;

// TODO : rename to a better name
pub struct Network {
    pub peers: Arc<RwLock<PeerCollection>>,
    pub io_handler: Box<dyn InterfaceIO + Send + Sync>,
}

impl Network {
    pub async fn propagate_block(&self, block: Block) {
        debug!("propagating block : {:?}", hex::encode(block.get_hash()));

        let mut excluded_peers = vec![];
        // finding block sender to avoid resending the block to that node
        if block.source_connection_id.is_some() {
            trace!("waiting for the peers read lock");
            let peers = self.peers.read().await;
            trace!("acquired the peers read lock");
            let peer = peers
                .address_to_peers
                .get(&block.source_connection_id.unwrap());
            if peer.is_some() {
                excluded_peers.push(*peer.unwrap());
            }
        }
        debug!(
            "sending block : {:?} to peers",
            hex::encode(block.get_hash())
        );
        let message = Message::BlockHeaderHash(block.get_hash());
        self.io_handler
            .send_message_to_all(message.serialize(), excluded_peers)
            .await
            .unwrap();
    }
    pub async fn propagate_transaction(&self, transaction: Transaction) {
        debug!("propagating transaction");
        todo!()
    }
}
