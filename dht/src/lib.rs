use nimiq_blockchain_proxy::BlockchainProxy;
use nimiq_keys::{Address, KeyPair};
use nimiq_network_libp2p::{
    dht::{DhtRecord, DhtVerifierError, Verifier as DhtVerifier},
    libp2p::kad::Record,
    PeerId,
};
use nimiq_serde::Deserialize;
use nimiq_utils::tagged_signing::{TaggedSignable, TaggedSigned};
use nimiq_validator_network::validator_record::ValidatorRecord;

pub struct Verifier {
    blockchain: BlockchainProxy,
}

impl Verifier {
    pub fn new(blockchain: BlockchainProxy) -> Self {
        Self { blockchain }
    }

    fn verify_validator_record(&self, record: &Record) -> Result<DhtRecord, DhtVerifierError> {
        // Deserialize the value of the record, which is a ValidatorRecord. If it fails return an error.
        let validator_record =
            TaggedSigned::<ValidatorRecord<PeerId>, KeyPair>::deserialize_from_vec(&record.value)
                .map_err(DhtVerifierError::MalformedValue)?;

        // Deserialize the key of the record which is an Address. If it fails return an error.
        let validator_address = Address::deserialize_from_vec(record.key.as_ref())
            .map_err(DhtVerifierError::MalformedKey)?;

        // Acquire blockchain read access. For now exclude Light clients.
        let blockchain = match self.blockchain {
            BlockchainProxy::Light(ref _light_blockchain) => {
                return Err(DhtVerifierError::UnknownTag)
            }
            BlockchainProxy::Full(ref full_blockchain) => full_blockchain,
        };
        let blockchain_read = blockchain.read();

        // Get the staking contract to retrieve the public key for verification.
        let staking_contract = blockchain_read
            .get_staking_contract_if_complete(None)
            .ok_or(DhtVerifierError::StateIncomplete)?;

        // Get the public key needed for verification.
        let data_store = blockchain_read.get_staking_contract_store();
        let txn = blockchain_read.read_transaction();
        let public_key = staking_contract
            .get_validator(&data_store.read(&txn), &validator_address)
            .ok_or(DhtVerifierError::UnknownValidator(validator_address))?
            .signing_key;

        // Verify the record.
        validator_record
            .verify(&public_key)
            .then(|| {
                DhtRecord::Validator(
                    record.publisher.unwrap(),
                    validator_record.record,
                    record.clone(),
                )
            })
            .ok_or(DhtVerifierError::InvalidSignature)
    }
}

impl DhtVerifier for Verifier {
    fn verify(&self, record: &Record) -> Result<DhtRecord, DhtVerifierError> {
        // Peek the tag to know what kind of record this is.
        let Some(tag) = TaggedSigned::<ValidatorRecord<PeerId>, KeyPair>::peek_tag(&record.value)
        else {
            log::warn!(?record, "DHT Tag not peekable.");
            return Err(DhtVerifierError::MalformedTag);
        };

        // Depending on tag perform the verification.
        match tag {
            ValidatorRecord::<PeerId>::TAG => self.verify_validator_record(record),
            _ => {
                log::error!(tag, "DHT invalid record tag received");
                Err(DhtVerifierError::UnknownTag)
            }
        }
    }
}
