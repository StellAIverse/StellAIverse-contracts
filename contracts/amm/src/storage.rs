use soroban_sdk::{Address, Env};

use crate::types::{CircuitBreakerState, DataKey, Pool, RiskParams, UserPosition};

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

pub fn invalidate_query_cache(env: &Env, pool_id: u64) {
    env.storage()
        .instance()
        .remove(&DataKey::PriceCache(pool_id));
    env.storage()
        .instance()
        .remove(&DataKey::ReserveCache(pool_id));

    env.events().publish(
        (soroban_sdk::Symbol::new(env, "CacheInvalidated"),),
        (pool_id, env.ledger().timestamp()),
    );
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
        .unwrap_or(RiskParams {
            max_position_per_user: 1_000_000_000,
            max_position_per_asset: 10_000_000_000,
            concentration_threshold_bps: 3000,
            circuit_breaker_threshold_bps: 1500,
            circuit_breaker_cooldown: 3600,
            min_lp_token_threshold: 1000,
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
    get_risk_params(env).min_lp_token_threshold
}

/* ---------------- FEE CONFIGURATION ---------------- */

pub fn get_governance_collector(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::GovernanceCollector)
}

pub fn set_governance_collector(env: &Env, collector: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::GovernanceCollector, collector);
}

pub fn get_protocol_fee_share_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ProtocolFeeShareBps)
        .unwrap_or(0)
}

pub fn set_protocol_fee_share_bps(env: &Env, bps: u32) {
    env.storage()
        .instance()
        .set(&DataKey::ProtocolFeeShareBps, &bps);
}

/* ---------------- LP REWARDS ---------------- */

pub fn get_lp_reward_balance(env: &Env, pool_id: u64, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::LpRewardBalance(pool_id, token.clone()))
        .unwrap_or(0)
}

pub fn set_lp_reward_balance(env: &Env, pool_id: u64, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::LpRewardBalance(pool_id, token.clone()), &amount);
}

/* ---------------- REENTRANCY ---------------- */

pub fn is_reentrancy_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::ReentrancyLock)
        .unwrap_or(false)
}

pub fn set_reentrancy_lock(env: &Env, locked: bool) {
    env.storage()
        .instance()
        .set(&DataKey::ReentrancyLock, &locked);
}
