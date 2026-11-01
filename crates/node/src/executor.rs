use pasion_types::{Result, PasionError};
use pasion_ledger::{SharedReplicatedLedger, Transaction};
use pasion_debt_ledger::{DebtLedger, DebtEvent, LedgerStatus, WithdrawalRecord, WithdrawalStatus, LoanRef, TrackedFund, TrackedFundStatus, ObligationStatus, LossEvent, RepaymentRecord};
use pasion_types::{LedgerId, Amount, BlockHeight, Address, AssetId, TxId, BountyId};

pub struct Executor;

pub const LOANABILITY_THRESHOLD: i128 = 30_000_000_000_000_000;
pub const MATURITY_BLOCKS: u64 = 100;
pub const TAKER_PARTIAL_DELTA: i64 = 2;
pub const TAKER_FULL_BONUS: i64 = 8;
pub const LENDER_MATURE_LOST_LOANABILITY_DELTA: i64 = 10;
pub const LENDER_EARLY_WITHDRAWAL_DELTA: i64 = -5;
pub const LENDER_MATURE_WITH_LOANABILITY_DELTA: i64 = 3;
pub const LENDER_FAILED_BOUNTY_DELTA: i64 = -10;
pub const YIELD_SPLIT_DEFAULT_PERCENTAGE_BPS: u32 = 1000;
pub const STARTER_SALE_PAYMENT_BPS: u32 = 4000;
pub const YIELD_SPLIT_RECURRING_PERCENT: u64 = 10;
pub const YIELD_SPLIT_RECURRING_INTERVAL: u32 = 6;
pub const YIELD_SPLIT_TAKER_DELTA: i64 = 8;
pub const YIELD_SPLITTER_MIN_TAKER_SCORE: i64 = 1;

