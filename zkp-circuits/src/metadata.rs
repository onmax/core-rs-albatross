use std::{fs::File, io, path::Path};

use nimiq_hash::Blake2bHash;
use nimiq_primitives::{networks::NetworkId, policy::Policy};
use nimiq_serde::{Deserialize, Serialize};

/// This data structure holds metadata about the verifying keys.
/// It can be used to check whether verifying keys are still up to date.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct VerifyingKeyMetadata {
    genesis_hash: Blake2bHash,
    #[serde(with = "nimiq_serde::fixint::be")]
    blocks_per_epoch: u32,
}

impl VerifyingKeyMetadata {
    pub fn new(genesis_hash: Blake2bHash) -> Self {
        Self {
            genesis_hash,
            blocks_per_epoch: Policy::blocks_per_epoch(),
        }
    }

    pub fn matches(&self, _network_id: NetworkId) -> bool {
        // We store the genesis block hash for future reference.
        // However, our circuits currently are generic over the genesis block,
        // which is why we exclude it from the check.
        self.blocks_per_epoch == Policy::blocks_per_epoch()
    }

    pub fn save_to_file(self, path: &Path) -> Result<(), io::Error> {
        let mut file = File::create(path.join("meta_data.bin"))?;
        self.serialize_to_writer(&mut file)?;
        file.sync_all()?;

        Ok(())
    }
}
