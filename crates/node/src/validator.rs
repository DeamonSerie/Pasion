use pasion_types::{Address, Result};
use pasion_ledger::{Block, BlockRelevantState};
use pasion_consensus::Attestation;

pub struct ValidatorNode {
    pub address: Address,
    pub validated_count: u64,
}

impl ValidatorNode {
    pub fn new(address: Address) -> Self {
        ValidatorNode { address, validated_count: 0 }
    }
    
    pub fn validate_block_relevant(&mut self, block: &Block, _relevant_state: &BlockRelevantState) -> Result<bool> {
        if block.transactions.is_empty() {
            self.validated_count += 1;
            return Ok(true);
        }
        for _tx in &block.transactions {}
        self.validated_count += 1;
        Ok(true)
    }
    
    pub fn attest(&self, block: &Block, valid: bool) -> Attestation {
        let sig_data = [self.address.0.as_slice(), block.state_root.0.as_slice(), block.bundle_id.0.as_slice()].concat();
        Attestation {
            validator: self.address,
            bundle_id: block.bundle_id,
            block_hash: block.state_root,
            valid,
            signature: blake3::hash(&sig_data).as_bytes().to_vec(),
        }
    }
}
