use std::cmp;

use nimiq_keys::Address;
use nimiq_utils::math::powi;
use once_cell::sync::OnceCell;
#[cfg(feature = "ts-types")]
use wasm_bindgen::prelude::*;

/// Global policy
static GLOBAL_POLICY: OnceCell<Policy> = OnceCell::new();

#[derive(Clone, Copy)]
#[cfg_attr(feature = "ts-types", cfg_eval::cfg_eval, wasm_bindgen)]
pub struct Policy {
    /// Length of a batch including the macro block
    #[cfg_attr(feature = "ts-types", wasm_bindgen(skip))]
    pub blocks_per_batch: u32,
    /// How many batches constitute an epoch
    #[cfg_attr(feature = "ts-types", wasm_bindgen(skip))]
    pub batches_per_epoch: u16,
    /// Maximum size of accounts trie chunks.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(skip))]
    pub state_chunks_max_size: u32,
    /// Number of batches a transaction is valid with Albatross consensus.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(skip))]
    pub transaction_validity_window: u32,
    /// Genesis block number
    #[cfg_attr(feature = "ts-types", wasm_bindgen(skip))]
    pub genesis_block_number: u32,
}

impl Policy {
    /// This is the address for the staking contract. Corresponds to
    /// 'NQ77 0000 0000 0000 0000 0000 0000 0000 0001'
    pub const STAKING_CONTRACT_ADDRESS: Address = Address([
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x01,
    ]);

    /// This is the address for the coinbase. Note that this is not a real account, it is just the
    /// address we use to denote that some coins originated from a coinbase event. Corresponds to
    /// 'NQ81 C01N BASE 0000 0000 0000 0000 0000 0000'
    pub const COINBASE_ADDRESS: Address = Address([
        0x60, 0x03, 0x65, 0xab, 0x4e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00,
    ]);

    /// The maximum allowed size, in bytes, for a micro block body.
    pub const MAX_SIZE_MICRO_BODY: usize = 100_000;

    // Maximum lengths for transaction sender_data and recipient_data fields used during deserialization.
    /// Maximum size for the sender data field of the transactions.
    /// The only transactions using this field are RemoveStake and Delete Validator.
    pub const MAX_TX_SENDER_DATA_SIZE: usize = 1;
    /// Maximum size for the recipient data field of the transactions.
    /// This is used by the HTLC, Vesting and staking contract transactions. Basic transactions are allowed to use it as well.
    /// The biggest transactions are the create validator and update validator with a web auth signature.
    pub const MAX_TX_RECIPIENT_DATA_SIZE: usize = 2112;
    /// Maximum length for basic transaction recipient_data used during intrinsic transaction verification.
    pub const MAX_BASIC_TX_RECIPIENT_DATA_SIZE: usize = 64;
    /// Maximum size for transaction's merkle path proofs.
    /// 32 bytes for Merkle Path Node * 32 max length + 4 bytes to encode the bools for left/right + 1 byte for the serialize vec count.
    pub const MAX_MERKLE_PATH_SIZE: usize = 1029;
    /// Maximum size for the total web auth fields.
    pub const MAX_SUPPORTED_WEB_AUTH_SIZE: usize = 512;

    /// The current version number of the protocol. Changing this always results in a hard fork.
    pub const VERSION: u16 = 1;

    /// Number of available validator slots. Note that a single validator may own several validator slots.
    pub const SLOTS: u16 = 512;

    /// Calculates 2f+1 slots which is the minimum number of slots necessary to produce a macro block,
    /// a skip block and other actions.
    /// It is also the minimum number of slots necessary to be guaranteed to have a majority of honest
    /// slots. That's because from a total of 3f+1 slots at most f will be malicious. If in a group of
    /// 2f+1 slots we have f malicious ones (which is the worst case scenario), that still leaves us
    /// with f+1 honest slots. Which is more than the f slots that are not in this group (which must all
    /// be honest).
    /// It is calculated as `ceil(SLOTS*2/3)` and we use the formula `ceil(x/y) = (x+y-1)/y` for the
    /// ceiling division.
    pub const TWO_F_PLUS_ONE: u16 = (2 * Self::SLOTS).div_ceil(3);

    /// Calculates f+1 slots which is the minimum number of slots necessary to be guaranteed to have at
    /// least one honest slots. That's because from a total of 3f+1 slots at most f will be malicious.
    /// It is calculated as `ceil(SLOTS/3)` and we use the formula `ceil(x/y) = (x+y-1)/y` for the
    /// ceiling division.
    pub const F_PLUS_ONE: u16 = Self::SLOTS.div_ceil(3);

    /// The minimum timeout in milliseconds for a validator to produce a block (4s)
    pub const MIN_PRODUCER_TIMEOUT: u64 = 4 * 1000;

    /// The optimal time in milliseconds between blocks (1s)
    pub const BLOCK_SEPARATION_TIME: u64 = 1000;

