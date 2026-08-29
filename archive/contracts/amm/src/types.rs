use soroban_sdk::{contracttype, Address, String, Vec};

/// Storage keys for the AMM contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address.
    Admin,
    /// Pool counter for generating unique pool IDs.
    PoolCounter,
    /// Pool data keyed by pool ID.
    Pool(u64),
    /// LP token balance: (pool_id, provider).
    LpBalance(u64, Address),
    /// Cached price data for pool (Issue #215)
    PriceCache(u64),
    /// Cached reserve data for pool (Issue #215)
    ReserveCache(u64),
    /// Timestamp of last cache invalidation (Issue #215)
    LastCacheInvalidation,
    /// Route counter for generating unique route IDs.
    RouteCounter,
    /// Route data keyed by route ID.
    Route(u64),
    /// Trading pause state.
    TradingPaused,
    /// Circuit breaker state.
    CircuitBreakerActive,
    /// Risk parameters for position limits.
    RiskParams,
    /// User position tracking.
    UserPosition(Address, Address), // (user, token)
    /// Minimum LP token threshold.
    MinLpTokenThreshold,
}

/// Represents a liquidity pool for a token pair.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Pool {
    /// Unique pool identifier.
    pub pool_id: u64,
    /// Address of token A.
    pub token_a: Address,
    /// Address of token B.
    pub token_b: Address,
    /// Reserve of token A in the pool.
    pub reserve_a: i128,
    /// Reserve of token B in the pool.
    pub reserve_b: i128,
    /// Total supply of LP tokens for this pool.
    pub lp_total_supply: i128,
    /// Fee in basis points (e.g. 30 = 0.30%).
    pub fee_bps: u32,
}

/// Represents a single hop in a multi-hop swap route.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Hop {
    /// Pool ID for this hop.
    pub pool_id: u64,
    /// Token input for this hop.
    pub token_in: Address,
    /// Token output for this hop.
    pub token_out: Address,
    /// Expected input amount for this hop.
    pub amount_in: i128,
    /// Minimum output amount (slippage protection).
    pub min_amount_out: i128,
}

/// Represents a complete multi-hop swap route.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Route {
    /// Token being swapped from.
    pub token_in: Address,
    /// Token being swapped to.
    pub token_out: Address,
    /// Input amount.
    pub amount_in: i128,
    /// Expected output amount.
    pub amount_out: i128,
    /// Total fee in basis points.
    pub total_fee_bps: u32,
    /// Sequence of hops to execute.
    pub hops: Vec<Hop>,
    /// Route creation timestamp.
    pub created_at: u64,
}

/// Risk management parameters for position limits.
#[contracttype]
#[derive(Clone, Debug)]
pub struct RiskParams {
    /// Maximum position size per user.
    pub max_position_per_user: i128,
    /// Maximum position size per asset.
    pub max_position_per_asset: i128,
    /// Portfolio concentration threshold (basis points, e.g., 3000 = 30%).
    pub concentration_threshold_bps: u32,
    /// Circuit breaker threshold for price moves (basis points, e.g., 1500 = 15%).
    pub circuit_breaker_threshold_bps: u32,
    /// Circuit breaker cooldown period in seconds.
    pub circuit_breaker_cooldown: u64,
    /// Minimum LP token minting threshold.
    pub min_lp_token_threshold: i128,
}

/// User position tracking for risk management.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserPosition {
    /// User address.
    pub user: Address,
    /// Token address.
    pub token: Address,
    /// Current position size.
    pub position_size: i128,
    /// Last updated timestamp.
    pub last_updated: u64,
}

/// Circuit breaker state.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircuitBreakerState {
    /// Whether circuit breaker is currently active.
    pub is_active: bool,
    /// Timestamp when circuit breaker was triggered.
    pub triggered_at: u64,
    /// Reason for triggering.
    pub reason: String,
    /// Cooldown expiry timestamp.
    pub cooldown_until: u64,
}
