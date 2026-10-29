use pasion_types::{Address, Result, BlockHeight, Hash, BundleId, Amount};
use pasion_ledger::{Block, Transaction};
use pasion_consensus::Attestation;

pub struct BundlerNode {
    pub address: Address,
    pub bundled_count: u64,
}

impl BundlerNode {
    pub fn new(address: Address) -> Self {
        BundlerNode { address, bundled_count: 0 }
    }
    
    pub fn bundle_transactions(&mut self, txs: Vec<Transaction>, height: BlockHeight, parent_hash: Hash) -> Result<Block> {
        let bundle_id = BundleId(blake3::hash(&[self.address.0.as_slice(), &height.0.to_le_bytes()].concat()).into());
        self.bundled_count += 1;
        let mut state_data = Vec::new();
        state_data.extend_from_slice(&height.0.to_le_bytes());
        for tx in &txs {
            if let Ok(b) = bincode::serialize(tx) {
                state_data.extend_from_slice(&b);
            }
        }
        let state_root = Hash(blake3::hash(&state_data).into());
        let receipts_root = Hash(blake3::hash(&state_root.0).into());
        Ok(Block {
            height,
            parent_hash,
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            transactions: txs,
            state_root,
            receipts_root,
            bundle_id,
            bundler: self.address,
            validator: Address::zero(),
            committee_attestations: vec![],
            base_fee: Amount(1000000000),
            pai_reward: Amount(10000000000000000000),
            gas_used: 21000,
        })
    }
    
    pub fn verify_attestation(&self, attestation: &Attestation, block: &Block) -> bool {
        if !attestation.valid {
            return false;
        }
        attestation.bundle_id == block.bundle_id
    }
    
    pub fn collect_attestations(&mut self, block: &mut Block, attestations: Vec<Attestation>) {
        for att in attestations {
            block.committee_attestations.push(att.signature);
        }
    }
}
