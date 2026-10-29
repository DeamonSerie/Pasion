use serde::{Deserialize, Serialize};
use pasion_types::{Address, Amount, AssetId};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub is_external: bool,
    pub total_supply: Amount,
    pub bridge_address: Option<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRegistry {
    pub assets: HashMap<AssetId, Asset>,
}

impl Default for AssetRegistry {
    fn default() -> Self {
        let mut assets = HashMap::new();
        // Create PAI as native
        let pai_id = AssetId([1u8; 32]);
        assets.insert(pai_id, Asset {
            id: pai_id,
            symbol: "PAI".to_string(),
            name: "Pasion".to_string(),
            decimals: 18,
            is_external: false,
            total_supply: Amount(0),
            bridge_address: None,
        });
        AssetRegistry { assets }
    }
}

impl AssetRegistry {
    pub fn register_external_asset(&mut self, symbol: String, name: String, decimals: u8, bridge_address: Address) -> AssetId {
        let id = AssetId(blake3::hash(symbol.as_bytes()).into());
        self.assets.insert(id, Asset {
            id,
            symbol,
            name,
            decimals,
            is_external: true,
            total_supply: Amount(0),
            bridge_address: Some(bridge_address),
        });
        id
    }
    
    pub fn get_asset(&self, id: &AssetId) -> Option<&Asset> {
        self.assets.get(id)
    }
}
