use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use futures::{FutureExt, StreamExt};
use instant::Instant;
use nimiq_block::Block;
use nimiq_blockchain_interface::AbstractBlockchain;
use nimiq_blockchain_proxy::{BlockchainProxy, BlockchainReadProxy};
use nimiq_hash::Blake2bHash;
use nimiq_network_interface::{network::Network, request::request_handler};
use nimiq_utils::spawn;
use nimiq_zkp_component::zkp_component::ZKPComponentProxy;
use tokio::sync::{
    broadcast::{channel as broadcast, Sender as BroadcastSender},
    mpsc::{
        channel as mpsc_channel, error::SendError, Receiver as MpscReceiver, Sender as MpscSender,
    },
    oneshot::{error::RecvError, Sender as OneshotSender},
};
use tokio_stream::wrappers::BroadcastStream;

use self::consensus_proxy::ConsensusProxy;
#[cfg(feature = "full")]
use self::remote_event_dispatcher::RemoteEventDispatcher;
use crate::{
    consensus::head_requests::{HeadRequests, HeadRequestsResult},
    messages::{RequestBlock, RequestHead, RequestMacroChain, RequestMissingBlocks},
    sync::{live::block_queue::BlockSource, syncer::LiveSyncPushEvent, syncer_proxy::SyncerProxy},
};
#[cfg(feature = "full")]
use crate::{
    messages::{
        RequestBatchSet, RequestBlocksProof, RequestHistoryChunk,
        RequestTransactionReceiptsByAddress, RequestTransactionsProof, RequestTrieProof,
    },
    sync::live::{diff_queue::RequestTrieDiff, state_queue::RequestChunk},
};

pub mod consensus_proxy;
mod head_requests;
mod remote_data_store;
#[cfg(feature = "full")]
mod remote_event_dispatcher;

/// Events that are generated by the consensus component to convey the two possible states of consensus:
/// Established consensus (by satisfying some specific consensus criteria), or we lost it
#[derive(Clone)]
pub enum ConsensusEvent {
    /// Consensus is established
    /// Also includes a flag that indicates if we are ready for transaction verification.
    /// The established event can be triggered multiple times with different values for the flag.
    Established { synced_validity_window: bool },
    /// Consensus was lost
    Lost,
}

/// This enum is used to represent different kinds of events that are generated by other peers.
/// This is used for cases where we want to subscribe to other peers to receive notifications about those events.
/// For instance: we might be interested in knowing about transactions, from some specific address, that are included in a block
#[derive(Clone)]
pub enum RemoteEvent {
    /// Interesting receipts: They belong to an address that is interesting to us
    /// We get a vector of (transaction_hash, block_number) tuples
    InterestingReceipts(Vec<(Blake2bHash, u32)>),
    /// Other events, generated by peers, that might be interesting to us
    Placeholder,
}

/// Different Errors for a failed ResolveBlockRequest.
pub enum ResolveBlockError<N: Network> {
    Outdated,
    Duplicate,
    ReceiveError(RecvError),
    SendError(SendError<ConsensusRequest<N>>),
}

impl<N: Network> std::fmt::Debug for ResolveBlockError<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveBlockError::Outdated => f.debug_tuple("ResolveBlockError::Outdated").finish(),
            ResolveBlockError::Duplicate => f.debug_tuple("ResolveBlockError::Duplicate").finish(),
            ResolveBlockError::ReceiveError(e) => f
                .debug_tuple("ResolveBlockError::ReceiveError")
                .field(e)
                .finish(),
            ResolveBlockError::SendError(e) => f
                .debug_tuple("ResolveBlockError::SendError")
                .field(e)
                .finish(),
        }
    }
}

/// Requests the consensus to resolve a given `block_hash` at a specific `block_height`.
/// Additionally the sender of a response channel is presented and a number of peers who are
/// well suited to provide the required data.
pub struct ResolveBlockRequest<N: Network> {
    /// Block number of the to be resolved block.
    pub(crate) block_number: u32,

    /// Block hash of the to be resolved block.
    pub(crate) block_hash: Blake2bHash,

