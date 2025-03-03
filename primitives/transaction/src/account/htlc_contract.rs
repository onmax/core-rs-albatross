use std::{borrow::Cow, str::FromStr};

use nimiq_hash::{
    sha512::{Sha512Hash, Sha512Hasher},
    Blake2bHash, Blake2bHasher, Hasher, Sha256Hash, Sha256Hasher,
};
use nimiq_keys::Address;
use nimiq_macros::{add_hex_io_fns_typed_arr, add_serialization_fns_typed_arr, create_typed_array};
use nimiq_primitives::account::AccountType;
use nimiq_serde::{Deserialize, Serialize};

use crate::{
    account::AccountTransactionVerification, PoWSignatureProof, SignatureProof, Transaction,
    TransactionError, TransactionFlags,
};

/// The verifier trait for a hash time locked contract. This only uses data available in the transaction.
pub struct HashedTimeLockedContractVerifier {}

impl AccountTransactionVerification for HashedTimeLockedContractVerifier {
    fn verify_incoming_transaction(transaction: &Transaction) -> Result<(), TransactionError> {
        assert_eq!(transaction.recipient_type, AccountType::HTLC);

        if !transaction
            .flags
            .contains(TransactionFlags::CONTRACT_CREATION)
        {
            warn!(
                "Only contract creation is allowed for the following transaction:\n{:?}",
                transaction
            );
            return Err(TransactionError::InvalidForRecipient);
        }

        if transaction.flags.contains(TransactionFlags::SIGNALING) {
            warn!(
                "Signaling not allowed for the following transaction:\n{:?}",
                transaction
            );
            return Err(TransactionError::InvalidForRecipient);
        }

        if transaction.recipient != transaction.contract_creation_address() {
            warn!("Recipient address must match contract creation address for the following transaction:\n{:?}",
                transaction);
            return Err(TransactionError::InvalidForRecipient);
        }

        if transaction.network_id.is_albatross() {
            if transaction.recipient_data.len() != (20 * 2 + 1 + 32 + 1 + 8)
                && transaction.recipient_data.len() != (20 * 2 + 1 + 64 + 1 + 8)
            {
                warn!(
                    data_len = transaction.recipient_data.len(),
                    ?transaction,
                    "Invalid data length. For the following transaction",
                );
                return Err(TransactionError::InvalidData);
            }

            CreationTransactionData::parse(transaction)?.verify()
        } else {
            // PoW HTLC creation data specified the timeout (last field) as a u32 block number instead of a timestamp.
            if transaction.recipient_data.len() != (20 * 2 + 1 + 32 + 1 + 4)
                && transaction.recipient_data.len() != (20 * 2 + 1 + 64 + 1 + 4)
            {
                return Err(TransactionError::InvalidData);
            }
            PoWCreationTransactionData::parse(transaction)?.verify()
        }
    }