    /// Tendermint's initial timeout, in milliseconds.
    ///
    /// See <https://arxiv.org/abs/1807.04938v3> for more information.
    pub const TENDERMINT_TIMEOUT_INIT: u64 = 4 * 1000;

    /// Tendermint's timeout delta, in milliseconds.
    ///
    /// See <https://arxiv.org/abs/1807.04938v3> for more information.
    pub const TENDERMINT_TIMEOUT_DELTA: u64 = 1000;

    /// Minimum number of epochs that the ChainStore will store fully
    pub const MIN_EPOCHS_STORED: u32 = 1;

    /// The maximum drift, in milliseconds, that is allowed between any block's timestamp and the node's
    /// system time. We only care about drifting to the future.
    pub const TIMESTAMP_MAX_DRIFT: u64 = 600000;

    /// Reward decay for epochs produced late.
    ///
    /// See [`Policy::blocks_delay_penalty`] for more details on the calculation.
    pub const BLOCKS_DELAY_DECAY: f64 = 0.9999999989;

    /// The minimum rewards percentage that we allow
    pub const MINIMUM_REWARDS_PERCENTAGE: f64 = 0.5;

    /// The deposit necessary to create a validator in Lunas (1 NIM = 100,000 Lunas).
    /// A validator is someone who actually participates in block production. They are akin to miners
    /// in proof-of-work.
    /// The validator's balance can go below this amount only once retired and only in case the validator
    /// deletion fails and thus a fee gets deducted from it.
    pub const VALIDATOR_DEPOSIT: u64 = 10_000_000_000;

    /// The stake necessary to create a staker in Lunas (1 NIM = 100,000 Lunas).
    /// This minimum is applied to the amount of:
    ///     - non-retired stake
    ///     - total stake balance
    pub const MINIMUM_STAKE: u64 = 10_000_000;

    /// The number of epochs a validator is put in jail for. The jailing only happens for severe offenses.
    pub const JAIL_EPOCHS: u32 = 8;

    /// Total supply in units.
    pub const TOTAL_SUPPLY: u64 = 2_100_000_000_000_000;

    /// The supply decay is the base with which the remainder of the supply
    /// decreases. The available supply increases as a consequence.
    ///
    /// This constant describes the supply decay of one millisecond (timestamps
    /// are measured in milliseconds in this codebase).
    ///
    /// It mirrors the supply curve that existed prior to the PoS upgrade. Back
    /// then, the targeted block time was 60 s, and each block had a reward of
    /// 1/(2**22) of the remaining supply. Thus, this constant is the 60000th
    /// root of (1 - 1/(2**22)).
    pub const SUPPLY_DECAY: f64 = 0.9999999999960264;

    /// The maximum size of the BLS public key cache.
    pub const BLS_CACHE_MAX_CAPACITY: usize = 1000;

    /// Maximum size of history chunks.
    /// 25 MB.
    pub const HISTORY_CHUNKS_MAX_SIZE: u64 = 25 * 1024 * 1024;

    #[inline]
    fn get_blocks_per_epoch(&self) -> u32 {
        self.blocks_per_batch * self.batches_per_epoch as u32
    }

    #[inline]
    pub fn get_or_init(policy: Policy) -> Policy {
        *GLOBAL_POLICY.get_or_init(|| policy)
    }
}

#[cfg_attr(feature = "ts-types", wasm_bindgen)]
impl Policy {
    /// Number of batches a transaction is valid with Albatross consensus.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = TRANSACTION_VALIDITY_WINDOW))]
    pub fn transaction_validity_window() -> u32 {
        let policy = GLOBAL_POLICY.get_or_init(Self::default);
        assert!(policy.batches_per_epoch as u32 >= policy.transaction_validity_window);
        policy.transaction_validity_window
    }

    /// Number of blocks a transaction is valid with Albatross consensus.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = TRANSACTION_VALIDITY_WINDOW_BLOCKS))]
    pub fn transaction_validity_window_blocks() -> u32 {
        let policy = GLOBAL_POLICY.get_or_init(Self::default);
        assert!(policy.batches_per_epoch as u32 >= policy.transaction_validity_window);
        policy.transaction_validity_window
            * GLOBAL_POLICY.get_or_init(Self::default).blocks_per_batch
    }

