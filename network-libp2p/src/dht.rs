use libp2p::{kad::Record, PeerId};
use nimiq_keys::Address;
use nimiq_serde::DeserializeError;
use nimiq_validator_network::validator_record::ValidatorRecord;

pub use crate::network_types::DhtRecord;

#[derive(Debug)]
pub enum DhtVerifierError {
    MalformedTag,
    MalformedKey(DeserializeError),
    MalformedValue(DeserializeError),
    UnknownTag,
    UnknownValidator(Address),
    StateIncomplete,
    InvalidSignature,
}

pub trait Verifier: Send + Sync {
    fn verify(&self, record: &Record) -> Result<DhtRecord, DhtVerifierError>;
}

/// Dummy implementation for testcases
impl Verifier for () {
    fn verify(&self, record: &Record) -> Result<DhtRecord, DhtVerifierError> {
        Ok(DhtRecord::Validator(
            PeerId::random(),
            ValidatorRecord::<PeerId>::new(PeerId::random(), 0u64),
            record.clone(),
        ))
    }
}
