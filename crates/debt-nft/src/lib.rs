use serde::{Deserialize, Serialize};
use pasion_types::{Address, Amount, AssetId, BlockHeight, NftId, TxId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NftStatus {
    Active,
    Liquidating,
    Burned,
    PaidOff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebtEvent {
    Deposit {
        tx_id: TxId,
        amount: Amount,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtNft {
    pub id: NftId,
    pub owner: Address,
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
    pub available_collateral_backing: Amount,
    pub dealing_currencies: Vec<(AssetId, Amount)>,
    pub can_be_transferred: bool,
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
            available_collateral_backing: deposit_amount,
            dealing_currencies: Vec::new(),
            can_be_transferred: true,
        }
    }
    
    pub fn add_debt_event(&mut self, event: DebtEvent) {
        self.debt_history.push(event);
    }
    
    pub fn update_value_factor(&mut self) {
        if self.total_minted_from_nft.0 == 0 {
            self.value_factor = 1.0;
        } else {
            self.value_factor = (self.available_collateral_backing.0 as f64) 
                / (self.total_minted_from_nft.0 as f64);
            if self.value_factor > 1.0 {
                self.value_factor = 1.0;
            }
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
