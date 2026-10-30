use pasion_types::{Result, PasionError};
use pasion_ledger::{SharedReplicatedLedger, Transaction};
use pasion_debt_nft::{DebtNft, DebtEvent, NftStatus, WithdrawalRecord, WithdrawalStatus, LoanRef, TrackedFund, TrackedFundStatus, ObligationStatus, LossEvent};
use pasion_types::{NftId, Amount, BlockHeight, Address, AssetId, TxId, BountyId};

pub struct Executor;

pub const LOANABILITY_THRESHOLD: i128 = 30_000_000_000_000_000;
pub const MATURITY_BLOCKS: u64 = 100;
pub const TAKER_PARTIAL_DELTA: i64 = 2;
pub const TAKER_FULL_BONUS: i64 = 8;
pub const LENDER_MATURE_LOST_LOANABILITY_DELTA: i64 = 10;
pub const LENDER_EARLY_WITHDRAWAL_DELTA: i64 = -5;
pub const LENDER_MATURE_WITH_LOANABILITY_DELTA: i64 = 3;
pub const LENDER_FAILED_BOUNTY_DELTA: i64 = -10;

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
            let loan_count: u32 = 0;
            let loanable_pct: u128 = (70i32 - (loan_count as i32) * 5).clamp(20, 70) as u128;
            let loanable = Amount(amount.0 * loanable_pct / 100);
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
        if let Transaction::Repay { tx_id, borrower, nft_id, amount } = tx {
            let mut taker_delta: i64 = 0;
            let mut do_taker_score = false;
            if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                if nft.status == NftStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedNft);
                }
                if nft.status == NftStatus::PaidOff {
                    return Ok(());
                }
                let debt_before = nft.debt_balance.0;
                if debt_before == 0 {
                    nft.status = NftStatus::PaidOff;
                    nft.update_value_factor();
                    return Ok(());
                }
                let loaned_amount = Amount(debt_before + nft.cumulative_repaid.0);
                let is_full = debt_before <= amount.0;
                let incremental = nft.repayment_count > 0;
                let height = BlockHeight(ledger.blocks.len() as u64);
                if is_full {
                    let boost = if incremental { loaned_amount.0 / 10 } else { loaned_amount.0 };
                    nft.available_collateral_backing.0 += boost;
                    nft.add_debt_event(DebtEvent::FullRepaymentBoost { tx_id: *tx_id, loaned_amount, boost_amount: Amount(boost), incremental, height });
                } else {
                    nft.available_collateral_backing.0 += amount.0 / 2;
                }
                nft.debt_balance.0 = nft.debt_balance.0.saturating_sub(amount.0);
                nft.cumulative_repaid.0 += amount.0;
                nft.repayment_count += 1;
                if nft.debt_balance.0 == 0 {
                    nft.status = NftStatus::PaidOff;
                    nft.add_debt_event(DebtEvent::DebtPaidInFull { height });
                }
                if amount.0 > 0 {
                    if debt_before > amount.0 {
                        taker_delta = TAKER_PARTIAL_DELTA;
                    } else {
                        taker_delta = TAKER_FULL_BONUS;
                    }
                    do_taker_score = true;
                }
                nft.update_value_factor();
            } else {
                return Err(PasionError::NftNotFound);
            }
            if do_taker_score {
                ledger.adjust_taker_score(*borrower, taker_delta);
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
    
    pub fn execute_depositor_withdraw(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::DepositorWithdraw { tx_id, nft_id, depositor } = tx {
            let height = BlockHeight(ledger.blocks.len() as u64);
            {
                let nft = match ledger.nfts.get(nft_id) {
                    Some(n) => n,
                    None => return Err(PasionError::NftNotFound),
                };
                if nft.status == NftStatus::Burned { return Err(PasionError::CannotTransferBurnedNft); }
                if nft.depositor != *depositor { return Err(PasionError::InvalidBlock); }
                if nft.withdrawal_executed { return Err(PasionError::InvalidBlock); }
            }
            let (principal, interest_accrued, minted_currency) = {
                let nft = ledger.nfts.get(nft_id).unwrap();
                (nft.deposit_amount, nft.deposit_interest_accrued, nft.loaned_currency_id)
            };
            let total = Amount(principal.0 + interest_accrued.0);
            {
                let entry = ledger.assets.entry(minted_currency).or_insert(Amount(0));
                entry.0 += total.0;
            }
            {
                let depositor_account = ledger.state.entry(*depositor).or_insert_with(|| pasion_ledger::AccountState {
                    address: *depositor, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                });
                depositor_account.balances.entry(minted_currency).and_modify(|e| e.0 += total.0).or_insert(total);
            }
            let withdrawal_id = { let nft = ledger.nfts.get(nft_id).unwrap(); nft.withdrawals.len() as u64 + 1 };
            let affected_refs: Vec<LoanRef> = {
                let nft = ledger.nfts.get(nft_id).unwrap();
                nft.affected_loans.iter().map(|al| al.loan_ref.clone()).collect()
            };
            if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                nft.add_debt_event(DebtEvent::DepositorWithdraw {
                    tx_id: *tx_id, depositor: *depositor, principal_returned: principal, interest_returned: interest_accrued,
                    minted_amount: total, minted_currency, height,
                });
                nft.withdrawals.push(WithdrawalRecord {
                    id: withdrawal_id, tx_id: *tx_id, depositor: *depositor, principal_returned: principal, interest_returned: interest_accrued,
                    minted_amount: total, minted_currency, height, affected_loan_refs: affected_refs, status: WithdrawalStatus::Completed,
                });
                nft.withdrawal_executed = true;
                nft.total_withdrawn_minted = Amount(nft.total_withdrawn_minted.0 + total.0);
                nft.deposit_interest_accrued = Amount(0);
                nft.withdrawn_funds_tracked.push(TrackedFund {
                    id: 1, withdrawal_id, currency_id: minted_currency, amount: total, holder: Some(*depositor), status: TrackedFundStatus::Held, loss_events: Vec::new(),
                });
                nft.add_debt_event(DebtEvent::WithdrawalFundTracked { tx_id: *tx_id, withdrawal_id, tracked_fund_id: 1, status: TrackedFundStatus::Held, height });
            }
            // Adjust loans separately
            let loan_list: Vec<(NftId, Address, Amount)> = {
                let nft = ledger.nfts.get(nft_id).unwrap();
                nft.affected_loans.iter().map(|al| (al.loan_ref.loan_id, al.loaner, al.original_obligation)).collect()
            };
            if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                let mut i = 0;
                while i < nft.affected_loans.len() && i < loan_list.len() {
                    let orig = loan_list[i].2;
                    let adj40 = Amount(orig.0 * 40 / 100);
                    nft.affected_loans[i].adjusted_obligation = adj40;
                    nft.affected_loans[i].obligation_status = ObligationStatus::DiscountedToFortyPercent;
                    nft.affected_loans[i].released = false;
                    let lid = loan_list[i].0;
                    let laddr = loan_list[i].1;
                    nft.add_debt_event(DebtEvent::LoanObligationAdjusted {
                        tx_id: *tx_id, loan_id: lid, loaner: laddr, original_obligation: orig, adjusted_obligation: adj40, adjustment_rate_bps: 4000, height,
                    });
                    i += 1;
                }
            }

            // Lender scoring
            let total_loaned: u128 = ledger.nfts.get(nft_id).map(|n| n.affected_loans.iter().map(|al| al.original_obligation.0).sum()).unwrap_or(0);
            let created_at = ledger.nfts.get(nft_id).map(|n| n.created_at_height).unwrap_or(BlockHeight(0));
            if total_loaned as i128 <= LOANABILITY_THRESHOLD {
                ledger.adjust_lender_score(*depositor, LENDER_MATURE_LOST_LOANABILITY_DELTA);
            } else if height.0 < created_at.0 + MATURITY_BLOCKS {
                ledger.adjust_lender_score(*depositor, LENDER_EARLY_WITHDRAWAL_DELTA);
            } else {
                ledger.adjust_lender_score(*depositor, LENDER_MATURE_WITH_LOANABILITY_DELTA);
            }

            // Bounty creation if unpaid affected loans exist
            let unpaid_exists = {
                if let Some(nft) = ledger.nfts.get(nft_id) {
                    !nft.affected_loans.is_empty() && nft.affected_loans.iter().any(|al| !al.released)
                } else { false }
            };
            if unpaid_exists {
                let (source_nft_id, reward_wallet, currency_id, original_amount, loan_count, consistency_score, avg_interest) = {
                    let nft = ledger.nfts.get(nft_id).unwrap();
                    let loan_count = nft.affected_loans.len() as u32;
                    let repaid = nft.affected_loans.iter().filter(|al| al.paid_at_start.0 > 0).count() as f64;
                    let loan_total = nft.affected_loans.len() as f64;
                    let consistency_score = if loan_total > 0.0 { repaid / loan_total } else { 1.0 };
                    let avg_interest = if nft.affected_loans.is_empty() {
                        Amount(0)
                    } else {
                        let sum: u128 = nft.affected_loans.iter().map(|al| {
                            (al.original_obligation.0 * (nft.interest_rate_bps as u128)) / 10000
                        }).sum();
                        Amount(sum / (nft.affected_loans.len() as u128))
                    };
                    (nft.id, nft.depositor, nft.loaned_currency_id, total, loan_count, consistency_score, avg_interest)
                };
                let base_blocks: u64 = 100;
                let discount: u64 = (consistency_score * 50.0) as u64;
                let time_limit = base_blocks.saturating_sub(discount).max(5);
                let bounty_id = pasion_types::BountyId(blake3::hash(&[tx_id.0.as_slice(), nft_id.0.as_slice()].concat()).into());
                let bounty = pasion_debt_nft::Bounty {
                    id: bounty_id,
                    source_nft_id,
                    reward_wallet,
                    currency_id,
                    original_amount,
                    current_amount: original_amount,
                    created_at_height: height,
                    time_limit_blocks: time_limit,
                    end_height: BlockHeight(height.0 + time_limit),
                    loan_count,
                    consistency_score,
                    avg_interest,
                    burn_remaining: Amount(0),
                    burn_per_block: avg_interest,
                    winner: None,
                    winning_bid: None,
                    status: pasion_debt_nft::BountyStatus::Open,
                };
                let auction = pasion_debt_nft::BountyAuction {
                    bounty_id,
                    bids: Vec::new(),
                    highest: None,
                    end_height: BlockHeight(height.0 + time_limit),
                };
                ledger.bounties.insert(bounty_id, bounty);
                ledger.bounty_auctions.insert(bounty_id, auction);
            }
        }
        Ok(())
    }
    
    pub fn execute_place_bounty_bid(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::PlaceBountyBid { tx_id: _, bounty_id, bidder, amount } = tx {
            let height = BlockHeight(ledger.blocks.len() as u64);
            let bounty = ledger.bounties.get(bounty_id).ok_or(PasionError::BountyNotFound)?;
            if bounty.status != pasion_debt_nft::BountyStatus::Open && bounty.status != pasion_debt_nft::BountyStatus::Auctioning {
                return Err(PasionError::BountyAlreadySettled);
            }
            if height.0 >= bounty.end_height.0 {
                return Err(PasionError::BountyExpired);
            }
            let current_highest = ledger.bounty_auctions.get(bounty_id).and_then(|a| a.highest.clone());
            if let Some(h) = &current_highest {
                if amount.0 <= h.amount.0 {
                    return Err(PasionError::InvalidBountyBid);
                }
            }
            let currency_id = bounty.currency_id;
            let pai_id = AssetId([1u8; 32]);
            let sufficient = {
                match ledger.state.get(bidder) {
                    Some(acc) => {
                        let pai_bal = acc.balances.get(&pai_id).map(|x| x.0).unwrap_or(0);
                        let cur_bal = acc.balances.get(&currency_id).map(|x| x.0).unwrap_or(0);
                        pai_bal >= amount.0 || cur_bal >= amount.0
                    }
                    None => false,
                }
            };
            if !sufficient {
                return Err(PasionError::InsufficientBalance);
            }
            let bid = pasion_debt_nft::BountyBid { bidder: *bidder, amount: *amount, height };
            if let Some(auction) = ledger.bounty_auctions.get_mut(bounty_id) {
                auction.bids.push(bid.clone());
                auction.highest = Some(bid.clone());
            }
            if let Some(b) = ledger.bounties.get_mut(bounty_id) {
                b.winning_bid = Some(*amount);
                b.winner = Some(*bidder);
                b.status = pasion_debt_nft::BountyStatus::Auctioning;
            }
        }
        Ok(())
    }

    pub fn apply_withdrawn_funds_loss(ledger: &mut SharedReplicatedLedger, nft_id: &NftId, withdrawal_id: u64, total_lost: Amount, tx_id: TxId, reason: String, height: BlockHeight) -> Result<()> {
        // Get currency
        let currency_id = {
            match ledger.nfts.get(nft_id) {
                Some(n) => n.loaned_currency_id,
                None => return Err(PasionError::NftNotFound),
            }
        };
        // Mark funds lost
        if let Some(nft) = ledger.nfts.get_mut(nft_id) {
            for fund in nft.withdrawn_funds_tracked.iter_mut() {
                if fund.withdrawal_id == withdrawal_id {
                    fund.status = TrackedFundStatus::Lost;
                    fund.loss_events.push(LossEvent { height, reason: reason.clone(), amount_lost: total_lost });
                }
            }
            nft.add_debt_event(DebtEvent::WithdrawnFundsLost {
                tx_id,
                withdrawal_id,
                total_lost,
                currency_id,
                height,
            });
        }
        // Check if all lost for this withdrawal
        let mut all_lost_flag = true;
        if let Some(nft) = ledger.nfts.get(nft_id) {
            for fund in &nft.withdrawn_funds_tracked {
                if fund.withdrawal_id == withdrawal_id {
                    if fund.status != TrackedFundStatus::Lost {
                        all_lost_flag = false;
                        break;
                    }
                }
            }
        }
        if all_lost_flag {
            // Collect refunds
            let mut refunds = Vec::new();
            if let Some(nft) = ledger.nfts.get(nft_id) {
                for affected in &nft.affected_loans {
                    if !affected.released && affected.paid_at_start.0 > 0 {
                        refunds.push((affected.loaner, affected.paid_at_start));
                    }
                }
            }
            for (loaner, refund_amount) in refunds {
                let entry = ledger.assets.entry(currency_id).or_insert(Amount(0));
                entry.0 += refund_amount.0;
                let loaner_account = ledger.state.entry(loaner).or_insert_with(|| pasion_ledger::AccountState {
                    address: loaner,
                    balances: std::collections::HashMap::new(),
                    nonce: 0,
                    bond: Amount(0),
                    bonded_role: None,
                });
                loaner_account.balances.entry(currency_id).and_modify(|e| e.0 += refund_amount.0).or_insert(refund_amount);
                if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                    nft.add_debt_event(DebtEvent::LoanerRefunded {
                        tx_id,
                        loaner,
                        amount_refunded: refund_amount,
                        currency_id,
                        height,
                    });
                }
            }
            if let Some(nft) = ledger.nfts.get_mut(nft_id) {
                for affected in nft.affected_loans.iter_mut() {
                    if !affected.released {
                        affected.obligation_status = if affected.paid_at_start.0 > 0 { ObligationStatus::Refunded } else { ObligationStatus::ReleasedNoPay };
                        affected.released = true;
                        affected.adjusted_obligation = Amount(0);
                    }
                }
            }
        }
        Ok(())
    }

    
    pub fn process_bounty_expiry<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
        process_bounty_expiry(ledger, height);
    }

    pub fn process_timed_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
        process_timed_burn(ledger, height);
    }

    pub fn process_bounty_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
        process_timed_burn(ledger, height);
    }

    pub fn process_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
        process_timed_burn(ledger, height);
    }

    pub fn finalize_block<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
        process_bounty_expiry(ledger, height);
        process_timed_burn(ledger, height);
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

