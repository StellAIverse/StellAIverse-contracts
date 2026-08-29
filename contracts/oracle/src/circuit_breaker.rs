use crate::storage_keys::*;
use crate::types::{CircuitBreakerState, PriceFeed};
use soroban_sdk::Env;

pub struct CircuitBreaker;

impl CircuitBreaker {
    /// Check if a new price would trigger the circuit breaker
    pub fn check_price_movement(
        _env: &Env,
        feed: &PriceFeed,
        new_price: i128,
        previous_price: i128,
    ) -> (bool, u32) {
        if !feed.circuit_breaker_enabled || previous_price == 0 {
            return (false, 0);
        }

        // Calculate percentage change in basis points
        let price_diff = if new_price > previous_price {
            new_price - previous_price
        } else {
            previous_price - new_price
        };

        let change_bps = (price_diff * 10000) / previous_price;

        // Check if change exceeds maximum allowed
        if change_bps > feed.max_price_change_bps as i128 {
            return (true, change_bps as u32);
        }

        (false, change_bps as u32)
    }

    /// Trigger the circuit breaker for a feed
    pub fn trigger(
        env: &Env,
        feed_id: &soroban_sdk::Symbol,
        new_price: i128,
        previous_price: i128,
        change_bps: u32,
        cooldown_period: u64,
    ) {
        let cb_key = get_circuit_breaker_key(feed_id);
        let current_time = env.ledger().timestamp();

        let cb_state = CircuitBreakerState {
            triggered: true,
            triggered_at: Some(current_time),
            trigger_price: Some(new_price),
            previous_price: Some(previous_price),
            price_change_bps: Some(change_bps),
            cooldown_period,
            can_resume_at: Some(current_time + cooldown_period),
        };

        env.storage().instance().set(&cb_key, &cb_state);

        // Emit circuit breaker triggered event
        env.events().publish(
            (
                soroban_sdk::Symbol::new(env, "circuit_breaker"),
                feed_id.clone(),
            ),
            (new_price, previous_price, change_bps, current_time),
        );
    }

    /// Check if circuit breaker is currently active
    pub fn is_triggered(env: &Env, feed_id: &soroban_sdk::Symbol) -> bool {
        let cb_key = get_circuit_breaker_key(feed_id);
        let stored: Option<CircuitBreakerState> = env.storage().instance().get(&cb_key);
        if let Some(cb_state) = stored {
            if cb_state.triggered {
                let current_time = env.ledger().timestamp();
                if let Some(can_resume_at) = cb_state.can_resume_at {
                    if current_time < can_resume_at {
                        return true; // Still in cooldown
                    } else {
                        // Auto-reset after cooldown
                        Self::reset(env, feed_id);
                        return false;
                    }
                }
                return true;
            }
        }
        false
    }

    /// Reset the circuit breaker
    pub fn reset(env: &Env, feed_id: &soroban_sdk::Symbol) {
        let cb_key = get_circuit_breaker_key(feed_id);
        let cb_state = CircuitBreakerState {
            triggered: false,
            triggered_at: None,
            trigger_price: None,
            previous_price: None,
            price_change_bps: None,
            cooldown_period: 3600, // Default 1 hour cooldown
            can_resume_at: None,
        };
        env.storage().instance().set(&cb_key, &cb_state);

        // Emit circuit breaker reset event
        env.events().publish(
            (soroban_sdk::Symbol::new(env, "cb_reset"), feed_id.clone()),
            (env.ledger().timestamp(),),
        );
    }

    /// Manual reset by admin
    pub fn admin_reset(env: &Env, feed_id: &soroban_sdk::Symbol, admin: &soroban_sdk::Address) {
        // Admin authorization would be checked in the main contract
        admin.require_auth();
        Self::reset(env, feed_id);
    }

    /// Get current circuit breaker state
    pub fn get_state(env: &Env, feed_id: &soroban_sdk::Symbol) -> Option<CircuitBreakerState> {
        let cb_key = get_circuit_breaker_key(feed_id);
        env.storage().instance().get(&cb_key)
    }

    /// Initialize circuit breaker for a new feed
    pub fn initialize(env: &Env, feed_id: &soroban_sdk::Symbol, cooldown_period: u64) {
        let cb_key = get_circuit_breaker_key(feed_id);
        let cb_state = CircuitBreakerState {
            triggered: false,
            triggered_at: None,
            trigger_price: None,
            previous_price: None,
            price_change_bps: None,
            cooldown_period,
            can_resume_at: None,
        };
        env.storage().instance().set(&cb_key, &cb_state);
    }
}
