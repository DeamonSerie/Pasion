use pasion_types::{Amount, NftId, BlockHeight, Address};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loan {
    pub loan_id: NftId,
    pub nft_id: NftId,
    pub borrower: Address,
    pub amount: Amount,
    pub interest_rate_bps: u32,
    pub borrowed_at_height: BlockHeight,
    pub repaid: bool,
    pub accrued_interest: Amount,
}

impl Loan {
    pub fn new(loan_id: NftId, nft_id: NftId, borrower: Address, amount: Amount, interest_rate_bps: u32, height: BlockHeight) -> Self {
        Loan {
            loan_id,
            nft_id,
            borrower,
            amount,
            interest_rate_bps,
            borrowed_at_height: height,
            repaid: false,
            accrued_interest: Amount(0),
        }
    }
    
    pub fn calculate_interest(&self, blocks_elapsed: u64) -> Amount {
        // Simple interest calculation
        Amount((self.amount.0 * (self.interest_rate_bps as u128) / 10000) * (blocks_elapsed as u128) / 1000)
    }
}
