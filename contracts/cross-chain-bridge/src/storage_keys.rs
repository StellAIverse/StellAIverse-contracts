use soroban_sdk::{Address, Env, Symbol};

// Storage key constants
pub const ADMIN_KEY: &str = "bridge_admin";
pub const PAUSED_KEY: &str = "bridge_paused";
pub const INITIALIZED_KEY: &str = "bridge_init";
pub const CHAIN_ID_KEY: &str = "chain_id";
pub const TRANSFER_COUNTER_KEY: &str = "tx_counter";

// Validator storage keys
pub const VALIDATOR_COUNT_KEY: &str = "val_count";
pub const VALIDATOR_LIST_KEY: &str = "val_list";
pub const VALIDATOR_PREFIX: &str = "validator_";
pub const SIGNATURE_CONFIG_KEY: &str = "sig_config";

// Token storage keys
pub const TOKEN_COUNT_KEY: &str = "token_count";
pub const TOKEN_PREFIX: &str = "token_";
pub const SUPPORTED_TOKENS_KEY: &str = "sup_tokens";

// Transfer storage keys
pub const TRANSFER_PREFIX: &str = "transfer_";
pub const NONCE_PREFIX: &str = "nonce_";

// Rate limit storage
pub const RATE_LIMIT_CONFIG_KEY: &str = "rate_config";
pub const RATE_LIMIT_STATE_KEY: &str = "rate_state";

// Fee storage
pub const FEE_CONFIG_KEY: &str = "fee_config";
pub const TOTAL_FEES_KEY: &str = "total_fees";

// Helper to create validator storage key
pub fn validator_key(env: &Env, address: &Address) -> (Symbol, Address) {
    (Symbol::new(env, VALIDATOR_PREFIX), address.clone())
}

// Helper to create token storage key
pub fn token_key(env: &Env, token_address: &Address) -> (Symbol, Address) {
    (Symbol::new(env, TOKEN_PREFIX), token_address.clone())
}

// Helper to create transfer storage key
pub fn transfer_key(env: &Env, transfer_id: u64) -> (Symbol, u64) {
    (Symbol::new(env, TRANSFER_PREFIX), transfer_id)
}

// Helper to create nonce storage key
pub fn nonce_key(env: &Env, sender: &Address, nonce: u64) -> (Symbol, Address, u64) {
    (Symbol::new(env, NONCE_PREFIX), sender.clone(), nonce)
}
