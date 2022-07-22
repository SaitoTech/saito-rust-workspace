use std::io::Error;
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
    // TODO : manage peers from network
    peers: Arc<RwLock<PeerCollection>>,
    pub io_interface: Box<dyn InterfaceIO + Send + Sync>,
    blockchain: Arc<RwLock<Blockchain>>,
}

impl Network {
    pub fn new(
        io_handler: Box<dyn InterfaceIO + Send + Sync>,
        peers: Arc<RwLock<PeerCollection>>,
        blockchain: Arc<RwLock<Blockchain>>,
    ) -> Network {
        Network {
            peers,
            io_interface: io_handler,
            blockchain,
        }
    }

    pub async fn propagate_block(&self, block: &Block) {
        debug!("propagating block : {:?}", hex::encode(&block.hash));

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
        debug!("sending block : {:?} to peers", hex::encode(&block.hash));
        let message = Message::BlockHeaderHash(block.hash);
        self.io_interface
            .send_message_to_all(message.serialize(), excluded_peers)
            .await
            .unwrap();
    }

    pub async fn propagate_transaction(&self, _transaction: &Transaction) {
        debug!("propagating transaction");
        todo!()
    }

    pub async fn fetch_missing_block(
        &self,
        block_hash: SaitoHash,
        public_key: &SaitoPublicKey,
    ) -> Result<(), Error> {
        debug!(
            "fetch missing block : block : {:?} from : {:?}",
            block_hash, public_key
        );
        let peer_index;
        let url;
        {
            let peers = self.peers.read().await;
            let peer = peers.find_peer_by_address(public_key);
            let peer = peer.unwrap().read().await;
            url = peer.get_block_fetch_url(block_hash);
            peer_index = peer.peer_index;
        }

        self.io_interface
            .fetch_block_from_peer(block_hash, peer_index, url)
            .await
    }

    async fn request_blockchain_from_peer(
        &self,
        peer_index: u64,
        connection: &mut impl PeerConnection,
    ) {
        debug!("requesting blockchain from peer : {:?}", peer_index);

        // TODO : should this be moved inside peer ?
        let request;
        {
            let blockchain = self.blockchain.read().await;
            request = BlockchainRequest {
                latest_block_id: blockchain.get_latest_block_id(),
                latest_block_hash: blockchain.get_latest_block_hash(),
                fork_id: blockchain.get_fork_id(),
            };
        }

        let buffer = Message::BlockchainRequest(request).serialize();
        connection.send_message(buffer).await.unwrap();
    }

    pub async fn process_incoming_blockchain_request(
        &self,
        request: BlockchainRequest,
        peer_index: u64,
        connection: &mut impl PeerConnection,
    ) {
        debug!(
            "processing incoming blockchain request : {:?}-{:?}-{:?} from peer : {:?}",
            request.latest_block_id,
            hex::encode(request.latest_block_hash),
            hex::encode(request.fork_id),
            peer_index
        );
        // TODO : can we ignore the functionality if it's a lite node ?

        let blockchain = self.blockchain.read().await;

        let last_shared_ancestor =
            blockchain.generate_last_shared_ancestor(request.latest_block_id, request.fork_id);
        debug!("last shared ancestor = {:?}", last_shared_ancestor);

        for i in last_shared_ancestor..(blockchain.blockring.get_latest_block_id() + 1) {
            let block_hash = blockchain
                .blockring
                .get_longest_chain_block_hash_by_block_id(i);
            if block_hash == [0; 32] {
                // TODO : can the block hash not be in the ring if we are going through the longest chain ?
                continue;
            }
            let buffer = Message::BlockHeaderHash(block_hash).serialize();

            connection.send_message(buffer).await.unwrap();
        }
    }

    pub async fn process_incoming_block_hash(&self, block_hash: SaitoHash, peer_index: u64) {
        let block_exists;
        {
            let blockchain = self.blockchain.read().await;
            block_exists = blockchain.is_block_indexed(block_hash);
        }
        let url;
        {
            let peers = self.peers.read().await;
            let peer = peers.find_peer_by_index(peer_index).unwrap().read().await;
            url = peer.get_block_fetch_url(block_hash);
        }
        if !block_exists {
            self.io_interface
                .fetch_block_from_peer(block_hash, peer_index, url)
                .await
                .unwrap();
        }
    }
}