    /// How many batches constitute an epoch
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = BATCHES_PER_EPOCH))]
    pub fn batches_per_epoch() -> u16 {
        GLOBAL_POLICY.get_or_init(Self::default).batches_per_epoch
    }

    /// Length of a batch including the macro block
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = BLOCKS_PER_BATCH))]
    pub fn blocks_per_batch() -> u32 {
        GLOBAL_POLICY.get_or_init(Self::default).blocks_per_batch
    }

    /// Length of an epoch including the election block
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = BLOCKS_PER_EPOCH))]
    pub fn blocks_per_epoch() -> u32 {
        GLOBAL_POLICY
            .get_or_init(Self::default)
            .get_blocks_per_epoch()
    }

    /// Genesis block number
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = GENESIS_BLOCK_NUMBER))]
    pub fn genesis_block_number() -> u32 {
        GLOBAL_POLICY
            .get_or_init(Self::default)
            .genesis_block_number
    }

    /// Maximum size of accounts trie chunks.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = STATE_CHUNKS_MAX_SIZE))]
    pub fn state_chunks_max_size() -> u32 {
        GLOBAL_POLICY
            .get_or_init(Policy::default)
            .state_chunks_max_size
    }

    /// Returns the epoch number at a given block number (height).
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = epochAt))]
    pub fn epoch_at(block_number: u32) -> u32 {
        // If the block number is less than the genesis, we are at epoch 0
        if block_number <= Self::genesis_block_number() {
            0
        } else {
            let block_number = block_number - Self::genesis_block_number();
            let blocks_per_epoch = Self::blocks_per_epoch();
            block_number.div_ceil(blocks_per_epoch)
        }
    }

    /// Returns the epoch index at a given block number. The epoch index is the number of a block relative
    /// to the epoch it is in. For example, the first block of any epoch always has an epoch index of 0.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = epochIndexAt))]
    pub fn epoch_index_at(block_number: u32) -> u32 {
        // Any block before the genesis is considered to be part of epoch 0
        if block_number < Self::genesis_block_number() {
            block_number
        } else {
            let blocks_per_epoch = Self::blocks_per_epoch();
            let block_number = block_number - Self::genesis_block_number();
            (block_number + blocks_per_epoch - 1) % blocks_per_epoch
        }
    }

    /// Returns the batch number at a given `block_number` (height)
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = batchAt))]
    pub fn batch_at(block_number: u32) -> u32 {
        // If the block number is less than the genesis, we are at batch 0
        if block_number <= Self::genesis_block_number() {
            0
        } else {
            let block_number = block_number - Self::genesis_block_number();
            let blocks_per_batch = Self::blocks_per_batch();
            block_number.div_ceil(blocks_per_batch)
        }
    }

    /// Returns the batch index at a given block number. The batch index is the number of a block relative
    /// to the batch it is in. For example, the first block of any batch always has an batch index of 0.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = batchIndexAt))]
    pub fn batch_index_at(block_number: u32) -> u32 {
        // No batches before the genesis block number
        if block_number < Self::genesis_block_number() {
            block_number
        } else {
            let blocks_per_batch = Self::blocks_per_batch();
            let block_number = block_number - Self::genesis_block_number();
            (block_number + blocks_per_batch - 1) % blocks_per_batch
        }
    }

    /// Returns the number (height) of the next election macro block after a given block number (height).
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = electionBlockAfter))]
    pub fn election_block_after(block_number: u32) -> u32 {
        // The next election block of any block before the genesis, is the genesis itself
        if block_number < Self::genesis_block_number() {
            Self::genesis_block_number()
        } else {
            let blocks_per_epoch = Self::blocks_per_epoch();
            let block_number = block_number - Self::genesis_block_number();
            ((block_number / blocks_per_epoch + 1) * blocks_per_epoch)
                + Self::genesis_block_number()
        }
    }

    /// Returns the block number (height) of the preceding election macro block before a given block number (height).
    /// If the given block number is an election macro block, it returns the election macro block before it.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = electionBlockBefore))]
    pub fn election_block_before(block_number: u32) -> u32 {
        match block_number.cmp(&Self::genesis_block_number()) {
            cmp::Ordering::Less => {
                panic!("No election blocks before the genesis block");
            }
            cmp::Ordering::Equal => {
                // The genesis is the first election block
                Self::genesis_block_number()
            }
            cmp::Ordering::Greater => {
                let blocks_per_epoch = Self::blocks_per_epoch();
                let block_number = block_number - Self::genesis_block_number();
                ((block_number - 1) / blocks_per_epoch * blocks_per_epoch)
                    + Self::genesis_block_number()
            }
        }
    }

    /// Returns the block number (height) of the last election macro block at a given block number (height).
    /// If the given block number is an election macro block, then it returns that block number.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = lastElectionBlock))]
    pub fn last_election_block(block_number: u32) -> u32 {
        // The last election block of any block before the genesis, is the genesis itself
        if block_number < Self::genesis_block_number() {
            panic!("No election blocks before the genesis block");
        } else {
            let blocks_per_epoch = Self::blocks_per_epoch();
            let block_number = block_number - Self::genesis_block_number();
            (block_number / blocks_per_epoch * blocks_per_epoch) + Self::genesis_block_number()
        }
    }

    /// Returns a boolean expressing if the block at a given block number (height) is an election macro block.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = isElectionBlockAt))]
    pub fn is_election_block_at(block_number: u32) -> bool {
        Self::epoch_index_at(block_number) == Self::blocks_per_epoch() - 1
    }

    /// Returns the block number (height) of the next macro block after a given block number (height).
    /// If the given block number is a macro block, it returns the macro block after it.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = macroBlockAfter))]
    pub fn macro_block_after(block_number: u32) -> u32 {
        // The next macro block of any block before the genesis, is the genesis itself
        if block_number < Self::genesis_block_number() {
            Self::genesis_block_number()
        } else {
            let block_number = block_number - Self::genesis_block_number();
            let blocks_per_batch = Self::blocks_per_batch();
            ((block_number / blocks_per_batch + 1) * blocks_per_batch)
                + Self::genesis_block_number()
        }
    }

    /// Returns the block number (height) of the preceding macro block before a given block number (height).
    /// If the given block number is a macro block, it returns the macro block before it.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = macroBlockBefore))]
    pub fn macro_block_before(block_number: u32) -> u32 {
        if block_number <= Self::genesis_block_number() {
            panic!("No macro blocks before genesis block");
        } else {
            let blocks_per_batch = Self::blocks_per_batch();
            let block_number = block_number - Self::genesis_block_number();
            ((block_number - 1) / blocks_per_batch * blocks_per_batch)
                + Self::genesis_block_number()
        }
    }

    /// Returns the block number (height) of the last macro block at a given block number (height).
    /// If the given block number is a macro block, then it returns that block number.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = lastMacroBlock))]
    pub fn last_macro_block(block_number: u32) -> u32 {
        // There is no macro block before the genesis
        if block_number < Self::genesis_block_number() {
            panic!("No macro blocks before genesis block");
        } else {
            let blocks_per_batch = Self::blocks_per_batch();
            let block_number = block_number - Self::genesis_block_number();
            (block_number / blocks_per_batch * blocks_per_batch) + Self::genesis_block_number()
        }
    }

    /// Returns a boolean expressing if the block at a given block number (height) is a macro block.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = isMacroBlockAt))]
    pub fn is_macro_block_at(block_number: u32) -> bool {
        // No macro blocks before genesis
        if block_number < Self::genesis_block_number() {
            false
        } else {
            Self::batch_index_at(block_number) == Self::blocks_per_batch() - 1
        }
    }

    /// Returns a boolean expressing if the block at a given block number (height) is a micro block.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = isMicroBlockAt))]
    pub fn is_micro_block_at(block_number: u32) -> bool {
        // No micro blocks before genesis
        if block_number < Self::genesis_block_number() {
            false
        } else {
            Self::batch_index_at(block_number) != Self::blocks_per_batch() - 1
        }
    }

    /// Returns the block number of the first block of the given epoch (which is always a micro block).
    /// If the index is out of bounds, None is returned
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = firstBlockOf))]
    pub fn first_block_of(epoch: u32) -> Option<u32> {
        epoch
            .checked_sub(1)?
            .checked_mul(Self::blocks_per_epoch())?
            .checked_add(1)?
            .checked_add(Self::genesis_block_number())
    }

    /// Returns the block number of the first block of the given batch (which is always a micro block).
    /// If the index is out of bounds, None is returned
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = firstBlockOfBatch))]
    pub fn first_block_of_batch(batch: u32) -> Option<u32> {
        batch
            .checked_sub(1)?
            .checked_mul(Self::blocks_per_batch())?
            .checked_add(1)?
            .checked_add(Self::genesis_block_number())
    }

    /// Returns the block number of the election macro block of the given epoch (which is always the last block).
    /// If the index is out of bounds, None is returned
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = electionBlockOf))]
    pub fn election_block_of(epoch: u32) -> Option<u32> {
        epoch
            .checked_mul(Self::blocks_per_epoch())?
            .checked_add(Self::genesis_block_number())
    }

    /// Returns the block number of the macro block (checkpoint or election) of the given batch (which
    /// is always the last block).
    /// If the index is out of bounds, None is returned
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = macroBlockOf))]
    pub fn macro_block_of(batch: u32) -> Option<u32> {
        batch
            .checked_mul(Self::blocks_per_batch())?
            .checked_add(Self::genesis_block_number())
    }

    /// Returns a boolean expressing if the batch at a given block number (height) is the first batch
    /// of the epoch.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = firstBatchOfEpoch))]
    pub fn first_batch_of_epoch(block_number: u32) -> bool {
        Self::epoch_index_at(block_number) < Self::blocks_per_batch()
    }

    /// Returns the block height for the last block of the reporting window of a given block number.
    /// Note: This window is meant for reporting malicious behaviour (aka `jailable` behaviour).
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = lastBlockOfReportingWindow))]
    pub fn last_block_of_reporting_window(block_number: u32) -> u32 {
        block_number + Self::blocks_per_epoch()
    }

    /// Returns the first block after the reporting window of a given block number has ended.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = blockAfterReportingWindow))]
    pub fn block_after_reporting_window(block_number: u32) -> u32 {
        Self::last_block_of_reporting_window(block_number) + 1
    }

    /// Returns the first block after the jail period of a given block number has ended.
    #[inline]
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = blockAfterJail))]
    pub fn block_after_jail(block_number: u32) -> u32 {
        block_number + Self::blocks_per_epoch() * Self::JAIL_EPOCHS + 1
    }

    /// Returns the supply at a given time (as Unix time) in Lunas (1 NIM = 100,000 Lunas). It is
    /// calculated using the following formula:
    /// ```text
    /// supply(t) = total_supply - (total_supply - genesis_supply) * supply_decay^t
    /// ```
    /// Where t is the time in milliseconds since the PoS genesis block and `genesis_supply` is the supply at
    /// the genesis of the Nimiq 2.0 chain.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = supplyAt))]
    pub fn supply_at(genesis_supply: u64, genesis_time: u64, current_time: u64) -> u64 {
        let t = current_time
            .checked_sub(genesis_time)
            .expect("current_time must be greater or equal to genesis_time");

        Policy::TOTAL_SUPPLY
            - ((Policy::TOTAL_SUPPLY - genesis_supply) as f64 * powi(Policy::SUPPLY_DECAY, t))
                as u64
    }

    /// Returns the percentage reduction that should be applied to the rewards due to a delayed batch.
    /// This function returns a float in the range [0, 1]
    /// I.e 1 means that the full rewards should be given, whereas 0.5 means that half of the rewards should be given
    /// The input to this function is the batch delay, in milliseconds
    /// The function is: [(1 - MINIMUM_REWARDS_PERCENTAGE) * BLOCKS_DELAY_DECAY ^ (t^2)] + MINIMUM_REWARDS_PERCENTAGE
    #[cfg_attr(feature = "ts-types", wasm_bindgen(js_name = batchDelayPenalty))]
    pub fn batch_delay_penalty(delay: u64) -> f64 {
        (1.0 - Self::MINIMUM_REWARDS_PERCENTAGE)
            * powi(powi(Self::BLOCKS_DELAY_DECAY, delay), delay)
            + Self::MINIMUM_REWARDS_PERCENTAGE
    }
}