    fn verify_outgoing_transaction(transaction: &Transaction) -> Result<(), TransactionError> {
        assert_eq!(transaction.sender_type, AccountType::HTLC);

        if !transaction.sender_data.is_empty() {
            warn!(
                "The following transaction can't have sender data:\n{:?}",
                transaction
            );
            return Err(TransactionError::Overflow);
        }

        if transaction.network_id.is_albatross() {
            let proof = OutgoingHTLCTransactionProof::parse(transaction)?;
            proof.verify(transaction)?;
        } else {
            let proof = PoWOutgoingHTLCTransactionProof::parse(transaction)?;
            proof.verify(transaction)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum AnyHash {
    Blake2b(AnyHash32),
    Sha256(AnyHash32),
    Sha512(AnyHash64),
}

impl AnyHash {
    /// Returns the hex string representation of the hash
    pub fn to_hex(&self) -> String {
        match self {
            AnyHash::Blake2b(hash) => hash.to_hex(),
            AnyHash::Sha256(hash) => hash.to_hex(),
            AnyHash::Sha512(hash) => hash.to_hex(),
        }
    }

    /// Returns the raw bytes of the hash
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            AnyHash::Blake2b(hash) => &hash.0,
            AnyHash::Sha256(hash) => &hash.0,
            AnyHash::Sha512(hash) => &hash.0,
        }
    }
}

impl Default for AnyHash {
    fn default() -> Self {
        AnyHash::Blake2b(AnyHash32::default())
    }
}

impl From<Blake2bHash> for AnyHash {
    fn from(value: Blake2bHash) -> Self {
        AnyHash::Blake2b(AnyHash32(value.into()))
    }
}

impl From<Sha256Hash> for AnyHash {
    fn from(value: Sha256Hash) -> Self {
        AnyHash::Sha256(AnyHash32(value.into()))
    }
}

impl From<Sha512Hash> for AnyHash {
    fn from(value: Sha512Hash) -> Self {
        AnyHash::Sha512(AnyHash64(value.into()))
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum PreImage {
    PreImage32(AnyHash32),
    PreImage64(AnyHash64),
}

impl PreImage {
    /// Returns the hex string representation of the pre-image
    pub fn to_hex(&self) -> String {
        match self {
            PreImage::PreImage32(hash) => hash.to_hex(),
            PreImage::PreImage64(hash) => hash.to_hex(),
        }
    }

    /// Returns the raw bytes of the pre-image
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            PreImage::PreImage32(hash) => &hash.0,
            PreImage::PreImage64(hash) => &hash.0,
        }
    }
}

impl Default for PreImage {
    fn default() -> Self {
        PreImage::PreImage32(AnyHash32::default())
    }
}

impl From<Blake2bHash> for PreImage {
    fn from(value: Blake2bHash) -> Self {
        PreImage::PreImage32(AnyHash32(value.into()))
    }
}

impl From<Sha256Hash> for PreImage {
    fn from(value: Sha256Hash) -> Self {
        PreImage::PreImage32(AnyHash32(value.into()))
    }
}

impl From<Sha512Hash> for PreImage {
    fn from(value: Sha512Hash) -> Self {
        PreImage::PreImage64(AnyHash64(value.into()))
    }
}

impl FromStr for PreImage {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == AnyHash32::SIZE * 2 {
            Ok(PreImage::PreImage32(AnyHash32::from_str(s)?))
        } else if s.len() == AnyHash64::SIZE * 2 {
            Ok(PreImage::PreImage64(AnyHash64::from_str(s)?))
        } else {
            Err(hex::FromHexError::InvalidStringLength)
        }
    }
}

create_typed_array!(AnyHash32, u8, 32);
add_hex_io_fns_typed_arr!(AnyHash32, AnyHash32::SIZE);
add_serialization_fns_typed_arr!(AnyHash32, AnyHash32::SIZE);

create_typed_array!(AnyHash64, u8, 64);
add_hex_io_fns_typed_arr!(AnyHash64, AnyHash64::SIZE);
add_serialization_fns_typed_arr!(AnyHash64, AnyHash64::SIZE);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct CreationTransactionData {
    /// Address that is allowed to redeem the funds after the timeout.
    pub sender: Address,
    /// Address that is allowed to redeem the funds before the timeout.
    pub recipient: Address,
    /// The hash root of the contract. The recipient can redeem the funds before the timeout by providing
    /// a pre-image that hashes to this root.
    pub hash_root: AnyHash,
    /// The number of times the pre-image must be hashed to match the `hash_root`. Must be at least 1.
    /// A number higher than 1 allows the recipient to provide an already hashed pre-image, with the
    /// remaining number of hashes required to match the `hash_root` corresponding to the fraction of
    /// the funds that can be claimed.
    pub hash_count: u8,
    #[serde(with = "nimiq_serde::fixint::be")]
    /// The timeout as a millisecond timestamp before which the `recipient` and after which the `sender`
    /// can claim the funds.
    pub timeout: u64,
}

impl CreationTransactionData {
    pub fn parse_data(data: &[u8]) -> Result<Self, TransactionError> {
        Ok(Self::deserialize_all(data)?)
    }

    pub fn parse(transaction: &Transaction) -> Result<Self, TransactionError> {
        Self::parse_data(&transaction.recipient_data)
    }

    pub fn verify(&self) -> Result<(), TransactionError> {
        if self.hash_count == 0 {
            warn!("Invalid creation data: hash_count may not be zero");
            return Err(TransactionError::InvalidData);
        }
        Ok(())
    }
}

/// This struct represents HTLC creation data in the Proof-of-Work chain. The only difference to the data in
/// the Albatross chain is that the `timeout` was a u32 block number in PoW instead of a u64 timestamp.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PoWCreationTransactionData {
    /// Address that is allowed to redeem the funds after the timeout.
    pub sender: Address,
    /// Address that is allowed to redeem the funds before the timeout.
    pub recipient: Address,
    /// The hash root of the contract. The recipient can redeem the funds before the timeout by providing
    /// a pre-image that hashes to this root.
    pub hash_root: AnyHash,
    /// The number of times the pre-image must be hashed to match the `hash_root`. Must be at least 1.
    /// A number higher than 1 allows the recipient to provide an already hashed pre-image, with the
    /// remaining number of hashes required to match the `hash_root` corresponding to the fraction of
    /// the funds that can be claimed.
    pub hash_count: u8,
    #[serde(with = "nimiq_serde::fixint::be")]
    /// The timeout as a block height before which the `recipient` and after which the `sender`
    /// can claim the funds.
    pub timeout: u32,
}

