use serde::{Deserialize, Serialize};
use pasion_types::BlockHeight;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub method: String,
    pub params: serde_json::Value,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub id: u64,
}

pub struct RpcServer {
    port: u16,
    running: bool,
    chain_height: BlockHeight,
}

impl RpcServer {
    pub fn new(port: u16) -> Self {
        RpcServer { port, running: false, chain_height: BlockHeight(0) }
    }
    
    pub fn start(&mut self) -> pasion_types::Result<()> {
        self.running = true;
        Ok(())
    }
    
    pub fn stop(&mut self) {
        self.running = false;
    }
    
    pub fn set_height(&mut self, h: BlockHeight) {
        self.chain_height = h;
    }
    
    pub fn call(&self, req: RpcRequest) -> RpcResponse {
        if !self.running && req.method != "pasion_start" && req.method != "pasion_isRunning" {
            return RpcResponse {
                result: None,
                error: Some("RPC server not running".to_string()),
                id: req.id,
            };
        }
        match req.method.as_str() {
            "pasion_getBlockByHeight" => {
                let height = req.params.get("height").and_then(|v| v.as_u64()).unwrap_or(self.chain_height.0);
                RpcResponse {
                    result: Some(serde_json::json!({"height": height, "transactions": [], "state_root": "0x0"})),
                    error: None,
                    id: req.id,
                }
            },
            "pasion_getBalance" => RpcResponse {
                result: Some(serde_json::json!({"balance": "0", "nonce": 0})),
                error: None,
                id: req.id,
            },
            "pasion_getLedger" => RpcResponse {
                result: Some(serde_json::json!({"ledger": null})),
                error: None,
                id: req.id,
            },
            "pasion_getChainInfo" => RpcResponse {
                result: Some(serde_json::json!({"chain_id": "pasion-1", "height": self.chain_height.0, "consensus": "CustomAttestedBundle"})),
                error: None,
                id: req.id,
            },
            "pasion_submitTransaction" => {
                let tx_hash = blake3::hash(&[]);
                RpcResponse {
                    result: Some(serde_json::json!({"tx_hash": hex::encode(tx_hash.as_bytes()), "accepted": true})),
                    error: None,
                    id: req.id,
                }
            },
            "pasion_isRunning" => RpcResponse {
                result: Some(serde_json::json!({"running": self.running})),
                error: None,
                id: req.id,
            },
            "pasion_getPeers" => RpcResponse {
                result: Some(serde_json::json!({"peers": []})),
                error: None,
                id: req.id,
            },
            _ => RpcResponse {
                result: None,
                error: Some(format!("Method '{}' not found", req.method)),
                id: req.id,
            },
        }
    }
}
