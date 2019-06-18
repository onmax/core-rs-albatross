use std::borrow::Cow;
use std::io;
use std::io::Write;
use std::iter::FromIterator;
use std::sync::Arc;

use failure::Fail;

use beserial::{Deserialize, Serialize};
use block::{Block, MicroBlock};
use collections::bitset::BitSet;
use database::{AsDatabaseBytes, Database, DatabaseFlags, Environment, FromDatabaseValue, ReadTransaction, WriteTransaction};
use database::cursor::{ReadCursor, WriteCursor};
use hash::{Blake2bHasher, Hasher};
use primitives::coin::Coin;
use primitives::policy;
use primitives::validators::{Slot, Slots};

use crate::chain_store::ChainStore;
use crate::reward_registry::reward_pot::RewardPot;

mod reward_pot;
mod slashed_slots;

pub use crate::reward_registry::slashed_slots::SlashedSlots;

pub struct SlashRegistry<'env> {
    env: &'env Environment,
    chain_store: Arc<ChainStore<'env>>,
    slash_registry_db: Database<'env>,
    reward_pot: RewardPot<'env>,
}

// TODO Better error messages
#[derive(Debug, Fail)]
pub enum SlashPushError {
    #[fail(display = "Redundant fork proofs in block")]
    DuplicateForkProof,
    #[fail(display = "Block contains fork proof targeting a slot that was already slashes")]
    SlotAlreadySlashed,
    #[fail(display = "Block slashes slots in wrong epoch")]
    InvalidEpochTarget,
    #[fail(display = "Got block with unexpected block number")]
    UnexpectedBlock,
}

#[derive(Debug, Fail)]
pub enum EpochStateError {
    #[fail(display = "Block precedes requested epoch")]
    BlockPrecedesEpoch,
    #[fail(display = "Requested epoch too old to be tracked at block number")]
    HistoricEpoch,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlockDescriptor {
    epoch_state: BitSet,
    prev_epoch_state: BitSet,
}

// TODO Pass in active validator set + seed through parameters
//      or always load from chain store?
impl<'env> SlashRegistry<'env> {
    const SLASH_REGISTRY_DB_NAME: &'static str = "SlashRegistry";

    pub fn new(env: &'env Environment, chain_store: Arc<ChainStore<'env>>) -> Self {
        let slash_registry_db = env.open_database_with_flags(SlashRegistry::SLASH_REGISTRY_DB_NAME.to_string(), DatabaseFlags::UINT_KEYS);

        Self {
            env,
            chain_store,
            slash_registry_db,
            reward_pot: RewardPot::new(env),
        }
    }

    #[inline]
    pub fn current_reward_pot(&self) -> Coin {
        self.reward_pot.current_reward_pot()
    }

    #[inline]
    pub fn previous_reward_pot(&self) -> Coin {
        self.reward_pot.previous_reward_pot()
    }

    /// Register slashes of block
    ///  * `block` - Block to commit
    ///  * `seed`- Seed of previous block
    ///  * `staking_contract` - Contract used to check minimum stakes
    #[inline]
    pub fn commit_block(&self, txn: &mut WriteTransaction, block: &Block, slots: &Slots) -> Result<(), SlashPushError> {
        match block {
            Block::Macro(ref macro_block) => {
                self.reward_pot.commit_macro_block(macro_block, txn);
                self.gc(txn, policy::epoch_at(macro_block.header.block_number));
                Ok(())
            },
            Block::Micro(ref micro_block) => {
                self.reward_pot.commit_micro_block(micro_block, slots, txn);
                self.commit_micro_block(txn, micro_block, slots)
            },
        }
    }

    fn commit_micro_block(&self, txn: &mut WriteTransaction, block: &MicroBlock, slots: &Slots) -> Result<(), SlashPushError> {
        let block_epoch = policy::epoch_at(block.header.block_number);
        let mut epoch_diff = BitSet::new();
        let mut prev_epoch_diff = BitSet::new();

        // Mark from fork proofs.
        let fork_proofs = &block.extrinsics.as_ref().unwrap().fork_proofs;
        for fork_proof in fork_proofs {
            let block_number = fork_proof.header1.block_number;
            let view_number = fork_proof.header1.view_number;
            let slot_owner = self.slot_owner(block_number, view_number, slots).expect("Could not determine block producer in the current epoch");

            let slash_epoch = policy::epoch_at(block_number);
            if block_epoch == slash_epoch {
                if epoch_diff.contains(slot_owner.0 as usize) {
                    return Err(SlashPushError::DuplicateForkProof);
                }
                epoch_diff.insert(slot_owner.0 as usize);
            } else if block_epoch == slash_epoch + 1 {
                if prev_epoch_diff.contains(slot_owner.0 as usize) {
                    return Err(SlashPushError::DuplicateForkProof);
                }
                prev_epoch_diff.insert(slot_owner.0 as usize);
            } else {
                return Err(SlashPushError::InvalidEpochTarget);
            }
        }

        // Lookup slash state.
        let mut cursor = txn.cursor(&self.slash_registry_db);
        // Move cursor to first entry with a block number >= ours (or end of the database).
        let _: Option<(u32, BlockDescriptor)> = cursor.seek_range_key(&block.header.block_number);
        // Then move cursor back by one.
        let last_change: Option<(u32, BlockDescriptor)> = cursor.prev();

        let mut prev_epoch_state: BitSet;
        let mut epoch_state: BitSet;
        if let Some((change_block_number, change)) = last_change {
            if change_block_number >= policy::first_block_of(block_epoch) {
                // last_change was in current epoch
                prev_epoch_state = change.prev_epoch_state;
                epoch_state = change.epoch_state;
            } else if block_epoch > 0 && change_block_number >= policy::first_block_of(block_epoch - 1) {
                // last_change was in previous epoch
                prev_epoch_state = change.epoch_state;
                epoch_state = BitSet::new();
            } else {
                // no change in the last two epochs
                prev_epoch_state = BitSet::new();
                epoch_state = BitSet::new();
            }
        } else {
            // no change at all
            prev_epoch_state = BitSet::new();
            epoch_state = BitSet::new();
        }

        drop(cursor);

        // Detect duplicate slashes
        if (&prev_epoch_state & &prev_epoch_diff).len() != 0
            || (&epoch_state & &epoch_diff).len() != 0 {
            return Err(SlashPushError::SlotAlreadySlashed);
        }

        // Mark from view changes, ignoring duplicates.
        for view in 0..block.header.view_number {
            let slot_owner = self.slot_owner(block.header.block_number, view, slots).expect("Could not determine block producer in the current epoch");
            epoch_diff.insert(slot_owner.0 as usize);
        }

        // Apply slashes.
        prev_epoch_state |= prev_epoch_diff;
        epoch_state |= epoch_diff;

        // Push block descriptor and remember slash hashes.
        let descriptor = BlockDescriptor { epoch_state, prev_epoch_state };

        // Put descriptor into database.
        txn.put(&self.slash_registry_db, &block.header.block_number, &descriptor);

        Ok(())
    }

