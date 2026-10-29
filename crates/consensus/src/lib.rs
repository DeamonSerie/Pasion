use serde::{Deserialize, Serialize};
use pasion_types::{Address, Amount, BlockHeight, BundleId, Hash, Result, PasionError};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedBid {
    pub bidder: Address,
    pub commit_hash: Hash,
    pub role: NodeRole,
    pub bond: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeRole {
    Bundler,
    Validator,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealedBid {
    pub bidder: Address,
    pub bid_amount: Amount,
    pub role: NodeRole,
    pub nonce: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlePair {
    pub bundler: Address,
    pub validator: Address,
    pub bid_amount: Amount,
    pub selected_at_height: BlockHeight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitteeMember {
    pub address: Address,
    pub reputation: i64,
    pub uptime_score: f64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub validator: Address,
    pub bundle_id: BundleId,
    pub block_hash: Hash,
    pub valid: bool,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuctionState {
    pub phase: AuctionPhase,
    pub height: BlockHeight,
    pub commits: HashMap<Address, SealedBid>,
    pub reveals: HashMap<Address, RevealedBid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuctionPhase {
    Commit,
    Reveal,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusState {
    pub current_height: BlockHeight,
    pub active_pair: Option<BundlePair>,
    pub committee: Vec<CommitteeMember>,
    pub reputation: HashMap<Address, i64>,
    pub bonded_set: HashMap<Address, (Amount, NodeRole)>,
    pub auction: Option<AuctionState>,
}

impl Default for ConsensusState {
    fn default() -> Self {
        ConsensusState {
            current_height: BlockHeight(0),
            active_pair: None,
            committee: Vec::new(),
            reputation: HashMap::new(),
            bonded_set: HashMap::new(),
            auction: None,
        }
    }
}

impl ConsensusState {
    pub fn start_auction(&mut self, height: BlockHeight) {
        self.auction = Some(AuctionState {
            phase: AuctionPhase::Commit,
            height,
            commits: HashMap::new(),
            reveals: HashMap::new(),
        });
    }
    
    pub fn advance_phase(&mut self) {
        if let Some(ref mut auction) = self.auction {
            match auction.phase {
                AuctionPhase::Commit => {
                    auction.phase = AuctionPhase::Reveal;
                }
                AuctionPhase::Reveal => {
                    let height = auction.height;
                    let reveals: Vec<(Address, Amount)> = auction.reveals.iter().map(|(a,b)| (*a, b.bid_amount)).collect();
                    if !reveals.is_empty() {
                        let mut candidates = reveals.clone();
                        candidates.sort_by(|a,b| b.1.0.cmp(&a.1.0));
                        let bundler = candidates[0].0;
                        let bid_amt = candidates[0].1;
                        let mut validator: Option<Address> = None;
                        for (addr, _) in candidates.iter().skip(1) {
                            if *addr != bundler {
                                validator = Some(*addr);
                                break;
                            }
                        }
                        if let Some(val) = validator {
                            if val != bundler {
                                self.active_pair = Some(BundlePair {
                                    bundler,
                                    validator: val,
                                    bid_amount: bid_amt,
                                    selected_at_height: height,
                                });
                            }
                        }
                    }
                    auction.phase = AuctionPhase::Complete;
                }
                AuctionPhase::Complete => {}
            }
        }
    }
    
    pub fn commit_bid(&mut self, bid: SealedBid) -> Result<()> {
        if let Some(ref mut auction) = self.auction {
            if !matches!(auction.phase, AuctionPhase::Commit) {
                return Err(PasionError::InvalidBid);
            }
            if !self.bonded_set.contains_key(&bid.bidder) {
                return Err(PasionError::NotBonded);
            }
            auction.commits.insert(bid.bidder, bid);
            Ok(())
        } else {
            Err(PasionError::InvalidBid)
        }
    }
    
    pub fn reveal_bid(&mut self, bid: RevealedBid) -> Result<()> {
        if let Some(ref mut auction) = self.auction {
            if !matches!(auction.phase, AuctionPhase::Reveal) {
                return Err(PasionError::InvalidBid);
            }
            if !auction.commits.contains_key(&bid.bidder) {
                return Err(PasionError::InvalidBid);
            }
            auction.reveals.insert(bid.bidder, bid);
            Ok(())
        } else {
            Err(PasionError::InvalidBid)
        }
    }
    
    pub fn select_pair(&mut self) -> Result<Option<BundlePair>> {
        if let Some(ref mut auction) = self.auction {
            if auction.reveals.is_empty() {
                return Ok(None);
            }
            let mut candidates: Vec<(Address, Amount)> = auction.reveals.iter().map(|(a,b)| (*a, b.bid_amount)).collect();
            candidates.sort_by(|a,b| b.1.0.cmp(&a.1.0));
            if candidates.is_empty() { return Ok(None); }
            let bundler = candidates[0].0;
            let bid_amt = candidates[0].1;
            let mut validator: Option<Address> = None;
            for (addr, _) in candidates.iter().skip(1) {
                if *addr != bundler {
                    validator = Some(*addr);
                    break;
                }
            }
            if let Some(val) = validator {
                if val == bundler { return Ok(None); }
                let pair = BundlePair {
                    bundler,
                    validator: val,
                    bid_amount: bid_amt,
                    selected_at_height: auction.height,
                };
                self.active_pair = Some(pair.clone());
                auction.phase = AuctionPhase::Complete;
                return Ok(Some(pair));
            }
            Ok(None)
        } else {
            Ok(None)
        }
    }
    
    pub fn select_committee(&mut self, exclude: &[Address]) -> Vec<CommitteeMember> {
        let mut candidates: Vec<(Address, i64)> = self.reputation
            .iter()
            .filter(|(addr, _)| !exclude.contains(addr))
            .map(|(a, r)| (*a, *r))
            .collect();
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        candidates.truncate(4);
        candidates.into_iter().map(|(addr, rep)| CommitteeMember {
            address: addr,
            reputation: if rep < 0 { 0i64 } else { rep },
            uptime_score: 1.0,
            success_rate: 1.0,
        }).collect()
    }
    
    pub fn apply_reputation_decay(&mut self) {
        for (_addr, rep) in self.reputation.iter_mut() {
            *rep = ((*rep as f64) * 0.99) as i64;
            if *rep < 0 { *rep = 0; }
        }
    }
    
    pub fn update_reputation(&mut self, addr: Address, delta: i64) {
        let entry = self.reputation.entry(addr).or_insert(1000i64);
        *entry += delta;
        if *entry > 10000 { *entry = 10000; }
        if *entry < 0 { *entry = 0; }
    }
    
    pub fn slash(&mut self, addr: Address, severity_bps: u64) -> Result<Amount> {
        if let Some((bond, _role)) = self.bonded_set.get_mut(&addr) {
            let slash_amount = Amount((bond.0 * (severity_bps as u128)) / 10000);
            bond.0 = bond.0.saturating_sub(slash_amount.0);
            if severity_bps >= 10000 {
                self.bonded_set.remove(&addr);
                self.update_reputation(addr, -5000);
                return Err(PasionError::Slashing("Permanent removal from bonded set".to_string()));
            }
            self.update_reputation(addr, - (severity_bps as i64));
            Ok(slash_amount)
        } else {
            Err(PasionError::NotBonded)
        }
    }
}
