use soroban_sdk::{contracttype, Address, Symbol};

/// Storage keys used by the oracle contract.
///
/// Keys are structured enums rather than flattened strings: Soroban
/// symbols cap at 32 bytes, which a composite key like
/// `<prefix>_<feed>_<provider>` blows straight past once a real address is
/// embedded, and typed variants keep every lookup collision-free by
/// construction instead of by naming discipline.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    // Instance configuration
    Admin,
    Treasury,
    ProviderCount,
    FeedCount,
    DistributionCount,
    BaseRewardRate,

    // Records
    /// An oracle provider, keyed by its address.
    Provider(Address),
    /// A price feed configuration, keyed by feed id.
    PriceFeed(Symbol),
    /// The most recent price accepted for a feed, keyed by feed id.
    LatestPrice(Symbol),
    /// The most recent price a specific provider submitted to a feed.
    ///
    /// Aggregation needs one entry per provider, while the feed-level
    /// latest price drives update-interval and staleness checks; keeping
    /// the two key spaces separate lets each reader cost one lookup.
    ProviderPrice(Symbol, Address),
    /// Historical prices for a feed, keyed by feed id.
    PriceHistory(Symbol),
    /// Circuit breaker state for a feed.
    CircuitBreaker(Symbol),
    /// Fallback provider configuration for a feed.
    FallbackConfig(Symbol),
    /// Health metrics for a provider.
    ProviderHealth(Address),
    /// A non-price data feed.
    CustomFeed(Symbol),
    /// Submitted entries of a custom data feed.
    CustomData(Symbol),
    /// A user's subscription to a feed.
    Subscription(Address, Symbol),
    /// Query rate-limit bookkeeping for a user.
    RateLimit(Address),
    /// Withdrawable incentive balance for a provider.
    IncentiveBalance(Address),
}

// Key helpers. Each returns the [`StorageKey`] variant the rest of the
// contract stores and reads through, so call sites stay short and the
// key layout lives in exactly one place.

pub fn get_admin_key() -> StorageKey {
    StorageKey::Admin
}

pub fn get_treasury_key() -> StorageKey {
    StorageKey::Treasury
}

pub fn get_provider_count_key() -> StorageKey {
    StorageKey::ProviderCount
}

pub fn get_feed_count_key() -> StorageKey {
    StorageKey::FeedCount
}

pub fn get_distribution_count_key() -> StorageKey {
    StorageKey::DistributionCount
}

pub fn get_base_reward_rate_key() -> StorageKey {
    StorageKey::BaseRewardRate
}

pub fn get_provider_key(provider: &Address) -> StorageKey {
    StorageKey::Provider(provider.clone())
}

pub fn get_feed_key(feed_id: &Symbol) -> StorageKey {
    StorageKey::PriceFeed(feed_id.clone())
}

pub fn get_latest_price_key(feed_id: &Symbol) -> StorageKey {
    StorageKey::LatestPrice(feed_id.clone())
}

pub fn get_provider_price_key(feed_id: &Symbol, provider: &Address) -> StorageKey {
    StorageKey::ProviderPrice(feed_id.clone(), provider.clone())
}

pub fn get_price_history_key(feed_id: &Symbol) -> StorageKey {
    StorageKey::PriceHistory(feed_id.clone())
}

pub fn get_circuit_breaker_key(feed_id: &Symbol) -> StorageKey {
    StorageKey::CircuitBreaker(feed_id.clone())
}

pub fn get_fallback_config_key(feed_id: &Symbol) -> StorageKey {
    StorageKey::FallbackConfig(feed_id.clone())
}

pub fn get_provider_health_key(provider: &Address) -> StorageKey {
    StorageKey::ProviderHealth(provider.clone())
}

pub fn get_custom_feed_key(feed_id: &Symbol) -> StorageKey {
    StorageKey::CustomFeed(feed_id.clone())
}

pub fn get_custom_data_key(feed_id: &Symbol) -> StorageKey {
    StorageKey::CustomData(feed_id.clone())
}

pub fn get_subscription_key(subscriber: &Address, feed_id: &Symbol) -> StorageKey {
    StorageKey::Subscription(subscriber.clone(), feed_id.clone())
}

pub fn get_rate_limit_key(user: &Address) -> StorageKey {
    StorageKey::RateLimit(user.clone())
}

pub fn get_incentive_balance_key(provider: &Address) -> StorageKey {
    StorageKey::IncentiveBalance(provider.clone())
}
