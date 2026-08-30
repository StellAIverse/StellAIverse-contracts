use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::*;

// ── Admin ───────────────────────────────────────────────────────────────────

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

pub fn assert_admin(env: &Env, caller: &Address) {
    let admin = get_admin(env).expect("admin not set");
    if *caller != admin {
        panic!("Unauthorized: admin required");
    }
}

// ── Oracle ──────────────────────────────────────────────────────────────────

pub fn get_oracle(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Oracle)
}

pub fn set_oracle(env: &Env, oracle: &Address) {
    env.storage().instance().set(&StorageKey::Oracle, oracle);
}

// ── Treasury ────────────────────────────────────────────────────────────────

pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::Treasury)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::Treasury, treasury);
}

// ── Pause ───────────────────────────────────────────────────────────────────

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::Paused)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&StorageKey::Paused, &paused);
}

pub fn check_paused(env: &Env) {
    if is_paused(env) {
        panic!("Contract is paused");
    }
}

// ── Circuit Breaker ─────────────────────────────────────────────────────────

pub fn is_circuit_breaker_active(env: &Env) -> bool {
    let state: Option<CircuitBreakerState> = env
        .storage()
        .instance()
        .get(&StorageKey::CircuitBreakerActive);
    match state {
        Some(s) => {
            if s.triggered {
                if let Some(resume_at) = s.can_resume_at {
                    let now = env.ledger().timestamp();
                    if now >= resume_at {
                        // Auto-reset after cooldown
                        set_circuit_breaker(
                            env,
                            &CircuitBreakerState {
                                triggered: false,
                                triggered_at: None,
                                price_at_trigger: None,
                                previous_price: None,
                                change_bps: None,
                                cooldown_seconds: s.cooldown_seconds,
                                can_resume_at: None,
                            },
                        );
                        return false;
                    }
                }
                return true;
            }
            false
        }
        None => false,
    }
}

pub fn set_circuit_breaker(env: &Env, state: &CircuitBreakerState) {
    env.storage()
        .instance()
        .set(&StorageKey::CircuitBreakerActive, state);
}

pub fn get_circuit_breaker(env: &Env) -> CircuitBreakerState {
    env.storage()
        .instance()
        .get(&StorageKey::CircuitBreakerActive)
        .unwrap_or(CircuitBreakerState {
            triggered: false,
            triggered_at: None,
            price_at_trigger: None,
            previous_price: None,
            change_bps: None,
            cooldown_seconds: 3600,
            can_resume_at: None,
        })
}

// ── Option Counter ──────────────────────────────────────────────────────────

pub fn get_option_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&StorageKey::OptionCounter)
        .unwrap_or(0)
}

pub fn set_option_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&StorageKey::OptionCounter, &counter);
}

// ── Option Series ───────────────────────────────────────────────────────────

pub fn get_option_series(env: &Env, series_id: u64) -> Option<OptionSeriesConfig> {
    env.storage()
        .persistent()
        .get(&StorageKey::OptionSeries(series_id))
}

pub fn set_option_series(env: &Env, series: &OptionSeriesConfig) {
    env.storage()
        .persistent()
        .set(&StorageKey::OptionSeries(series.series_id), series);
}

// ── Option Positions ────────────────────────────────────────────────────────

pub fn get_option_position(env: &Env, option_id: u64, holder: &Address) -> Option<OptionPosition> {
    env.storage()
        .persistent()
        .get(&StorageKey::OptionPosition(option_id, holder.clone()))
}

pub fn set_option_position(env: &Env, position: &OptionPosition) {
    env.storage().persistent().set(
        &StorageKey::OptionPosition(position.option_id, position.holder.clone()),
        position,
    );
}

pub fn remove_option_position(env: &Env, option_id: u64, holder: &Address) {
    env.storage()
        .persistent()
        .remove(&StorageKey::OptionPosition(option_id, holder.clone()));
}

// ── Writer Collateral ───────────────────────────────────────────────────────

pub fn get_writer_collateral(env: &Env, writer: &Address, underlying: &Symbol) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::WriterCollateral(
            writer.clone(),
            underlying.clone(),
        ))
        .unwrap_or(0)
}

pub fn set_writer_collateral(env: &Env, writer: &Address, underlying: &Symbol, amount: i128) {
    env.storage().persistent().set(
        &StorageKey::WriterCollateral(writer.clone(), underlying.clone()),
        &amount,
    );
}

// ── Pool Collateral ─────────────────────────────────────────────────────────

pub fn get_pool_collateral(env: &Env, underlying: &Symbol) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::PoolCollateral(underlying.clone()))
        .unwrap_or(0)
}

pub fn set_pool_collateral(env: &Env, underlying: &Symbol, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::PoolCollateral(underlying.clone()), &amount);
}

// ── User Option IDs (written options) ──────────────────────────────────────

