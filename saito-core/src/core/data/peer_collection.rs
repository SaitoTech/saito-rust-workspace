use crate::common::command::NetworkEvent;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};

use crate::common::defs::SaitoPublicKey;
use crate::core::data::configuration::PeerConfig;
use crate::core::data::context::Context;
use crate::core::data::peer::Peer;

//#[derive(Debug)]
pub struct PeerCollection {
    next_peer_index: Mutex<u64>,
    pub index_to_peers: HashMap<u64, Arc<RwLock<Peer>>>,
    pub address_to_peers: HashMap<SaitoPublicKey, u64>,
}

impl PeerCollection {
    pub fn new() -> PeerCollection {
        PeerCollection {
            next_peer_index: Mutex::new(0 as u64),
            index_to_peers: Default::default(),
            address_to_peers: Default::default(),
        }
    }

    pub async fn add(
        &mut self,
        context: &Context,
        event_sender: Sender<NetworkEvent>,
        config: Option<PeerConfig>,
    ) -> Arc<RwLock<Peer>> {
        let index;
        {
            index = *self.next_peer_index.lock().await + 1;
        }

        let peer = Arc::new(RwLock::new(Peer::new(context, index, config, event_sender)));

        self.index_to_peers.insert(index, peer.clone());
        return peer;
    }

    pub fn find_peer_by_address(&self, address: &SaitoPublicKey) -> Option<&Arc<RwLock<Peer>>> {
        let result = self.address_to_peers.get(address);
        if result.is_none() {
            return None;
        }

        return self.find_peer_by_index(*result.unwrap());
    }

    pub fn find_peer_by_index(&self, peer_index: u64) -> Option<&Arc<RwLock<Peer>>> {
        return self.index_to_peers.get(&peer_index);
    }
}
