use crate::core::consensus::peers::peer::{Peer, PeerStatus};
use crate::core::consensus::peers::peer_state_writer::PeerStateWriter;
use crate::core::defs::{PeerIndex, SaitoPublicKey, Timestamp};
use std::collections::HashMap;
use std::time::Duration;
use log::{debug, warn, error};


const PEER_REMOVAL_WINDOW: Timestamp = Duration::from_secs(3600 * 24).as_millis() as Timestamp;

#[derive(Clone, Debug, Default)]
pub struct PeerCounter {
    counter: PeerIndex,
}

impl PeerCounter {
    pub fn get_next_index(&mut self) -> PeerIndex {
        self.counter += 1;
        self.counter
    }
}
#[derive(Debug, Clone, Default)]
pub struct PeerCollection {
    pub index_to_peers: HashMap<PeerIndex, Peer>,
    pub address_to_peers: HashMap<SaitoPublicKey, PeerIndex>,
    pub peer_counter: PeerCounter,
    pub peer_state_writer: PeerStateWriter,
}

impl PeerCollection {
    pub fn find_peer_by_address(&self, address: &SaitoPublicKey) -> Option<&Peer> {
        let result = self.address_to_peers.get(address)?;

        self.find_peer_by_index(*result)
    }
    pub fn find_peer_by_address_mut(&mut self, address: &SaitoPublicKey) -> Option<&mut Peer> {
        let result = self.address_to_peers.get(address)?;

        self.find_peer_by_index_mut(*result)
    }

    pub fn find_peer_by_index(&self, peer_index: u64) -> Option<&Peer> {
        self.index_to_peers.get(&peer_index)
    }
    pub fn find_peer_by_index_mut(&mut self, peer_index: u64) -> Option<&mut Peer> {
        self.index_to_peers.get_mut(&peer_index)
    }

    pub fn remove_reconnected_peer(&mut self, public_key: &SaitoPublicKey) -> Option<Peer> {
        let peer_index;
        {
            let peer = self.find_peer_by_address(&public_key)?;
            if let PeerStatus::Connected = peer.peer_status {
                // since peer is already connected
                return None;
            }
            peer_index = peer.index;
        }

        let peer = self.index_to_peers.remove(&peer_index)?;
        self.address_to_peers.remove(&peer.public_key?);

        Some(peer)
    }

    pub fn remove_disconnected_peers(&mut self, current_time: Timestamp) {
        let peer_indices: Vec<PeerIndex> = self
            .index_to_peers
            .iter()
            .filter_map(|(peer_index, peer)| {
                if peer.static_peer_config.is_some() {
                    // static peers always remain in memory
                    return None;
                }
                if peer.is_stun_peer() {
                    // stun peers remain unless explicity removed
                    return None;
                }
                // if peer.is_archive_peer {
                //     // archive peers should remain in memory
                //     return None;
                // }

                // debug!(" disconnected at {} {} {}", peer.disconnected_at, PEER_REMOVAL_WINDOW, current_time);
                match peer.disconnected_at {
                    Timestamp::MAX => None, // Peer hasn't been disconnected yet
                    0 => {
                        warn!("Peer {} has disconnected_at set to 0", peer_index);
                        None
                    }
                    
                    disconnected_at => {
                        debug!(
                            "Peer {}: disconnected_at: {}, current_time: {}, PEER_REMOVAL_WINDOW: {}",
                            peer_index, disconnected_at, current_time, PEER_REMOVAL_WINDOW
                        );
                        if let Some(removal_time) = disconnected_at.checked_add(PEER_REMOVAL_WINDOW) {
                            if removal_time <= current_time {
                                Some(*peer_index)
                            } else {
                                None
                            }
                        } else {
                            error!("Overflow occurred when calculating removal time for peer {}", peer_index);
                            None
                        }}};
                    
                Some(*peer_index)
            })
            .collect();

        for peer_index in peer_indices {
            let peer = self.index_to_peers.remove(&peer_index).unwrap();
            if let Some(public_key) = peer.get_public_key() {
                self.address_to_peers.remove(&public_key);
            }
        }
    }
}