pub fn get_user_option_ids(env: &Env, user: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&StorageKey::UserOptionIds(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn add_user_option_id(env: &Env, user: &Address, option_id: u64) {
    let mut ids = get_user_option_ids(env, user);
    ids.push_back(option_id);
    env.storage()
        .persistent()
        .set(&StorageKey::UserOptionIds(user.clone()), &ids);
}

// ── User Held Option IDs ───────────────────────────────────────────────────

pub fn get_user_held_option_ids(env: &Env, user: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&StorageKey::UserHeldOptionIds(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn add_user_held_option_id(env: &Env, user: &Address, option_id: u64) {
    let mut ids = get_user_held_option_ids(env, user);
    ids.push_back(option_id);
    env.storage()
        .persistent()
        .set(&StorageKey::UserHeldOptionIds(user.clone()), &ids);
}

// ── Listing Counter ─────────────────────────────────────────────────────────

pub fn get_listing_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&StorageKey::ListingCounter)
        .unwrap_or(0)
}

pub fn set_listing_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&StorageKey::ListingCounter, &counter);
}

// ── Listings ────────────────────────────────────────────────────────────────

pub fn get_listing(env: &Env, listing_id: u64) -> Option<OptionListing> {
    env.storage()
        .persistent()
        .get(&StorageKey::Listing(listing_id))
}

pub fn set_listing(env: &Env, listing: &OptionListing) {
    env.storage()
        .persistent()
        .set(&StorageKey::Listing(listing.listing_id), listing);
}

// ── Price Cache ─────────────────────────────────────────────────────────────

pub fn get_price_cache(env: &Env, underlying: &Symbol) -> Option<i128> {
    env.storage()
        .persistent()
        .get(&StorageKey::PriceCache(underlying.clone()))
}

pub fn set_price_cache(env: &Env, underlying: &Symbol, price: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::PriceCache(underlying.clone()), &price);
}

// ── Volatility Cache ────────────────────────────────────────────────────────

pub fn get_volatility_cache(env: &Env, underlying: &Symbol) -> Option<i128> {
    env.storage()
        .persistent()
        .get(&StorageKey::VolatilityCache(underlying.clone()))
}

pub fn set_volatility_cache(env: &Env, underlying: &Symbol, volatility: i128) {
    env.storage().persistent().set(
        &StorageKey::VolatilityCache(underlying.clone()),
        &volatility,
    );
}

// ── Risk Management ─────────────────────────────────────────────────────────

pub fn get_max_position_per_user(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::MaxPositionPerUser)
        .unwrap_or(1_000_000 * PRECISION) // Default: 1M contracts
}

pub fn set_max_position_per_user(env: &Env, max: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::MaxPositionPerUser, &max);
}

pub fn get_max_total_exposure(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::MaxTotalExposure)
        .unwrap_or(100_000_000 * PRECISION) // Default: 100M contracts
}

pub fn set_max_total_exposure(env: &Env, max: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::MaxTotalExposure, &max);
}

pub fn get_total_exposure(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalExposure)
        .unwrap_or(0)
}

pub fn set_total_exposure(env: &Env, exposure: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalExposure, &exposure);
}

pub fn get_user_exposure(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::UserExposure(user.clone()))
        .unwrap_or(0)
}

pub fn set_user_exposure(env: &Env, user: &Address, exposure: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::UserExposure(user.clone()), &exposure);
}

// ── Greeks Cache ────────────────────────────────────────────────────────────

pub fn get_greeks_cache(env: &Env, option_id: u64) -> Option<Greeks> {
    env.storage()
        .persistent()
        .get(&StorageKey::GreeksCache(option_id))
}

pub fn set_greeks_cache(env: &Env, option_id: u64, greeks: &Greeks) {
    env.storage()
        .persistent()
        .set(&StorageKey::GreeksCache(option_id), greeks);
}

// ── Withdrawal Requests (Multi-sig) ────────────────────────────────────────

pub fn get_withdrawal_request_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&StorageKey::WithdrawalRequestCounter)
        .unwrap_or(0)
}

pub fn set_withdrawal_request_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&StorageKey::WithdrawalRequestCounter, &counter);
}

pub fn get_withdrawal_request(env: &Env, request_id: u64) -> Option<WithdrawalRequest> {
    env.storage()
        .persistent()
        .get(&StorageKey::WithdrawalRequest(request_id))
}

pub fn set_withdrawal_request(env: &Env, request: &WithdrawalRequest) {
    env.storage()
        .persistent()
        .set(&StorageKey::WithdrawalRequest(request.request_id), request);
}

pub fn has_withdrawal_approval(env: &Env, request_id: u64, approver: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&StorageKey::WithdrawalApprovals(
            request_id,
            approver.clone(),
        ))
}

pub fn set_withdrawal_approval(env: &Env, request_id: u64, approver: &Address) {
    env.storage().persistent().set(
        &StorageKey::WithdrawalApprovals(request_id, approver.clone()),
        &true,
    );
}

// ── Price History (for circuit breaker) ────────────────────────────────────

pub fn get_price_history(env: &Env, underlying: &Symbol) -> Vec<(u64, i128)> {
    env.storage()
        .persistent()
        .get(&StorageKey::PriceHistory(underlying.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn add_price_history(env: &Env, underlying: &Symbol, timestamp: u64, price: i128) {
    let mut history = get_price_history(env, underlying);
    // Keep last 100 entries max
    if history.len() >= 100 {
        let mut new_history = Vec::new(env);
        for i in 1..history.len() {
            new_history.push_back(history.get(i).unwrap());
        }
        history = new_history;
    }
    history.push_back((timestamp, price));
    env.storage()
        .persistent()
        .set(&StorageKey::PriceHistory(underlying.clone()), &history);
}
