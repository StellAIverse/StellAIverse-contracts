use soroban_sdk::Env;

/// Maximum lock duration for veToken (4 years in seconds)
pub const MAX_LOCK_DURATION: u64 = 4 * 365 * 24 * 60 * 60;

/// Minimum lock duration for veToken (7 days in seconds)
pub const MIN_LOCK_DURATION: u64 = 7 * 24 * 60 * 60;

/// Safe addition with overflow check
pub fn safe_add(a: i128, b: i128) -> i128 {
    a.checked_add(b)
        .unwrap_or_else(|| panic!("Overflow in addition"))
}

/// Safe subtraction with underflow check
pub fn safe_sub(a: i128, b: i128) -> i128 {
    a.checked_sub(b)
        .unwrap_or_else(|| panic!("Underflow in subtraction"))
}

/// Calculate basis points: amount * bps / 10000
pub fn calculate_bps(amount: i128, bps: i128) -> i128 {
    (amount * bps) / 10000
}

/// Get current ledger timestamp
pub fn get_timestamp(env: &Env) -> u64 {
    env.ledger().timestamp()
}

/// Get current ledger sequence
pub fn get_block_number(env: &Env) -> u32 {
    env.ledger().sequence()
}
