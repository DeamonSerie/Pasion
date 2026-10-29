pub mod bundler;
pub mod validator;
pub mod executor;

pub struct Node {
    pub is_bundler: bool,
    pub is_validator: bool,
}