pub fn process_bounty_expiry<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
    let ids: Vec<BountyId> = ledger.bounties.iter().filter(|(_, b)| b.end_height == height).map(|(id, _)| *id).collect();
    for id in ids {
        let highest = ledger.bounty_auctions.get(&id).and_then(|a| a.highest.clone());
        if let Some(h) = highest {
            // settle
            if let Some(bounty) = ledger.bounties.get(&id) {
                let currency_id = bounty.currency_id;
                let current_amount = bounty.current_amount;
                let avg_interest = bounty.avg_interest;
                let reward_wallet = bounty.reward_wallet;
                let winner = h.bidder;
                // winner receives current_amount of bounty currency
                {
                    let entry = ledger.assets.entry(currency_id).or_insert(Amount(0));
                    entry.0 = entry.0.saturating_add(current_amount.0);
                }
                {
                    let acc = ledger.state.entry(winner).or_insert_with(|| pasion_ledger::AccountState {
                        address: winner, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                    });
                    acc.balances.entry(currency_id).and_modify(|e| e.0 += current_amount.0).or_insert(current_amount);
                }
                // winner pays PAI (interest + avg_interest) to reward_wallet
                let pai_id = AssetId([1u8; 32]);
                let pay_amount = Amount(avg_interest.0); // interest + avg_interest; use avg_interest as interest component
                // transfer from winner to reward wallet
                {
                    if let Some(acc) = ledger.state.get_mut(&winner) {
                        if let Some(bal) = acc.balances.get_mut(&pai_id) {
                            bal.0 = bal.0.saturating_sub(pay_amount.0);
                        }
                    }
                }
                {
                    let acc = ledger.state.entry(reward_wallet).or_insert_with(|| pasion_ledger::AccountState {
                        address: reward_wallet, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                    });
                    acc.balances.entry(pai_id).and_modify(|e| e.0 += pay_amount.0).or_insert(pay_amount);
                }
                if let Some(b) = ledger.bounties.get_mut(&id) {
                    b.winner = Some(winner);
                    b.winning_bid = Some(h.amount);
                    b.status = pasion_debt_nft::BountyStatus::Settled;
                }
                let _ = currency_id;
            }
        } else {
            // no bids
            let (original_amount, currency_id, reward_wallet) = {
                if let Some(b) = ledger.bounties.get(&id) {
                    (b.original_amount, b.currency_id, b.reward_wallet)
                } else { (Amount(0), AssetId([0u8;32]), Address::zero()) }
            };
            let dist_total = Amount(original_amount.0 * 20 / 100);
            let wallets: Vec<Address> = ledger.state.keys().cloned().collect();
            let n = wallets.len().max(1) as u128;
            let per = Amount(dist_total.0 / n);
            let pai_id = AssetId([1u8; 32]);
            for w in wallets {
                let acc = ledger.state.entry(w).or_insert_with(|| pasion_ledger::AccountState {
                    address: w, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                });
                acc.balances.entry(pai_id).and_modify(|e| e.0 += per.0).or_insert(per);
            }
            {
                let entry = ledger.assets.entry(pai_id).or_insert(Amount(0));
                entry.0 = entry.0.saturating_add(dist_total.0);
            }
            let burn_remaining = Amount(original_amount.0.saturating_sub(dist_total.0));
            if let Some(b) = ledger.bounties.get_mut(&id) {
                b.burn_remaining = burn_remaining;
                b.status = pasion_debt_nft::BountyStatus::ExpiredNoBids;
            }
            ledger.adjust_lender_score(reward_wallet, LENDER_FAILED_BOUNTY_DELTA);
            let _ = currency_id;
        }
    }
}

