use serde::{Deserialize, Serialize};
use pasion_types::{Address, Amount, AssetId, BlockHeight, NftId, TxId};
pub use pasion_types::BountyId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NftStatus {
    Active,
    Liquidating,
    Burned,
    PaidOff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanRef {
    pub loan_id: NftId,
    pub loaner: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WithdrawalStatus {
    Pending,
    Completed,
    Reverted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRecord {
    pub id: u64,
    pub tx_id: TxId,
    pub depositor: Address,
    pub principal_returned: Amount,
    pub interest_returned: Amount,
    pub minted_amount: Amount,
    pub minted_currency: AssetId,
    pub height: BlockHeight,
    pub affected_loan_refs: Vec<LoanRef>,
    pub status: WithdrawalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackedFundStatus {
    Held,
    Transferred,
    Lost,
    Settled,
    Recovered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossEvent {
    pub height: BlockHeight,
    pub reason: String,
    pub amount_lost: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedFund {
    pub id: u64,
    pub withdrawal_id: u64,
    pub currency_id: AssetId,
    pub amount: Amount,
    pub holder: Option<Address>,
    pub status: TrackedFundStatus,
    pub loss_events: Vec<LossEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObligationStatus {
    Active,
    DiscountedToFortyPercent,
    ReleasedNoPay,
    Refunded,
    PartiallyPaid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedLoan {
    pub loan_ref: LoanRef,
    pub loaner: Address,
    pub original_obligation: Amount,
    pub paid_at_start: Amount,
    pub adjusted_obligation: Amount,
    pub obligation_status: ObligationStatus,
    pub released: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BountyStatus {
    Open,
    Auctioning,
    Settled,
    ExpiredNoBids,
    Burned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyBid {
    pub bidder: Address,
    pub amount: Amount,
    pub height: BlockHeight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounty {
    pub id: BountyId,
    pub source_nft_id: NftId,
    pub reward_wallet: Address,
    pub currency_id: AssetId,
    pub original_amount: Amount,
    pub current_amount: Amount,
    pub created_at_height: BlockHeight,
    pub time_limit_blocks: u64,
    pub end_height: BlockHeight,
    pub loan_count: u32,
    pub consistency_score: f64,
    pub avg_interest: Amount,
    pub burn_remaining: Amount,
    pub burn_per_block: Amount,
    pub winner: Option<Address>,
    pub winning_bid: Option<Amount>,
    pub status: BountyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyAuction {
    pub bounty_id: BountyId,
    pub bids: Vec<BountyBid>,
    pub highest: Option<BountyBid>,
    pub end_height: BlockHeight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebtEvent {
    Deposit {
        tx_id: TxId,
        amount: Amount,
        height: BlockHeight,
    },
    DepositConsistencyInterest {
        tx_id: TxId,
        amount: Amount,
        consistency_score: f64,
        height: BlockHeight,
    },
    LoanIssued {
        tx_id: TxId,
        amount: Amount,
        currency_id: AssetId,
        height: BlockHeight,
    },
    Repayment {
        tx_id: TxId,
        amount: Amount,
        interest_paid: Amount,
        height: BlockHeight,
    },
    Transferred {
        from: Address,
        to: Address,
        height: BlockHeight,
    },
    LoanabilityReset {
        tx_id: TxId,
        from: Address,
        to: Address,
        previous_loanability: Amount,
        new_loanability: Amount,
        height: BlockHeight,
    },
    LiquidationStarted {
        tx_id: TxId,
        height: BlockHeight,
    },
    Burned {
        reason: String,
        height: BlockHeight,
        dealing_currencies_burned: Vec<(AssetId, Amount)>,
    },
    DebtPaidInFull {
        height: BlockHeight,
    },
    DepositorWithdraw {
        tx_id: TxId,
        depositor: Address,
        principal_returned: Amount,
        interest_returned: Amount,
        minted_amount: Amount,
        minted_currency: AssetId,
        height: BlockHeight,
    },
    LoanObligationAdjusted {
        tx_id: TxId,
        loan_id: NftId,
        loaner: Address,
        original_obligation: Amount,
        adjusted_obligation: Amount,
        adjustment_rate_bps: u32,
        height: BlockHeight,
    },
    LoanerRefunded {
        tx_id: TxId,
        loaner: Address,
        amount_refunded: Amount,
        currency_id: AssetId,
        height: BlockHeight,
    },
    WithdrawnFundsLost {
        tx_id: TxId,
        withdrawal_id: u64,
        total_lost: Amount,
        currency_id: AssetId,
        height: BlockHeight,
    },
    WithdrawalFundTracked {
        tx_id: TxId,
        withdrawal_id: u64,
        tracked_fund_id: u64,
        status: TrackedFundStatus,
        height: BlockHeight,
    },
    FullRepaymentBoost {
        tx_id: TxId,
        loaned_amount: Amount,
        boost_amount: Amount,
        incremental: bool,
        height: BlockHeight,
    },
    YieldSplitterAdded { tx_id: TxId, splitter: Address, height: BlockHeight },
    YieldSplitFrozen { tx_id: TxId, depositor: Address, height: BlockHeight },
    MinimumYieldSplittersSet { tx_id: TxId, depositor: Address, minimum: u32, height: BlockHeight },
    YieldSplitPercentageVoted { tx_id: TxId, voter: Address, percentage_bps: u32, height: BlockHeight },
    YieldSplitEstablished { tx_id: TxId, percentage_bps: u32, height: BlockHeight },
    YieldSplitPaid { tx_id: TxId, splitter: Address, amount: Amount, height: BlockHeight },
    YieldSplitDistribution { tx_id: TxId, depositor: Address, amount: Amount, height: BlockHeight },
    StarterSaleDenied {
        tx_id: TxId,
        seller: Address,
        buyer: Address,
        height: BlockHeight,
    },
    StarterSaleCompleted {
        tx_id: TxId,
        seller: Address,
        buyer: Address,
        unpaid_debt: Amount,
        payment_amount: Amount,
        height: BlockHeight,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtNft {
    pub id: NftId,
    pub owner: Address,
    pub depositor: Address,
    pub collateral_asset: AssetId,
    pub deposit_amount: Amount,
    pub debt_history: Vec<DebtEvent>,
    pub loaned_currency_id: AssetId,
    pub debt_balance: Amount,
    pub interest_rate_bps: u32,
    pub interest_pool: Amount,
    pub created_at_height: BlockHeight,
    pub status: NftStatus,
    pub value_factor: f64,
    pub total_minted_from_nft: Amount,
    pub deposit_loanability: Amount,
    pub available_collateral_backing: Amount,
    pub dealing_currencies: Vec<(AssetId, Amount)>,
    pub can_be_transferred: bool,
    pub withdrawals: Vec<WithdrawalRecord>,
    pub withdrawal_executed: bool,
    pub total_withdrawn_minted: Amount,
    pub deposit_interest_accrued: Amount,
    pub affected_loans: Vec<AffectedLoan>,
    pub withdrawn_funds_tracked: Vec<TrackedFund>,
    pub repayment_count: u32,
    pub cumulative_repaid: Amount,
    pub loan_count: u32,
    #[serde(default)]
    pub yield_splitters: Vec<Address>,
    #[serde(default = "default_minimum_yield_splitter_users")]
    pub minimum_yield_splitter_users: u32,
    #[serde(default)]
    pub frozen: bool,
    #[serde(default)]
    pub yield_split_established: bool,
    #[serde(default = "default_yield_split_percentage_bps")]
    pub yield_split_percentage_bps: u32,
    #[serde(default)]
    pub yield_votes: std::collections::HashMap<Address, u32>,
    #[serde(default)]
    pub yield_splitter_pending: std::collections::HashMap<Address, Amount>,
}

impl DebtNft {
    pub fn new(
        id: NftId,
        owner: Address,
        collateral_asset: AssetId,
        deposit_amount: Amount,
        loaned_currency_id: AssetId,
        interest_rate_bps: u32,
        height: BlockHeight,
    ) -> Self {
        DebtNft {
            id,
            owner,
            depositor: owner,
            collateral_asset,
            deposit_amount,
            debt_history: Vec::new(),
            loaned_currency_id,
            debt_balance: Amount(0),
            interest_rate_bps,
            interest_pool: Amount(0),
            created_at_height: height,
            status: NftStatus::Active,
            value_factor: 1.0,
            total_minted_from_nft: Amount(0),
            deposit_loanability: Amount(deposit_amount.0 * 70 / 100),
            available_collateral_backing: deposit_amount,
            dealing_currencies: Vec::new(),
            can_be_transferred: true,
            withdrawals: Vec::new(),
            withdrawal_executed: false,
            total_withdrawn_minted: Amount(0),
            deposit_interest_accrued: Amount(0),
            affected_loans: Vec::new(),
            withdrawn_funds_tracked: Vec::new(),
            repayment_count: 0,
            cumulative_repaid: Amount(0),
            loan_count: 0,
            yield_splitters: Vec::new(),
            minimum_yield_splitter_users: 6,
            frozen: false,
            yield_split_established: false,
            yield_split_percentage_bps: 1000,
            yield_votes: std::collections::HashMap::new(),
            yield_splitter_pending: std::collections::HashMap::new(),
        }
    }
    
    pub fn add_debt_event(&mut self, event: DebtEvent) {
        self.debt_history.push(event);
    }

    pub fn most_loans_unpaid(&self) -> bool {
        let total = self.debt_balance.0.saturating_add(self.cumulative_repaid.0);
        total > 0 && self.debt_balance.0 * 2 > total
    }

    pub fn is_yield_splitter(&self, addr: &Address) -> bool {
        self.yield_splitters.contains(addr)
    }
    
    pub fn update_value_factor(&mut self) {
        if self.total_minted_from_nft.0 == 0 {
            self.value_factor = 1.0;
        } else {
            self.value_factor = (self.available_collateral_backing.0 as f64) 
                / (self.total_minted_from_nft.0 as f64);
            if self.value_factor < 0.0 {
                self.value_factor = 0.0;
            }
        }
    }
    
    pub fn calculate_interest(&self, principal: Amount) -> Amount {
        Amount((principal.0 * (self.interest_rate_bps as u128)) / 10000)
    }
    
    pub fn total_debt_with_interest(&self) -> Amount {
        let interest = self.calculate_interest(self.debt_balance);
        Amount(self.debt_balance.0 + interest.0)
    }
    
    pub fn burn(&mut self, reason: String, height: BlockHeight) {
        self.status = NftStatus::Burned;
        self.can_be_transferred = false;
        let dealing_currencies_burned = self.dealing_currencies.clone();
        self.dealing_currencies.clear();
        self.add_debt_event(DebtEvent::Burned { reason, height, dealing_currencies_burned });
    }
}


fn default_minimum_yield_splitter_users() -> u32 { 6 }
fn default_yield_split_percentage_bps() -> u32 { 1000 }
