pub mod config;
pub mod constants;

use serde::{Deserialize, Serialize};
use pasion_types::Amount;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    pub chain_id: String,
    pub initial_pai_supply: Amount,
    pub base_fee_per_gas: Amount,
    pub bundle_auction_blocks: u64,
    pub committee_size: usize,
    pub min_bundler_bond: Amount,
    pub min_validator_bond: Amount,
    pub fixed_block_reward: Amount,
}

impl Default for GenesisConfig {
    fn default() -> Self {
        GenesisConfig {
            chain_id: "pasion-1".to_string(),
            initial_pai_supply: Amount(1_000_000_000_000_000_000u128),
            base_fee_per_gas: Amount(1_000_000_000u128),
            bundle_auction_blocks: 1,
            committee_size: 4,
            min_bundler_bond: Amount(100_000_000_000_000_000_000u128),
            min_validator_bond: Amount(50_000_000_000_000_000_000u128),
            fixed_block_reward: Amount(10_000_000_000_000_000_000u128),
        }
    }
}
