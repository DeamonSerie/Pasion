use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub [u8; 32]);

impl Address {
    pub fn new(bytes: [u8; 32]) -> Self {
        Address(bytes)
    }
    
    pub fn zero() -> Self {
        Address([0u8; 32])
    }
    
    pub fn from_slice(s: &[u8]) -> Self {
        let mut arr = [0u8; 32];
        let len = s.len().min(32);
        arr[..len].copy_from_slice(&s[..len]);
        Address(arr)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pas_{}", hex::encode(&self.0[..8]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    pub fn new(bytes: [u8; 32]) -> Self {
        Hash(bytes)
    }
    
    pub fn zero() -> Self {
        Hash([0u8; 32])
    }
    
    pub fn from_bytes(b: &[u8]) -> Self {
        let mut arr = [0u8; 32];
        let len = b.len().min(32);
        arr[..len].copy_from_slice(&b[..len]);
        Hash(arr)
    }
    
    pub fn hash(data: &[u8]) -> Self {
        Hash(blake3::hash(data).into())
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.0[..8]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct BlockHeight(pub u64);

impl From<u64> for BlockHeight {
    fn from(v: u64) -> Self {
        BlockHeight(v)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Amount(pub u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NftId(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxId(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BundleId(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BountyId(pub [u8; 32]);

impl BountyId {
    pub fn new(bytes: [u8; 32]) -> Self {
        BountyId(bytes)
    }

    pub fn zero() -> Self {
        BountyId([0u8; 32])
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PasionError {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Invalid transaction")]
    InvalidTransaction,
    #[error("NFT not found")]
    NftNotFound,
    #[error("Debt not paid in full")]
    DebtNotPaidInFull,
    #[error("Liquidation failed")]
    LiquidationFailed,
    #[error("Invalid attestation")]
    InvalidAttestation,
    #[error("Slashing condition met: {0}")]
    Slashing(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid block")]
    InvalidBlock,
    #[error("Invalid auction bid")]
    InvalidBid,
    #[error("Not in bonded set")]
    NotBonded,
    #[error("Cannot transfer burned NFT")]
    CannotTransferBurnedNft,
    #[error("Must pay debt in full")]
    MustPayDebtInFull,
    #[error("Arbitrage failed")]
    ArbitrageFailed,
    #[error("Bounty not found")]
    BountyNotFound,
    #[error("Bounty expired")]
    BountyExpired,
    #[error("Invalid bounty bid")]
    InvalidBountyBid,
    #[error("Bounty already settled")]
    BountyAlreadySettled,
    #[error("NFT is frozen for yield-split governance")]
    NftFrozen,
}

pub type Result<T> = std::result::Result<T, PasionError>;

#[derive(Debug, Clone)]
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
