// Kani Proof Harnesses: Execution Hub Contract
//
// These harnesses verify the key invariants documented in
// formal-verification/specs/execution_hub.md using the Kani bounded model checker.
//
// Run with: cargo kani (from the formal-verification/ directory)
//
// Reference: https://model-checking.github.io/kani/

// ============================================================================
// INV-HUB-1: Rate Limit Enforcement
// ============================================================================
//
// The check_rate_limit function allows at most `max_operations` calls within
// any window of `window_seconds`. This harness verifies the core counter logic:
// after a window reset, the count starts at 1; within a window, the count
// increments; at max capacity the call panics.
//
// The harness models the state transition function directly.

#[derive(Clone, Copy)]
struct RateLimitData {
    last_reset: u64,
    count: u32,
}

/// Returns the new (last_reset, count) after a successful rate-limit check,
/// or None if the rate limit would be exceeded.
fn apply_rate_limit(
    now: u64,
    data: Option<RateLimitData>,
    max_operations: u32,
    window_seconds: u64,
) -> Option<RateLimitData> {
    let (last_reset, count) = match data {
        Some(d) => (d.last_reset, d.count),
        None => (now, 0),
    };

    let elapsed = now.saturating_sub(last_reset);

    if elapsed > window_seconds {
        // Window reset: start fresh
        Some(RateLimitData { last_reset: now, count: 1 })
    } else if count < max_operations {
        // Within window, within limit
        Some(RateLimitData { last_reset, count: count + 1 })
    } else {
        // Rate limit exceeded
        None
    }
}

#[cfg(kani)]
#[kani::proof]
fn verify_rate_limit_enforcement() {
    let now: u64 = kani::any();
    let max_operations: u32 = kani::any();
    let window_seconds: u64 = kani::any();

    // Pre-conditions matching contract validation
    kani::assume(max_operations > 0);
    kani::assume(window_seconds > 0);

    // Symbolic existing state (may or may not exist)
    let has_data: bool = kani::any();
    let data: Option<RateLimitData> = if has_data {
        let last_reset: u64 = kani::any();
        let count: u32 = kani::any();
        // Existing count can't exceed max_operations (invariant maintained by prior calls)
        kani::assume(count <= max_operations);
        kani::assume(last_reset <= now);
        Some(RateLimitData { last_reset, count })
    } else {
        None
    };

    match apply_rate_limit(now, data, max_operations, window_seconds) {
        Some(new_data) => {
            // New count must not exceed max_operations
            assert!(new_data.count <= max_operations);
            // Count must be at least 1 (this call was counted)
            assert!(new_data.count >= 1);
        }
        None => {
            // Was rejected: existing count was at max within the window
            if let Some(d) = data {
                let elapsed = now.saturating_sub(d.last_reset);
                assert!(elapsed <= window_seconds);
                assert!(d.count >= max_operations);
            }
        }
    }
}

// ============================================================================
// INV-HUB-3: Nonce Strictly Increases Per Agent
// ============================================================================
//
// execute_action rejects nonce <= stored_nonce and on success stores the
// new nonce. This harness verifies that the stored nonce after a successful
// call is strictly greater than the stored nonce before.

/// Returns the new stored nonce if the call succeeds, or None if rejected.
fn apply_nonce(stored_nonce: u64, submitted_nonce: u64) -> Option<u64> {
    if submitted_nonce <= stored_nonce {
        None // Replay protection triggered
    } else {
        Some(submitted_nonce)
    }
}

#[cfg(kani)]
#[kani::proof]
fn verify_nonce_monotonicity() {
    let stored_nonce: u64 = kani::any();
    let submitted_nonce: u64 = kani::any();

    match apply_nonce(stored_nonce, submitted_nonce) {
        Some(new_nonce) => {
            // New nonce is strictly greater than the old stored nonce
            assert!(new_nonce > stored_nonce);
            // New nonce equals the submitted nonce
            assert!(new_nonce == submitted_nonce);
        }
        None => {
            // Rejected: submitted nonce was not strictly greater
            assert!(submitted_nonce <= stored_nonce);
        }
    }
}