// wasm_bindgen does not support exposing `pub const` struct fields, so we reimplement those consts
// as getters when compiling for WASM.
#[cfg(feature = "ts-types")]
#[wasm_bindgen]
impl Policy {
    /// This is the address for the staking contract.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = STAKING_CONTRACT_ADDRESS))]
    pub fn wasm_staking_contract_address() -> String {
        Self::STAKING_CONTRACT_ADDRESS.to_user_friendly_address()
    }

    /// This is the address for the coinbase. Note that this is not a real account, it is just the
    /// address we use to denote that some coins originated from a coinbase event.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = COINBASE_ADDRESS))]
    pub fn wasm_coinbase_address() -> String {
        Self::COINBASE_ADDRESS.to_user_friendly_address()
    }

    /// The maximum allowed size, in bytes, for a micro block body.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = MAX_SIZE_MICRO_BODY))]
    pub fn wasm_max_size_micro_body() -> usize {
        Self::MAX_SIZE_MICRO_BODY
    }

    /// The current version number of the protocol. Changing this always results in a hard fork.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = VERSION))]
    pub fn wasm_version() -> u16 {
        Self::VERSION
    }

    /// Number of available validator slots. Note that a single validator may own several validator slots.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = SLOTS))]
    pub fn wasm_slots() -> u16 {
        Self::SLOTS
    }

