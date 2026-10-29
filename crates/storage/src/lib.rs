use pasion_types::{Hash, Result, PasionError};
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
    
    pub fn put_nft(&self, nft_id: &[u8], nft_bytes: &[u8]) -> Result<()> {
        let key = format!("nft:{}", hex::encode(nft_id));
        self.db.put(key.as_bytes(), nft_bytes)
            .map_err(|e| PasionError::StorageError(e.to_string()))
    }
    
    pub fn get_nft(&self, nft_id: &[u8]) -> Result<Option<Vec<u8>>> {
        let key = format!("nft:{}", hex::encode(nft_id));
        match self.db.get(key.as_bytes()) {
            Ok(v) => Ok(v),
            Err(e) => Err(PasionError::StorageError(e.to_string())),
        }
    }
}