    /// The id of the first peer to ask for this block.
    pub(crate) first_peer_id: N::PeerId,

    /// Sender to a oneshot channel where the response to the request is being awaited.
    pub(crate) response_sender: OneshotSender<Result<Block, ResolveBlockError<N>>>,
}

/// Enumeration of all ConsensusRequests available.
pub enum ConsensusRequest<N: Network> {
    ResolveBlock(ResolveBlockRequest<N>),
}

pub struct Consensus<N: Network> {
    pub blockchain: BlockchainProxy,
    pub network: Arc<N>,

    pub sync: SyncerProxy<N>,

    events: BroadcastSender<ConsensusEvent>,
    established_flag: Arc<AtomicBool>,
    last_batch_number: u32,
    synced_validity_window_flag: Arc<AtomicBool>,
    head_requests: Option<HeadRequests<N>>,
    head_requests_time: Option<Instant>,

    min_peers: usize,

    /// Sender and Receiver of a consensus request channel used to relay requests from any source
    /// to the Consensus instance. Currently the only source is a ConsensusProxy instance, but
    /// the Consensus is not limited to it.
    ///
    /// Both the sender and receiver are stored such that the sender can be cloned as required,
    /// while the receiver is actually polled within the Consensus poll function.
    ///
    /// The consensus itself is chosen, even though for the initial single request a structure
    /// somewhere deeper down the call stack would be adequate, as other requests may require different
    /// structures. Putting it here seemed to be the most flexible.
    requests: (
        MpscSender<ConsensusRequest<N>>,
        MpscReceiver<ConsensusRequest<N>>,
    ),

    zkp_proxy: ZKPComponentProxy<N>,
}

impl<N: Network> Consensus<N> {
    /// Minimum number of peers for consensus to be established.
    const MIN_PEERS_ESTABLISHED: usize = 3;

    /// Minimum number of block announcements extending the chain for consensus to be established.
    const MIN_BLOCKS_ESTABLISHED: usize = 5;

    /// Timeout after which head requests will be performed (again) to determine consensus
    /// established state and to advance the chain.
    const HEAD_REQUESTS_TIMEOUT: Duration = Duration::from_secs(5);

    pub fn from_network(
        blockchain: BlockchainProxy,
        network: Arc<N>,
        syncer: SyncerProxy<N>,
        zkp_proxy: ZKPComponentProxy<N>,
    ) -> Self {
        Self::new(
            blockchain,
            network,
            syncer,
            Self::MIN_PEERS_ESTABLISHED,
            zkp_proxy,
        )
    }

    pub fn new(
        blockchain: BlockchainProxy,
        network: Arc<N>,
        syncer: SyncerProxy<N>,
        min_peers: usize,
        zkp_proxy: ZKPComponentProxy<N>,
    ) -> Self {
        let (tx, _rx) = broadcast(256);

        Self::init_network_request_receivers(&network, &blockchain);

        #[cfg(feature = "full")]
        Self::init_remote_event_dispatcher(&network, &blockchain);

        let established_flag = Arc::new(AtomicBool::new(false));
        let mut synced_validity_window_flag = true;
        #[cfg(feature = "full")]
        {
            if let BlockchainReadProxy::Full(blockchain) = blockchain.read() {
                synced_validity_window_flag = blockchain.can_enforce_validity_window();
            }
        }
        let synced_validity_window_flag = Arc::new(AtomicBool::new(synced_validity_window_flag));

        Consensus {
            blockchain,
            network,
            sync: syncer,
            events: tx,
            established_flag,
            last_batch_number: 0,
            synced_validity_window_flag,
            head_requests: None,
            head_requests_time: None,
            min_peers,
            // Choose a small buffer as having a lot of items buffered here indicates a bigger problem.
            requests: mpsc_channel(10),
            zkp_proxy,
        }
    }

