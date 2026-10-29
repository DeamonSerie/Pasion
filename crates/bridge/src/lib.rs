use serde::{Deserialize, Serialize};
use pasion_types::{Address, Amount, AssetId, Hash};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTransfer {
    pub id: Hash,
    pub from_chain: String,
    pub to_chain: String,
    pub asset_id: AssetId,
    pub amount: Amount,
    pub recipient: Address,
    pub signatures: Vec<Vec<u8>>,
    pub confirmed: bool,
    pub nonce: u64,
}

#[derive(Debug, Clone)]
pub struct FederatedBridge {
    pub validators: Vec<Address>,
    pub threshold: usize,
    pub transfers: HashMap<Hash, BridgeTransfer>,
    pub processed_nonces: HashMap<(Address, u64), bool>,
}

impl FederatedBridge {
    pub fn new(validators: Vec<Address>) -> Self {
        let threshold = if validators.is_empty() { 0 } else { (validators.len() * 2) / 3 + 1 };
        FederatedBridge {
            validators,
            threshold,
            transfers: HashMap::new(),
            processed_nonces: HashMap::new(),
        }
    }
    
    pub fn submit_deposit(&mut self, asset_id: AssetId, amount: Amount, recipient: Address, proof: Vec<u8>) -> Hash {
        let transfer_id = Hash(blake3::hash(&[recipient.0.as_slice(), &amount.0.to_le_bytes()].concat()).into());
        let transfer = BridgeTransfer {
            id: transfer_id,
            from_chain: "external".to_string(),
            to_chain: "pasion".to_string(),
            asset_id,
            amount,
            recipient,
            signatures: vec![proof],
            confirmed: false,
            nonce: self.transfers.len() as u64,
        };
        self.transfers.insert(transfer_id, transfer);
        transfer_id
    }
    
    pub fn confirm_transfer(&mut self, transfer_id: Hash, signatures: Vec<Vec<u8>>) -> bool {
        if let Some(transfer) = self.transfers.get_mut(&transfer_id) {
            if signatures.len() >= self.threshold {
                transfer.signatures.extend(signatures);
                transfer.confirmed = true;
                return true;
            }
        }
        false
    }
}
