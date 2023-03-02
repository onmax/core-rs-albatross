use std::io;

use beserial::{Deserialize, ReadBytesExt, Serialize, SerializingError, WriteBytesExt};
use nimiq_hash::{Hash, SerializeContent};
use nimiq_keys::Address;
use nimiq_primitives::coin::Coin;

use crate::reward::RewardTransaction;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Serialize, Deserialize)]
#[repr(u8)]
/// Enum that represents the different types of inherents.
/// The moment when those inherents are applied depends upon the specific type of inherent.
pub enum InherentType {
    Reward,
    Slash,
    FinalizeBatch,
    FinalizeEpoch,
}

impl InherentType {
    /// Inherents can either be applied before transactions in a block or after them.
    /// In most cases, they will be applied after the transactions.
    /// An exception are slash transactions that park a staker.
    /// Following transactions should be able to unpark that staker, which is why slash inherents
    /// are applied before transactions.
    #[inline]
    pub fn is_pre_transactions(&self) -> bool {
        matches!(self, InherentType::Slash)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// An inherent is a special kind of transaction
/// It does not have a sender account, but it does have a target address and a value that is transfered
/// For instance, a reward (for producing blocks) is an inherent
pub struct Inherent {
    /// The type of the inherent
    pub ty: InherentType,
    /// The address where the value is deposit
    pub target: Address,
    /// The value
    pub value: Coin,
    /// Additional data (if needed)
    pub data: Vec<u8>,
}

impl Inherent {
    #[inline]
    pub fn is_pre_transactions(&self) -> bool {
        self.ty.is_pre_transactions()
    }
}

impl From<&RewardTransaction> for Inherent {
    fn from(tx: &RewardTransaction) -> Self {
        Self {
            ty: InherentType::Reward,
            target: tx.recipient.clone(),
            value: tx.value,
            data: vec![],
        }
    }
}

impl Hash for Inherent {}

impl SerializeContent for Inherent {
    fn serialize_content<W: io::Write>(&self, writer: &mut W) -> io::Result<usize> {
        let mut size = 0;
        size += Serialize::serialize(&self.ty, writer)?;
        size += Serialize::serialize(&self.target, writer)?;
        size += Serialize::serialize(&self.value, writer)?;
        // Serialize the length of the data.
        let length = self.data.len() as u32;
        size += Serialize::serialize(&length, writer)?;
        // Serialize each element of the vec.
        for i in 0..self.data.len() {
            size += Serialize::serialize(&self.data[i], writer)?;
        }
        Ok(size)
    }
}

impl Serialize for Inherent {
    fn serialize<W: WriteBytesExt>(&self, writer: &mut W) -> Result<usize, SerializingError> {
        let mut size = 0;
        size += Serialize::serialize(&self.ty, writer)?;
        size += Serialize::serialize(&self.target, writer)?;
        size += Serialize::serialize(&self.value, writer)?;
        // Serialize the length of the data.
        let length = self.data.len() as u32;
        size += Serialize::serialize(&length, writer)?;
        // Serialize each element of the vec.
        for i in 0..self.data.len() {
            size += Serialize::serialize(&self.data[i], writer)?;
        }
        Ok(size)
    }

    fn serialized_size(&self) -> usize {
        let mut size = 1;
        size += Serialize::serialized_size(&self.target);
        size += Serialize::serialized_size(&self.value);
        size += 4;
        size += self.data.len();
        size
    }
}

impl Deserialize for Inherent {
    fn deserialize<R: ReadBytesExt>(reader: &mut R) -> Result<Self, SerializingError> {
        let ty = Deserialize::deserialize(reader)?;
        let target = Deserialize::deserialize(reader)?;
        let value = Deserialize::deserialize(reader)?;
        let mut data = vec![];
        let length = Deserialize::deserialize(reader)?;
        for _i in 0..length {
            data.push(Deserialize::deserialize(reader)?);
        }
        Ok(Inherent {
            ty,
            target,
            value,
            data,
        })
    }
}