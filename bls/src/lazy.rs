use std::{cmp::Ordering, fmt, hash::Hasher, sync::OnceLock};

use nimiq_hash::Hash;
use nimiq_utils::interner::{Interned, Interner};

use crate::{CompressedPublicKey, PublicKey, SigHash, Signature};

fn cache() -> &'static Interner<CompressedPublicKey, OnceLock<Option<PublicKey>>> {
    static CACHE: OnceLock<Interner<CompressedPublicKey, OnceLock<Option<PublicKey>>>> =
        OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// A reference to an interned, lazily uncompressed BLS public key.
///
/// Since this is just a reference, it's small and cloning is cheap. The
/// interning makes sure that each compressed public key is uncompressed at most
/// once as long as at least one reference to it remains.
#[derive(Clone)]
pub struct LazyPublicKey(Interned<CompressedPublicKey, OnceLock<Option<PublicKey>>>);

impl fmt::Debug for LazyPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("LazyPublicKey")
            .field(self.compressed())
            .finish()
    }
}

impl fmt::Display for LazyPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.compressed(), f)
    }
}

impl PartialEq for LazyPublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.compressed().eq(other.compressed())
    }
}

impl Eq for LazyPublicKey {}

impl std::hash::Hash for LazyPublicKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(self.compressed(), state)
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for LazyPublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.compressed().partial_cmp(other.compressed())
    }
}

impl Ord for LazyPublicKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.compressed().cmp(other.compressed())
    }
}

impl LazyPublicKey {
    pub fn from_compressed(compressed: &CompressedPublicKey) -> LazyPublicKey {
        LazyPublicKey(cache().intern_with(compressed, OnceLock::new))
    }

    pub fn uncompress(&self) -> Option<&PublicKey> {
        self.0
            .value()
            .get_or_init(|| self.compressed().uncompress().ok())
            .as_ref()
    }

    pub fn compressed(&self) -> &CompressedPublicKey {
        self.0.key()
    }

    pub fn has_uncompressed(&self) -> bool {
        self.0.value().get().is_some()
    }

    pub fn verify<M: Hash>(&self, msg: &M, signature: &Signature) -> bool {
        if let Some(public_key) = self.uncompress() {
            return public_key.verify(msg, signature);
        }
        false
    }

    pub fn verify_hash(&self, hash: SigHash, signature: &Signature) -> bool {
        if let Some(public_key) = self.uncompress() {
            return public_key.verify_hash(hash, signature);
        }
        false
    }
}

impl From<PublicKey> for LazyPublicKey {
    fn from(key: PublicKey) -> LazyPublicKey {
        let result = LazyPublicKey(cache().intern(&key.compress(), OnceLock::from(Some(key))));
        // TODO: This might block while another thread computes the decompression. :/
        //
        // Needs an addition in the Rust standard library to fix.
        let _ = result.0.value().set(Some(key));
        result
    }
}

impl From<CompressedPublicKey> for LazyPublicKey {
    fn from(compressed: CompressedPublicKey) -> Self {
        LazyPublicKey::from_compressed(&compressed)
    }
}

impl From<LazyPublicKey> for CompressedPublicKey {
    fn from(key: LazyPublicKey) -> Self {
        key.compressed().clone()
    }
}

#[cfg(feature = "serde-derive")]
mod serialization {
    use nimiq_serde::SerializedSize;
    use serde::{Deserialize, Serialize};

    use super::*;

    impl SerializedSize for LazyPublicKey {
        const SIZE: usize = CompressedPublicKey::SIZE;
    }

    impl Serialize for LazyPublicKey {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Serialize::serialize(&self.compressed(), serializer)
        }
    }

    impl<'de> Deserialize<'de> for LazyPublicKey {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let compressed = CompressedPublicKey::deserialize(deserializer)?;
            Ok(LazyPublicKey::from_compressed(&compressed))
        }
    }
}
