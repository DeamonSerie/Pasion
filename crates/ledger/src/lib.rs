use serde::{Deserialize, Serialize};
use pasion_types::{Address, Amount, AssetId, BlockHeight, Hash, TxId, NftId, BundleId, BountyId};
use std::collections::HashMap;
use pasion_debt_nft::{DebtNft, Bounty, BountyAuction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    pub root: Hash,
    pub leaves: Vec<Hash>,
}

impl MerkleTree {
    pub fn new(leaves: Vec<Hash>) -> Self {
        if leaves.is_empty() {
            let empty_hash = blake3::hash(&[]);
            return MerkleTree { root: Hash(empty_hash.into()), leaves: vec![Hash(empty_hash.into())] };
        }
        let mut current = leaves.clone();
        while current.len() > 1 {
            let mut next = Vec::new();
            for chunk in current.chunks(2) {
                let h1 = chunk[0];
                let h2 = if chunk.len() == 2 { chunk[1] } else { chunk[0] };
                let mut combined = Vec::new();
                combined.extend_from_slice(&h1.0);
                combined.extend_from_slice(&h2.0);
                let hash = blake3::hash(&combined);
                next.push(Hash(hash.into()));
            }
            current = next;
        }
        MerkleTree { root: current[0], leaves }
    }
    