    /// Calculates 2f+1 slots which is the minimum number of slots necessary to produce a macro block,
    /// a skip block and other actions.
    /// It is also the minimum number of slots necessary to be guaranteed to have a majority of honest
    /// slots. That's because from a total of 3f+1 slots at most f will be malicious. If in a group of
    /// 2f+1 slots we have f malicious ones (which is the worst case scenario), that still leaves us
    /// with f+1 honest slots. Which is more than the f slots that are not in this group (which must all
    /// be honest).
    /// It is calculated as `ceil(SLOTS*2/3)` and we use the formula `ceil(x/y) = (x+y-1)/y` for the
    /// ceiling division.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = TWO_F_PLUS_ONE))]
    pub fn wasm_two_f_plus_one() -> u16 {
        Self::TWO_F_PLUS_ONE
    }

    /// Calculates f+1 slots which is the minimum number of slots necessary to be guaranteed to have at
    /// least one honest slots. That's because from a total of 3f+1 slots at most f will be malicious.
    /// It is calculated as `ceil(SLOTS/3)` and we use the formula `ceil(x/y) = (x+y-1)/y` for the
    /// ceiling division.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = F_PLUS_ONE))]
    pub fn wasm_f_plus_one() -> u16 {
        Self::F_PLUS_ONE
    }

