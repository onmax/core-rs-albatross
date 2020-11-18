use beserial::{Deserialize, Serialize};
use nimiq_hash::{Blake2sHash, Hash, SerializeContent};
use std::io;

// Multiple things this needs to take care of when it comes to what needs signing here:
// First of all to be able to create a block proof the signatures must be over a hash which includes:
// * block-height
// * tendermint round
// * proposal header hash
// * the merkle root of the validator set.
// * implicit: TendermintStep which also works as the prefix for the specific message which is signed (read purpose byte)
//
// In addition to that the correct assignment of specific contributions to their aggregations also needs part of these informations.
// Additionally replay of any given contribution for a different aggregation must not be possible.
// * block_height
// * round_number
// * step
//
// in summary, the tag which handel will be working on will be `TendermintIdentifier`
// The signature will then be over the follwing serialized values (in order):
// `id.step(also prefix) + id.block_number + id.round_number + proposal.header.hash() + create_merkle_root()`
// Note that each one of those is fixed size and thus no overflow from one to the next can be constructed.
//
// the proof needs to contain additional miscallaneous information then, as it would otherwise be lost to time:
// * round_number
//
// that can be included plain text as the proof alongside it also contains it.

// TODO move to validator and merge with other prefixes.
// OR at least take the same value here (as we want to limit the enum to these 2 values only.)
// i.e
// enum Step {
//      PreVote = Validator::MessagePrefix::PreVote,
//      PreCommit = Validator::MessagePrefix::PreCommit,
// }
#[derive(
    Serialize, Deserialize, std::fmt::Debug, Clone, Ord, PartialOrd, PartialEq, Eq, Hash, Copy,
)]
#[repr(u8)]
pub enum TendermintStep {
    PreVote = 0x02, // works as a prefix to the hashing as well. Since View Change has prefix 0x01 we continue here with 0x02 and 0x03
    PreCommit = 0x03,
}

#[derive(Serialize, Deserialize, std::fmt::Debug, Clone, Eq, PartialEq)]
pub(crate) struct TendermintIdentifier {
    pub block_number: u32,
    pub round_number: u32,
    pub step: TendermintStep,
}

#[derive(std::fmt::Debug, Clone, Eq, PartialEq)]
pub(crate) struct TendermintVote {
    /// MacroHeader hash of the proposed macro block
    pub(crate) proposal_hash: Option<Blake2sHash>,
    /// Identifier to this votes aggregation
    pub(crate) id: TendermintIdentifier,
    /// The merkle root of validators is required for consensus.
    pub(crate) validator_merkle_root: Vec<u8>,
}

impl TendermintVote {
    pub fn proposal(&self) -> Option<Blake2sHash> {
        self.proposal_hash.clone()
    }
}

/// Custom Serialize Content, to make sure that
/// * step byte, which is also the message prefix always comes first
/// * options have the same byte length when they are None as when they are Some(x) to prevent overflowing one option into the other.
impl SerializeContent for TendermintVote {
    fn serialize_content<W: io::Write>(&self, writer: &mut W) -> io::Result<usize> {
        // First of all serialize step as this also serves as the unique prefix for this message type.
        let mut size = self.id.step.serialize(writer)?;

        // serialize the round number
        size += self.id.round_number.serialize(writer)?;

        // serialize the block number
        size += self.id.block_number.serialize(writer)?;

        // For the hash, make sure that if the Option is None the byte length stays the same, just filled with 0s.
        size += match &self.proposal_hash {
            Some(hash) => hash.serialize(writer),
            None => {
                let zero_bytes: Vec<u8> = vec![0u8, Blake2sHash::SIZE as u8];
                match writer.write_all(zero_bytes.as_slice()) {
                    Err(err) => Err(beserial::SerializingError::IoError(err)),
                    Ok(_) => Ok(Blake2sHash::SIZE),
                }
            }
        }?;

        // serialize the validator_merkle_root
        size += {
            writer.write_all(self.validator_merkle_root.as_slice())?;
            self.validator_merkle_root.len()
        };
        // Finally attempt to flush
        writer.flush()?;

        // And return the size
        Ok(size)
    }
}

impl Hash for TendermintVote {}
