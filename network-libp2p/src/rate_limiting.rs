use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use instant::Instant;
use libp2p::{gossipsub::TopicHash, PeerId};
use nimiq_network_interface::{
    network::Topic,
    request::{RequestCommon, RequestType},
};

/// The rate limiting request metadata that will be passed on between the network and the swarm.
/// This is not sent through the wire.
#[derive(Debug, PartialEq)]
pub(crate) struct RateLimitConfig {
    /// Maximum requests allowed in the time window.
    pub(crate) max_requests: u32,
    ///  The range/window of time to consider.
    pub(crate) time_window: Duration,
}

impl RateLimitConfig {
    pub(crate) fn from_request<Req: RequestCommon>() -> Self {
        Self {
            max_requests: Req::MAX_REQUESTS,
            time_window: Req::TIME_WINDOW,
        }
    }

    pub(crate) fn from_topic<T: Topic>() -> Self {
        Self {
            max_requests: T::MAX_MESSAGES,
            time_window: T::TIME_WINDOW,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum RateLimitId {
    Request(RequestType),
    Gossipsub(TopicHash),
}

/// Holds the expiration time for a given peer and request type. This struct defines the ordering for the btree set.
/// The smaller expiration times come first.
#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub(crate) struct Expiration {
    pub(crate) peer_id: PeerId,
    pub(crate) rate_limit_id: RateLimitId,
    pub(crate) expiration_time: Instant,
}

impl Expiration {
    pub(crate) fn new(
        peer_id: PeerId,
        rate_limit_id: RateLimitId,
        expiration_time: Instant,
    ) -> Self {
        Self {
            peer_id,
            rate_limit_id,
            expiration_time,
        }
    }
}

impl Ord for Expiration {
    fn cmp(&self, other: &Self) -> Ordering {
        self.expiration_time
            .cmp(&other.expiration_time)
            .then_with(|| self.peer_id.cmp(&other.peer_id))
            .then_with(|| self.rate_limit_id.cmp(&other.rate_limit_id))
    }
}
impl PartialOrd for Expiration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The structure to be used to store the pending to delete rate limits.
/// We must ensure there are no duplicates, constant complexity while accessing peer and request type and ordering by
/// expiration time.
/// These structs should contain the same information and maintain consistency.
#[derive(Debug, Default)]
pub(crate) struct PendingDeletion {
    /// The hash map of the rate limits.
    by_peer_and_id: HashMap<(PeerId, RateLimitId), RateLimit>,
    /// The ordered set of rate limits by expiration time, peer id and rate limit id.
    by_expiration_time: BTreeSet<Expiration>,
}

impl PendingDeletion {
    /// Retrieves the first item by expiration.
    pub(crate) fn first(&self) -> Option<&Expiration> {
        self.by_expiration_time.first()
    }

    /// Adds to both structures the new entry. If the entry already exists we replace it on both structs.
    pub(crate) fn insert(
        &mut self,
        peer_id: PeerId,
        rate_limit_id: RateLimitId,
        rate_limit: &RateLimit,
    ) {
        let key = (peer_id, rate_limit_id.clone());
        if let Some(expiration_peer) = self.by_peer_and_id.insert(key, rate_limit.clone()) {
            self.by_expiration_time.remove(&Expiration::new(
                peer_id,
                rate_limit_id.clone(),
                expiration_peer.next_reset_time(),
            ));
        }
        self.by_expiration_time.insert(Expiration::new(
            peer_id,
            rate_limit_id,
            rate_limit.next_reset_time(),
        ));
    }

    /// Removes the first item by expiration date, on both structures.
    pub(crate) fn remove_first(&mut self) {
        if let Some(expiration) = self.by_expiration_time.pop_first() {
            assert!(
                self.by_peer_and_id
                    .remove(&(expiration.peer_id, expiration.rate_limit_id))
                    .is_some(),
                "The pending for deletion rate limits should be consistent among them"
            )
        }
    }
}

/// The structure to be used to limit the number of requests to a limit of allowed_occurrences within a block_range.
#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct RateLimit {
    /// Max allowed requests.
    allowed_occurrences: u32,
    /// The range/window of time.
    time_window: Duration,
    /// The timestamp of the last reset.
    last_reset: Instant,
    /// The counter of requests submitted within the current block range.
    occurrences_counter: u32,
}

impl RateLimit {
    pub fn new(allowed_occurrences: u32, time_window: Duration, last_reset: Instant) -> Self {
        RateLimit {
            allowed_occurrences,
            time_window,
            last_reset,
            occurrences_counter: 0,
        }
    }

