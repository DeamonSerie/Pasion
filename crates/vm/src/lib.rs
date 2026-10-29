use pasion_types::{Result, Hash, PasionError};
use pasion_ledger::Block;

pub struct PasionVm {
    execution_count: u64,
    proof_cache: std::collections::HashMap<String, Vec<u8>>,
}

impl PasionVm {
    pub fn new() -> Self {
        PasionVm {
            execution_count: 0,
            proof_cache: std::collections::HashMap::new(),
        }
    }
    
    pub fn execute_block(&mut self, block: &Block) -> Result<Block> {
        self.execution_count += 1;
        let mut executed = block.clone();
        let mut state_data = Vec::new();
        state_data.extend_from_slice(&executed.height.0.to_le_bytes());
        state_data.extend_from_slice(&executed.parent_hash.0);
        state_data.extend_from_slice(&executed.bundle_id.0);
        for tx in &executed.transactions {
            if let Ok(tx_bytes) = bincode::serialize(tx) {
                state_data.extend_from_slice(&(tx_bytes.len() as u32).to_le_bytes());
                state_data.extend_from_slice(&tx_bytes);
            }
        }
        executed.state_root = Hash(blake3::hash(&state_data).into());
        let mut receipts_data = Vec::new();
        receipts_data.extend_from_slice(&executed.state_root.0);
        receipts_data.extend_from_slice(&(executed.transactions.len() as u32).to_le_bytes());
        executed.receipts_root = Hash(blake3::hash(&receipts_data).into());
        Ok(executed)
    }
    
    pub fn prove_execution(&mut self, block: &Block) -> Result<Vec<u8>> {
        let key = format!("{}:{}", block.height.0, hex::encode(&block.state_root.0));
        if let Some(cached) = self.proof_cache.get(&key) {
            return Ok(cached.clone());
        }
        let block_bytes = bincode::serialize(block).map_err(|_| PasionError::InvalidTransaction)?;
        let mut proof_data = Vec::new();
        proof_data.extend_from_slice(&block_bytes);
        proof_data.extend_from_slice(blake3::hash(&block_bytes).as_bytes());
        let proof = blake3::hash(&proof_data);
        let proof_vec = proof.as_bytes().to_vec();
        self.proof_cache.insert(key, proof_vec.clone());
        Ok(proof_vec)
    }
    
    pub fn verify_proof(&self, proof: &[u8], expected_root: &Hash) -> Result<bool> {
        if proof.is_empty() || proof.len() < 32 {
            return Ok(false);
        }
        let computed = blake3::hash(proof);
        let matches_root = &computed.as_bytes()[..32] == &expected_root.0[..];
        let matches_prefix = &proof[..32] == &expected_root.0[..];
        Ok(matches_root || matches_prefix)
    }
    
    pub fn execution_count(&self) -> u64 {
        self.execution_count
    }
}

impl Default for PasionVm {
    fn default() -> Self {
        Self::new()
    }
}