    fn init_remote_event_dispatcher(network: &Arc<N>, blockchain: &BlockchainProxy) {
        // We spawn the Remote Event Dispatcher into its own task (this is only available for full nodes and history nodes)

        match blockchain {
            #[cfg(feature = "full")]
            BlockchainProxy::Full(blockchain) => {
                let network = Arc::clone(network);
                let blockchain = Arc::clone(blockchain);
                let remote_event_dispatcher = RemoteEventDispatcher::new(network, blockchain);

                spawn(remote_event_dispatcher);
            }
            BlockchainProxy::Light(_) => {
                // The light blockchain does not provide this functionality
            }
        }
    }

    fn init_network_request_receivers(network: &Arc<N>, blockchain: &BlockchainProxy) {
        let stream = network.receive_requests::<RequestMacroChain>();
        spawn(Box::pin(request_handler(network, stream, blockchain)));

        let stream = network.receive_requests::<RequestBlock>();
        spawn(Box::pin(request_handler(network, stream, blockchain)));

        let stream = network.receive_requests::<RequestMissingBlocks>();
        spawn(Box::pin(request_handler(network, stream, blockchain)));

        let stream = network.receive_requests::<RequestHead>();
        spawn(Box::pin(request_handler(network, stream, blockchain)));
        match blockchain {
            #[cfg(feature = "full")]
            BlockchainProxy::Full(blockchain) => {
                let stream = network.receive_requests::<RequestBatchSet>();
                spawn(Box::pin(request_handler(network, stream, blockchain)));

                let stream = network.receive_requests::<RequestHistoryChunk>();
                spawn(Box::pin(request_handler(network, stream, blockchain)));

                let stream = network.receive_requests::<RequestTrieDiff>();
                spawn(Box::pin(request_handler(network, stream, blockchain)));

                let stream = network.receive_requests::<RequestChunk>();
                spawn(Box::pin(request_handler(network, stream, blockchain)));

                let supports_history_index = blockchain.read().history_store.supports_index();

                // Only spawn these handlers if the history index is enabled.
                if supports_history_index {
                    let stream = network.receive_requests::<RequestTransactionsProof>();
                    spawn(Box::pin(request_handler(network, stream, blockchain)));

                    let stream = network.receive_requests::<RequestTransactionReceiptsByAddress>();
                    spawn(Box::pin(request_handler(network, stream, blockchain)));
                }

                let stream = network.receive_requests::<RequestTrieProof>();
                spawn(Box::pin(request_handler(network, stream, blockchain)));

                let stream = network.receive_requests::<RequestBlocksProof>();
                spawn(Box::pin(request_handler(network, stream, blockchain)));
            }
            BlockchainProxy::Light(_) => {}
        }
    }

    pub fn subscribe_events(&self) -> BroadcastStream<ConsensusEvent> {
        BroadcastStream::new(self.events.subscribe())
    }

    pub fn is_established(&self) -> bool {
        self.established_flag.load(Ordering::Acquire)
    }

    pub fn num_agents(&self) -> usize {
        self.sync.num_peers()
    }

    pub fn proxy(&self) -> ConsensusProxy<N> {
        ConsensusProxy {
            blockchain: self.blockchain.clone(),
            network: Arc::clone(&self.network),
            established_flag: Arc::clone(&self.established_flag),
            synced_validity_window_flag: Arc::clone(&self.synced_validity_window_flag),
            events: self.events.clone(),
            request: self.requests.0.clone(),
        }
    }

    /// Forcefully sets consensus established, should be used for tests only.
    pub fn force_established(&mut self) {
        trace!("Consensus forcefully established.");
        self.established_flag.swap(true, Ordering::Release);

        // Also stop any other checks.
        self.head_requests = None;
        self.head_requests_time = None;

        // We don't care if anyone is listening.
        let (synced_validity_window, _) = self.check_validity_window();
        self.events
            .send(ConsensusEvent::Established {
                synced_validity_window,
            })
            .ok();
    }