// ============================================================================
// INV-HUB-4: Execution Counter Monotonicity
// ============================================================================
//
// next_execution_id uses saturating_add and panics if the result is 0
// (i.e., overflow). This harness verifies that for all non-overflowing inputs,
// the counter strictly increases.

/// Mirror of next_execution_id logic.
fn next_execution_id(current: u64) -> Option<u64> {
    let next = current.saturating_add(1);
    if next == 0 {
        None // Overflow
    } else {
        Some(next)
    }
}

#[cfg(kani)]
#[kani::proof]
fn verify_execution_counter_monotonicity() {
    let current: u64 = kani::any();

    match next_execution_id(current) {
        Some(next) => {
            // Strictly greater than before
            assert!(next > current);
            // Exactly one more
            assert!(next == current + 1);
        }
        None => {
            // Overflow case: current was u64::MAX (saturating_add(1) = u64::MAX, not 0)
            // In practice, saturating_add(u64::MAX, 1) = u64::MAX ≠ 0,
            // so this branch is unreachable. Kani will verify this.
            assert!(false, "next_execution_id overflow branch should be unreachable");
        }
    }
}

// ============================================================================
// INV-HUB-6: Rate Limit Config Always Positive
// ============================================================================
//
// validate_rate_limit_config rejects zero values for ops and window_secs.

/// Mirror of validate_rate_limit_config.
fn validate_rate_limit_config(ops: u32, window_secs: u64) -> Result<(), &'static str> {
    if ops == 0 {
        return Err("operations must be greater than 0");
    }
    if window_secs == 0 {
        return Err("window_seconds must be greater than 0");
    }
    Ok(())
}

#[cfg(kani)]
#[kani::proof]
fn verify_rate_limit_config_positive() {
    let ops: u32 = kani::any();
    let window_secs: u64 = kani::any();

    match validate_rate_limit_config(ops, window_secs) {
        Ok(()) => {
            assert!(ops > 0);
            assert!(window_secs > 0);
        }
        Err(_) => {
            assert!(ops == 0 || window_secs == 0);
        }
    }
}

// ============================================================================
// INV-HUB-2: Authorization Check — Owner or Non-Expired Operator
// ============================================================================
//
// This harness verifies the boolean authorization logic:
// a call is authorized iff executor == owner OR
// (executor == operator AND now <= expires_at).

fn is_authorized(
    executor: u64,
    owner: u64,
    operator: Option<(u64, u64)>, // (operator_id, expires_at)
    now: u64,
) -> bool {
    if executor == owner {
        return true;
    }
    if let Some((op_id, expires_at)) = operator {
        if op_id == executor && now <= expires_at {
            return true;
        }
    }
    false
}

#[cfg(kani)]
#[kani::proof]
fn verify_authorization_logic() {
    let executor: u64 = kani::any();
    let owner: u64 = kani::any();
    let now: u64 = kani::any();
    let has_operator: bool = kani::any();

    let operator: Option<(u64, u64)> = if has_operator {
        let op_id: u64 = kani::any();
        let expires_at: u64 = kani::any();
        Some((op_id, expires_at))
    } else {
        None
    };

    let authorized = is_authorized(executor, owner, operator, now);

    if authorized {
        // Must be owner, OR valid non-expired operator
        let is_owner = executor == owner;
        let is_valid_operator = operator
            .map(|(op_id, expires_at)| op_id == executor && now <= expires_at)
            .unwrap_or(false);
        assert!(is_owner || is_valid_operator);
    } else {
        // Neither owner nor valid operator
        assert!(executor != owner);
        let is_valid_operator = operator
            .map(|(op_id, expires_at)| op_id == executor && now <= expires_at)
            .unwrap_or(false);
        assert!(!is_valid_operator);
    }
}
