use nimiq_primitives::coin::Coin;
use nimiq_transaction::account::vesting_contract::CreationTransactionData;
use wasm_bindgen::prelude::*;

#[cfg(feature = "primitives")]
use crate::common::transaction::{PlainTransactionProofType, PlainTransactionRecipientDataType};
use crate::common::{
    signature_proof::SignatureProof,
    transaction::{PlainTransactionProof, PlainTransactionRecipientData, PlainVestingData},
};

/// Utility class providing methods to parse Vesting Contract transaction data and proofs.
#[wasm_bindgen]
pub struct VestingContract;

#[cfg(feature = "primitives")]
#[wasm_bindgen]
impl VestingContract {
    /// Parses the data of a Vesting Contract creation transaction into a plain object.
    #[wasm_bindgen(js_name = dataToPlain)]
    pub fn data_to_plain(
        data: &[u8],
        tx_value: u64,
    ) -> Result<PlainTransactionRecipientDataType, JsError> {
        let plain = VestingContract::parse_data(data, tx_value)?;
        Ok(serde_wasm_bindgen::to_value(&plain)?.into())
    }

    /// Parses the proof of a Vesting Contract claiming transaction into a plain object.
    #[wasm_bindgen(js_name = proofToPlain)]
    pub fn proof_to_plain(proof: &[u8]) -> Result<PlainTransactionProofType, JsError> {
        let plain = VestingContract::parse_proof(proof)?;
        Ok(serde_wasm_bindgen::to_value(&plain)?.into())
    }
}

impl VestingContract {
    pub fn parse_data(
        bytes: &[u8],
        tx_value: u64,
    ) -> Result<PlainTransactionRecipientData, JsError> {
        let data = CreationTransactionData::parse_data(bytes, Coin::try_from(tx_value)?)?;

        Ok(PlainTransactionRecipientData::Vesting(PlainVestingData {
            raw: hex::encode(bytes),
            owner: data.owner.to_user_friendly_address(),
            start_time: data.start_time,
            step_amount: data.step_amount.into(),
            time_step: data.time_step,
        }))
    }

    pub fn parse_proof(bytes: &[u8]) -> Result<PlainTransactionProof, JsError> {
        let proof = SignatureProof::deserialize(bytes)?;
        Ok(proof.to_plain_transaction_proof())
    }
}