impl PoWCreationTransactionData {
    pub fn parse_data(data: &[u8]) -> Result<Self, TransactionError> {
        Ok(Self::deserialize_all(data)?)
    }

    pub fn parse(transaction: &Transaction) -> Result<Self, TransactionError> {
        Self::parse_data(&transaction.recipient_data)
    }

    pub fn verify(&self) -> Result<(), TransactionError> {
        if self.hash_count == 0 {
            return Err(TransactionError::InvalidData);
        }
        Ok(())
    }

    pub fn into_pos(
        self,
        genesis_block_number: u32,
        genesis_timestamp: u64,
    ) -> CreationTransactionData {
        let timeout = if self.timeout <= genesis_block_number {
            genesis_timestamp - (genesis_block_number - self.timeout) as u64 * 60_000
        } else {
            genesis_timestamp + (self.timeout - genesis_block_number) as u64 * 60_000
        };

        CreationTransactionData {
            sender: self.sender,
            recipient: self.recipient,
            hash_root: self.hash_root,
            hash_count: self.hash_count,
            timeout,
        }
    }
}

/// The `OutgoingHTLCTransactionProof` represents a serializable form of all possible proof types
/// for a transaction from a HTLC contract.
///
/// The funds can be unlocked by one of three mechanisms:
/// 1. After a blockchain height called `timeout` is reached, the `sender` can withdraw the funds.
///     (called `TimeoutResolve`)
/// 2. The contract stores a `hash_root`. The `recipient` can withdraw the funds before the
///     `timeout` has been reached by presenting a hash that will yield the `hash_root`
///     when re-hashing it `hash_count` times.
///     By presenting a hash that will yield the `hash_root` after re-hashing it k < `hash_count`
///     times, the `recipient` can retrieve 1/k of the funds.
///     (called `RegularTransfer`)
/// 3. If both `sender` and `recipient` sign the transaction, the funds can be withdrawn at any time.
///     (called `EarlyResolve`)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum OutgoingHTLCTransactionProof {
    RegularTransfer {
        hash_depth: u8,
        hash_root: AnyHash,
        pre_image: PreImage,
        signature_proof: SignatureProof,
    },
    EarlyResolve {
        signature_proof_recipient: SignatureProof,
        signature_proof_sender: SignatureProof,
    },
    TimeoutResolve {
        signature_proof_sender: SignatureProof,
    },
}

impl OutgoingHTLCTransactionProof {
    pub fn parse(transaction: &Transaction) -> Result<Self, TransactionError> {
        Ok(Self::deserialize_all(&transaction.proof)?)
    }

    pub fn verify(&self, transaction: &Transaction) -> Result<(), TransactionError> {
        // Verify proof.
        let tx_content = transaction.serialize_content();
        let tx_buf = tx_content.as_slice();

        match self {
            OutgoingHTLCTransactionProof::RegularTransfer {
                hash_depth,
                hash_root,
                pre_image,
                signature_proof,
            } => {
                let mut tmp_hash = pre_image.clone();
                for _ in 0..*hash_depth {
                    match &hash_root {
                        AnyHash::Blake2b(_) => {
                            tmp_hash = PreImage::from(
                                Blake2bHasher::default().digest(tmp_hash.as_bytes()),
                            );
                        }
                        AnyHash::Sha256(_) => {
                            tmp_hash =
                                PreImage::from(Sha256Hasher::default().digest(tmp_hash.as_bytes()));
                        }
                        AnyHash::Sha512(_) => {
                            tmp_hash =
                                PreImage::from(Sha512Hasher::default().digest(tmp_hash.as_bytes()));
                        }
                    }
                }

                if hash_root.as_bytes() != tmp_hash.as_bytes() {
                    warn!(
                        "Hash algorithm mismatch for the following transaction:\n{:?}",
                        transaction
                    );
                    return Err(TransactionError::InvalidProof);
                }

                if !signature_proof.verify(tx_buf) {
                    warn!(
                        "Invalid signature for the following transaction:\n{:?}",
                        transaction
                    );
                    return Err(TransactionError::InvalidProof);
                }
            }
            OutgoingHTLCTransactionProof::EarlyResolve {
                signature_proof_recipient,
                signature_proof_sender,
            } => {
                if !signature_proof_recipient.verify(tx_buf)
                    || !signature_proof_sender.verify(tx_buf)
                {
                    warn!(
                        "Invalid signature for the following transaction:\n{:?}",
                        transaction
                    );
                    return Err(TransactionError::InvalidProof);
                }
            }
            OutgoingHTLCTransactionProof::TimeoutResolve {
                signature_proof_sender,
            } => {
                if !signature_proof_sender.verify(tx_buf) {
                    warn!(
                        "Invalid signature for the following transaction:\n{:?}",
                        transaction
                    );
                    return Err(TransactionError::InvalidProof);
                }
            }
        }

        Ok(())
    }
}