    /// Checks if the validity window is available.
    /// This function contains optimizations to only run the check when necessary.
    /// It returns a boolean indicating if the validity window is available
    /// and an consensus event if the value changed in this call.
    fn check_validity_window(&mut self) -> (bool, Option<ConsensusEvent>) {
        // We only check for the validity window if consensus is established
        // and we are in a new batch.
        // The `can_enforce_validity_window` flag can only change on macro blocks:
        // It can change to false during macro sync when pushing macro blocks.
        // It can change to true when we reach an offset of the transaction validity window
        // into a new epoch we have the history for. The validity window is a multiple
        // of the batch size – thus it is again a macro block.
        // We do not subscribe to blockchain events since the consensus polls all relevant channels
        // that add new blocks to the blockchain.
        #[cfg(feature = "full")]
        if let BlockchainReadProxy::Full(ref full_blockchain) = self.blockchain.read() {
            let current_batch_number = full_blockchain.batch_number();
            if current_batch_number > self.last_batch_number {
                self.last_batch_number = current_batch_number;
                let can_enforce_validity_window = full_blockchain.can_enforce_validity_window();
                let old_value = self
                    .synced_validity_window_flag
                    .swap(can_enforce_validity_window, Ordering::Release);
                // If the value changed, send an Established event.
                let mut event = None;
                if old_value != can_enforce_validity_window {
                    event = Some(ConsensusEvent::Established {
                        synced_validity_window: can_enforce_validity_window,
                    });
                }
                return (can_enforce_validity_window, event);
            }
        }
        (true, None)
    }

    /// Calculates and sets established state, returns a ConsensusEvent if the state changed.
    /// Once consensus is established, we can only lose it if we lose all our peers.
    /// To reach consensus established state, we need at least `minPeers` peers and
    /// one of the following conditions must be true:
    /// - we accepted at least `MIN_BLOCKS_ESTABLISHED` block announcements
    /// - we know at least 2/3 of the head blocks of our peers
    ///
    /// The latter check is started immediately once we reach the minimum number of peers
    /// and is potentially repeated in an interval of `HEAD_REQUESTS_TIMEOUT` until one
    /// of the conditions above is true.
    /// Any unknown blocks resulting of the head check are handled similarly as block announcements
    /// via the block queue.
    fn check_established(
        &mut self,
        finished_head_request: Option<HeadRequestsResult<N>>,
    ) -> Option<ConsensusEvent> {
        // We can only lose established state right now if we drop below our minimum peer threshold.
        if self.is_established() {
            if self.num_agents() < self.min_peers {
                warn!("Lost consensus!");
                self.established_flag.swap(false, Ordering::Release);
                return Some(ConsensusEvent::Lost);
            }
            // Check if validity window availability changed.
            if let (_, Some(event)) = self.check_validity_window() {
                return Some(event);
            }
        } else {
            // We have three conditions on whether we move to the established state.
            // First, we always need a minimum number of peers connected.
            // Second, the state must always be complete.
            // Then, we check that we either:
            // - accepted a minimum number of block announcements, or
            // - know the head state of a majority of our peers
            if self.num_agents() >= self.min_peers && self.sync.state_complete() {
                if self.sync.accepted_block_announcements() >= Self::MIN_BLOCKS_ESTABLISHED {
                    info!("Consensus established, number of accepted announcements satisfied.");
                    self.established_flag.swap(true, Ordering::Release);

                    // Also stop any other checks.
                    self.head_requests = None;
                    self.head_requests_time = None;
                    self.zkp_proxy
                        .request_zkp_from_peers(self.sync.peers(), false);
                    let (synced_validity_window, _) = self.check_validity_window();
                    return Some(ConsensusEvent::Established {
                        synced_validity_window,
                    });
                } else {
                    // The head state check is carried out immediately after we reach the minimum
                    // number of peers and then after certain time intervals until consensus is reached.
                    // If we have a finished one, check its outcome.
                    if let Some(head_request) = finished_head_request {
                        debug!("Trying to establish consensus, checking head request ({} known, {} unknown).", head_request.num_known_blocks, head_request.num_unknown_blocks);
                        // We would like that 2/3 of our peers have a known state.
                        if head_request.num_known_blocks >= 2 * head_request.num_unknown_blocks {
                            info!("Consensus established, 2/3 of heads known.");
                            self.established_flag.swap(true, Ordering::Release);
                            self.zkp_proxy
                                .request_zkp_from_peers(self.sync.peers(), false);
                            let (synced_validity_window, _) = self.check_validity_window();
                            return Some(ConsensusEvent::Established {
                                synced_validity_window,
                            });
                        }
                    }
                    // If there's no ongoing head request, check whether we should start a new one.
                    self.request_heads();
                }
            }
        }
        None
    }

