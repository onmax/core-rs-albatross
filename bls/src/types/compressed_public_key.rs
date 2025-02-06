use std::{
    cmp::Ordering,
    fmt,
    io::{Error, ErrorKind},
};

use ark_ec::AffineRepr;
use ark_mnt6_753::G2Affine;
use ark_serialize::CanonicalDeserialize;

use crate::PublicKey;

/// The serialized compressed form of a public key.
/// This form consists of the x-coordinate of the point (in the affine form),
/// one bit indicating the sign of the y-coordinate
/// and one bit indicating if it is the "point-at-infinity".
#[derive(Clone)]
#[cfg_attr(
    feature = "serde-derive",
    derive(
        nimiq_hash_derive::SerializeContent,
        nimiq_serde::Deserialize,
        nimiq_serde::Serialize,
        nimiq_serde::SerializedMaxSize,
    )
)]
#[cfg_attr(feature = "serde-derive", serde(transparent))]
pub struct CompressedPublicKey {
    #[cfg_attr(feature = "serde-derive", serde(with = "nimiq_serde::HexArray"))]
    pub public_key: [u8; CompressedPublicKey::SIZE],
}

impl CompressedPublicKey {
    pub const SIZE: usize = 285;

    /// Transforms the compressed form back into the projective form.
    pub fn uncompress(&self) -> Result<PublicKey, Error> {
        log::info!(
            compressed = &self.to_hex()[..16],
            "decompressing BLS public key",
        );
        let affine_point: G2Affine =
            CanonicalDeserialize::deserialize_compressed(&mut &self.public_key[..])
                .map_err(|e| Error::new(ErrorKind::Other, e))?;
        Ok(PublicKey {
            public_key: affine_point.into_group(),
        })
    }

    /// Formats the compressed form into a hexadecimal string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_ref())
    }
}

impl Eq for CompressedPublicKey {}

impl PartialEq for CompressedPublicKey {
    fn eq(&self, other: &CompressedPublicKey) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Ord for CompressedPublicKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl PartialOrd<CompressedPublicKey> for CompressedPublicKey {
    fn partial_cmp(&self, other: &CompressedPublicKey) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl AsRef<[u8]> for CompressedPublicKey {
    fn as_ref(&self) -> &[u8] {
        self.public_key.as_ref()
    }
}

impl fmt::Display for CompressedPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", &self.to_hex())
    }
}

impl fmt::Debug for CompressedPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "CompressedPublicKey({})", &self.to_hex())
    }
}

impl std::hash::Hash for CompressedPublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.public_key.to_vec(), state);
    }
}

#[cfg(feature = "serde-derive")]
mod serde_derive {
    // TODO: Replace this with a generic serialization using `ToHex` and `FromHex`.

    use std::str::FromStr;

    use nimiq_serde::Deserialize;

    use super::CompressedPublicKey;
    use crate::ParseError;

    impl FromStr for CompressedPublicKey {
        type Err = ParseError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let raw = hex::decode(s)?;
            if raw.len() != CompressedPublicKey::SIZE {
                return Err(ParseError::IncorrectLength(raw.len()));
            }
            CompressedPublicKey::deserialize_from_vec(&raw)
                .map_err(|_| ParseError::SerializationError)
        }
    }
}

impl Default for CompressedPublicKey {
    fn default() -> Self {
        CompressedPublicKey {
            public_key: [0u8; CompressedPublicKey::SIZE],
        }
    }
}
