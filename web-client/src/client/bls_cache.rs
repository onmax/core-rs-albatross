use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress};
use idb::{Database, Error, KeyPath, ObjectStore, TransactionMode};
use nimiq_bls::{G2Projective, LazyPublicKey, PublicKey};
use nimiq_serde::{Deserialize, Serialize};

/// Caches decompressed BlsPublicKeys in an IndexedDB
pub(crate) struct BlsCache {
    db: Option<Database>,
    keys: Vec<LazyPublicKey>,
}

#[derive(Deserialize, Serialize)]
struct BlsKeyEntry {
    #[serde(with = "serde_bytes")]
    public_key: Vec<u8>,
}

const BLS_KEYS: &str = "bls_keys";
const PUBLIC_KEY: &str = "public_key";

impl BlsCache {
    pub async fn new() -> Self {
        let db = match Database::builder("nimiq_client_cache")
            .version(1)
            .add_object_store(
                ObjectStore::builder(BLS_KEYS).key_path(Some(KeyPath::new_single(PUBLIC_KEY))),
            )
            .build()
            .await
        {
            Ok(db) => Some(db),
            Err(err) => {
                log::warn!("idb: Couldn't create database {}", err);
                None
            }
        };

        BlsCache { db, keys: vec![] }
    }

    /// Add the given keys into IndexedDB.
    ///
    /// The given keys must be correctly decompressed already, otherwise this
    /// function will panic.
    pub async fn add_keys(&self, keys: Vec<LazyPublicKey>) -> Result<(), Error> {
        if let Some(db) = &self.db {
            let transaction = db.transaction(&[BLS_KEYS], TransactionMode::ReadWrite)?;
            let bls_keys_store = transaction.object_store(BLS_KEYS)?;

            for key in keys {
                let mut public_key = Vec::new();
                assert!(key.has_uncompressed());
                key.uncompress()
                    .expect("must not pass invalid keys to `BlsCache::add_keys`")
                    .public_key
                    .serialize_with_mode(&mut public_key, Compress::No)
                    .unwrap();

                let entry = BlsKeyEntry { public_key };
                let entry_js_value = serde_wasm_bindgen::to_value(&entry).unwrap();
                bls_keys_store.put(&entry_js_value, None)?.await?;
            }
        }
        Ok(())
    }

    /// Fetches all bls keys from the IndexedDB and stores them, which makes
    /// the decompressed keys available in other places.
    pub async fn init(&mut self) -> Result<(), Error> {
        if let Some(db) = &self.db {
            let transaction = db.transaction(&[BLS_KEYS], TransactionMode::ReadOnly)?;
            let bls_keys_store = transaction.object_store(BLS_KEYS)?;

            let js_keys = bls_keys_store.get_all(None, None)?.await?;

            for js_key in &js_keys {
                let value: BlsKeyEntry = serde_wasm_bindgen::from_value(js_key.clone()).unwrap();
                let public_key = PublicKey::new(
                    G2Projective::deserialize_uncompressed_unchecked(&*value.public_key).unwrap(),
                );
                self.keys.push(LazyPublicKey::from(public_key));
            }
            transaction.await?;
        }
        Ok(())
    }
}