    /// Requests heads from connected peers in a predefined interval.
    fn request_heads(&mut self) {
        // If there's no ongoing head request and we have at least one peer, check whether we should
        // start a new one.
        if self.head_requests.is_none() && (self.num_agents() > 0 || self.min_peers == 0) {
            // This is the case if `head_requests_time` is unset or the timeout is hit.
            let should_start_request = self
                .head_requests_time
                .map(|time| time.elapsed() >= Self::HEAD_REQUESTS_TIMEOUT)
                .unwrap_or(true);
            if should_start_request {
                debug!(
                    "Initiating head requests (to {} peers)",
                    self.sync.num_peers()
                );
                self.head_requests = Some(HeadRequests::new(
                    self.sync.peers(),
                    Arc::clone(&self.network),
                    self.blockchain.clone(),
                ));
                self.head_requests_time = Some(Instant::now());
            }
        }
    }

    fn resolve_block(&mut self, request: ResolveBlockRequest<N>) {
        self.sync.resolve_block(request)
    }
}

impl<N: Network> Future for Consensus<N> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll and advance block queue
        while let Poll::Ready(Some(event)) = self.sync.poll_next_unpin(cx) {
            match event {
                LiveSyncPushEvent::AcceptedAnnouncedBlock(_) => {
                    // Reset the head request timer when an announced block was accepted.
                    self.head_requests_time = Some(Instant::now());
                }
                LiveSyncPushEvent::AcceptedBufferedBlock(_, remaining_in_buffer) => {
                    if !self.is_established() {
                        // Note: this output is parsed by our testing infrastructure (specifically devnet.sh),
                        // so please test that nothing breaks in there if you change this.
                        let block_number = {
                            let blockchain = self.blockchain.read();
                            blockchain.block_number()
                        };

                        info!(
                            "Catching up to tip of the chain (now at #{}, {} blocks remaining)",
                            block_number, remaining_in_buffer
                        );

                        if remaining_in_buffer == 0 {
                            self.head_requests_time = None;
                        }
                    }
                }
                LiveSyncPushEvent::ReceivedMissingBlocks(_) => {
                    if !self.is_established() {
                        // When syncing a stopped chain, we want to immediately start a new head request
                        // after receiving blocks for the current epoch.
                        self.head_requests_time = None;
                    }
                }
                LiveSyncPushEvent::RejectedBlock(hash) => {
                    warn!("Rejected block {}", hash);
                }
                LiveSyncPushEvent::AcceptedChunks(_) => {}
            }
        }

        // Check consensus established state on changes.
        if let Some(event) = self.check_established(None) {
            self.events.send(event).ok();
        }

        // Poll any head requests if active.
        if let Some(ref mut head_requests) = self.head_requests {
            if let Poll::Ready(mut result) = head_requests.poll_unpin(cx) {
                // Reset head requests.
                self.head_requests = None;

                // Push unknown blocks to the block queue, trying to sync.
                for (block, peer_id) in result.unknown_blocks.drain(..) {
                    self.sync.push_block(block, BlockSource::requested(peer_id));
                }

                // Update established state using the result.
                if let Some(event) = self.check_established(Some(result)) {
                    self.events.send(event).ok();
                }
            }
        }

        // Check if a ConsensusRequest was received
        while let Poll::Ready(Some(request)) = self.requests.1.poll_recv(cx) {
            match request {
                ConsensusRequest::ResolveBlock(request) => self.resolve_block(request),
            }
        }

        // Advance consensus and catch-up through head requests.
        self.request_heads();

        Poll::Pending
    }
}
