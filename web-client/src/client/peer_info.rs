use nimiq_network_interface::peer_info::{NodeType, Services};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// Information about a networking peer.
#[derive(serde::Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct PlainPeerInfo {
    /// A libp2p peer ID
    pub peer_id: String,
    /// Address of the peer in `Multiaddr` format
    address: String,
    /// Node type of the peer
    #[tsify(type = "'full' | 'history' | 'light'")]
    #[serde(rename = "type")]
    node_type: String,
    /// List of services the peer is providing
    services: Vec<PlainService>,
}

/// Available peer service flags
#[derive(serde::Serialize, Tsify)]
#[serde(rename_all = "kebab-case")]
pub enum PlainService {
    FullBlocks,
    History,
    AccountsProof,
    AccountsChunk,
    Mempool,
    TransactionIndex,
    Validator,
    PreGenesisTransactions,

    // Catch-all to not have to panic when new services are added
    Unknown,
}

impl From<Services> for PlainService {
    fn from(services: Services) -> Self {
        match services {
            Services::FULL_BLOCKS => Self::FullBlocks,
            Services::HISTORY => Self::History,
            Services::ACCOUNTS_PROOF => Self::AccountsProof,
            Services::ACCOUNTS_CHUNKS => Self::AccountsChunk,
            Services::MEMPOOL => Self::Mempool,
            Services::TRANSACTION_INDEX => Self::TransactionIndex,
            Services::VALIDATOR => Self::Validator,
            Services::PRE_GENESIS_TRANSACTIONS => Self::PreGenesisTransactions,
            _ => Self::Unknown,
        }
    }
}

impl PlainPeerInfo {
    pub fn from(peer_id: String, peer_info: nimiq_network_interface::peer_info::PeerInfo) -> Self {
        let node_type = if peer_info
            .get_services()
            .contains(Services::provided(NodeType::History))
        {
            "history"
        } else if peer_info
            .get_services()
            .contains(Services::provided(NodeType::Full))
        {
            "full"
        } else {
            "light"
        };

        Self {
            peer_id,
            address: peer_info.get_address().to_string(),
            node_type: node_type.to_string(),
            services: peer_info
                .get_services()
                .into_iter()
                .map(|s| s.into())
                .collect(),
        }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "PlainPeerInfo[]")]
    pub type PlainPeerInfoArrayType;
}
