use soroban_sdk::{contracttype, Address, String, Symbol, Vec};

/// Supported oracle providers
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum OracleProviderType {
    Chainlink = 0,
    Pyth = 1,
    Band = 2,
    Custom = 3,
}

/// Provider configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct OracleProvider {
    pub address: Address,
    pub provider_type: OracleProviderType,
    pub is_primary: bool,
    pub is_active: bool,
    pub reputation_score: u32,
    pub total_updates: u64,
    pub successful_updates: u64,
    pub staked_amount: i128,
    pub created_at: u64,
}

/// Price feed configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceFeed {
    pub feed_id: Symbol,
    pub asset_symbol: String,
    pub description: String,
    pub decimals: u32,
    pub min_update_interval: u64,
    pub max_staleness: u64,
    pub is_active: bool,
    pub circuit_breaker_enabled: bool,
    pub max_price_change_bps: u32, // Maximum percentage change in basis points
    pub providers: Vec<Address>,   // Authorized providers for this feed
    pub created_at: u64,
}

/// Price entry with metadata
#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceEntry {
    pub price: i128,
    pub timestamp: u64,
    pub provider: Address,
    pub provider_type: OracleProviderType,
    pub block_number: u32,
}

/// Historical price record
#[contracttype]
#[derive(Clone, Debug)]
pub struct HistoricalPrice {
    pub price: i128,
    pub timestamp: u64,
    pub block_number: u32,
    pub aggregated: bool,
}

/// Aggregated price result
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AggregatedPrice {
    pub price: i128,
    pub timestamp: u64,
    pub sources_used: u32,
    pub min_price: i128,
    pub max_price: i128,
    pub median_price: i128,
    pub is_fresh: bool,
}

/// Circuit breaker state
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircuitBreakerState {
    pub triggered: bool,
    pub triggered_at: Option<u64>,
    pub trigger_price: Option<i128>,
    pub previous_price: Option<i128>,
    pub price_change_bps: Option<u32>,
    pub cooldown_period: u64,
    pub can_resume_at: Option<u64>,
}

/// Rate limit configuration for querying
#[contracttype]
#[derive(Clone, Debug)]
pub struct QueryRateLimit {
    pub window_seconds: u64,
    pub max_queries: u32,
    pub queries_used: u32,
    pub window_start: u64,
}

/// User subscription for feed access
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeedSubscription {
    pub subscriber: Address,
    pub feed_id: Symbol,
    pub tier: SubscriptionTier,
    pub expires_at: u64,
    pub queries_remaining: u32,
    pub auto_renew: bool,
    pub created_at: u64,
}

/// Subscription tier
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum SubscriptionTier {
    Free = 0,
    Basic = 1,
    Premium = 2,
    Unlimited = 3,
}

/// Incentive distribution record
#[contracttype]
#[derive(Clone, Debug)]
pub struct IncentiveDistribution {
    pub distribution_id: u64,
    pub provider: Address,
    pub amount: i128,
    pub feed_id: Symbol,
    pub timestamp: u64,
    pub reward_type: RewardType,
}

/// Reward types for oracle operators
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum RewardType {
    UpdateReward = 0,
    QualityReward = 1,
    Penalty = 2,
}

/// Custom data feed for non-price data
#[contracttype]
#[derive(Clone, Debug)]
pub struct CustomDataFeed {
    pub feed_id: Symbol,
    pub description: String,
    pub data_type: String,
    pub is_active: bool,
    pub authorized_providers: Vec<Address>,
    pub last_update: u64,
    pub max_staleness: u64,
}

/// Custom data entry
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CustomDataEntry {
    pub data: String,
    pub timestamp: u64,
    pub provider: Address,
    pub block_number: u32,
}

/// Fallback configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct FallbackConfig {
    pub enabled: bool,
    pub fallback_providers: Vec<Address>,
    pub primary_failure_threshold: u32,
    pub current_failures: u32,
    pub using_fallback: bool,
}

/// Provider health metrics
#[contracttype]
#[derive(Clone, Debug)]
pub struct ProviderHealth {
    pub provider: Address,
    pub consecutive_failures: u32,
    pub last_successful_update: u64,
    pub price_deviation_count: u32,
    pub availability_score: u32,
}