pub fn process_timed_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, _height: H) {
    let ids: Vec<BountyId> = ledger.bounties.iter().filter(|(_, b)| b.status == pasion_debt_nft::BountyStatus::ExpiredNoBids && b.burn_remaining.0 > 0).map(|(id, _)| *id).collect();
    for id in ids {
        let (currency_id, burn_per_block) = {
            let b = ledger.bounties.get(&id).unwrap();
            (b.currency_id, b.burn_per_block)
        };
        let burn_amt = {
            let b = ledger.bounties.get(&id).unwrap();
            std::cmp::min(burn_per_block.0, b.burn_remaining.0)
        };
        if burn_amt > 0 {
            {
                let entry = ledger.assets.entry(currency_id).or_insert(Amount(0));
                entry.0 = entry.0.saturating_sub(burn_amt);
            }
            if let Some(b) = ledger.bounties.get_mut(&id) {
                b.burn_remaining.0 = b.burn_remaining.0.saturating_sub(burn_amt);
                if b.burn_remaining.0 == 0 {
                    b.status = pasion_debt_nft::BountyStatus::Burned;
                }
            }
        }
    }
}

// aliases
pub fn process_bounty_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) { process_timed_burn(ledger, height); }
pub fn process_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) { process_timed_burn(ledger, height); }
pub fn process_bounties<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) { let height: BlockHeight = height.into(); process_bounty_expiry(ledger, height); process_timed_burn(ledger, height); }

