#[cfg(test)]
pub mod test {
    use std::io::Error;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::core::consensus::block::Block;
    use crate::core::consensus::blockchain::Blockchain;
    use crate::core::consensus::blockchain_sync_state::BlockchainSyncState;
    use crate::core::consensus::context::Context;
    use crate::core::consensus::mempool::Mempool;
    use crate::core::consensus::peer_collection::PeerCollection;
    use crate::core::consensus::transaction::Transaction;
    use crate::core::consensus::wallet::Wallet;
    use crate::core::consensus_thread::{ConsensusEvent, ConsensusStats, ConsensusThread};
    use crate::core::defs::{BlockId, Currency, SaitoPrivateKey, StatVariable, STAT_BIN_COUNT};
    use crate::core::defs::{SaitoPublicKey, Timestamp};
    use crate::core::io::network::Network;
    use crate::core::io::storage::Storage;
    use crate::core::mining_thread::{MiningEvent, MiningThread};
    use crate::core::process::keep_time::KeepTime;
    use crate::core::process::keep_time::Timer;
    use crate::core::process::process_event::ProcessEvent;
    use crate::core::routing_thread::{RoutingEvent, RoutingStats, RoutingThread};
    use crate::core::stat_thread::StatThread;
    use crate::core::util::configuration::{
        BlockchainConfig, Configuration, Endpoint, PeerConfig, Server,
    };
    use crate::core::util::test::test_io_handler::test::TestIOHandler;
    use crate::core::verification_thread::{VerificationThread, VerifyRequest};
    use serde::Deserialize;
    use tokio::sync::mpsc::Receiver;
    use tokio::sync::RwLock;

    #[derive(Clone)]
    pub struct TestTimeKeeper {}

    impl KeepTime for TestTimeKeeper {
        fn get_timestamp_in_ms(&self) -> Timestamp {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as Timestamp
        }
    }

    #[derive(Deserialize, Debug)]
    pub struct TestConfiguration {
        server: Option<Server>,
        peers: Vec<PeerConfig>,
        blockchain: Option<BlockchainConfig>,
        spv_mode: bool,
        browser_mode: bool,
    }
    impl Configuration for TestConfiguration {
        fn get_server_configs(&self) -> Option<&Server> {
            self.server.as_ref()
        }

        fn get_peer_configs(&self) -> &Vec<PeerConfig> {
            &self.peers
        }

        fn get_blockchain_configs(&self) -> Option<BlockchainConfig> {
            self.blockchain.clone()
        }

        fn get_block_fetch_url(&self) -> String {
            "".to_string()
        }

        fn is_spv_mode(&self) -> bool {
            self.spv_mode
        }

        fn is_browser(&self) -> bool {
            self.browser_mode
        }

        fn replace(&mut self, config: &dyn Configuration) {
            todo!()
        }
    }
    impl Default for TestConfiguration {
        fn default() -> Self {
            TestConfiguration {
                server: Option::Some(Server {
                    host: "localhost".to_string(),
                    port: 12100,
                    protocol: "http".to_string(),
                    endpoint: Endpoint {
                        host: "localhost".to_string(),
                        port: 12101,
                        protocol: "http".to_string(),
                    },
                    verification_threads: 2,
                    channel_size: 1000,
                    stat_timer_in_ms: 10000,
                    reconnection_wait_time: 10000,
                    thread_sleep_time_in_ms: 10,
                    block_fetch_batch_size: 0,
                }),
                peers: vec![],
                blockchain: None,
                spv_mode: false,
                browser_mode: false,
            }
        }
    }

    pub struct NodeTester {
        consensus_thread: ConsensusThread,
        routing_thread: RoutingThread,
        mining_thread: MiningThread,
        verification_thread: VerificationThread,
        stat_thread: StatThread,
        timer: Timer,
        receiver_for_router: Receiver<RoutingEvent>,
        receiver_for_consensus: Receiver<ConsensusEvent>,
        receiver_for_miner: Receiver<MiningEvent>,
        receiver_for_verification: Receiver<VerifyRequest>,
        receiver_for_stats: Receiver<String>,
        context: Context,
        timeout_in_ms: u64,
    }

