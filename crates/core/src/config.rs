#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub data_dir: String,
    pub p2p_port: u16,
    pub rpc_port: u16,
    pub is_bundler: bool,
    pub is_validator: bool,
    pub bond_amount: u128,
    pub listen_addr: String,
}
