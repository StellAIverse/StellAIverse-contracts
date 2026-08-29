use soroban_sdk::{Address, Env};

use crate::types::{CircuitBreakerState, DataKey, Pool, RiskParams, Route, UserPosition};

/* ---------------- ADMIN ---------------- */

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("Contract not initialized")
}

pub fn require_admin(env: &Env, caller: &Address) {
    let admin = get_admin(env);
    if caller != &admin {
        panic!("Unauthorized: caller is not admin");
    }
}

/* ---------------- POOL COUNTER ---------------- */

pub fn get_pool_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::PoolCounter)
        .unwrap_or(0)
}

pub fn set_pool_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::PoolCounter, &counter);
}

/* ---------------- POOL DATA ---------------- */

pub fn set_pool(env: &Env, pool: &Pool) {
    env.storage()
        .persistent()
        .set(&DataKey::Pool(pool.pool_id), pool);
}

pub fn get_pool(env: &Env, pool_id: u64) -> Pool {
    env.storage()
        .persistent()
        .get(&DataKey::Pool(pool_id))
        .expect("Pool not found")
}

/* ---------------- LP BALANCES ---------------- */

pub fn get_lp_balance(env: &Env, pool_id: u64, provider: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::LpBalance(pool_id, provider.clone()))
        .unwrap_or(0)
}

pub fn set_lp_balance(env: &Env, pool_id: u64, provider: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::LpBalance(pool_id, provider.clone()), &amount);
}

/* ---------------- QUERY CACHE INVALIDATION (Issue #215) ---------------- */

/// Invalidate query cache for a specific pool
/// Called after all state-mutating operations to prevent stale data
pub fn invalidate_query_cache(env: &Env, pool_id: u64) {
    // Remove cached price data
    let price_cache_key = DataKey::PriceCache(pool_id);
    env.storage().instance().remove(&price_cache_key);

    // Remove cached reserve data
    let reserve_cache_key = DataKey::ReserveCache(pool_id);
    env.storage().instance().remove(&reserve_cache_key);

    // Emit cache invalidation event for monitoring
    env.events().publish(
        (soroban_sdk::Symbol::new(env, "CacheInvalidated"),),
        (pool_id, env.ledger().timestamp()),
    );
}

/// Invalidate all query caches (for major state changes)
pub fn invalidate_all_query_caches(env: &Env) {
    // Instance-level caches are cleared automatically on TTL expiry
    // This function triggers immediate invalidation
    let invalidation_timestamp = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&DataKey::LastCacheInvalidation, &invalidation_timestamp);
}

/* ---------------- ROUTE MANAGEMENT ---------------- */

pub fn get_route_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::RouteCounter)
        .unwrap_or(0)
}

pub fn set_route_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::RouteCounter, &counter);
}

pub fn set_route(env: &Env, route_id: u64, route: &Route) {
    env.storage()
        .persistent()
        .set(&DataKey::Route(route_id), route);
}

pub fn get_route(env: &Env, route_id: u64) -> Route {
    env.storage()
        .persistent()
        .get(&DataKey::Route(route_id))
        .expect("Route not found")
}

/* ---------------- TRADING PAUSE & CIRCUIT BREAKER ---------------- */

pub fn is_trading_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TradingPaused)
        .unwrap_or(false)
}

pub fn set_trading_paused(env: &Env, paused: bool) {
    env.storage()
        .instance()
        .set(&DataKey::TradingPaused, &paused);
}

pub fn get_circuit_breaker_state(env: &Env) -> Option<CircuitBreakerState> {
    env.storage().instance().get(&DataKey::CircuitBreakerActive)
}

pub fn set_circuit_breaker_state(env: &Env, state: &CircuitBreakerState) {
    env.storage()
        .instance()
        .set(&DataKey::CircuitBreakerActive, state);
}

/* ---------------- RISK MANAGEMENT ---------------- */

pub fn get_risk_params(env: &Env) -> RiskParams {
    env.storage()
        .instance()
        .get(&DataKey::RiskParams)
        .unwrap_or_else(|| RiskParams {
            max_position_per_user: 1_000_000_000,   // Default: 1B tokens
            max_position_per_asset: 10_000_000_000, // Default: 10B tokens
            concentration_threshold_bps: 3000,      // Default: 30%
            circuit_breaker_threshold_bps: 1500,    // Default: 15%
            circuit_breaker_cooldown: 3600,         // Default: 1 hour
            min_lp_token_threshold: 1000,           // Default: 1000 tokens
        })
}

pub fn set_risk_params(env: &Env, params: &RiskParams) {
    env.storage().instance().set(&DataKey::RiskParams, params);
}

pub fn get_user_position(env: &Env, user: &Address, token: &Address) -> Option<UserPosition> {
    env.storage()
        .persistent()
        .get(&DataKey::UserPosition(user.clone(), token.clone()))
}

pub fn set_user_position(env: &Env, user: &Address, token: &Address, position: &UserPosition) {
    env.storage().persistent().set(
        &DataKey::UserPosition(user.clone(), token.clone()),
        position,
    );
}

pub fn get_min_lp_token_threshold(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MinLpTokenThreshold)
        .unwrap_or(1000) // Default: 1000 tokens
}

pub fn set_min_lp_token_threshold(env: &Env, threshold: i128) {
    env.storage()
        .instance()
        .set(&DataKey::MinLpTokenThreshold, &threshold);
}