    fn gc(&self, txn: &mut WriteTransaction, current_epoch: u32) {
        let mut cursor = txn.write_cursor(&self.slash_registry_db);
        let mut pos: Option<(u32, BlockDescriptor)> = cursor.first();

        let cutoff = if current_epoch > 2 {
            policy::first_block_of(current_epoch - 1)
        } else {
            0u32
        };

        while let Some((block_number, _)) = pos {
            if block_number >= cutoff {
                return;
            }
            cursor.remove();
            pos = cursor.next();
        }
    }

    #[inline]
    pub fn revert_block(&self, txn: &mut WriteTransaction, block: &Block, slots: &Slots) -> Result<(), SlashPushError> {
        if let Block::Micro(ref block) = block {
            self.reward_pot.revert_micro_block(block, &slots, txn);
            self.revert_micro_block(txn, block)
        } else {
            unreachable!()
        }
    }

    fn revert_micro_block(&self, txn: &mut WriteTransaction, block: &MicroBlock) -> Result<(), SlashPushError> {
        txn.remove(&self.slash_registry_db, &block.header.block_number);
        Ok(())
    }

    // Get slot owner at block and view number
    pub fn slot_owner(&self, block_number: u32, view_number: u32, slots: &Slots) -> Option<(u16, Slot)> {
        // Get context
        if let Some(prev_block) = self.chain_store
            .get_block_at(block_number - 1, None) {

            // Get slots of epoch
            let slashed_set = self.slashed_set_at(policy::epoch_at(block_number), block_number).unwrap();
            let honest_validators = Vec::from_iter(SlashedSlots::new(&slots, &slashed_set).enabled().cloned());

            // Hash seed and index
            let mut hash_state = Blake2bHasher::new();
            prev_block.seed().serialize(&mut hash_state).unwrap();
            hash_state.write(&view_number.to_be_bytes()).unwrap();
            let hash = hash_state.finish();

            // Get number from first 8 bytes
            let mut num_bytes = [0u8; 8];
            num_bytes.copy_from_slice(&hash.as_bytes()[..8]);
            let num = u64::from_be_bytes(num_bytes);

            // XXX This is not uniform!
            let index = num % honest_validators.len() as u64;
            Some((index as u16, honest_validators[index as usize].clone()))
        }
        else {
            // XXX No slot owner available for this block. Use an Result?
            None
        }
    }

    // Get latest known slash set of epoch
    pub fn slashed_set(&self, epoch_number: u32) -> BitSet {
        self.slashed_set_at(epoch_number, policy::first_block_of(epoch_number + 2)).unwrap()
    }

    // Get slash set of epoch at specific block number
    // Returns slash set before applying block with that block_number (TODO Tests)
    pub fn slashed_set_at(&self, epoch_number: u32, block_number: u32) -> Result<BitSet, EpochStateError> {
        let epoch_start = policy::first_block_of(policy::epoch_at(block_number));

        // Epoch cannot have slashes if in the future
        if block_number < epoch_start {
            return Err(EpochStateError::BlockPrecedesEpoch);
        }

        // Epoch slashes are only tracked for two epochs
        // First block of (epoch + 2) is fine because upper lookup bound is exclusive.
        if block_number > policy::first_block_of(epoch_number + 2) {
            return Err(EpochStateError::HistoricEpoch);
        }

        let txn = ReadTransaction::new(self.env);

        // Lookup slash state.
        let mut cursor = txn.cursor(&self.slash_registry_db);
        // Move cursor to first entry with a block number >= ours (or end of the database).
        let _: Option<(u32, BlockDescriptor)> = cursor.seek_range_key(&block_number);
        // Then move cursor back by one.
        let last_change: Option<(u32, BlockDescriptor)> = cursor.prev();

        if let Some((change_block_number, change)) = last_change {
            if change_block_number >= epoch_start {
                Ok(change.epoch_state)
            } else {
                Ok(BitSet::new())
            }
        } else {
            Ok(BitSet::new())
        }
    }
}

impl AsDatabaseBytes for BlockDescriptor {
    fn as_database_bytes(&self) -> Cow<[u8]> {
        let v = Serialize::serialize_to_vec(&self);
        Cow::Owned(v)
    }
}

impl FromDatabaseValue for BlockDescriptor {
    fn copy_from_database(bytes: &[u8]) -> io::Result<Self> where Self: Sized {
        let mut cursor = io::Cursor::new(bytes);
        Ok(Deserialize::deserialize(&mut cursor)?)
    }
}