/// This struct represents a HTLC redeem proof for the regular transfer case in the Proof-of-Work chain.
/// Differences to the Proof-of-Stake is the serialization (PoW had a different position for the algorithm type
/// and no PreImage type prefix) and that the signature proof is a PoWSignatureProof and thus shorter than in PoS.
#[derive(Clone, Debug)]
pub struct PoWRegularTransfer {
    // PoW regular transfers encode the hash algorithm as the first u8 byte,
    // but in Rust, the algorithm is encoded in the hash_root AnyHash enum.
    hash_depth: u8,
    hash_root: AnyHash,
    pre_image: PreImage,
    signature_proof: PoWSignatureProof,
}

/// Enum over the different types of outgoing HTLC transaction proofs in the Proof-of-Work chain.
/// The differences to Proof-of-Stake are the variant IDs (they start at 1 in PoW, while they start at 0 in PoS)
/// and that all signature proofs are PoWSignatureProofs.
#[derive(Clone, Debug, Deserialize)]
#[repr(u8)]
pub enum PoWOutgoingHTLCTransactionProof {
    DummyZero, // In PoW, RegularTransfer has ID 1, so we need a dummy ID 0
    RegularTransfer(PoWRegularTransfer),
    EarlyResolve {
        signature_proof_recipient: PoWSignatureProof,
        signature_proof_sender: PoWSignatureProof,
    },
    TimeoutResolve {
        signature_proof_sender: PoWSignatureProof,
    },
}

impl PoWOutgoingHTLCTransactionProof {
    pub fn parse(transaction: &Transaction) -> Result<Self, TransactionError> {
        Ok(Self::deserialize_all(&transaction.proof)?)
    }

    pub fn verify(&self, transaction: &Transaction) -> Result<(), TransactionError> {
        // Verify proof.
        let tx_content = transaction.serialize_content();
        let tx_buf = tx_content.as_slice();

        match self {
            PoWOutgoingHTLCTransactionProof::DummyZero => {
                return Err(TransactionError::InvalidProof);
            }
            PoWOutgoingHTLCTransactionProof::RegularTransfer(PoWRegularTransfer {
                hash_depth,
                hash_root,
                pre_image,
                signature_proof,
            }) => {
                let mut tmp_hash = pre_image.clone();
                for _ in 0..*hash_depth {
                    match &hash_root {
                        AnyHash::Blake2b(_) => {
                            tmp_hash = PreImage::from(
                                Blake2bHasher::default().digest(tmp_hash.as_bytes()),
                            );
                        }
                        AnyHash::Sha256(_) => {
                            tmp_hash =
                                PreImage::from(Sha256Hasher::default().digest(tmp_hash.as_bytes()));
                        }
                        AnyHash::Sha512(_) => {
                            tmp_hash =
                                PreImage::from(Sha512Hasher::default().digest(tmp_hash.as_bytes()));
                        }
                    }
                }

                if hash_root.as_bytes() != tmp_hash.as_bytes() {
                    return Err(TransactionError::InvalidProof);
                }

                if !signature_proof.verify(tx_buf) {
                    return Err(TransactionError::InvalidProof);
                }
            }
            PoWOutgoingHTLCTransactionProof::EarlyResolve {
                signature_proof_recipient,
                signature_proof_sender,
            } => {
                if !signature_proof_recipient.verify(tx_buf)
                    || !signature_proof_sender.verify(tx_buf)
                {
                    return Err(TransactionError::InvalidProof);
                }
            }
            PoWOutgoingHTLCTransactionProof::TimeoutResolve {
                signature_proof_sender,
            } => {
                if !signature_proof_sender.verify(tx_buf) {
                    return Err(TransactionError::InvalidProof);
                }
            }
        }

        Ok(())
    }

