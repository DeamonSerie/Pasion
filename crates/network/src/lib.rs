use pasion_types::{Address, Result};
use std::collections::HashMap;

pub struct P2PNetwork {
    peers: Vec<Address>,
    listening: bool,
    port: u16,
    messages_sent: u64,
    message_handlers: HashMap<String, u64>,
}

impl P2PNetwork {
    pub fn new() -> Self {
        P2PNetwork {
            peers: Vec::new(),
            listening: false,
            port: 0,
            messages_sent: 0,
            message_handlers: HashMap::new(),
        }
    }
    
    pub fn start(&mut self, port: u16) -> Result<()> {
        self.port = port;
        self.listening = true;
        Ok(())
    }
    
    pub fn stop(&mut self) {
        self.listening = false;
    }
    
    pub fn add_peer(&mut self, addr: Address) {
        if !self.peers.contains(&addr) {
            self.peers.push(addr);
        }
    }
    
    pub fn remove_peer(&mut self, addr: &Address) {
        self.peers.retain(|p| p != addr);
    }
    
    pub fn broadcast_block(&mut self, block_bytes: &[u8]) -> Result<()> {
        self.messages_sent += 1;
        *self.message_handlers.entry("block".to_string()).or_insert(0) += 1;
        for _peer in &self.peers {
            let _ = block_bytes;
        }
        Ok(())
    }
    
    pub fn broadcast_tx(&mut self, tx_bytes: &[u8]) -> Result<()> {
        self.messages_sent += 1;
        *self.message_handlers.entry("tx".to_string()).or_insert(0) += 1;
        for _peer in &self.peers {
            let _ = tx_bytes;
        }
        Ok(())
    }
    
    pub fn gossip_attestation(&mut self, attestation_bytes: &[u8]) -> Result<()> {
        self.messages_sent += 1;
        *self.message_handlers.entry("attestation".to_string()).or_insert(0) += 1;
        for _peer in &self.peers {
            let _ = attestation_bytes;
        }
        Ok(())
    }
    
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
    
    pub fn is_listening(&self) -> bool {
        self.listening
    }
}

impl Default for P2PNetwork {
    fn default() -> Self {
        Self::new()
    }
}