    /// The minimum timeout in milliseconds for a validator to produce a block (4s)
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = MIN_PRODUCER_TIMEOUT))]
    pub fn wasm_min_block_producer_timeout() -> u64 {
        Self::MIN_PRODUCER_TIMEOUT
    }

    /// The optimal time in milliseconds between blocks (1s)
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = BLOCK_SEPARATION_TIME))]
    pub fn wasm_block_separation_time() -> u64 {
        Self::BLOCK_SEPARATION_TIME
    }

    /// Minimum number of epochs that the ChainStore will store fully
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = MIN_EPOCHS_STORED))]
    pub fn wasm_min_epochs_stored() -> u32 {
        Self::MIN_EPOCHS_STORED
    }

    /// The maximum drift, in milliseconds, that is allowed between any block's timestamp and the node's
    /// system time. We only care about drifting to the future.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = TIMESTAMP_MAX_DRIFT))]
    pub fn wasm_timestamp_max_drift() -> u64 {
        Self::TIMESTAMP_MAX_DRIFT
    }

    /// The minimum rewards percentage that we allow
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = MINIMUM_REWARDS_PERCENTAGE))]
    pub fn wasm_minimum_rewards_percentage() -> f64 {
        Self::MINIMUM_REWARDS_PERCENTAGE
    }

    /// The deposit necessary to create a validator in Lunas (1 NIM = 100,000 Lunas).
    /// A validator is someone who actually participates in block production. They are akin to miners
    /// in proof-of-work.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = VALIDATOR_DEPOSIT))]
    pub fn wasm_validator_deposit() -> u64 {
        Self::VALIDATOR_DEPOSIT
    }

    /// The number of epochs a validator is put in jail for. The jailing only happens for severe offenses.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = JAIL_EPOCHS))]
    pub fn wasm_jail_epochs() -> u32 {
        Self::JAIL_EPOCHS
    }

    /// Total supply in units.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = TOTAL_SUPPLY))]
    pub fn wasm_total_supply() -> u64 {
        Self::TOTAL_SUPPLY
    }

    /// The maximum size of the BLS public key cache.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = BLS_CACHE_MAX_CAPACITY))]
    pub fn wasm_bls_cache_max_capacity() -> usize {
        Self::BLS_CACHE_MAX_CAPACITY
    }

    /// Maximum size of history chunks.
    /// 25 MB.
    #[cfg_attr(feature = "ts-types", wasm_bindgen(getter = HISTORY_CHUNKS_MAX_SIZE))]
    pub fn wasm_history_chunks_max_size() -> u64 {
        Self::HISTORY_CHUNKS_MAX_SIZE
    }
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            blocks_per_batch: 60,
            batches_per_epoch: 720,
            state_chunks_max_size: 1000,
            transaction_validity_window: 120,
            genesis_block_number: 0,
        }
    }
}

pub const TEST_POLICY: Policy = Policy {
    blocks_per_batch: 32,
    batches_per_epoch: 4,
    state_chunks_max_size: 3,
    transaction_validity_window: 2,
    // This number should match the one that is defined in the `unit` network genesis file which is the genesis used for unit testing
    genesis_block_number: 200,
};

#[cfg(test)]
mod tests {
    use nimiq_test_log::test;

    use super::*;

    fn initialize_policy() {
        let _ = Policy::get_or_init(TEST_POLICY);
    }

    #[test]
    fn it_correctly_computes_epoch() {
        initialize_policy();
        assert_eq!(Policy::epoch_at(Policy::genesis_block_number()), 0);
        assert_eq!(Policy::epoch_at(1 + Policy::genesis_block_number()), 1);
        assert_eq!(
            Policy::epoch_at(Policy::blocks_per_epoch() + Policy::genesis_block_number()),
            1
        );
        assert_eq!(
            Policy::epoch_at(Policy::blocks_per_epoch() + Policy::genesis_block_number() + 1),
            2
        );
    }

    #[test]
    fn it_correctly_computes_epoch_index() {
        initialize_policy();
        assert_eq!(
            Policy::epoch_index_at(1 + Policy::genesis_block_number()),
            0
        );
        assert_eq!(
            Policy::epoch_index_at(2 + Policy::genesis_block_number()),
            1
        );
        assert_eq!(
            Policy::epoch_index_at(Policy::blocks_per_epoch() + Policy::genesis_block_number()),
            127
        );
        assert_eq!(
            Policy::epoch_index_at(Policy::blocks_per_epoch() + Policy::genesis_block_number() + 1),
            0
        );
    }

    #[test]
    fn it_correctly_computes_batch() {
        initialize_policy();
        assert_eq!(Policy::batch_at(Policy::genesis_block_number()), 0);
        assert_eq!(Policy::batch_at(1 + Policy::genesis_block_number()), 1);
        assert_eq!(
            Policy::batch_at(Policy::blocks_per_batch() + Policy::genesis_block_number()),
            1
        );
        assert_eq!(
            Policy::batch_at(Policy::blocks_per_batch() + Policy::genesis_block_number() + 1),
            2
        );
    }

