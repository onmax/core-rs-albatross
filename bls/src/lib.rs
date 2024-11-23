pub use lazy::LazyPublicKey;
use nimiq_hash::Blake2sHash;
pub use types::*;

// Implements the LazyPublicKey type. Which is a faster, cached version of PublicKey.
mod lazy;

// Implements all of the types needed to do BLS signatures.
mod types;

// Specifies the hash algorithm used for signatures
pub type SigHash = Blake2sHash;

// Implements the tagged-signing traits
#[cfg(feature = "serde-derive")]
mod tagged_signing;
