use std::collections::HashMap;

use nimiq_bls::{CompressedPublicKey, KeyPair as BlsKeyPair};

pub struct VotingKeys {
    keys: HashMap<CompressedPublicKey, BlsKeyPair>,
    current_key: BlsKeyPair,
}

impl VotingKeys {
    pub fn new(keys: Vec<BlsKeyPair>) -> Self {
        assert!(!keys.is_empty());
        let mut key_hm = HashMap::new();
        for key in &keys {
            key_hm.insert(key.public_key.compress(), key.clone());
        }
        VotingKeys {
            keys: key_hm,
            current_key: keys.first().unwrap().clone(),
        }
    }

    pub fn add_key(&mut self, key: BlsKeyPair) {
        self.keys.insert(key.public_key.compress(), key);
    }

    pub fn get_current_key(&self) -> BlsKeyPair {
        self.current_key.clone()
    }

    pub fn get_keys(&self) -> Vec<BlsKeyPair> {
        self.keys.values().cloned().collect()
    }

    #[allow(clippy::result_unit_err)]
    pub fn update_current_key(&mut self, public_key: &CompressedPublicKey) -> Result<(), ()> {
        self.current_key = self.keys.get(public_key).ok_or(())?.clone();
        Ok(())
    }
}
