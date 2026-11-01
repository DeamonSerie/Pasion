use pasion_types::{Amount, LedgerId, BlockHeight, Address};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoanAdjustmentState {
    None,
    DiscountedToFortyPercent,
    Released,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loan {
    pub loan_id: LedgerId,
    pub ledger_id: LedgerId,
    pub borrower: Address,
    pub amount: Amount,
    pub interest_rate_bps: u32,
    pub borrowed_at_height: BlockHeight,
    pub repaid: bool,
    pub accrued_interest: Amount,
    pub loaner: Option<Address>,
    pub original_obligation: Amount,
    pub paid_at_start: Amount,
    pub adjustment_state: LoanAdjustmentState,
    pub tied_to_withdrawal_id: Option<u64>,
    pub loan_count: u32,
    pub payment_due_height: Option<BlockHeight>,
}

impl Loan {
    pub fn new(loan_id: LedgerId, ledger_id: LedgerId, borrower: Address, amount: Amount, interest_rate_bps: u32, height: BlockHeight) -> Self {
        Loan {
            loan_id,
            ledger_id,
            borrower,
            amount,
            interest_rate_bps,
            borrowed_at_height: height,
            repaid: false,
            accrued_interest: Amount(0),
            loaner: Some(borrower),
            original_obligation: amount,
            paid_at_start: Amount(0),
            adjustment_state: LoanAdjustmentState::None,
            tied_to_withdrawal_id: None,
            loan_count: 0,
            payment_due_height: None,
        }
    }
    
    pub fn calculate_interest(&self, blocks_elapsed: u64) -> Amount {
        // Simple interest calculation
        Amount((self.amount.0 * (self.interest_rate_bps as u128) / 10000) * (blocks_elapsed as u128) / 1000)
    }

    pub fn payment_schedule_blocks(total_possible_loans: u32, interest_rate_bps: u32, deposit_amount: Amount) -> u64 {
        let base = 100u64 + (total_possible_loans as u64) * 5;
        let rate_factor = if interest_rate_bps == 0 { 100u64 } else { 10000u64 / (interest_rate_bps as u64) };
        let amount_factor = if deposit_amount.0 == 0 { 1u64 } else { (deposit_amount.0 / 1_000_000_000_000_000_000u128).max(1) as u64 };
        base.saturating_add(rate_factor).saturating_add(amount_factor)
    }

pub fn loanable_pct(loan_count: u32) -> u32 {
    let val = 70i32 - (loan_count as i32) * 5;
    val.clamp(20, 70) as u32
}
}
