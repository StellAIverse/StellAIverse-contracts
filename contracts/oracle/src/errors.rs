use soroban_sdk::{contracterror, panic_with_error, Env};

/// Error codes returned by the oracle contract.
///
/// The contract reports failures by panicking with one of these codes so
/// that any caller — CLI, SDK, or another contract — receives a typed,
/// stable error instead of an opaque abort.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OracleError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    ProviderNotFound = 3,
    ProviderAlreadyExists = 4,
    FeedNotFound = 5,
    FeedAlreadyExists = 6,
    StalePrice = 7,
    CircuitBreakerTriggered = 8,
    RateLimitExceeded = 9,
    InsufficientStake = 10,
    InvalidPrice = 11,
    UpdateTooEarly = 12,
    NoActiveProviders = 13,
    AggregationFailed = 14,
    SubscriptionExpired = 15,
    InvalidInput = 16,
    NotEnoughSources = 17,
    ProviderInactive = 18,
    FeedInactive = 19,
    CooldownActive = 20,
    InsufficientBalance = 21,
}

// Oracle error panics. Each helper raises the matching [`OracleError`]
// code; the `&Env` argument is what the panic macro needs to attach the
// error to the failing invocation.
#[inline(always)]
pub fn already_initialized(env: &Env) -> ! {
    panic_with_error!(env, OracleError::AlreadyInitialized)
}

#[inline(always)]
pub fn unauthorized(env: &Env) -> ! {
    panic_with_error!(env, OracleError::Unauthorized)
}

#[inline(always)]
pub fn provider_not_found(env: &Env) -> ! {
    panic_with_error!(env, OracleError::ProviderNotFound)
}

#[inline(always)]
pub fn provider_already_exists(env: &Env) -> ! {
    panic_with_error!(env, OracleError::ProviderAlreadyExists)
}

#[inline(always)]
pub fn feed_not_found(env: &Env) -> ! {
    panic_with_error!(env, OracleError::FeedNotFound)
}

#[inline(always)]
pub fn feed_already_exists(env: &Env) -> ! {
    panic_with_error!(env, OracleError::FeedAlreadyExists)
}

#[inline(always)]
pub fn stale_price(env: &Env) -> ! {
    panic_with_error!(env, OracleError::StalePrice)
}

#[inline(always)]
pub fn circuit_breaker_triggered(env: &Env) -> ! {
    panic_with_error!(env, OracleError::CircuitBreakerTriggered)
}

#[inline(always)]
pub fn rate_limit_exceeded(env: &Env) -> ! {
    panic_with_error!(env, OracleError::RateLimitExceeded)
}

#[inline(always)]
pub fn insufficient_stake(env: &Env) -> ! {
    panic_with_error!(env, OracleError::InsufficientStake)
}

#[inline(always)]
pub fn invalid_price(env: &Env) -> ! {
    panic_with_error!(env, OracleError::InvalidPrice)
}

#[inline(always)]
pub fn update_too_early(env: &Env) -> ! {
    panic_with_error!(env, OracleError::UpdateTooEarly)
}

#[inline(always)]
pub fn no_active_providers(env: &Env) -> ! {
    panic_with_error!(env, OracleError::NoActiveProviders)
}

#[inline(always)]
pub fn aggregation_failed(env: &Env) -> ! {
    panic_with_error!(env, OracleError::AggregationFailed)
}

#[inline(always)]
pub fn subscription_expired(env: &Env) -> ! {
    panic_with_error!(env, OracleError::SubscriptionExpired)
}

#[inline(always)]
pub fn invalid_input(env: &Env) -> ! {
    panic_with_error!(env, OracleError::InvalidInput)
}

#[inline(always)]
pub fn not_enough_sources(env: &Env) -> ! {
    panic_with_error!(env, OracleError::NotEnoughSources)
}

#[inline(always)]
pub fn provider_inactive(env: &Env) -> ! {
    panic_with_error!(env, OracleError::ProviderInactive)
}

#[inline(always)]
pub fn feed_inactive(env: &Env) -> ! {
    panic_with_error!(env, OracleError::FeedInactive)
}

#[inline(always)]
pub fn cooldown_active(env: &Env) -> ! {
    panic_with_error!(env, OracleError::CooldownActive)
}

#[inline(always)]
pub fn insufficient_balance(env: &Env) -> ! {
    panic_with_error!(env, OracleError::InsufficientBalance)
}
