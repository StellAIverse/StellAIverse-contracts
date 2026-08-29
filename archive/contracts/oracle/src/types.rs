use soroban_sdk::{contracttype, Address, BytesN, Symbol, Val, Vec};

#[contracttype]
pub enum DataKey {
    Oracle(BytesN<32>),
    OracleNonce(BytesN<32>),
}

/// Subscription tier controlling access level and pricing
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum SubscriptionTier {
    Basic = 0,
    Standard = 1,
    Premium = 2,
}

/// Admin-defined plan for a specific oracle feed
#[contracttype]
#[derive(Clone, Debug)]
pub struct SubscriptionPlan {
    pub feed_key: Symbol,
    pub tier: SubscriptionTier,
    pub price_per_period: i128, // in stroops
    pub period_seconds: u64,
    pub max_calls_per_period: u32,
    pub active: bool,
}

/// User subscription to an oracle feed
#[contracttype]
#[derive(Clone, Debug)]
pub struct Subscription {
    pub subscriber: Address,
    pub feed_key: Symbol,
    pub tier: SubscriptionTier,
    pub expires_at: u64,
    pub calls_used: u32,
    pub calls_limit: u32,
    pub auto_renew: bool,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct RelayRequest {
    pub relay_contract: Address,
    pub oracle_pubkey: BytesN<32>,
    pub target_contract: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
    pub nonce: u64,
    pub deadline: u64,
}