    pub fn into_pos(self) -> OutgoingHTLCTransactionProof {
        match self {
            Self::DummyZero => panic!("DummyZero is not a valid PoWOutgoingHTLCTransactionProof"),
            Self::RegularTransfer(PoWRegularTransfer {
                hash_depth,
                hash_root,
                pre_image,
                signature_proof,
            }) => OutgoingHTLCTransactionProof::RegularTransfer {
                hash_depth,
                hash_root,
                pre_image,
                signature_proof: signature_proof.into_pos(),
            },
            Self::EarlyResolve {
                signature_proof_recipient,
                signature_proof_sender,
            } => OutgoingHTLCTransactionProof::EarlyResolve {
                signature_proof_recipient: signature_proof_recipient.into_pos(),
                signature_proof_sender: signature_proof_sender.into_pos(),
            },
            Self::TimeoutResolve {
                signature_proof_sender,
            } => OutgoingHTLCTransactionProof::TimeoutResolve {
                signature_proof_sender: signature_proof_sender.into_pos(),
            },
        }
    }
}

mod serde_derive {
    use std::{borrow::Cow, fmt, str::FromStr};

    use serde::{
        de::{Deserialize, Deserializer, Error, MapAccess, SeqAccess, Visitor},
        ser::{Serialize, SerializeStruct, Serializer},
    };

    use super::{AnyHash, AnyHash32, AnyHash64, PoWRegularTransfer, PoWSignatureProof, PreImage};

    const ANYHASH_FIELDS: &[&str] = &["algorithm", "hash"];
    const PREIMAGE_FIELDS: &[&str] = &["type", "pre_image"];
    const POW_REGULAR_TRANSFER_FIELDS: &[&str] = &[
        "hash_algorithm",
        "hash_depth",
        "hash_root",
        "pre_image",
        "signature_proof",
    ];

    #[derive(nimiq_serde::Deserialize)]
    #[serde(field_identifier, rename_all = "lowercase")]
    enum AnyHashField {
        Algorithm,
        Hash,
    }

    struct PreImageVisitor;
    struct AnyHashVisitor;
    struct PoWRegularTransferVisitor;

