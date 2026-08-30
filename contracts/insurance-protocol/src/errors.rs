use soroban_sdk::{contracterror, panic_with_error, Env};

/// Error codes for the insurance protocol contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InsuranceError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    PoolNotFound = 3,
    PoolAlreadyExists = 4,
    PoolInactive = 5,
    InsufficientDeposit = 6,
    InsufficientShares = 7,
    InsufficientReserve = 8,
    PolicyNotFound = 9,
    PolicyInactive = 10,
    PolicyExpired = 11,
    ClaimNotFound = 12,
    ClaimAlreadyProcessed = 13,
    ClaimDenied = 14,
    ClaimAmountExceedsCoverage = 15,
    InvalidCoverageAmount = 16,
    InvalidPremium = 17,
    TimelockNotExpired = 18,
    VotingPeriodNotEnded = 19,
    AlreadyVoted = 20,
    NotAnUnderwriter = 21,
    MaxUnderwritersReached = 22,
    ReserveRatioExceeded = 23,
    InvalidInput = 24,
    PoolNotPaused = 25,
    PayoutQueueEmpty = 26,
    AlreadyPaused = 27,
    DeductibleNotMet = 28,
}

// Error helper functions that panic with the appropriate error code.

#[inline(always)]
pub fn already_initialized(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::AlreadyInitialized)
}

#[inline(always)]
pub fn unauthorized(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::Unauthorized)
}

#[inline(always)]
pub fn pool_not_found(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PoolNotFound)
}

#[inline(always)]
pub fn pool_already_exists(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PoolAlreadyExists)
}

#[inline(always)]
pub fn pool_inactive(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PoolInactive)
}

#[inline(always)]
pub fn insufficient_deposit(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::InsufficientDeposit)
}

#[inline(always)]
pub fn insufficient_shares(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::InsufficientShares)
}

#[inline(always)]
pub fn insufficient_reserve(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::InsufficientReserve)
}

#[inline(always)]
pub fn policy_not_found(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PolicyNotFound)
}

#[inline(always)]
pub fn policy_inactive(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PolicyInactive)
}

#[inline(always)]
pub fn policy_expired(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PolicyExpired)
}

#[inline(always)]
pub fn claim_not_found(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::ClaimNotFound)
}

#[inline(always)]
pub fn claim_already_processed(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::ClaimAlreadyProcessed)
}

#[inline(always)]
pub fn claim_denied(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::ClaimDenied)
}

#[inline(always)]
pub fn claim_amount_exceeds_coverage(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::ClaimAmountExceedsCoverage)
}

#[inline(always)]
pub fn invalid_coverage_amount(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::InvalidCoverageAmount)
}

#[inline(always)]
pub fn invalid_premium(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::InvalidPremium)
}

#[inline(always)]
pub fn timelock_not_expired(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::TimelockNotExpired)
}

#[inline(always)]
pub fn voting_period_not_ended(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::VotingPeriodNotEnded)
}

#[inline(always)]
pub fn already_voted(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::AlreadyVoted)
}

#[inline(always)]
pub fn not_an_underwriter(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::NotAnUnderwriter)
}

#[inline(always)]
pub fn max_underwriters_reached(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::MaxUnderwritersReached)
}

#[inline(always)]
pub fn reserve_ratio_exceeded(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::ReserveRatioExceeded)
}

#[inline(always)]
pub fn invalid_input(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::InvalidInput)
}

#[inline(always)]
pub fn pool_not_paused(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PoolNotPaused)
}

#[inline(always)]
pub fn payout_queue_empty(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::PayoutQueueEmpty)
}

#[inline(always)]
pub fn already_paused(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::AlreadyPaused)
}

#[inline(always)]
pub fn deductible_not_met(env: &Env) -> ! {
    panic_with_error!(env, InsuranceError::DeductibleNotMet)
}