impl Executor {
    pub fn execute_deposit(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::Deposit { tx_id, from, asset_id, amount, target_ledger_id, .. } = tx {
            if let Some(target) = target_ledger_id {
                let height = BlockHeight(ledger.blocks.len() as u64);
                let nft = ledger.ledgers.get_mut(target).ok_or(PasionError::LedgerNotFound)?;
                if nft.frozen {
                    return Err(PasionError::LedgerFrozen);
                }
                if nft.status == LedgerStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedLedger);
                }
                if nft.depositor != *from {
                    return Err(PasionError::InvalidTransaction);
                }
                if nft.collateral_asset != *asset_id {
                    return Err(PasionError::InvalidTransaction);
                }
                let loan_count = nft.loan_count;
                let loanable_pct: u128 = (70i32 - (loan_count as i32) * 5).clamp(20, 70) as u128;
                let new_loanable = Amount(amount.0 * loanable_pct / 100);
                let interest_add = Amount(amount.0 * 10 / 100);
                nft.deposit_amount = Amount(nft.deposit_amount.0.saturating_add(amount.0));
                nft.total_minted_from_ledger = Amount(nft.total_minted_from_ledger.0.saturating_add(new_loanable.0));
                nft.deposit_loanability = Amount(nft.deposit_loanability.0.saturating_add(new_loanable.0));
                nft.interest_pool = Amount(nft.interest_pool.0.saturating_add(interest_add.0));
                nft.available_collateral_backing = Amount(nft.available_collateral_backing.0.saturating_add(amount.0.saturating_sub(new_loanable.0)));
                let consistency_score = if !nft.affected_loans.is_empty() {
                    let total = nft.affected_loans.len() as f64;
                    let repaid = nft.affected_loans.iter().filter(|al| al.paid_at_start.0 > 0).count() as f64;
                    if total > 0.0 { repaid / total } else { 0.0 }
                } else {
                    let total = nft.cumulative_repaid.0.saturating_add(nft.debt_balance.0);
                    if total > 0 { nft.cumulative_repaid.0 as f64 / total as f64 } else { 0.0 }
                };
                let consistency_bps = (consistency_score * 10000.0).round() as u128;
                let interest_bonus = Amount(interest_add.0 * consistency_bps / 10000);
                nft.deposit_interest_accrued = Amount(nft.deposit_interest_accrued.0.saturating_add(interest_bonus.0));
                nft.add_debt_event(DebtEvent::DepositConsistencyInterest {
                    tx_id: *tx_id,
                    amount: interest_bonus,
                    consistency_score,
                    height,
                });
                nft.add_debt_event(DebtEvent::Deposit { tx_id: *tx_id, amount: *amount, height });
                nft.update_value_factor();
            } else {
                let ledger_id = LedgerId(blake3::hash(&[from.0.as_slice(), tx_id.0.as_slice()].concat()).into());
                let mut nft = DebtLedger::new(
                    ledger_id,
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
                nft.total_minted_from_ledger = loanable;
                nft.deposit_loanability = loanable;
                nft.interest_pool = interest_pool;
                nft.available_collateral_backing = Amount(amount.0.saturating_sub(loanable.0));
                nft.update_value_factor();
                ledger.ledgers.insert(ledger_id, nft);
            }
        }
        Ok(())
    }
    
    pub fn execute_ledger_transfer(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::LedgerTransfer { tx_id, ledger_id, from, to, .. } = tx {
            if ledger.ledgers.get(ledger_id).map(|n| n.frozen).unwrap_or(false) {
                return Err(PasionError::LedgerFrozen);
            }
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                if nft.status == LedgerStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedLedger);
                }
                let total_debt = nft.total_debt_with_interest();
                if total_debt.0 > 0 {
                    nft.burn("Failed to pay debts in one transaction".to_string(), BlockHeight(ledger.blocks.len() as u64));
                    return Err(PasionError::MustPayDebtInFull);
                }
                if *from != *to {
                    let previous = nft.deposit_loanability;
                    let new_loanability = Amount(nft.deposit_amount.0 * 70 / 100);
                    nft.add_debt_event(DebtEvent::LoanabilityReset {
                        tx_id: *tx_id,
                        from: *from,
                        to: *to,
                        previous_loanability: previous,
                        new_loanability,
                        height: BlockHeight(ledger.blocks.len() as u64),
                    });
                    nft.deposit_loanability = new_loanability;
                    nft.total_minted_from_ledger = new_loanability;
                    nft.loan_count = 0;
                    nft.repayment_records.clear();
                    nft.update_value_factor();
                }
                nft.owner = *to;
                nft.add_debt_event(DebtEvent::Transferred { from: *from, to: *to, height: BlockHeight(ledger.blocks.len() as u64) });
            } else {
                return Err(PasionError::LedgerNotFound);
            }
        }
        Ok(())
    }
    
    pub fn execute_repay(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::Repay { tx_id, borrower, ledger_id, amount } = tx {
            if ledger.ledgers.get(ledger_id).map(|n| n.frozen).unwrap_or(false) {
                return Err(PasionError::LedgerFrozen);
            }
            let mut taker_delta: i64 = 0;
            let mut do_taker_score = false;
            let mut pai_yield_credits: Vec<(Address, u128)> = Vec::new();
            let mut recurring_transfers: Vec<(Address, Address, u128)> = Vec::new();
            let mut recurring_taker_scores: Vec<Address> = Vec::new();
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                if nft.status == LedgerStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedLedger);
                }
                if nft.status == LedgerStatus::PaidOff {
                    return Ok(());
                }
                let debt_before = nft.debt_balance.0;
                if debt_before == 0 {
                    nft.status = LedgerStatus::PaidOff;
                    nft.update_value_factor();
                    return Ok(());
                }
                let loaned_amount = Amount(debt_before + nft.cumulative_repaid.0);
                let is_full = debt_before <= amount.0;
                let incremental = nft.repayment_count > 0;
                let was_full_no_prior = nft.repayment_count == 0 && debt_before > 0 && is_full;
                let height = BlockHeight(ledger.blocks.len() as u64);
                let was_consistent_full = !was_full_no_prior
                    && is_full
                    && debt_before > 0
                    && nft.consistent_repayment_history(borrower, *amount, height);
                if is_full {
                    let boost = if incremental { loaned_amount.0 / 10 } else { loaned_amount.0 };
                    nft.available_collateral_backing.0 += boost;
                    nft.add_debt_event(DebtEvent::FullRepaymentBoost { tx_id: *tx_id, loaned_amount, boost_amount: Amount(boost), incremental, height });
                } else {
                    nft.available_collateral_backing.0 += amount.0 / 2;
                }
                if was_full_no_prior || was_consistent_full {
                    if !nft.yield_splitters.contains(borrower) {
                        nft.yield_splitters.push(*borrower);
                        nft.add_debt_event(DebtEvent::YieldSplitterAdded { tx_id: *tx_id, splitter: *borrower, height });
                    }
                    let interest = nft.calculate_interest(loaned_amount);
                    let split_amount = interest.0 * (nft.yield_split_percentage_bps as u128) / 10000;
                    pai_yield_credits.push((*borrower, split_amount));
                    let entry = nft.yield_splitter_pending.entry(*borrower).or_insert(Amount(0));
                    entry.0 += split_amount;
                    nft.deposit_interest_accrued.0 = nft.deposit_interest_accrued.0.saturating_sub(split_amount);
                    nft.add_debt_event(DebtEvent::YieldSplitPaid { tx_id: *tx_id, splitter: *borrower, amount: Amount(split_amount), height });
                }
                nft.debt_balance.0 = nft.debt_balance.0.saturating_sub(amount.0);
                nft.cumulative_repaid.0 += amount.0;
                nft.repayment_records.push(RepaymentRecord {
                    borrower: *borrower,
                    amount: *amount,
                    height,
                });
                nft.repayment_count += 1;
                if nft.repayment_count % YIELD_SPLIT_RECURRING_INTERVAL == 0 && nft.yield_split_established {
                    let splitters = nft.yield_splitters.clone();
                    let depositor = nft.depositor;
                    for splitter in splitters {
                        let pending = nft.yield_splitter_pending.get(&splitter).copied().unwrap_or(Amount(0));
                        let share = Amount(pending.0 * (YIELD_SPLIT_RECURRING_PERCENT as u128) / 100);
                        if share.0 > 0 {
                            recurring_transfers.push((splitter, depositor, share.0));
                            nft.add_debt_event(DebtEvent::YieldSplitDistribution { tx_id: *tx_id, depositor, amount: share, height });
                            recurring_taker_scores.push(splitter);
                        }
                    }
                    for splitter in nft.yield_splitters.clone() {
                        nft.yield_splitter_pending.insert(splitter, Amount(0));
                    }
                }
                if nft.debt_balance.0 == 0 {
                    nft.status = LedgerStatus::PaidOff;
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
                return Err(PasionError::LedgerNotFound);
            }
            // apply PAI yield credits
            let pai_id = AssetId([1u8; 32]);
            for (addr, amt) in pai_yield_credits {
                if amt > 0 {
                    {
                        let entry = ledger.assets.entry(pai_id).or_insert(Amount(0));
                        entry.0 = entry.0.saturating_add(amt);
                    }
                    {
                        let acc = ledger.state.entry(addr).or_insert_with(|| pasion_ledger::AccountState {
                            address: addr, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                        });
                        acc.balances.entry(pai_id).and_modify(|e| e.0 += amt).or_insert(Amount(amt));
                    }
                }
            }
            // apply recurring transfers
            for (splitter, depositor, share) in recurring_transfers {
                if let Some(acc) = ledger.state.get_mut(&splitter) {
                    if let Some(bal) = acc.balances.get_mut(&pai_id) {
                        bal.0 = bal.0.saturating_sub(share);
                    }
                }
                {
                    let acc = ledger.state.entry(depositor).or_insert_with(|| pasion_ledger::AccountState {
                        address: depositor, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                    });
                    acc.balances.entry(pai_id).and_modify(|e| e.0 += share).or_insert(Amount(share));
                }
            }
            if do_taker_score {
                ledger.adjust_taker_score(*borrower, taker_delta);
            }
            for splitter in recurring_taker_scores {
                ledger.adjust_taker_score(splitter, YIELD_SPLIT_TAKER_DELTA);
            }
        }
        Ok(())
    }

    pub fn execute_liquidate(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::Liquidate { tx_id: _, ledger_id, liquidator } = tx {
            if ledger.ledgers.get(ledger_id).map(|n| n.frozen).unwrap_or(false) {
                return Err(PasionError::LedgerFrozen);
            }
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                if nft.status == LedgerStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedLedger);
                }
                nft.status = LedgerStatus::Liquidating;
                nft.add_debt_event(DebtEvent::LiquidationStarted { tx_id: pasion_types::TxId([0u8; 32]), height: BlockHeight(ledger.blocks.len() as u64) });
                let _ = liquidator;
            } else {
                return Err(PasionError::LedgerNotFound);
            }
        }
        Ok(())
    }
    
    pub fn execute_depositor_withdraw(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::DepositorWithdraw { tx_id, ledger_id, depositor } = tx {
            if ledger.ledgers.get(ledger_id).map(|n| n.frozen).unwrap_or(false) {
                return Err(PasionError::LedgerFrozen);
            }
            let height = BlockHeight(ledger.blocks.len() as u64);
            {
                let nft = match ledger.ledgers.get(ledger_id) {
                    Some(n) => n,
                    None => return Err(PasionError::LedgerNotFound),
                };
                if nft.status == LedgerStatus::Burned { return Err(PasionError::CannotTransferBurnedLedger); }
                if nft.depositor != *depositor { return Err(PasionError::InvalidBlock); }
                if nft.withdrawal_executed { return Err(PasionError::InvalidBlock); }
            }
            let (principal, interest_accrued, minted_currency) = {
                let nft = ledger.ledgers.get(ledger_id).unwrap();
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
            let withdrawal_id = { let nft = ledger.ledgers.get(ledger_id).unwrap(); nft.withdrawals.len() as u64 + 1 };
            let affected_refs: Vec<LoanRef> = {
                let nft = ledger.ledgers.get(ledger_id).unwrap();
                nft.affected_loans.iter().map(|al| al.loan_ref.clone()).collect()
            };
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
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
            let loan_list: Vec<(LedgerId, Address, Amount)> = {
                let nft = ledger.ledgers.get(ledger_id).unwrap();
                nft.affected_loans.iter().map(|al| (al.loan_ref.loan_id, al.loaner, al.original_obligation)).collect()
            };
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
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
            let total_loaned: u128 = ledger.ledgers.get(ledger_id).map(|n| n.affected_loans.iter().map(|al| al.original_obligation.0).sum()).unwrap_or(0);
            let created_at = ledger.ledgers.get(ledger_id).map(|n| n.created_at_height).unwrap_or(BlockHeight(0));
            if total_loaned as i128 <= LOANABILITY_THRESHOLD {
                ledger.adjust_lender_score(*depositor, LENDER_MATURE_LOST_LOANABILITY_DELTA);
            } else if height.0 < created_at.0 + MATURITY_BLOCKS {
                ledger.adjust_lender_score(*depositor, LENDER_EARLY_WITHDRAWAL_DELTA);
            } else {
                ledger.adjust_lender_score(*depositor, LENDER_MATURE_WITH_LOANABILITY_DELTA);
            }

            // Bounty creation if unpaid affected loans exist
            let unpaid_exists = {
                if let Some(nft) = ledger.ledgers.get(ledger_id) {
                    !nft.affected_loans.is_empty() && nft.affected_loans.iter().any(|al| !al.released)
                } else { false }
            };
            if unpaid_exists {
                let (source_ledger_id, reward_wallet, currency_id, original_amount, loan_count, consistency_score, avg_interest) = {
                    let nft = ledger.ledgers.get(ledger_id).unwrap();
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
                let bounty_id = pasion_types::BountyId(blake3::hash(&[tx_id.0.as_slice(), ledger_id.0.as_slice()].concat()).into());
                let bounty = pasion_debt_ledger::Bounty {
                    id: bounty_id,
                    source_ledger_id,
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
                    status: pasion_debt_ledger::BountyStatus::Open,
                };
                let auction = pasion_debt_ledger::BountyAuction {
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
    
    pub fn execute_yield_split_freeze(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::YieldSplitFreeze { tx_id, ledger_id, depositor } = tx {
            let nft = ledger.ledgers.get(ledger_id).ok_or(PasionError::LedgerNotFound)?;
            if nft.status == LedgerStatus::Burned {
                return Err(PasionError::CannotTransferBurnedLedger);
            }
            if nft.depositor != *depositor {
                return Err(PasionError::InvalidBlock);
            }
            if nft.frozen {
                return Err(PasionError::InvalidBlock);
            }
            let height = BlockHeight(ledger.blocks.len() as u64);
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                nft.frozen = true;
                nft.add_debt_event(DebtEvent::YieldSplitFrozen { tx_id: *tx_id, depositor: *depositor, height });
            }
        }
        Ok(())
    }

    pub fn execute_set_minimum_yield_splitters(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::SetMinimumYieldSplitters { tx_id, ledger_id, depositor, minimum } = tx {
            let nft = ledger.ledgers.get(ledger_id).ok_or(PasionError::LedgerNotFound)?;
            if nft.depositor != *depositor {
                return Err(PasionError::InvalidBlock);
            }
            if !nft.frozen {
                return Err(PasionError::InvalidBlock);
            }
            let height = BlockHeight(ledger.blocks.len() as u64);
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                nft.minimum_yield_splitter_users = *minimum;
                nft.add_debt_event(DebtEvent::MinimumYieldSplittersSet { tx_id: *tx_id, depositor: *depositor, minimum: *minimum, height });
            }
        }
        Ok(())
    }

    pub fn execute_yield_split_vote(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::YieldSplitVote { tx_id, ledger_id, voter, percentage_bps } = tx {
            let nft = ledger.ledgers.get(ledger_id).ok_or(PasionError::LedgerNotFound)?;
            if !nft.frozen {
                return Err(PasionError::InvalidBlock);
            }
            if !nft.yield_splitters.contains(voter) {
                return Err(PasionError::InvalidBlock);
            }
            if ledger.taker_score(*voter) < YIELD_SPLITTER_MIN_TAKER_SCORE {
                return Err(PasionError::InvalidBlock);
            }
            let height = BlockHeight(ledger.blocks.len() as u64);
            // record vote
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                nft.yield_votes.insert(*voter, *percentage_bps);
                nft.add_debt_event(DebtEvent::YieldSplitPercentageVoted { tx_id: *tx_id, voter: *voter, percentage_bps: *percentage_bps, height });
            }
            // check majority
            let (should_finalize, winning_pct) = {
                let nft = ledger.ledgers.get(ledger_id).unwrap();
                if nft.yield_votes.len() * 2 > nft.yield_splitters.len() && nft.yield_splitters.len() >= nft.minimum_yield_splitter_users as usize {
                    // tally
                    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
                    for pct in nft.yield_votes.values() {
                        *counts.entry(*pct).or_insert(0) += 1;
                    }
                    let mut best_pct = nft.yield_split_percentage_bps;
                    let mut best_count = 0usize;
                    for (pct, count) in counts.iter() {
                        if *count > best_count || (*count == best_count && *pct > best_pct) {
                            best_pct = *pct;
                            best_count = *count;
                        }
                    }
                    (true, best_pct)
                } else {
                    (false, 0u32)
                }
            };
            if should_finalize {
                if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                    nft.yield_split_percentage_bps = winning_pct;
                    nft.yield_split_established = true;
                    nft.frozen = false;
                    nft.add_debt_event(DebtEvent::YieldSplitEstablished { tx_id: *tx_id, percentage_bps: winning_pct, height });
                }
            }
        }
        Ok(())
    }

    pub fn execute_place_bounty_bid(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::PlaceBountyBid { tx_id: _, bounty_id, bidder, amount } = tx {
            let height = BlockHeight(ledger.blocks.len() as u64);
            let bounty = ledger.bounties.get(bounty_id).ok_or(PasionError::BountyNotFound)?;
            if bounty.status != pasion_debt_ledger::BountyStatus::Open && bounty.status != pasion_debt_ledger::BountyStatus::Auctioning {
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
            let bid = pasion_debt_ledger::BountyBid { bidder: *bidder, amount: *amount, height };
            if let Some(auction) = ledger.bounty_auctions.get_mut(bounty_id) {
                auction.bids.push(bid.clone());
                auction.highest = Some(bid.clone());
            }
            if let Some(b) = ledger.bounties.get_mut(bounty_id) {
                b.winning_bid = Some(*amount);
                b.winner = Some(*bidder);
                b.status = pasion_debt_ledger::BountyStatus::Auctioning;
            }
        }
        Ok(())
    }

    pub fn apply_withdrawn_funds_loss(ledger: &mut SharedReplicatedLedger, ledger_id: &LedgerId, withdrawal_id: u64, total_lost: Amount, tx_id: TxId, reason: String, height: BlockHeight) -> Result<()> {
        // Get currency
        let currency_id = {
            match ledger.ledgers.get(ledger_id) {
                Some(n) => n.loaned_currency_id,
                None => return Err(PasionError::LedgerNotFound),
            }
        };
        // Mark funds lost
        if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
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
        if let Some(nft) = ledger.ledgers.get(ledger_id) {
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
            if let Some(nft) = ledger.ledgers.get(ledger_id) {
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
                if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                    nft.add_debt_event(DebtEvent::LoanerRefunded {
                        tx_id,
                        loaner,
                        amount_refunded: refund_amount,
                        currency_id,
                        height,
                    });
                }
            }
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
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

    pub fn execute_ledger_starter_sale(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::LedgerStarterSale { tx_id, ledger_id, seller, buyer, accepted } = tx {
            // frozen check first
            if ledger.ledgers.get(ledger_id).map(|n| n.frozen).unwrap_or(false) {
                return Err(PasionError::LedgerFrozen);
            }
            // lookup
            {
                let nft = ledger.ledgers.get(ledger_id).ok_or(PasionError::LedgerNotFound)?;
                if nft.status == LedgerStatus::Burned {
                    return Err(PasionError::CannotTransferBurnedLedger);
                }
                if nft.owner != *seller {
                    return Err(PasionError::InvalidTransaction);
                }
                if !nft.most_loans_unpaid() {
                    return Err(PasionError::StarterSaleNotAllowed);
                }
            }
            let height = BlockHeight(ledger.blocks.len() as u64);

            if !*accepted {
                if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                    nft.add_debt_event(DebtEvent::StarterSaleDenied {
                        tx_id: *tx_id,
                        seller: *seller,
                        buyer: *buyer,
                        height,
                    });
                }
                return Ok(());
            }

            // compute amounts
            let unpaid_debt = ledger.ledgers.get(ledger_id).unwrap().debt_balance;
            let payment_total = Amount(unpaid_debt.0 * STARTER_SALE_PAYMENT_BPS as u128 / 10000);
            let share = payment_total.0 / 3;
            let mut depositor_share = payment_total.0 - 2 * share;
            let buyer_share = share;
            let mut loaner_share = share;

            let loaned_currency_id = ledger.ledgers.get(ledger_id).unwrap().loaned_currency_id;
            let depositor = ledger.ledgers.get(ledger_id).unwrap().depositor;

            // Buyer credit
            let buyer_ledgers: Vec<LedgerId> = ledger.ledgers.iter()
                .filter(|(id, n)| **id != *ledger_id && n.owner == *buyer && n.debt_balance.0 > 0)
                .map(|(id, _)| *id)
                .collect();
            if !buyer_ledgers.is_empty() {
                let total_buyer_debt: u128 = buyer_ledgers.iter()
                    .map(|id| ledger.ledgers.get(id).unwrap().debt_balance.0)
                    .sum();
                if total_buyer_debt > 0 {
                    for id in buyer_ledgers.iter() {
                        let debt = ledger.ledgers.get(id).unwrap().debt_balance.0;
                        let portion = buyer_share * debt / total_buyer_debt;
                        if let Some(n) = ledger.ledgers.get_mut(id) {
                            n.debt_balance.0 = n.debt_balance.0.saturating_sub(portion);
                        }
                    }
                }
            } else if buyer_share > 0 {
                {
                    let entry = ledger.assets.entry(loaned_currency_id).or_insert(Amount(0));
                    entry.0 = entry.0.saturating_add(buyer_share);
                }
                {
                    let acc = ledger.state.entry(*buyer).or_insert_with(|| pasion_ledger::AccountState {
                        address: *buyer, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                    });
                    acc.balances.entry(loaned_currency_id).and_modify(|e| e.0 += buyer_share).or_insert(Amount(buyer_share));
                }
            }

            // Determine unreleased affected loans & denom
            let (unreleased_indices, denom) = {
                let nft = ledger.ledgers.get(ledger_id).unwrap();
                let mut idxs = Vec::new();
                for (i, al) in nft.affected_loans.iter().enumerate() {
                    if !al.released {
                        idxs.push(i);
                    }
                }
                let d: u128 = idxs.iter().map(|&i| nft.affected_loans[i].original_obligation.0).sum();
                (idxs, d)
            };

            let redirect_loaner = unreleased_indices.is_empty() || denom == 0;
            if redirect_loaner {
                depositor_share += loaner_share;
                loaner_share = 0;
            }

            // Depositor credit
            if depositor_share > 0 {
                {
                    let entry = ledger.assets.entry(loaned_currency_id).or_insert(Amount(0));
                    entry.0 = entry.0.saturating_add(depositor_share);
                }
                {
                    let acc = ledger.state.entry(depositor).or_insert_with(|| pasion_ledger::AccountState {
                        address: depositor, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                    });
                    acc.balances.entry(loaned_currency_id).and_modify(|e| e.0 += depositor_share).or_insert(Amount(depositor_share));
                }
                if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                    nft.deposit_interest_accrued.0 = nft.deposit_interest_accrued.0.saturating_sub(depositor_share);
                }
            }

            // Loaner credit
            if loaner_share > 0 && !redirect_loaner {
                // need clone loaner info first
                let loaner_info: Vec<(usize, Address, u128)> = {
                    let nft = ledger.ledgers.get(ledger_id).unwrap();
                    unreleased_indices.iter().map(|&i| (i, nft.affected_loans[i].loaner, nft.affected_loans[i].original_obligation.0)).collect()
                };
                for (i, loaner, orig) in loaner_info {
                    let portion = if denom > 0 { loaner_share * orig / denom } else { 0 };
                    if portion > 0 {
                        {
                            let entry = ledger.assets.entry(loaned_currency_id).or_insert(Amount(0));
                            entry.0 = entry.0.saturating_add(portion);
                        }
                        {
                            let acc = ledger.state.entry(loaner).or_insert_with(|| pasion_ledger::AccountState {
                                address: loaner, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                            });
                            acc.balances.entry(loaned_currency_id).and_modify(|e| e.0 += portion).or_insert(Amount(portion));
                        }
                    }
                    if portion > 0 {
                        if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                            let al = &mut nft.affected_loans[i];
                            al.adjusted_obligation.0 = al.adjusted_obligation.0.saturating_sub(portion);
                            if al.adjusted_obligation.0 == 0 {
                                al.released = true;
                                al.obligation_status = ObligationStatus::ReleasedNoPay;
                            } else {
                                al.obligation_status = ObligationStatus::PartiallyPaid;
                            }
                        }
                    }
                }
            }

            // Reduce sold NFT debt
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                nft.debt_balance.0 = nft.debt_balance.0.saturating_sub(payment_total.0);
                nft.cumulative_repaid.0 = nft.cumulative_repaid.0.saturating_add(payment_total.0);
            }

            // Loanability reset if seller != buyer
            if *seller != *buyer {
                let previous = ledger.ledgers.get(ledger_id).unwrap().deposit_loanability;
                let new_loanability = Amount(ledger.ledgers.get(ledger_id).unwrap().deposit_amount.0 * 70 / 100);
                if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                    nft.add_debt_event(DebtEvent::LoanabilityReset {
                        tx_id: *tx_id,
                        from: *seller,
                        to: *buyer,
                        previous_loanability: previous,
                        new_loanability,
                        height,
                    });
                    nft.deposit_loanability = new_loanability;
                    nft.total_minted_from_ledger = new_loanability;
                    nft.loan_count = 0;
                    nft.repayment_records.clear();
                    nft.update_value_factor();
                }
            }

            // Set owner
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                nft.owner = *buyer;
            }

            // Bounty creation
            let has_unreleased = ledger.ledgers.get(ledger_id).map(|n| n.affected_loans.iter().any(|al| !al.released)).unwrap_or(false);
            let remaining_debt_positive = ledger.ledgers.get(ledger_id).map(|n| n.debt_balance.0 > 0).unwrap_or(false);
            if has_unreleased && remaining_debt_positive {
                let (source_ledger_id, reward_wallet, currency_id, remaining_debt, loan_count, consistency_score, avg_interest) = {
                    let nft = ledger.ledgers.get(ledger_id).unwrap();
                    let loan_count = nft.affected_loans.iter().filter(|al| !al.released).count() as u32;
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
                    (nft.id, nft.depositor, nft.loaned_currency_id, nft.debt_balance, loan_count, consistency_score, avg_interest)
                };
                let base_blocks: u64 = 100;
                let discount: u64 = (consistency_score * 50.0) as u64;
                let time_limit = base_blocks.saturating_sub(discount).max(5);
                let bounty_id = pasion_types::BountyId(blake3::hash(&[tx_id.0.as_slice(), ledger_id.0.as_slice()].concat()).into());
                let bounty = pasion_debt_ledger::Bounty {
                    id: bounty_id,
                    source_ledger_id,
                    reward_wallet,
                    currency_id,
                    original_amount: remaining_debt,
                    current_amount: remaining_debt,
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
                    status: pasion_debt_ledger::BountyStatus::Open,
                };
                let auction = pasion_debt_ledger::BountyAuction {
                    bounty_id,
                    bids: Vec::new(),
                    highest: None,
                    end_height: BlockHeight(height.0 + time_limit),
                };
                ledger.bounties.insert(bounty_id, bounty);
                ledger.bounty_auctions.insert(bounty_id, auction);
            }

            // Record events
            if let Some(nft) = ledger.ledgers.get_mut(ledger_id) {
                nft.add_debt_event(DebtEvent::Transferred { from: *seller, to: *buyer, height });
                nft.add_debt_event(DebtEvent::StarterSaleCompleted {
                    tx_id: *tx_id,
                    seller: *seller,
                    buyer: *buyer,
                    unpaid_debt,
                    payment_amount: payment_total,
                    height,
                });
            }
        }
        Ok(())
    }

    pub fn execute_arb_borrow(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> Result<()> {
        if let Transaction::ArbBorrow { tx_id: _, arbitrageur, debt_ledger_id, pool_id } = tx {
            if !ledger.ledgers.contains_key(debt_ledger_id) {
                return Err(PasionError::LedgerNotFound);
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
                    b.status = pasion_debt_ledger::BountyStatus::Settled;
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
                b.status = pasion_debt_ledger::BountyStatus::ExpiredNoBids;
            }
            ledger.adjust_lender_score(reward_wallet, LENDER_FAILED_BOUNTY_DELTA);
            let _ = currency_id;
        }
    }
}

