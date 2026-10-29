use pasion_types::Amount;

// PAI
pub const PAI_DECIMALS: u8 = 18;

// Consensus
pub const COMMITTEE_SIZE: usize = 4;
pub const BOND_SLASH_MINOR_BPS: u64 = 1000; // 10%
pub const BOND_SLASH_MAJOR_BPS: u64 = 5000; // 50%
pub const BOND_SLASH_CRITICAL_BPS: u64 = 10000; // 100%

// EIP-1559 style
pub const BASE_FEE_MAX_CHANGE_DENOMINATOR: u128 = 8;
pub const TARGET_GAS_USED: u64 = 15_000_000;
pub const MAX_GAS_LIMIT: u64 = 30_000_000;

// Reputation
pub const REPUTATION_DECAY_FACTOR: f64 = 0.99;
pub const REPUTATION_MIN: i64 = 0;
pub const REPUTATION_MAX: i64 = 10000;
pub const REPUTATION_INITIAL: i64 = 1000;

// NFT
pub const NFT_BURN_ON_FAILED_PAYMENT: bool = true;

// Liquidation
pub const LIQUIDATION_THRESHOLD_BPS: u32 = 8000; // 80%
pub const LIQUIDATION_PENALTY_BPS: u32 = 500; // 5%

// Interest
pub const DEFAULT_INTEREST_RATE_BPS: u32 = 500; // 5%

// Auctions
pub const COMMIT_PHASE_BLOCKS: u64 = 1;
pub const REVEAL_PHASE_BLOCKS: u64 = 1;