    pub fn verify_proof(&self, leaf: &Hash, proof: &[Hash]) -> bool {
        let mut current = *leaf;
        for p in proof {
            let mut combined = Vec::new();
            combined.extend_from_slice(&current.0);
            combined.extend_from_slice(&p.0);
            current = Hash(blake3::hash(&combined).into());
        }
        current == self.root
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRelevantState {
    pub height: BlockHeight,
    pub touched_accounts: Vec<Address>,
    pub touched_assets: Vec<AssetId>,
    pub touched_nfts: Vec<NftId>,
    pub state_roots: HashMap<String, Hash>,
    pub merkle_tree: MerkleTree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Transaction {
    Deposit {
        tx_id: TxId,
        from: Address,
        asset_id: AssetId,
        amount: Amount,
        bridge_proof: Vec<u8>,
    },
    Loan {
        tx_id: TxId,
        borrower: Address,
        nft_id: NftId,
        amount: Amount,
        currency_id: AssetId,
    },
    Repay {
        tx_id: TxId,
        borrower: Address,
        nft_id: NftId,
        amount: Amount,
    },
    Liquidate {
        tx_id: TxId,
        nft_id: NftId,
        liquidator: Address,
    },
    NftTransfer {
        tx_id: TxId,
        nft_id: NftId,
        from: Address,
        to: Address,
        pay_debts_in_one_tx: bool,
    },
    ArbBorrow {
        tx_id: TxId,
        arbitrageur: Address,
        debt_nft_id: NftId,
        pool_id: NftId,
    },
    DepositorWithdraw {
        tx_id: TxId,
        nft_id: NftId,
        depositor: Address,
    },
    PlaceBountyBid {
        tx_id: TxId,
        bounty_id: BountyId,
        bidder: Address,
        amount: Amount,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub height: BlockHeight,
    pub parent_hash: Hash,
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,
    pub state_root: Hash,
    pub receipts_root: Hash,
    pub bundle_id: BundleId,
    pub bundler: Address,
    pub validator: Address,
    pub committee_attestations: Vec<Vec<u8>>,
    pub base_fee: Amount,
    pub pai_reward: Amount,
    pub gas_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub address: Address,
    pub balances: HashMap<AssetId, Amount>,
    pub nonce: u64,
    pub bond: Amount,
    pub bonded_role: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SharedReplicatedLedger {
    pub blocks: Vec<Block>,
    pub state: HashMap<Address, AccountState>,
    pub assets: HashMap<AssetId, Amount>,
    pub nfts: HashMap<NftId, DebtNft>,
    pub bounties: HashMap<BountyId, Bounty>,
    pub bounty_auctions: HashMap<BountyId, BountyAuction>,
    pub bonded_set: HashMap<Address, (Amount, String)>,
    pub reputation: HashMap<Address, i64>,
}

impl Default for SharedReplicatedLedger {
    fn default() -> Self {
        SharedReplicatedLedger {
            blocks: Vec::new(),
            state: HashMap::new(),
            assets: HashMap::new(),
            nfts: HashMap::new(),
            bounties: HashMap::new(),
            bounty_auctions: HashMap::new(),
            bonded_set: HashMap::new(),
            reputation: HashMap::new(),
        }
    }
}

impl SharedReplicatedLedger {
    pub fn append_block(&mut self, block: Block) -> pasion_types::Result<()> {
        if let Some(last) = self.blocks.last() {
            if block.height.0 != last.height.0 + 1 {
                return Err(pasion_types::PasionError::InvalidBlock);
            }
        }
        self.blocks.push(block);
        Ok(())
    }
    
    pub fn compute_state_root(&self) -> Hash {
        let mut data = Vec::new();
        data.extend_from_slice(&(self.blocks.len() as u64).to_le_bytes());
        data.extend_from_slice(&(self.nfts.len() as u64).to_le_bytes());
        data.extend_from_slice(&(self.state.len() as u64).to_le_bytes());
        Hash(blake3::hash(&data).into())
    }
    
    pub fn get_block_relevant_state(&self, txs: &[Transaction]) -> BlockRelevantState {
        let mut touched_accounts = Vec::new();
        let mut touched_assets = Vec::new();
        let mut touched_nfts = Vec::new();
        for tx in txs {
            match tx {
                Transaction::Deposit { from, asset_id, .. } => {
                    touched_accounts.push(*from);
                    touched_assets.push(*asset_id);
                }
                Transaction::Loan { borrower, nft_id, currency_id, .. } => {
                    touched_accounts.push(*borrower);
                    touched_nfts.push(*nft_id);
                    touched_assets.push(*currency_id);
                }
                Transaction::Repay { borrower, nft_id, .. } => {
                    touched_accounts.push(*borrower);
                    touched_nfts.push(*nft_id);
                }
                Transaction::Liquidate { nft_id, liquidator, .. } => {
                    touched_accounts.push(*liquidator);
                    touched_nfts.push(*nft_id);
                }
                Transaction::NftTransfer { from, to, nft_id, .. } => {
                    touched_accounts.push(*from);
                    touched_accounts.push(*to);
                    touched_nfts.push(*nft_id);
                }
                Transaction::ArbBorrow { arbitrageur, debt_nft_id, pool_id, .. } => {
                    touched_accounts.push(*arbitrageur);
                    touched_nfts.push(*debt_nft_id);
                    touched_nfts.push(*pool_id);
                }
                Transaction::DepositorWithdraw { depositor, nft_id, .. } => {
                    touched_accounts.push(*depositor);
                    touched_nfts.push(*nft_id);
                }
                Transaction::PlaceBountyBid { bidder, .. } => {
                    touched_accounts.push(*bidder);
                }
            }
        }
        touched_accounts.sort_by(|a,b| a.0.iter().cmp(b.0.iter()));
        touched_accounts.dedup_by(|a,b| a.0 == b.0);
        touched_nfts.sort_by(|a,b| a.0.iter().cmp(b.0.iter()));
        touched_nfts.dedup_by(|a,b| a.0 == b.0);
        touched_assets.sort_by(|a,b| a.0.iter().cmp(b.0.iter()));
        touched_assets.dedup_by(|a,b| a.0 == b.0);
        
        let mut leaves = Vec::new();
        for acc in &touched_accounts {
            leaves.push(Hash(blake3::hash(&acc.0).into()));
        }
        for nft in &touched_nfts {
            leaves.push(Hash(blake3::hash(&nft.0).into()));
        }
        if leaves.is_empty() {
            leaves.push(Hash(blake3::hash(&[]).into()));
        }
        
        BlockRelevantState {
            height: BlockHeight(self.blocks.len() as u64),
            touched_accounts,
            touched_assets,
            touched_nfts,
            state_roots: HashMap::new(),
            merkle_tree: MerkleTree::new(leaves),
        }
    }
}

impl SharedReplicatedLedger {
    pub fn process_bounty_expiry<H: Into<BlockHeight>>(&mut self, height: H) {
        let height = height.into();
        let ids: Vec<BountyId> = self.bounties.iter().filter(|(_, b)| b.end_height == height).map(|(id, _)| *id).collect();
        for id in ids {
            let highest = self.bounty_auctions.get(&id).and_then(|a| a.highest.clone());
            if let Some(h) = highest {
                if let Some(bounty) = self.bounties.get(&id) {
                    let currency_id = bounty.currency_id;
                    let current_amount = bounty.current_amount;
                    let avg_interest = bounty.avg_interest;
                    let reward_wallet = bounty.reward_wallet;
                    let winner = h.bidder;
                    {
                        let entry = self.assets.entry(currency_id).or_insert(Amount(0));
                        entry.0 = entry.0.saturating_add(current_amount.0);
                    }
                    {
                        let acc = self.state.entry(winner).or_insert_with(|| AccountState {
                            address: winner, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                        });
                        acc.balances.entry(currency_id).and_modify(|e| e.0 += current_amount.0).or_insert(current_amount);
                    }
                    let pai_id = AssetId([1u8; 32]);
                    let pay_amount = avg_interest;
                    {
                        if let Some(acc) = self.state.get_mut(&winner) {
                            if let Some(bal) = acc.balances.get_mut(&pai_id) {
                                bal.0 = bal.0.saturating_sub(pay_amount.0);
                            }
                        }
                    }
                    {
                        let acc = self.state.entry(reward_wallet).or_insert_with(|| AccountState {
                            address: reward_wallet, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                        });
                        acc.balances.entry(pai_id).and_modify(|e| e.0 += pay_amount.0).or_insert(pay_amount);
                    }
                    if let Some(b) = self.bounties.get_mut(&id) {
                        b.winner = Some(winner);
                        b.winning_bid = Some(h.amount);
                        b.status = pasion_debt_nft::BountyStatus::Settled;
                    }
                }
            } else {
                let (original_amount, _currency_id) = {
                    if let Some(b) = self.bounties.get(&id) {
                        (b.original_amount, b.currency_id)
                    } else { (Amount(0), AssetId([0u8;32])) }
                };
                let dist_total = Amount(original_amount.0 * 20 / 100);
                let wallets: Vec<Address> = self.state.keys().cloned().collect();
                let n = wallets.len().max(1) as u128;
                let per = Amount(dist_total.0 / n);
                let pai_id = AssetId([1u8; 32]);
                for w in wallets {
                    let acc = self.state.entry(w).or_insert_with(|| AccountState {
                        address: w, balances: std::collections::HashMap::new(), nonce: 0, bond: Amount(0), bonded_role: None,
                    });
                    acc.balances.entry(pai_id).and_modify(|e| e.0 += per.0).or_insert(per);
                }
                {
                    let entry = self.assets.entry(pai_id).or_insert(Amount(0));
                    entry.0 = entry.0.saturating_add(dist_total.0);
                }
                let burn_remaining = Amount(original_amount.0.saturating_sub(dist_total.0));
                if let Some(b) = self.bounties.get_mut(&id) {
                    b.burn_remaining = burn_remaining;
                    b.status = pasion_debt_nft::BountyStatus::ExpiredNoBids;
                }
            }
        }
    }

    pub fn process_timed_burn<H: Into<BlockHeight>>(&mut self, height: H) {
        let height = height.into();
        let ids: Vec<BountyId> = self.bounties.iter().filter(|(_, b)| b.status == pasion_debt_nft::BountyStatus::ExpiredNoBids && b.burn_remaining.0 > 0).map(|(id, _)| *id).collect();
        for id in ids {
            let currency_id = {
                let b = self.bounties.get(&id).unwrap();
                b.currency_id
            };
            let burn_per_block = {
                let b = self.bounties.get(&id).unwrap();
                b.burn_per_block
            };
            let burn_amt = {
                let b = self.bounties.get(&id).unwrap();
                std::cmp::min(burn_per_block.0, b.burn_remaining.0)
            };
            if burn_amt > 0 {
                {
                    let entry = self.assets.entry(currency_id).or_insert(Amount(0));
                    entry.0 = entry.0.saturating_sub(burn_amt);
                }
                if let Some(b) = self.bounties.get_mut(&id) {
                    b.burn_remaining.0 = b.burn_remaining.0.saturating_sub(burn_amt);
                    if b.burn_remaining.0 == 0 {
                        b.status = pasion_debt_nft::BountyStatus::Burned;
                    }
                }
            }
        }
    }
}

pub fn process_bounty_expiry<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
    ledger.process_bounty_expiry(height);
}

pub fn process_timed_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
    ledger.process_timed_burn(height);
}

pub fn process_bounty_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
    ledger.process_timed_burn(height);
}

pub fn process_burn<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
    ledger.process_timed_burn(height);
}

pub fn process_bounties<H: Into<BlockHeight>>(ledger: &mut SharedReplicatedLedger, height: H) {
        let height = height.into();
    ledger.process_bounty_expiry(height);
    ledger.process_timed_burn(height);
}