    impl Serialize for AnyHash {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let human_readable = serializer.is_human_readable();
            let mut state = serializer.serialize_struct("AnyHash", ANYHASH_FIELDS.len())?;
            match self {
                AnyHash::Blake2b(hash) => {
                    if human_readable {
                        state.serialize_field(ANYHASH_FIELDS[0], &"blake2b")?;
                    } else {
                        state.serialize_field(ANYHASH_FIELDS[0], &1u8)?;
                    }
                    state.serialize_field(ANYHASH_FIELDS[1], hash)?;
                }
                AnyHash::Sha256(hash) => {
                    if human_readable {
                        state.serialize_field(ANYHASH_FIELDS[0], &"sha256")?;
                    } else {
                        state.serialize_field(ANYHASH_FIELDS[0], &3u8)?;
                    }
                    state.serialize_field(ANYHASH_FIELDS[1], hash)?;
                }
                AnyHash::Sha512(hash) => {
                    if human_readable {
                        state.serialize_field(ANYHASH_FIELDS[0], &"sha512")?;
                    } else {
                        state.serialize_field(ANYHASH_FIELDS[0], &4u8)?;
                    }
                    state.serialize_field(ANYHASH_FIELDS[1], hash)?;
                }
            }
            state.end()
        }
    }

    impl<'de> Visitor<'de> for AnyHashVisitor {
        type Value = AnyHash;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("enum AnyHash")
        }

        /// If the deserializer is not human friendly most likely will use `visit_seq` for deserializing
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let algorithm: u8 = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(0, &self))?;
            match algorithm {
                1u8 => {
                    let hash: AnyHash32 = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                    Ok(AnyHash::Blake2b(hash))
                }
                3u8 => {
                    let hash: AnyHash32 = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                    Ok(AnyHash::Sha256(hash))
                }
                4u8 => {
                    let hash: AnyHash64 = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                    Ok(AnyHash::Sha512(hash))
                }
                _ => Err(A::Error::invalid_value(
                    serde::de::Unexpected::Unsigned(algorithm as u64),
                    &"an AnyHash variant",
                )),
            }
        }

        /// If the deserializer is human friendly most likely will use `visit_map` for deserializing
        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut algorithm = None;
            let mut hash = None;
            while let Some(key) = map.next_key()? {
                match key {
                    AnyHashField::Algorithm => {
                        if algorithm.is_some() {
                            return Err(A::Error::duplicate_field("algorithm"));
                        }
                        algorithm = Some(map.next_value()?);
                    }
                    AnyHashField::Hash => {
                        if hash.is_some() {
                            return Err(A::Error::duplicate_field("hash"));
                        }
                        hash = Some(map.next_value()?);
                    }
                }
            }
            let hash: String = hash.ok_or_else(|| A::Error::missing_field("hash"))?;
            let algorithm: String =
                algorithm.ok_or_else(|| A::Error::missing_field("algorithm"))?;
            match algorithm.as_str() {
                "blake2b" => {
                    let hash = AnyHash32::from_str(hash.as_str()).map_err(A::Error::custom)?;
                    Ok(AnyHash::Blake2b(hash))
                }
                "sha256" => {
                    let hash = AnyHash32::from_str(hash.as_str()).map_err(A::Error::custom)?;
                    Ok(AnyHash::Sha256(hash))
                }
                "sha512" => {
                    let hash = AnyHash64::from_str(hash.as_str()).map_err(A::Error::custom)?;
                    Ok(AnyHash::Sha512(hash))
                }
                _ => Err(A::Error::invalid_value(
                    serde::de::Unexpected::Str(algorithm.as_str()),
                    &"an AnyHash variant",
                )),
            }
        }
    }

    impl<'de> Deserialize<'de> for AnyHash {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            // Here we are not making any distinction between a human readable deserializer and a non human readable ones
            // because regularly the first one uses `visit_map` while the latter uses `visit_seq` instead.
            deserializer.deserialize_struct("AnyHash", ANYHASH_FIELDS, AnyHashVisitor)
        }
    }

    impl Serialize for PreImage {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            if serializer.is_human_readable() {
                match self {
                    PreImage::PreImage32(hash) => Serialize::serialize(hash, serializer),
                    PreImage::PreImage64(hash) => Serialize::serialize(hash, serializer),
                }
            } else {
                let mut state = serializer.serialize_struct("PreImage", PREIMAGE_FIELDS.len())?;
                match self {
                    PreImage::PreImage32(pre_image) => {
                        state.serialize_field(PREIMAGE_FIELDS[0], &32u8)?;
                        state.serialize_field(PREIMAGE_FIELDS[1], pre_image)?;
                    }
                    PreImage::PreImage64(pre_image) => {
                        state.serialize_field(PREIMAGE_FIELDS[0], &64u8)?;
                        state.serialize_field(PREIMAGE_FIELDS[1], pre_image)?;
                    }
                }
                state.end()
            }
        }
    }

    impl<'de> Visitor<'de> for PreImageVisitor {
        type Value = PreImage;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("enum PreImage")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let pre_image_type: u8 = seq
                .next_element()?
                .ok_or_else(|| A::Error::invalid_length(0, &self))?;
            match pre_image_type {
                32u8 => {
                    let pre_image: AnyHash32 = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                    Ok(PreImage::PreImage32(pre_image))
                }
                64u8 => {
                    let pre_image: AnyHash64 = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                    Ok(PreImage::PreImage64(pre_image))
                }
                _ => Err(A::Error::invalid_value(
                    serde::de::Unexpected::Unsigned(pre_image_type as u64),
                    &"a PreImage variant",
                )),
            }
        }
    }

    impl<'de> Deserialize<'de> for PreImage {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            if deserializer.is_human_readable() {
                let data: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
                data.parse().map_err(Error::custom)
            } else {
                deserializer.deserialize_struct("PreImage", PREIMAGE_FIELDS, PreImageVisitor)
            }
        }
    }

    impl<'de> Visitor<'de> for PoWRegularTransferVisitor {
        type Value = PoWRegularTransfer;

        fn expecting(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
            write!(f, "a PoWRegularTransfer")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<PoWRegularTransfer, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let hash_algorithm: u8 = seq
                .next_element()?
                .ok_or_else(|| Error::invalid_length(0, &self))?;

            let hash_depth: u8 = seq
                .next_element()?
                .ok_or_else(|| Error::invalid_length(1, &self))?;

            let hash_root = match hash_algorithm {
                1u8 => {
                    let hash_root: AnyHash32 = seq
                        .next_element()?
                        .ok_or_else(|| Error::invalid_length(2, &self))?;
                    AnyHash::Blake2b(hash_root)
                }
                3u8 => {
                    let hash_root: AnyHash32 = seq
                        .next_element()?
                        .ok_or_else(|| Error::invalid_length(2, &self))?;
                    AnyHash::Sha256(hash_root)
                }
                4u8 => {
                    let hash_root: AnyHash64 = seq
                        .next_element()?
                        .ok_or_else(|| Error::invalid_length(2, &self))?;

                    AnyHash::Sha512(hash_root)
                }
                _ => {
                    return Err(Error::custom(format!(
                        "Invalid hash algorithm type: {}",
                        hash_algorithm
                    )))
                }
            };

            let pre_image = match hash_root {
                AnyHash::Blake2b(_) | AnyHash::Sha256(_) => {
                    let pre_image: AnyHash32 = seq
                        .next_element()?
                        .ok_or_else(|| Error::invalid_length(3, &self))?;
                    PreImage::PreImage32(pre_image)
                }
                AnyHash::Sha512(_) => {
                    let pre_image: AnyHash64 = seq
                        .next_element()?
                        .ok_or_else(|| Error::invalid_length(3, &self))?;
                    PreImage::PreImage64(pre_image)
                }
            };

            let signature_proof: PoWSignatureProof = seq
                .next_element()?
                .ok_or_else(|| Error::invalid_length(4, &self))?;

            Ok(PoWRegularTransfer {
                hash_depth,
                hash_root,
                pre_image,
                signature_proof,
            })
        }
    }

    impl<'de> Deserialize<'de> for PoWRegularTransfer {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_struct(
                "PoWRegularTransfer",
                POW_REGULAR_TRANSFER_FIELDS,
                PoWRegularTransferVisitor,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use nimiq_serde::{Deserialize, Serialize};
    use nimiq_test_log::test;

    use super::{AnyHash, AnyHash32, AnyHash64, PoWOutgoingHTLCTransactionProof, PreImage};

    fn sample_anyhashes() -> [AnyHash; 3] {
        let hash_32 = AnyHash32([0xC; AnyHash32::SIZE]);
        let hash_64 = AnyHash64([0xC; AnyHash64::SIZE]);
        [
            AnyHash::Sha256(hash_32.clone()),
            AnyHash::Blake2b(hash_32),
            AnyHash::Sha512(hash_64),
        ]
    }

    fn sample_preimages() -> [PreImage; 2] {
        let hash_32 = AnyHash32([0xC; AnyHash32::SIZE]);
        let hash_64 = AnyHash64([0xC; AnyHash64::SIZE]);
        [PreImage::PreImage32(hash_32), PreImage::PreImage64(hash_64)]
    }

    #[test]
    fn it_can_correctly_serialize_anyhash() {
        let hashes = sample_anyhashes();
        let bin = hashes[0].serialize_to_vec();
        assert_eq!(
            hex::decode("030C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C")
                .unwrap(),
            bin
        );
        let bin = hashes[1].serialize_to_vec();
        assert_eq!(
            hex::decode("010C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C")
                .unwrap(),
            bin
        );
        let bin = hashes[2].serialize_to_vec();
        assert_eq!(hex::decode("040C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C").unwrap(), bin);
    }

    #[test]
    fn it_can_correctly_deserialize_anyhash() {
        let hashes = sample_anyhashes();
        let bin = hex::decode("030C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C")
            .unwrap();
        let set = AnyHash::deserialize_from_vec(&bin).unwrap();
        assert_eq!(hashes[0], set);
        let bin = hex::decode("010C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C")
            .unwrap();
        let set = AnyHash::deserialize_from_vec(&bin).unwrap();
        assert_eq!(hashes[1], set);
        let bin = hex::decode("040C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C").unwrap();
        let set = AnyHash::deserialize_from_vec(&bin).unwrap();
        assert_eq!(hashes[2], set);
    }

    #[test]
    fn it_can_correctly_serialize_anyhash_human_readably() {
        let hashes = sample_anyhashes();
        assert_eq!(
            serde_json::to_string(&hashes[0]).unwrap(),
            r#"{"algorithm":"sha256","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#,
        );
        assert_eq!(
            serde_json::to_string(&hashes[1]).unwrap(),
            r#"{"algorithm":"blake2b","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#,
        );
        assert_eq!(
            serde_json::to_string(&hashes[2]).unwrap(),
            r#"{"algorithm":"sha512","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#,
        );
    }

    #[test]
    fn it_can_correctly_deserialize_anyhash_human_readably() {
        let hashes = sample_anyhashes();
        assert_eq!(
            serde_json::from_str::<AnyHash>(r#"{"algorithm":"sha256","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#,)
                .unwrap(),
            hashes[0],
        );
        assert_eq!(
            serde_json::from_str::<AnyHash>(r#"{"algorithm":"blake2b","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#)
                .unwrap(),
            hashes[1],
        );
        assert_eq!(
            serde_json::from_str::<AnyHash>(r#"{"algorithm":"sha512","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#)
                .unwrap(),
            hashes[2],
        );
    }

    #[test]
    fn it_can_error_on_human_readable_anyhash_deserialization() {
        assert!(serde_json::from_str::<AnyHash>(
            "0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"
        )
        .is_err()); // invalid type
        assert!(serde_json::from_str::<AnyHash>(r#"sha256"#).is_err()); // invalid type
        assert!(serde_json::from_str::<AnyHash>(r#"{"algorithm":"baKe2b","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#).is_err()); // invalid algorithm
        assert!(serde_json::from_str::<AnyHash>(r#"{"algorithm":"blake2b","algorithm":"blake2b","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#).is_err()); // duplicate algorithm
        assert!(serde_json::from_str::<AnyHash>(r#"{"algorithm":"sha256","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#).is_err());
        // too small hash for 32 byte hash
        assert!(serde_json::from_str::<AnyHash>(r#"{"algorithm":"sha256","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#).is_err());
        // too large hash for 32 byte hash
        assert!(serde_json::from_str::<AnyHash>(r#"{"algorithm":"sha512","hash":"0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"}"#).is_err());
        // too large hash for 64 byte hash
    }

    #[test]
    fn it_can_correctly_serialize_preimage() {
        let hashes = sample_preimages();
        let bin = hashes[0].serialize_to_vec();
        assert_eq!(
            hex::decode("200C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C")
                .unwrap(),
            bin
        );
        let bin = hashes[1].serialize_to_vec();
        assert_eq!(hex::decode("400C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C").unwrap(), bin);
    }

    #[test]
    fn it_can_correctly_deserialize_preimage() {
        let hashes = sample_preimages();
        let bin = hex::decode("200C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C")
            .unwrap();
        let set = PreImage::deserialize_from_vec(&bin).unwrap();
        assert_eq!(hashes[0], set);
        let bin = hex::decode("400C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C0C").unwrap();
        let set = PreImage::deserialize_from_vec(&bin).unwrap();
        assert_eq!(hashes[1], set);
    }

    #[test]
    fn it_can_correctly_serialize_preimage_human_readably() {
        let hashes = sample_preimages();
        assert_eq!(
            serde_json::to_string(&hashes[0]).unwrap(),
            r#""0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c""#,
        );
        assert_eq!(
            serde_json::to_string(&hashes[1]).unwrap(),
            r#""0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c""#,
        );
    }

    #[test]
    fn it_can_correctly_deserialize_preimage_human_readably() {
        let hashes = sample_preimages();
        assert_eq!(
            serde_json::from_str::<PreImage>(
                r#""0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c""#,
            )
            .unwrap(),
            hashes[0],
        );
        assert_eq!(
            serde_json::from_str::<PreImage>(r#""0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c""#)
                .unwrap(),
            hashes[1],
        );
    }

    #[test]
    fn it_can_error_on_human_readable_preimage_deserialization() {
        assert!(serde_json::from_str::<PreImage>("[]").is_err()); // invalid type
        assert!(serde_json::from_str::<PreImage>(r#""123""#).is_err()); // too short
        assert!(serde_json::from_str::<PreImage>(
            r#""0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c""#
        )
        .is_err()); // too large for 32 byte and too small for 64 byte
        assert!(serde_json::from_str::<PreImage>(
            r#""0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c""#
        )
        .is_err()); // too large for 64 byte
    }

    #[test]
    fn it_can_correctly_deserialize_pow_outgoing_htlc_transaction_proof() {
        let bin = hex::decode("0103013913543fa4e5b6c41176ee552d314db28d786bd87f103ee25f49f4e2555e51d1bff5b88ef94cd7c2ba354a8e4b50fef063ab1659646570b34effbb48f36ecb4c08600ec9f0d44dc8d43275c705d7780caa31497d2620da4d7838d10574a6dfa100410b82decb73b7c6f4047b4fb504000c364edd9a3337e5194b60f896d31904ccab8bf310cf808fd98a9b3b13096b6701d53bbba8402465d08cb99948c8407500")
            .unwrap();
        let _ = PoWOutgoingHTLCTransactionProof::deserialize_from_vec(&bin).unwrap();
    }
}