    #[test]
    fn it_correctly_computes_batch_index() {
        initialize_policy();
        assert_eq!(
            Policy::batch_index_at(1 + Policy::genesis_block_number()),
            0
        );
        assert_eq!(
            Policy::batch_index_at(2 + Policy::genesis_block_number()),
            1
        );
        assert_eq!(
            Policy::batch_index_at(Policy::blocks_per_epoch() + Policy::genesis_block_number()),
            31
        );
        assert_eq!(
            Policy::batch_index_at(Policy::blocks_per_epoch() + Policy::genesis_block_number() + 1),
            0
        );
    }

    #[test]
    fn it_correctly_computes_block_positions() {
        initialize_policy();
        assert!(Policy::is_macro_block_at(Policy::genesis_block_number()));
        assert!(!Policy::is_micro_block_at(Policy::genesis_block_number()));
        assert!(Policy::is_election_block_at(Policy::genesis_block_number()));

        assert!(!Policy::is_macro_block_at(
            1 + Policy::genesis_block_number()
        ));
        assert!(Policy::is_micro_block_at(
            1 + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_election_block_at(
            1 + Policy::genesis_block_number()
        ));

        assert!(!Policy::is_macro_block_at(
            2 + Policy::genesis_block_number()
        ));
        assert!(Policy::is_micro_block_at(
            2 + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_election_block_at(
            2 + Policy::genesis_block_number()
        ));

        assert!(Policy::is_macro_block_at(
            Policy::blocks_per_batch() + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_micro_block_at(
            Policy::blocks_per_batch() + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_election_block_at(
            Policy::blocks_per_batch() + Policy::genesis_block_number()
        ));

        assert!(!Policy::is_macro_block_at(
            127 + Policy::genesis_block_number()
        ));
        assert!(Policy::is_micro_block_at(
            127 + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_election_block_at(
            127 + Policy::genesis_block_number()
        ));

        assert!(Policy::is_macro_block_at(
            Policy::blocks_per_epoch() + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_micro_block_at(
            Policy::blocks_per_epoch() + Policy::genesis_block_number()
        ));
        assert!(Policy::is_election_block_at(
            Policy::blocks_per_epoch() + Policy::genesis_block_number()
        ));

        assert!(!Policy::is_macro_block_at(
            Policy::blocks_per_epoch() + Policy::genesis_block_number() + 1
        ));
        assert!(Policy::is_micro_block_at(
            Policy::blocks_per_epoch() + Policy::genesis_block_number() + 1
        ));
        assert!(!Policy::is_election_block_at(
            Policy::blocks_per_epoch() + Policy::genesis_block_number() + 1
        ));

        assert!(Policy::is_macro_block_at(
            Policy::blocks_per_epoch()
                + Policy::blocks_per_batch()
                + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_micro_block_at(
            Policy::blocks_per_epoch()
                + Policy::blocks_per_batch()
                + Policy::genesis_block_number()
        ));
        assert!(!Policy::is_election_block_at(
            Policy::blocks_per_epoch()
                + Policy::blocks_per_batch()
                + Policy::genesis_block_number()
        ));
    }

    #[test]
    fn it_correctly_computes_macro_numbers() {
        initialize_policy();
        assert_eq!(
            Policy::macro_block_after(Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_batch()
        );
        assert_eq!(
            Policy::macro_block_after(1 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_batch()
        );
        assert_eq!(
            Policy::macro_block_after(127 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::macro_block_after(Policy::blocks_per_epoch() + Policy::genesis_block_number()),
            Policy::genesis_block_number() + 160
        );
        assert_eq!(
            Policy::macro_block_after(129 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + 160
        );

        assert_eq!(
            Policy::macro_block_before(1 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::macro_block_before(2 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::macro_block_before(127 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + 96
        );
        assert_eq!(
            Policy::macro_block_before(128 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + 96
        );
        assert_eq!(
            Policy::macro_block_before(129 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::macro_block_before(130 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::last_macro_block(Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::last_macro_block(1 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::last_macro_block(31 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );

        assert_eq!(
            Policy::last_macro_block(
                Policy::blocks_per_batch() + Policy::genesis_block_number() + 1
            ),
            Policy::genesis_block_number() + 32
        );
    }

    #[test]
    fn it_correctly_computes_election_numbers() {
        initialize_policy();
        assert_eq!(
            Policy::election_block_after(Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::election_block_after(1 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::election_block_after(127 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::election_block_after(128 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + 256
        );
        assert_eq!(
            Policy::election_block_after(129 + Policy::genesis_block_number()),
            Policy::genesis_block_number() + 256
        );

        assert_eq!(
            Policy::election_block_before(1 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::election_block_before(2 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::election_block_before(127 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::election_block_before(
                Policy::blocks_per_epoch() + Policy::genesis_block_number()
            ),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::election_block_before(
                Policy::blocks_per_epoch() + 1 + Policy::genesis_block_number()
            ),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::election_block_before(
                Policy::blocks_per_epoch() + 2 + Policy::genesis_block_number()
            ),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );

        assert_eq!(
            Policy::last_election_block(Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::last_election_block(1 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::last_election_block(127 + Policy::genesis_block_number()),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::last_election_block(
                Policy::blocks_per_epoch() + Policy::genesis_block_number()
            ),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
        assert_eq!(
            Policy::last_election_block(
                Policy::blocks_per_epoch() + Policy::genesis_block_number() + 1
            ),
            Policy::genesis_block_number() + Policy::blocks_per_epoch()
        );
    }

    #[test]
    fn it_correctly_commutes_first_ofs() {
        initialize_policy();
        assert_eq!(
            Policy::first_block_of(1),
            Some(Policy::genesis_block_number() + 1)
        );
        assert_eq!(
            Policy::first_block_of(2),
            Some(Policy::genesis_block_number() + Policy::blocks_per_epoch() + 1)
        );

        assert_eq!(
            Policy::first_block_of_batch(1),
            Some(1 + Policy::genesis_block_number())
        );
        assert_eq!(
            Policy::first_block_of_batch(2),
            Some(33 + Policy::genesis_block_number())
        );
        assert_eq!(
            Policy::first_block_of_batch(3),
            Some(65 + Policy::genesis_block_number())
        );
        assert_eq!(
            Policy::first_block_of_batch(4),
            Some(97 + Policy::genesis_block_number())
        );
        assert_eq!(
            Policy::first_block_of_batch(5),
            Some(129 + Policy::genesis_block_number())
        );
        assert_eq!(Policy::first_block_of_batch(4294967295), None);
    }

    #[test]
    fn it_correctly_computes_first_batch_of_epoch() {
        initialize_policy();
        assert!(Policy::first_batch_of_epoch(
            1 + Policy::genesis_block_number()
        ));
        assert!(Policy::first_batch_of_epoch(
            Policy::blocks_per_batch() + Policy::genesis_block_number()
        ));
        assert!(!Policy::first_batch_of_epoch(
            Policy::blocks_per_batch() + 1 + Policy::genesis_block_number()
        ));
        assert!(!Policy::first_batch_of_epoch(
            Policy::blocks_per_epoch() + Policy::genesis_block_number()
        ));
        assert!(Policy::first_batch_of_epoch(
            Policy::blocks_per_epoch() + 1 + Policy::genesis_block_number()
        ));
    }

    #[test]
    fn non_zero_genesis_extra_tests() {
        initialize_policy();

        // Anything prior to genesis belongs to epoch 0
        assert_eq!(Policy::epoch_at(Policy::genesis_block_number()), 0);
        assert_eq!(Policy::epoch_at(40), 0);
        // Epoch 1 starts at genesis + 1
        assert_eq!(Policy::epoch_at(1 + Policy::genesis_block_number()), 1);

        // If genesis is 200, this corresponds to block 401.
        assert_eq!(
            Policy::epoch_index_at(2 * Policy::genesis_block_number() + 1),
            401 - (Policy::genesis_block_number() + Policy::blocks_per_epoch()) - 1
        );

        //First batch starts after genesis
        assert_eq!(Policy::batch_at(Policy::genesis_block_number() + 1), 1);
        //Anything prior to genesis belongs to batch 0
        assert_eq!(Policy::batch_at(Policy::genesis_block_number() - 15), 0);

        assert_eq!(
            Policy::batch_index_at(Policy::genesis_block_number() + 1),
            0
        );
        assert_eq!(
            Policy::batch_index_at(Policy::genesis_block_number() + 2),
            1
        );

        // No macro blocks before genesis
        assert!(!Policy::is_macro_block_at(1));
        assert!(Policy::is_macro_block_at(Policy::genesis_block_number()));

        // No micro blocks before genesis
        assert!(!Policy::is_micro_block_at(
            Policy::genesis_block_number() - 20
        ));
        assert!(!Policy::is_micro_block_at(15));

        // Genesis is a macro/election block
        assert!(Policy::is_macro_block_at(Policy::genesis_block_number()));
        assert!(Policy::is_election_block_at(Policy::genesis_block_number()));

        // The next macro for any pre-genesis block is the genesis itself
        assert_eq!(Policy::macro_block_after(0), Policy::genesis_block_number());
        assert_eq!(Policy::macro_block_after(5), Policy::genesis_block_number());

        // The next election for any pre-genesis block is the genesis itself
        assert_eq!(
            Policy::election_block_after(0),
            Policy::genesis_block_number()
        );
        assert_eq!(
            Policy::election_block_after(10),
            Policy::genesis_block_number()
        );
    }
}