    /// Updates the last_reset if needed and then increments the counter of number of requests by
    /// the specified number.
    /// Receives the number to increment the counter and the current time measured in seconds.
    pub fn increment_and_is_allowed(&mut self, request_count: u32) -> bool {
        let current_time = Instant::now();
        if self.next_reset_time() <= current_time {
            self.last_reset = current_time;
            self.occurrences_counter = 0;
        }
        self.occurrences_counter += request_count;
        self.occurrences_counter <= self.allowed_occurrences
    }

    /// Checks if this object can be deleted by understanding if there are still active counters.
    pub fn can_delete(&self, current_time: Instant) -> bool {
        self.occurrences_counter == 0 || self.next_reset_time() <= current_time
    }

    /// Returns the timestamp for the next reset of the counters.
    pub fn next_reset_time(&self) -> Instant {
        self.last_reset + self.time_window
    }
}

// Rate limiting overarching structure. It holds the rate limits by peer and request type.
// This handles the case of a peer reconnecting within the time window to attempt to bypass the rate limits established.
#[derive(Default)]
pub(crate) struct RateLimits {
    /// The rate limits per active peer.
    rate_limits: HashMap<PeerId, HashMap<RateLimitId, RateLimit>>,
    /// All the pending deletion rate limits.
    rate_limits_pending_deletion: PendingDeletion,
}

impl RateLimits {
    /// Increases the counter of the rate limit and returns a bool in case the defined rate limit is surpassed.
    pub(crate) fn exceeds_rate_limit(
        &mut self,
        peer_id: PeerId,
        rate_limit_id: RateLimitId,
        rate_limit_config: &RateLimitConfig,
    ) -> bool {
        // If the peer has never sent a request of this type, creates a new entry.
        let requests_limit = self
            .rate_limits
            .entry(peer_id)
            .or_default()
            .entry(rate_limit_id)
            .or_insert_with(|| {
                RateLimit::new(
                    rate_limit_config.max_requests,
                    rate_limit_config.time_window,
                    Instant::now(),
                )
            });

        // Ensures that the request is allowed based on the set limits and updates the counter.
        !requests_limit.increment_and_is_allowed(1)
    }

    /// Mark all rate limits of a given peer as pending for deletion.
    /// Every time this is called the expired rate limits will get delete pruned.
    pub(crate) fn remove_rate_limits(&mut self, peer_id: PeerId) {
        // Every time a peer disconnects, we delete all expired pending limits.
        self.clean_up();

        // Go through all existing request types of the given peer and deletes the limit counters if possible or marks it for deletion.
        if let Some(rate_limits) = self.rate_limits.get_mut(&peer_id) {
            rate_limits.retain(|rate_limit_id, rate_limit| {
                // Gets the requests limit and deletes it if no counter info would be lost, otherwise places it as pending deletion.
                if !rate_limit.can_delete(Instant::now()) {
                    self.rate_limits_pending_deletion.insert(
                        peer_id,
                        rate_limit_id.clone(),
                        rate_limit,
                    );
                    true
                } else {
                    false
                }
            });
            // If the peer no longer has any pending rate limits, then it gets removed.
            if rate_limits.is_empty() {
                self.rate_limits.remove(&peer_id);
            }
        }
    }

    /// Deletes the rate limits that were previously marked as pending if its expiration time has passed.
    fn clean_up(&mut self) {
        // Iterates from the oldest to the most recent expiration date and deletes the entries that have expired.
        // The pending to deletion is ordered from the oldest to the most recent expiration date, thus we break early
        // from the loop once we find a non expired rate limit.
        while let Some(expiration) = self.rate_limits_pending_deletion.first() {
            let current_timestamp = Instant::now();
            if expiration.expiration_time <= current_timestamp {
                if let Some(rate_limits) =
                    self.rate_limits
                        .get_mut(&expiration.peer_id)
                        .and_then(|rate_limits| {
                            if let Some(rate_limit) = rate_limits.get(&expiration.rate_limit_id) {
                                // If the peer has reconnected the rate limit may be enforcing a new limit. In this case we only remove
                                // the pending deletion.
                                if rate_limit.can_delete(current_timestamp) {
                                    rate_limits.remove(&expiration.rate_limit_id);
                                }
                                return Some(rate_limits);
                            }
                            // Only returns None if no request type was found.
                            None
                        })
                {
                    // If the peer no longer has any pending rate limits, then it gets removed from both rate limits and pending deletion.
                    if rate_limits.is_empty() {
                        self.rate_limits.remove(&expiration.peer_id);
                    }
                } else {
                    // If the information is in pending deletion, that should mean it was not deleted from peer_request_limits yet, so that
                    // reconnection doesn't bypass the limits we are enforcing.
                    unreachable!(
                        "Tried to remove a non existing rate limit from peer_request_limits."
                    );
                }
                // Removes the entry from the pending for deletion.
                self.rate_limits_pending_deletion.remove_first();
            } else {
                break;
            }
        }
    }
}
