use serde::{Deserialize, Serialize};
use pasion_types::{Address, Amount, NftId, BlockHeight};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationPool {
    pub id: NftId, // Provider NFT for the pool
    pub provider_nft: NftId,
    pub total_deposits: Amount,
    pub total_shares: Amount,
    pub interest_rate_bps: u32,
    pub depositors: HashMap<Address, Amount>, // shares
    pub active_arbs: Vec<ArbitragePosition>,
    pub accumulated_interest: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitragePosition {
    pub arbitrageur: Address,
    pub debt_nft_id: NftId,
    pub borrowed_amount: Amount,
    pub temp_nft_id: NftId,
    pub started_at_height: BlockHeight,
    pub repaid: bool,
    pub profit_retained: bool,
    pub debt_assigned: bool,
}

impl LiquidationPool {
    pub fn new(id: NftId, provider_nft: NftId) -> Self {
        LiquidationPool {
            id,
            provider_nft,
            total_deposits: Amount(0),
            total_shares: Amount(0),
            interest_rate_bps: 500,
            depositors: HashMap::new(),
            active_arbs: Vec::new(),
            accumulated_interest: Amount(0),
        }
    }
    
    pub fn deposit(&mut self, depositor: Address, amount: Amount) {
        let shares = if self.total_shares.0 == 0 {
            amount.0
        } else {
            (amount.0 * self.total_shares.0) / self.total_deposits.0
        };
        self.depositors.insert(depositor, Amount(shares));
        self.total_shares.0 += shares;
        self.total_deposits.0 += amount.0;
    }
    
    pub fn withdraw(&mut self, depositor: Address) -> Option<Amount> {
        if let Some(shares) = self.depositors.get(&depositor).cloned() {
            let amount = Amount((shares.0 * self.total_deposits.0) / self.total_shares.0);
            self.depositors.remove(&depositor);
            self.total_shares.0 = self.total_shares.0.saturating_sub(shares.0);
            self.total_deposits.0 = self.total_deposits.0.saturating_sub(amount.0);
            Some(amount)
        } else {
            None
        }
    }
    
    pub fn assign_debt_to_arb(&mut self, arbitrageur: Address, debt_nft_id: NftId, borrowed_amount: Amount, height: BlockHeight) -> NftId {
        let temp_nft_id = NftId(blake3::hash(&[arbitrageur.0.as_slice(), debt_nft_id.0.as_slice()].concat()).into());
        let pos = ArbitragePosition {
            arbitrageur,
            debt_nft_id,
            borrowed_amount,
            temp_nft_id,
            started_at_height: height,
            repaid: false,
            profit_retained: false,
            debt_assigned: true,
        };
        self.active_arbs.push(pos);
        temp_nft_id
    }
    
    pub fn repay_arb(&mut self, temp_nft_id: NftId, repaid_full: bool) -> bool {
        if let Some(pos) = self.active_arbs.iter_mut().find(|p| p.temp_nft_id == temp_nft_id) {
            if repaid_full {
                pos.repaid = true;
                pos.profit_retained = true;
                true
            } else {
                // Failed to recoup - loss absorption
                pos.repaid = false;
                pos.profit_retained = false;
                true
            }
        } else {
            false
        }
    }
}
