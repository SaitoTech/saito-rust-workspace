use std::io::{Error, ErrorKind};
use std::sync::Arc;

use log::{debug, info, trace};
use tokio::sync::RwLock;

use crate::common::defs::{SaitoHash, SaitoPublicKey};
use crate::common::interface_io::InterfaceIO;
use crate::core::data;
use crate::core::data::block::Block;
use crate::core::data::blockchain::Blockchain;
use crate::core::data::configuration::Configuration;
use crate::core::data::msg::block_request::BlockchainRequest;
use crate::core::data::msg::handshake::{
    HandshakeChallenge, HandshakeCompletion, HandshakeResponse,
};
use crate::core::data::msg::message::Message;
use crate::core::data::peer::{Peer, PeerConnection};
use crate::core::data::peer_collection::PeerCollection;
use crate::core::data::transaction::Transaction;
use crate::core::data::wallet::Wallet;

pub struct Network {
    io_interface: Box<dyn InterfaceIO + Send + Sync>,
    peers: Arc<RwLock<PeerCollection>>,
}

impl Network {
    pub fn new(
        io_interface: Box<dyn InterfaceIO + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
    ) -> Self {
        Network {
            io_interface,
            peers,
        }
    }

    pub async fn propagate_block(&self, block: &Block) {
        debug!("propagating block : {:?}", hex::encode(&block.hash));

        let message = Message::BlockHeaderHash(block.hash);
        let buffer = message.serialize();

        let peers = self.peers.read().await;
        trace!("acquired the peers read lock");

        for entry in peers.index_to_peers.iter() {
            let peer = entry.1.read().await;

            if let Some(public_key) = block.source_connection_id {
                if peer.peer_public_key == public_key {
                    continue;
                }
            }

            peer.send_buffer(buffer.clone());
        }
    }

    pub async fn propagate_transaction(&self, _transaction: &Transaction) {
        debug!("propagating transaction");
        todo!()
    }

    pub async fn fetch_missing_block(
        &self,
        block_hash: SaitoHash,
        public_key: &SaitoPublicKey,
    ) -> Result<Block, Error> {
        debug!(
            "fetch missing block : block : {:?} from : {:?}",
            block_hash, public_key
        );
        let peers = self.peers.read().await;
        trace!("acquired the peers read lock");

        for entry in peers.index_to_peers.iter() {
            let peer = entry.1.read().await;

            if peer.peer_public_key == *public_key {
                let url = peer.get_block_fetch_url(block_hash);
                return self.fetch_missing_block_by_url(url).await;
            }
        }

        return Err(Error::from(ErrorKind::InvalidData));
    }

    pub async fn fetch_missing_block_by_url(&self, url: String) -> Result<Block, Error> {
        debug!("fetch missing block, url : {:?}", url);

        self.io_interface.fetch_block_by_url(url).await
    }
}