pub fn process_timed_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, _height: H) {
    let ids: Vec<BountyId> = ledger.bounties.iter().filter(|(_, b)| b.status == pasion_debt_ledger::BountyStatus::ExpiredNoBids && b.burn_remaining.0 > 0).map(|(id, _)| *id).collect();
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
                    b.status = pasion_debt_ledger::BountyStatus::Burned;
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
    use pasion_debt_ledger::{DebtLedger, AffectedLoan, LoanRef, ObligationStatus};
    use pasion_ledger::SharedReplicatedLedger;
    use pasion_types::{Address, Amount, AssetId, BlockHeight, LedgerId, TxId};

    #[test]
    fn bounty_created_on_withdraw() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.deposit_interest_accrued = Amount(10);
        nft.affected_loans.push(AffectedLoan {
            loan_ref: LoanRef { loan_id: LedgerId([5u8;32]), loaner: Address([6u8;32]) },
            loaner: Address([6u8;32]),
            original_obligation: Amount(50),
            paid_at_start: Amount(0),
            adjusted_obligation: Amount(0),
            obligation_status: ObligationStatus::Active,
            released: false,
        });
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::DepositorWithdraw { tx_id: TxId([7u8;32]), ledger_id, depositor };
        Executor::execute_depositor_withdraw(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.bounties.len(), 1);
    }

    #[test]
    fn bounty_bid_and_expiry() {
        let mut ledger = SharedReplicatedLedger::default();
        let bounty_id = pasion_types::BountyId([9u8;32]);
        let currency = AssetId([4u8;32]);
        let bounty = pasion_debt_ledger::Bounty {
            id: bounty_id,
            source_ledger_id: LedgerId([1u8;32]),
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
            status: pasion_debt_ledger::BountyStatus::Open,
        };
        let auction = pasion_debt_ledger::BountyAuction {
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
        assert_eq!(ledger.bounties.get(&bounty_id).unwrap().status, pasion_debt_ledger::BountyStatus::Settled);
    }

    #[test]
    fn timed_burn() {
        let mut ledger = SharedReplicatedLedger::default();
        let bounty_id = pasion_types::BountyId([12u8;32]);
        let currency = AssetId([4u8;32]);
        let bounty = pasion_debt_ledger::Bounty {
            id: bounty_id,
            source_ledger_id: LedgerId([1u8;32]),
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
            status: pasion_debt_ledger::BountyStatus::ExpiredNoBids,
        };
        ledger.bounties.insert(bounty_id, bounty);
        ledger.assets.insert(currency, Amount(100));
        process_timed_burn(&mut ledger, BlockHeight(1));
        assert_eq!(ledger.bounties.get(&bounty_id).unwrap().burn_remaining, Amount(20));
    }

    #[test]
    fn partial_repay_increases_taker_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let borrower = Address([2u8;32]);
        let mut nft = DebtLedger::new(ledger_id, borrower, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.debt_balance = Amount(100);
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Repay { tx_id: TxId([7u8;32]), borrower, ledger_id, amount: Amount(30) };
        Executor::execute_repay(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.taker_score(borrower), TAKER_PARTIAL_DELTA);
    }

    #[test]
    fn full_repay_increases_taker_score_more() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let borrower = Address([2u8;32]);
        let mut nft = DebtLedger::new(ledger_id, borrower, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.debt_balance = Amount(100);
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Repay { tx_id: TxId([7u8;32]), borrower, ledger_id, amount: Amount(100) };
        Executor::execute_repay(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.taker_score(borrower), TAKER_FULL_BONUS);
    }

    #[test]
    fn withdraw_negligible_increases_lender_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.deposit_interest_accrued = Amount(10);
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::DepositorWithdraw { tx_id: TxId([7u8;32]), ledger_id, depositor };
        Executor::execute_depositor_withdraw(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.lender_score(depositor), LENDER_MATURE_LOST_LOANABILITY_DELTA);
    }

    #[test]
    fn early_withdraw_significant_decreases_lender_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.deposit_interest_accrued = Amount(10);
        nft.created_at_height = BlockHeight(0);
        nft.affected_loans.push(AffectedLoan {
            loan_ref: LoanRef { loan_id: LedgerId([5u8;32]), loaner: Address([6u8;32]) },
            loaner: Address([6u8;32]),
            original_obligation: Amount(40_000_000_000_000_000),
            paid_at_start: Amount(0),
            adjusted_obligation: Amount(0),
            obligation_status: ObligationStatus::Active,
            released: false,
        });
        ledger.ledgers.insert(ledger_id, nft);
        ledger.adjust_lender_score(depositor, 20);
        let tx = Transaction::DepositorWithdraw { tx_id: TxId([7u8;32]), ledger_id, depositor };
        Executor::execute_depositor_withdraw(&mut ledger, &tx).unwrap();
        assert_eq!(ledger.lender_score(depositor), 15);
    }

    #[test]
    fn expired_bounty_no_bids_decreases_lender_score() {
        let mut ledger = SharedReplicatedLedger::default();
        let bounty_id = pasion_types::BountyId([9u8;32]);
        let currency = AssetId([4u8;32]);
        let reward_wallet = Address([2u8;32]);
        let bounty = pasion_debt_ledger::Bounty {
            id: bounty_id,
            source_ledger_id: LedgerId([1u8;32]),
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
            status: pasion_debt_ledger::BountyStatus::Open,
        };
        let auction = pasion_debt_ledger::BountyAuction {
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

    #[test]
    fn ledger_transfer_resets_loanability() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let from = Address([2u8;32]);
        let to = Address([9u8;32]);
        let mut nft = DebtLedger::new(ledger_id, from, AssetId([3u8;32]), Amount(100), AssetId([4u8;32]), 500, BlockHeight(0));
        nft.deposit_loanability = Amount(50);
        nft.total_minted_from_ledger = Amount(50);
        nft.loan_count = 3;
        nft.update_value_factor();
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::LedgerTransfer { tx_id: TxId([7u8;32]), ledger_id, from, to, pay_debts_in_one_tx: true };
        Executor::execute_ledger_transfer(&mut ledger, &tx).unwrap();
        let nft = ledger.ledgers.get(&ledger_id).unwrap();
        assert_eq!(nft.owner, to);
        assert_eq!(nft.deposit_loanability, Amount(70));
        assert_eq!(nft.total_minted_from_ledger, Amount(70));
        assert!(nft.debt_history.iter().any(|e| matches!(e, DebtEvent::LoanabilityReset { .. })));
    }

    #[test]
    fn targeted_deposit_consistency_full() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        nft.deposit_amount = Amount(100);
        nft.deposit_loanability = Amount(70);
        nft.interest_pool = Amount(10);
        nft.available_collateral_backing = Amount(30);
        nft.update_value_factor();
        nft.affected_loans.push(AffectedLoan {
            loan_ref: LoanRef { loan_id: LedgerId([5u8;32]), loaner: Address([6u8;32]) },
            loaner: Address([6u8;32]),
            original_obligation: Amount(50),
            paid_at_start: Amount(10),
            adjusted_obligation: Amount(0),
            obligation_status: ObligationStatus::Active,
            released: false,
        });
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        Executor::execute_deposit(&mut ledger, &tx).unwrap();
        let nft = ledger.ledgers.get(&ledger_id).unwrap();
        // bonus = 50*10/100 * 1.0 = 5
        assert_eq!(nft.deposit_interest_accrued, Amount(5));
        assert!(nft.debt_history.iter().any(|e| matches!(e, DebtEvent::DepositConsistencyInterest { .. })));
    }

    #[test]
    fn targeted_deposit_consistency_partial() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        nft.deposit_amount = Amount(100);
        nft.update_value_factor();
        nft.affected_loans.push(AffectedLoan {
            loan_ref: LoanRef { loan_id: LedgerId([5u8;32]), loaner: Address([6u8;32]) },
            loaner: Address([6u8;32]),
            original_obligation: Amount(50),
            paid_at_start: Amount(10),
            adjusted_obligation: Amount(0),
            obligation_status: ObligationStatus::Active,
            released: false,
        });
        nft.affected_loans.push(AffectedLoan {
            loan_ref: LoanRef { loan_id: LedgerId([8u8;32]), loaner: Address([9u8;32]) },
            loaner: Address([9u8;32]),
            original_obligation: Amount(50),
            paid_at_start: Amount(0),
            adjusted_obligation: Amount(0),
            obligation_status: ObligationStatus::Active,
            released: false,
        });
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(100), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        Executor::execute_deposit(&mut ledger, &tx).unwrap();
        let nft = ledger.ledgers.get(&ledger_id).unwrap();
        // interest_add = 100*10/100=10; score=0.5; bonus=5
        assert_eq!(nft.deposit_interest_accrued, Amount(5));
    }

    #[test]
    fn targeted_deposit_consistency_no_history_cumulative() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        nft.deposit_amount = Amount(100);
        nft.cumulative_repaid = Amount(30);
        nft.debt_balance = Amount(70);
        nft.update_value_factor();
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(100), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        Executor::execute_deposit(&mut ledger, &tx).unwrap();
        let nft = ledger.ledgers.get(&ledger_id).unwrap();
        // score = 30/(30+70)=0.3; interest_add=10; bonus=3
        assert_eq!(nft.deposit_interest_accrued, Amount(3));
    }

    #[test]
    fn targeted_deposit_consistency_none() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        Executor::execute_deposit(&mut ledger, &tx).unwrap();
        let nft = ledger.ledgers.get(&ledger_id).unwrap();
        assert_eq!(nft.deposit_interest_accrued, Amount(0));
    }

    #[test]
    fn targeted_deposit_success() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        nft.loan_count = 2;
        nft.deposit_amount = Amount(100);
        nft.total_minted_from_ledger = Amount(70);
        nft.deposit_loanability = Amount(70);
        nft.interest_pool = Amount(10);
        nft.available_collateral_backing = Amount(30);
        nft.update_value_factor();
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        Executor::execute_deposit(&mut ledger, &tx).unwrap();
        let nft = ledger.ledgers.get(&ledger_id).unwrap();
        assert_eq!(nft.deposit_amount, Amount(150));
        assert_eq!(nft.total_minted_from_ledger, Amount(100)); // 70 + 30
        assert_eq!(nft.deposit_loanability, Amount(100)); // 70 + 30
        assert_eq!(nft.interest_pool, Amount(15)); // 10 + 5
        assert_eq!(nft.available_collateral_backing, Amount(50)); // 30 + 20
        assert_eq!(nft.loan_count, 2);
        assert!(nft.debt_history.iter().any(|e| matches!(e, DebtEvent::Deposit { .. })));
    }

    #[test]
    fn targeted_deposit_missing_target() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        let res = Executor::execute_deposit(&mut ledger, &tx);
        assert!(matches!(res, Err(PasionError::LedgerNotFound)));
    }

    #[test]
    fn targeted_deposit_frozen() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        nft.frozen = true;
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        let res = Executor::execute_deposit(&mut ledger, &tx);
        assert!(matches!(res, Err(PasionError::LedgerFrozen)));
    }

    #[test]
    fn targeted_deposit_burned() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let mut nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        nft.status = LedgerStatus::Burned;
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        let res = Executor::execute_deposit(&mut ledger, &tx);
        assert!(matches!(res, Err(PasionError::CannotTransferBurnedLedger)));
    }

    #[test]
    fn targeted_deposit_depositor_mismatch() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let other = Address([9u8;32]);
        let asset = AssetId([3u8;32]);
        let nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: other, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        let res = Executor::execute_deposit(&mut ledger, &tx);
        assert!(matches!(res, Err(PasionError::InvalidTransaction)));
    }

    #[test]
    fn targeted_deposit_asset_mismatch() {
        let mut ledger = SharedReplicatedLedger::default();
        let ledger_id = LedgerId([1u8;32]);
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let other_asset = AssetId([4u8;32]);
        let nft = DebtLedger::new(ledger_id, depositor, asset, Amount(100), asset, 500, BlockHeight(0));
        ledger.ledgers.insert(ledger_id, nft);
        let tx = Transaction::Deposit { tx_id: TxId([7u8;32]), from: depositor, asset_id: other_asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: Some(ledger_id) };
        let res = Executor::execute_deposit(&mut ledger, &tx);
        assert!(matches!(res, Err(PasionError::InvalidTransaction)));
    }

    #[test]
    fn deposit_creates_new_nft_when_no_target() {
        let mut ledger = SharedReplicatedLedger::default();
        let depositor = Address([2u8;32]);
        let asset = AssetId([3u8;32]);
        let tx_id = TxId([7u8;32]);
        let tx = Transaction::Deposit { tx_id, from: depositor, asset_id: asset, amount: Amount(50), bridge_proof: vec![], target_ledger_id: None };
        Executor::execute_deposit(&mut ledger, &tx).unwrap();
        let ledger_id = LedgerId(blake3::hash(&[depositor.0.as_slice(), tx_id.0.as_slice()].concat()).into());
        assert!(ledger.ledgers.contains_key(&ledger_id));
    }
}


pub fn execute_place_bounty_bid(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> pasion_types::Result<()> {
    Executor::execute_place_bounty_bid(ledger, tx)
}

pub fn execute_yield_split_freeze(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> pasion_types::Result<()> {
    Executor::execute_yield_split_freeze(ledger, tx)
}
pub fn execute_set_minimum_yield_splitters(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> pasion_types::Result<()> {
    Executor::execute_set_minimum_yield_splitters(ledger, tx)
}
pub fn execute_ledger_starter_sale(ledger: &mut SharedReplicatedLedger, tx: &Transaction) -> pasion_types::Result<()> {
    Executor::execute_ledger_starter_sale(ledger, tx)
}
