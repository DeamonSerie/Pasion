use pasion_types::{Hash, Result, PasionError};
use pasion_debt_ledger::{Bounty, BountyAuction};
use pasion_types::BountyId;
use std::path::Path;

pub struct Storage {
    db: rocksdb::DB,
}

impl Storage {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        let db = rocksdb::DB::open(&opts, path)
            .map_err(|e| PasionError::StorageError(e.to_string()))?;
        Ok(Storage { db })
    }
    
    pub fn put_block(&self, height: u64, block_bytes: &[u8]) -> Result<()> {
        let key = format!("block:{}", height);
        self.db.put(key.as_bytes(), block_bytes)
            .map_err(|e| PasionError::StorageError(e.to_string()))
    }
    
    pub fn get_block(&self, height: u64) -> Result<Option<Vec<u8>>> {
        let key = format!("block:{}", height);
        match self.db.get(key.as_bytes()) {
            Ok(Some(v)) => Ok(Some(v)),
            Ok(None) => Ok(None),
            Err(e) => Err(PasionError::StorageError(e.to_string())),
        }
    }
    
    pub fn put_state(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.put(key, value)
            .map_err(|e| PasionError::StorageError(e.to_string()))
    }
    
    pub fn get_state(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.db.get(key) {
            Ok(v) => Ok(v),
            Err(e) => Err(PasionError::StorageError(e.to_string())),
        }
    }
    
    pub fn put_ledger(&self, ledger_id: &[u8], ledger_bytes: &[u8]) -> Result<()> {
        let key = format!("ledger:{}", hex::encode(ledger_id));
        self.db.put(key.as_bytes(), ledger_bytes)
            .map_err(|e| PasionError::StorageError(e.to_string()))
    }
    
    pub fn get_ledger(&self, ledger_id: &[u8]) -> Result<Option<Vec<u8>>> {
        let key = format!("ledger:{}", hex::encode(ledger_id));
        match self.db.get(key.as_bytes()) {
            Ok(v) => Ok(v),
            Err(e) => Err(PasionError::StorageError(e.to_string())),
        }
    }

    pub fn put_bounty(&self, bounty_id: &BountyId, bounty: &Bounty) -> Result<()> {
        let key = format!("bounty:{}", hex::encode(&bounty_id.0));
        let bytes = bincode::serialize(bounty).map_err(|e| PasionError::StorageError(e.to_string()))?;
        self.db.put(key.as_bytes(), &bytes).map_err(|e| PasionError::StorageError(e.to_string()))
    }

    pub fn get_bounty(&self, bounty_id: &BountyId) -> Result<Option<Bounty>> {
        let key = format!("bounty:{}", hex::encode(&bounty_id.0));
        match self.db.get(key.as_bytes()) {
            Ok(Some(v)) => {
                let b: Bounty = bincode::deserialize(&v).map_err(|e| PasionError::StorageError(e.to_string()))?;
                Ok(Some(b))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(PasionError::StorageError(e.to_string())),
        }
    }

    pub fn put_bounty_auction(&self, bounty_id: &BountyId, auction: &BountyAuction) -> Result<()> {
        let key = format!("bounty_auction:{}", hex::encode(&bounty_id.0));
        let bytes = bincode::serialize(auction).map_err(|e| PasionError::StorageError(e.to_string()))?;
        self.db.put(key.as_bytes(), &bytes).map_err(|e| PasionError::StorageError(e.to_string()))
    }

    pub fn get_bounty_auction(&self, bounty_id: &BountyId) -> Result<Option<BountyAuction>> {
        let key = format!("bounty_auction:{}", hex::encode(&bounty_id.0));
        match self.db.get(key.as_bytes()) {
            Ok(Some(v)) => {
                let a: BountyAuction = bincode::deserialize(&v).map_err(|e| PasionError::StorageError(e.to_string()))?;
                Ok(Some(a))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(PasionError::StorageError(e.to_string())),
        }
    }
}