#[cfg(test)]
mod tests {
    use super::*;
    use pasion_debt_nft::{DebtNft, AffectedLoan, LoanRef, ObligationStatus};
    use pasion_ledger::SharedReplicatedLedger;
    use pasion_types::{Address, Amount, AssetId, BlockHeight, NftId, TxId};

    #[test]
    fn bounty_created_on_withdraw() {
        let mut ledger = SharedReplicatedLedger::default();
        let nft_id = NftId([1u8;32]);
        let depositor = Address([2u8;32]);
        let mut nft = DebtNft::new(nft_id, depositor, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.deposit_interest_accrued = Amount(10);
        nft.affected_loans.push(AffectedLoan {
            loan_ref: LoanRef { loan_id: NftId([5u8;32]), loaner: Address([6u8;32]) },
            loaner: Address([6u8;32]),
            original_obligation: Amount(50),
            paid_at_start: Amount(0),
            adjusted_obligation: Amount(0),
            obligation_status: ObligationStatus::Active,
            released: false,
        });
        ledger.nfts.insert(nft_id, nft);
        let tx = Transaction::DepositorWithdraw { tx_id: TxId([7u8;32]), nft_id, depositor };
        Executor::execute_depositor_withdraw(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.bounties.len(), 1);
    }

    #[test]
    fn bounty_bid_and_expiry() {
        let mut ledger = SharedReplicatedLedger::default();
        let bounty_id = pasion_types::BountyId([9u8;32]);
        let currency = AssetId([4u8;32]);
        let bounty = pasion_debt_nft::Bounty {
            id: bounty_id,
            source_nft_id: NftId([1u8;32]),
            reward_wallet: Address([2u8;32]),
            currency_id: currency,
            original_amount: Amount(100),
            current_amount: Amount(100),
            created_at_height: BlockHeight(0),
            time_limit_blocks: 5,
            end_height: BlockHeight(10),
            loan_count: 1,
            consistency_score: 1.0,
            avg_interest: Amount(10),
            burn_remaining: Amount(0),
            burn_per_block: Amount(10),
            winner: None,
            winning_bid: None,
            status: pasion_debt_nft::BountyStatus::Open,
        };
        let auction = pasion_debt_nft::BountyAuction {
            bounty_id,
            bids: Vec::new(),
            highest: None,
            end_height: BlockHeight(10),
        };
        ledger.bounties.insert(bounty_id, bounty);
        ledger.bounty_auctions.insert(bounty_id, auction);
        // fund bidder PAI
        let bidder = Address([10u8;32]);
        let mut acc = pasion_ledger::AccountState { address: bidder, balances: std::collections::HashMap::new(), nonce:0, bond: Amount(0), bonded_role: None };
        acc.balances.insert(AssetId([1u8;32]), Amount(50));
        ledger.state.insert(bidder, acc);
        let tx = Transaction::PlaceBountyBid { tx_id: TxId([11u8;32]), bounty_id, bidder, amount: Amount(20) };
        Executor::execute_place_bounty_bid(&mut ledger, &tx).unwrap();
        assert!(ledger.bounty_auctions.get(&bounty_id).unwrap().highest.is_some());
        process_bounty_expiry(&mut ledger, BlockHeight(10));
        assert_eq!(ledger.bounties.get(&bounty_id).unwrap().status, pasion_debt_nft::BountyStatus::Settled);
    }

    #[test]
    fn timed_burn() {
        let mut ledger = SharedReplicatedLedger::default();
        let bounty_id = pasion_types::BountyId([12u8;32]);
        let currency = AssetId([4u8;32]);
        let bounty = pasion_debt_nft::Bounty {
            id: bounty_id,
            source_nft_id: NftId([1u8;32]),
            reward_wallet: Address([2u8;32]),
            currency_id: currency,
            original_amount: Amount(100),
            current_amount: Amount(100),
            created_at_height: BlockHeight(0),
            time_limit_blocks: 5,
            end_height: BlockHeight(0),
            loan_count: 1,
            consistency_score: 1.0,
            avg_interest: Amount(10),
            burn_remaining: Amount(30),
            burn_per_block: Amount(10),
            winner: None,
            winning_bid: None,
            status: pasion_debt_nft::BountyStatus::ExpiredNoBids,
        };
        ledger.bounties.insert(bounty_id, bounty);
        ledger.assets.insert(currency, Amount(100));
        process_timed_burn(&mut ledger, BlockHeight(1));
        assert_eq!(ledger.bounties.get(&bounty_id).unwrap().burn_remaining, Amount(20));
    }

    #[test]
    fn partial_repay_increases_taker_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let nft_id = NftId([1u8;32]);
        let borrower = Address([2u8;32]);
        let mut nft = DebtNft::new(nft_id, borrower, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.debt_balance = Amount(100);
        ledger.nfts.insert(nft_id, nft);
        let tx = Transaction::Repay { tx_id: TxId([7u8;32]), borrower, nft_id, amount: Amount(30) };
        Executor::execute_repay(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.taker_score(borrower), TAKER_PARTIAL_DELTA);
    }

    #[test]
    fn full_repay_increases_taker_score_more() {
        let mut ledger = SharedReplicatedLedger::default();
        let nft_id = NftId([1u8;32]);
        let borrower = Address([2u8;32]);
        let mut nft = DebtNft::new(nft_id, borrower, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.debt_balance = Amount(100);
        ledger.nfts.insert(nft_id, nft);
        let tx = Transaction::Repay { tx_id: TxId([7u8;32]), borrower, nft_id, amount: Amount(100) };
        Executor::execute_repay(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.taker_score(borrower), TAKER_FULL_BONUS);
    }

    #[test]
    fn withdraw_negligible_increases_lender_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let nft_id = NftId([1u8;32]);
        let depositor = Address([2u8;32]);
        let mut nft = DebtNft::new(nft_id, depositor, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.deposit_interest_accrued = Amount(10);
        ledger.nfts.insert(nft_id, nft);
        let tx = Transaction::DepositorWithdraw { tx_id: TxId([7u8;32]), nft_id, depositor };
        Executor::execute_depositor_withdraw(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.lender_score(depositor), LENDER_MATURE_LOST_LOANABILITY_DELTA);
    }

    #[test]
    fn early_withdraw_significant_decreases_lender_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let nft_id = NftId([1u8;32]);
        let depositor = Address([2u8;32]);
        let mut nft = DebtNft::new(nft_id, depositor, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.deposit_interest_accrued = Amount(10);
        nft.created_at_height = BlockHeight(0);
        nft.affected_loans.push(AffectedLoan {
            loan_ref: LoanRef { loan_id: NftId([5u8;32]), loaner: Address([6u8;32]) },
            loaner: Address([6u8;32]),
            original_obligation: Amount(40_000_000_000_000_000),
            paid_at_start: Amount(0),
            adjusted_obligation: Amount(0),
            obligation_status: ObligationStatus::Active,
            released: false,
        });
        ledger.nfts.insert(nft_id, nft);
        ledger.adjust_lender_score(depositor, 20);
        let tx = Transaction::DepositorWithdraw { tx_id: TxId([7u8;32]), nft_id, depositor };
        Executor::execute_depositor_withdraw(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.lender_score(depositor), 15);
    }

    #[test]
    fn expired_bounty_no_bids_decreases_lender_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let bounty_id = pasion_types::BountyId([9u8;32]);
        let currency = AssetId([4u8;32]);
        let reward_wallet = Address([2u8;32]);
        let bounty = pasion_debt_nft::Bounty {
            id: bounty_id,
            source_nft_id: NftId([1u8;32]),
            reward_wallet,
            currency_id: currency,
            original_amount: Amount(100),
            current_amount: Amount(100),
            created_at_height: BlockHeight(0),
            time_limit_blocks: 5,
            end_height: BlockHeight(10),
            loan_count: 1,
            consistency_score: 1.0,
            avg_interest: Amount(10),
            burn_remaining: Amount(0),
            burn_per_block: Amount(10),
            winner: None,
            winning_bid: None,
            status: pasion_debt_nft::BountyStatus::Open,
        };
        let auction = pasion_debt_nft::BountyAuction {
            bounty_id,
            bids: Vec::new(),
            highest: None,
            end_height: BlockHeight(10),
        };
        ledger.bounties.insert(bounty_id, bounty);
        ledger.bounty_auctions.insert(bounty_id, auction);
        ledger.adjust_lender_score(reward_wallet, 20);
        process_bounty_expiry(&mut ledger, BlockHeight(10));
        assert_eq!(ledger.lender_score(reward_wallet), 10);
    }
}

pub fn execute_place_bounty_bid(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> pasion_types::Result<()> {
    Executor::execute_place_bounty_bid(ledger, tx)
}
