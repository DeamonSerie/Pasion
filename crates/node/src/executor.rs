use pasion_types::{Result, PasionError};
use pasion_ledger::{SharedReplicatedLedger, Transaction};
use pasion_debt_nft::{DebtNft, DebtEvent, NftStatus};
use pasion_types::{NftId, Amount, BlockHeight};

pub struct Executor;

impl Executor {
    pub fn execute_deposit(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::Deposit { tx_id, from, asset_id, amount, .. } = tx {
            let nft_id = NftId(blake3::hash(&[from.0.as_slice(), tx_id.0.as_slice()].concat()).into());
            let mut nft = DebtNft::new(
                nft_id,
                *from,
                *asset_id,
                *amount,
                *asset_id,
                500,
                BlockHeight(ledger.blocks.len() as u64),
            );
            nft.add_debt_event(DebtEvent::Deposit { tx_id: *tx_id, amount: *amount, height: BlockHeight(ledger.blocks.len() as u64) });
            let loanable = Amount(amount.0 * 70 / 100);
            let interest_pool = Amount(amount.0 * 10 / 100);
            nft.total_minted_from_nft = loanable;
            nft.interest_pool = interest_pool;
            nft.available_collateral_backing = Amount(amount.0.saturating_sub(loanable.0));
            nft.update_value_factor();
            ledger.nfts.insert(nft_id, nft);
        }
        Ok(())
    }
    
    pub fn execute_nft_transfer(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::NftTransfer { nft_id, from, to, .. } = tx {
            if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                if nft.status == NftStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedNft);
                }
                let total_debt = nft.total_debt_with_interest();
                if total_debt.0 > 0 {
                    nft.burn("Failed to pay debts in one transaction".to_string(), BlockHeight(ledger.blocks.len() as u64));
                    return Err(PasionError::MustPayDebtInFull);
                }
                nft.owner = *to;
                nft.add_debt_event(DebtEvent::Transferred { from: *from, to: *to, height: BlockHeight(ledger.blocks.len() as u64) });
            } else {
                return Err(PasionError::NftNotFound);
            }
        }
        Ok(())
    }
    
    pub fn execute_repay(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::Repay { tx_id, borrower: _, nft_id, amount } = tx {
            if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                if nft.status == NftStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedNft);
                }
                let interest = nft.calculate_interest(*amount);
                nft.debt_balance.0 = nft.debt_balance.0.saturating_sub(amount.0);
                nft.add_debt_event(DebtEvent::Repayment { tx_id: *tx_id, amount: *amount, interest_paid: interest, height: BlockHeight(ledger.blocks.len() as u64) });
                if nft.debt_balance.0 == 0 {
                    nft.status = NftStatus::PaidOff;
                    nft.add_debt_event(DebtEvent::DebtPaidInFull { height: BlockHeight(ledger.blocks.len() as u64) });
                }
                nft.available_collateral_backing.0 += amount.0 / 2;
                nft.update_value_factor();
            } else {
                return Err(PasionError::NftNotFound);
            }
        }
        Ok(())
    }
    
    pub fn execute_liquidate(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::Liquidate { tx_id: _, nft_id, liquidator } = tx {
            if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                if nft.status == NftStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedNft);
                }
                nft.status = NftStatus::Liquidating;
                nft.add_debt_event(DebtEvent::LiquidationStarted { tx_id: pasion_types::TxId([0u8; 32]), height: BlockHeight(ledger.blocks.len() as u64) });
                let _ = liquidator;
            } else {
                return Err(PasionError::NftNotFound);
            }
        }
        Ok(())
    }
    
    pub fn execute_arb_borrow(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::ArbBorrow { tx_id: _, arbitrageur, debt_nft_id, pool_id } = tx {
            if !ledger.nfts.contains_key(debt_nft_id) {
                return Err(PasionError::NftNotFound);
            }
            let _ = (arbitrageur, pool_id);
        }
        Ok(())
    }
}
