use soroban_sdk::{Address, Env, IntoVal, Symbol, Val, Vec};

use crate::errors::OptionsError;
use crate::storage;
use crate::types::{UnderlyingAsset, PRECISION};

/// Get the underlying asset symbol for oracle queries.
pub fn underlying_to_symbol(env: &Env, underlying: UnderlyingAsset) -> Symbol {
    match underlying {
        UnderlyingAsset::XLM => Symbol::new(env, "XLM"),
        UnderlyingAsset::USDC => Symbol::new(env, "USDC"),
    }
}

/// Fetch the current price from the oracle for a given underlying asset.
/// Returns the price scaled by PRECISION.
pub fn get_oracle_price(env: &Env, underlying: UnderlyingAsset) -> i128 {
    let feed_id = underlying_to_symbol(env, underlying);

    // Check local cache first
    if let Some(cached) = storage::get_price_cache(env, &feed_id) {
        return cached;
    }

    // Try to fetch from oracle contract
    let oracle = match storage::get_oracle(env) {
        Some(o) => o,
        None => {
            // No oracle configured, use default prices for testing
            return default_price(underlying);
        }
    };

    // Call oracle contract to get aggregated price
    let mut args = Vec::<Val>::new(env);
    args.push_back(feed_id.clone().into_val(env));

    // Try get_aggregated_price first, fall back to get_price
    let result: Result<i128, _> =
        env.try_invoke_contract(&oracle, &Symbol::new(env, "get_price"), args.clone());

    match result {
        Ok(price) => {
            if price > 0 {
                storage::set_price_cache(env, &feed_id, price);
                price
            } else {
                default_price(underlying)
            }
        }
        Err(_) => {
            // Try alternative method
            let result2: Result<i128, _> =
                env.try_invoke_contract(&oracle, &Symbol::new(env, "get_latest_price"), args);
            match result2 {
                Ok(price) => {
                    if price > 0 {
                        storage::set_price_cache(env, &feed_id, price);
                        price
                    } else {
                        default_price(underlying)
                    }
                }
                Err(_) => default_price(underlying),
            }
        }
    }
}

/// Get the current implied volatility for an underlying asset.
/// Returns volatility scaled by PRECISION (e.g., 5000 = 50%).
pub fn get_volatility(env: &Env, underlying: UnderlyingAsset) -> i128 {
    let feed_id = underlying_to_symbol(env, underlying);

    // Check local cache
    if let Some(cached) = storage::get_volatility_cache(env, &feed_id) {
        return cached;
    }

    // Default volatility based on asset class
    let vol = default_volatility(underlying);
    storage::set_volatility_cache(env, &feed_id, vol);
    vol
}

/// Update the oracle price cache (called by oracle or admin).
pub fn update_oracle_price(env: &Env, underlying: UnderlyingAsset, price: i128) {
    let feed_id = underlying_to_symbol(env, underlying);
    let now = env.ledger().timestamp();

    // Store price
    storage::set_price_cache(env, &feed_id, price);

    // Add to price history for circuit breaker checks
    storage::add_price_history(env, &feed_id, now, price);

    // Check circuit breaker
    check_price_movement(env, underlying, price);
}

/// Update the volatility cache (called by oracle or admin).
pub fn update_volatility(env: &Env, underlying: UnderlyingAsset, volatility: i128) {
    if volatility < crate::types::MIN_VOLATILITY || volatility > crate::types::MAX_VOLATILITY {
        panic!("Invalid volatility value");
    }
    let feed_id = underlying_to_symbol(env, underlying);
    storage::set_volatility_cache(env, &feed_id, volatility);
}

/// Check if a price movement would trigger the circuit breaker.
fn check_price_movement(env: &Env, underlying: UnderlyingAsset, new_price: i128) {
    let feed_id = underlying_to_symbol(env, underlying);
    let history = storage::get_price_history(env, &feed_id);

    if history.len() < 2 {
        return;
    }

    // Get the second-to-last price
    let prev_entry = history.get(history.len() - 2).unwrap();
    let prev_price: i128 = prev_entry._1;

    if prev_price == 0 {
        return;
    }

    // Calculate change in basis points
    let diff = if new_price > prev_price {
        new_price - prev_price
    } else {
        prev_price - new_price
    };
    let change_bps = (diff * 10_000) / prev_price;

    // Trigger circuit breaker if price moved more than 20% (2000 bps)
    if change_bps > 2000 {
        let now = env.ledger().timestamp();
        let cb_state = crate::types::CircuitBreakerState {
            triggered: true,
            triggered_at: Some(now),
            price_at_trigger: Some(new_price),
            previous_price: Some(prev_price),
            change_bps: Some(change_bps as u32),
            cooldown_seconds: 3600,
            can_resume_at: Some(now + 3600),
        };
        storage::set_circuit_breaker(env, &cb_state);

        env.events().publish(
            (Symbol::new(env, "circuit_breaker"),),
            (new_price, prev_price, change_bps, now),
        );
    }
}

/// Default price for an underlying asset (for testing/fallback).
fn default_price(underlying: UnderlyingAsset) -> i128 {
    match underlying {
        UnderlyingAsset::XLM => 0_1200 * PRECISION / 100, // $0.12
        UnderlyingAsset::USDC => 1_0000 * PRECISION / 100, // $1.00
    }
}

/// Default volatility based on asset class.
fn default_volatility(underlying: UnderlyingAsset) -> i128 {
    match underlying {
        UnderlyingAsset::XLM => 6_000, // 60% annualized
        UnderlyingAsset::USDC => 500,  // 5% annualized (stablecoin)
    }
}

/// Calculate historical volatility from price history.
/// Returns annualized volatility scaled by PRECISION.
pub fn calculate_historical_volatility(env: &Env, underlying: UnderlyingAsset) -> i128 {
    let feed_id = underlying_to_symbol(env, underlying);
    let history = storage::get_price_history(env, &feed_id);

    if history.len() < 2 {
        return default_volatility(underlying);
    }

    // Calculate log returns
    let mut sum_squares: i128 = 0;
    let mut count: i128 = 0;
    let mut prev_price: Option<i128> = None;

    for entry in history.iter() {
        let price: i128 = entry._1;
        if let Some(prev) = prev_price {
            if prev > 0 && price > 0 {
                // Simple log return approximation: ln(p2/p1) * PRECISION
                let ratio = (price * PRECISION) / prev;
                if ratio > 0 {
                    let log_return = crate::math::approx_ln(ratio);
                    sum_squares += log_return * log_return / PRECISION;
                    count += 1;
                }
            }
        }
        prev_price = Some(price);
    }

    if count == 0 {
        return default_volatility(underlying);
    }

    // Variance = sum of squared returns / count
    let variance = sum_squares / count;

    // Standard deviation = sqrt(variance)
    let std_dev = crate::math::isqrt(variance * PRECISION);

    // Annualize: multiply by sqrt(365) ≈ 19.1
    let annualization_factor: i128 = 19_100; // 19.1 * PRECISION / 1000
    let annualized_vol = (std_dev * annualization_factor) / 1000;

    // Clamp to valid range
    if annualized_vol < crate::types::MIN_VOLATILITY {
        crate::types::MIN_VOLATILITY
    } else if annualized_vol > crate::types::MAX_VOLATILITY {
        crate::types::MAX_VOLATILITY
    } else {
        annualized_vol
    }
}