    impl Default for NodeTester {
        fn default() -> Self {
            let wallet = Arc::new(RwLock::new(Wallet::new([0; 32], [0; 33])));

            let configuration: Arc<RwLock<dyn Configuration + Send + Sync>> =
                Arc::new(RwLock::new(TestConfiguration::default()));

            let channel_size = 1_000_000;

            let peers = Arc::new(RwLock::new(PeerCollection::new()));
            let context = Context {
                blockchain_lock: Arc::new(RwLock::new(Blockchain::new(wallet.clone()))),
                mempool_lock: Arc::new(RwLock::new(Mempool::new(wallet.clone()))),
                wallet_lock: wallet.clone(),
                config_lock: configuration.clone(),
            };

            let (sender_to_consensus, receiver_in_mempool) =
                tokio::sync::mpsc::channel(channel_size);
            let (sender_to_blockchain, receiver_in_blockchain) =
                tokio::sync::mpsc::channel(channel_size);
            let (sender_to_miner, receiver_in_miner) = tokio::sync::mpsc::channel(channel_size);
            let (sender_to_stat, receiver_in_stats) = tokio::sync::mpsc::channel(channel_size);
            let (sender_to_verification, receiver_in_verification) =
                tokio::sync::mpsc::channel(channel_size);

            let timer = Timer {
                time_reader: Arc::new(TestTimeKeeper {}),
                hasten_multiplier: 1,
                start_time: TestTimeKeeper {}.get_timestamp_in_ms(),
            };

            NodeTester {
                routing_thread: RoutingThread {
                    blockchain_lock: context.blockchain_lock.clone(),
                    mempool_lock: context.mempool_lock.clone(),
                    sender_to_consensus: sender_to_consensus.clone(),
                    sender_to_miner: sender_to_miner.clone(),
                    config_lock: context.config_lock.clone(),
                    timer: timer.clone(),
                    wallet_lock: wallet.clone(),
                    network: Network::new(
                        Box::new(TestIOHandler {}),
                        peers.clone(),
                        context.wallet_lock.clone(),
                        context.config_lock.clone(),
                        timer.clone(),
                    ),
                    reconnection_timer: 0,
                    stats: RoutingStats::new(sender_to_stat.clone()),
                    senders_to_verification: vec![sender_to_verification.clone()],
                    last_verification_thread_index: 0,
                    stat_sender: sender_to_stat.clone(),
                    blockchain_sync_state: BlockchainSyncState::new(10),
                },
                consensus_thread: ConsensusThread {
                    mempool_lock: context.mempool_lock.clone(),
                    blockchain_lock: context.blockchain_lock.clone(),
                    wallet_lock: context.wallet_lock.clone(),
                    generate_genesis_block: false,
                    sender_to_router: sender_to_blockchain.clone(),
                    sender_to_miner: sender_to_miner.clone(),
                    // sender_global: (),
                    block_producing_timer: 0,
                    timer: timer.clone(),
                    network: Network::new(
                        Box::new(TestIOHandler {}),
                        peers.clone(),
                        context.wallet_lock.clone(),
                        configuration.clone(),
                        timer.clone(),
                    ),
                    storage: Storage::new(Box::new(TestIOHandler {})),
                    stats: ConsensusStats::new(sender_to_stat.clone()),
                    txs_for_mempool: vec![],
                    stat_sender: sender_to_stat.clone(),
                    config_lock: configuration.clone(),
                },
                mining_thread: MiningThread {
                    wallet_lock: context.wallet_lock.clone(),
                    sender_to_mempool: sender_to_consensus.clone(),
                    timer: timer.clone(),
                    miner_active: false,
                    target: [0; 32],
                    difficulty: 0,
                    public_key: [0; 33],
                    mined_golden_tickets: 0,
                    stat_sender: sender_to_stat.clone(),
                    config_lock: configuration.clone(),
                    enabled: true,
                    mining_iterations: 1_000,
                },
                verification_thread: VerificationThread {
                    sender_to_consensus: sender_to_consensus.clone(),
                    blockchain_lock: context.blockchain_lock.clone(),
                    peer_lock: peers.clone(),
                    wallet_lock: wallet.clone(),
                    processed_txs: StatVariable::new(
                        "verification::processed_txs".to_string(),
                        STAT_BIN_COUNT,
                        sender_to_stat.clone(),
                    ),
                    processed_blocks: StatVariable::new(
                        "verification::processed_blocks".to_string(),
                        STAT_BIN_COUNT,
                        sender_to_stat.clone(),
                    ),
                    processed_msgs: StatVariable::new(
                        "verification::processed_msgs".to_string(),
                        STAT_BIN_COUNT,
                        sender_to_stat.clone(),
                    ),
                    invalid_txs: StatVariable::new(
                        "verification::invalid_txs".to_string(),
                        STAT_BIN_COUNT,
                        sender_to_stat.clone(),
                    ),
                    stat_sender: sender_to_stat.clone(),
                },
                stat_thread: StatThread {
                    stat_queue: Default::default(),
                    io_interface: Box::new(TestIOHandler {}),
                    enabled: false,
                },
                timer,
                receiver_for_router: receiver_in_blockchain,
                receiver_for_consensus: receiver_in_mempool,
                receiver_for_miner: receiver_in_miner,
                receiver_for_verification: receiver_in_verification,
                context,
                receiver_for_stats: receiver_in_stats,
                timeout_in_ms: Duration::new(30, 0).as_millis() as u64,
            }
        }
    }
    impl NodeTester {
        pub async fn init(&mut self) -> Result<(), Error> {
            self.consensus_thread.on_init().await;
            self.routing_thread.on_init().await;
            self.mining_thread.on_init().await;
            self.verification_thread.on_init().await;
            self.stat_thread.on_init().await;

            Ok(())
        }
        async fn run_loop_once(&mut self) {
            if let Ok(event) = self.receiver_for_router.try_recv() {
                self.routing_thread.process_event(event).await.unwrap()
            }
            if let Ok(event) = self.receiver_for_miner.try_recv() {
                self.mining_thread.process_event(event).await.unwrap();
            }
            if let Ok(event) = self.receiver_for_stats.try_recv() {
                self.stat_thread.process_event(event).await.unwrap();
            }
            if let Ok(event) = self.receiver_for_consensus.try_recv() {
                self.consensus_thread.process_event(event).await.unwrap();
            }
            if let Ok(event) = self.receiver_for_verification.try_recv() {
                self.verification_thread.process_event(event).await.unwrap();
            }
        }
        async fn run_until(&mut self, timestamp: Timestamp) -> Result<(), Error> {
            loop {
                if self.timer.get_timestamp_in_ms() >= timestamp {
                    break;
                }
                self.run_loop_once().await;
            }
            Ok(())
        }
        pub async fn wait_till_block_id(&mut self, block_id: BlockId) -> Result<(), Error> {
            let timeout = self.timer.get_timestamp_in_ms() + self.timeout_in_ms;
            loop {
                {
                    let blockchain = self.routing_thread.blockchain_lock.read().await;
                    if blockchain.get_latest_block_id() >= block_id {
                        break;
                    }
                }
                self.run_loop_once().await;

                if self.timer.get_timestamp_in_ms() > timeout {
                    break;
                }
            }

            Ok(())
        }
        pub async fn wait_till_mempool_tx_count(&mut self, tx_count: u64) -> Result<(), Error> {
            let timeout = self.timer.get_timestamp_in_ms() + self.timeout_in_ms;
            loop {
                {
                    let mempool = self.routing_thread.mempool_lock.read().await;
                    if mempool.transactions.len() >= tx_count as usize {
                        break;
                    }
                }
                self.run_loop_once().await;

                if self.timer.get_timestamp_in_ms() > timeout {
                    break;
                }
            }

            Ok(())
        }
        pub async fn wait_till_wallet_balance(&mut self, balance: Currency) -> Result<(), Error> {
            let timeout = self.timer.get_timestamp_in_ms() + self.timeout_in_ms;
            loop {
                {
                    let wallet = self.routing_thread.wallet_lock.read().await;
                    if wallet.get_available_balance() >= balance {
                        break;
                    }
                }
                self.run_loop_once().await;

                if self.timer.get_timestamp_in_ms() > timeout {
                    break;
                }
            }

            Ok(())
        }
        pub async fn get_public_key(&self) -> SaitoPublicKey {
            self.routing_thread.wallet_lock.read().await.public_key
        }
        pub async fn get_private_key(&self) -> SaitoPrivateKey {
            self.routing_thread.wallet_lock.read().await.private_key
        }

        pub async fn add_transaction(&mut self, transaction: Transaction) {
            self.consensus_thread
                .process_event(ConsensusEvent::NewTransaction { transaction })
                .await
                .unwrap()
        }
        pub async fn add_block(&mut self, block: Block) {
            todo!()
        }
    }
}
