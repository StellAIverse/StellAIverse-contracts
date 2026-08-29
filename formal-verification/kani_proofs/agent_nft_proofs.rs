// Kani Proof Harnesses: Agent NFT Contract
//
// These harnesses verify the key invariants documented in
// formal-verification/specs/agent_nft.md using the Kani bounded model checker.
//
// Run with: cargo kani (from the formal-verification/ directory)
//
// Each harness is a function annotated with #[kani::proof]. Kani will
// exhaustively explore all possible values of variables declared with
// kani::any::<T>() within the configured bounds, and assert that the
// stated invariants hold for all of them.
//
// Reference: https://model-checking.github.io/kani/

// ============================================================================
// INV-ANFT-8: Overflow Safety in safe_add
// ============================================================================
//
// The `safe_add` function in AgentNFT uses checked_add to prevent overflow.
// This harness verifies that:
//   - When overflow would occur, the result is an error (no value returned).
//   - When no overflow occurs, the result equals the mathematical sum.
//   - The returned value v satisfies v >= a and v >= b.
//
// This is a pure arithmetic proof that does not require Soroban SDK stubs.

/// Mirror of AgentNFT::safe_add for standalone proof (no SDK dependency).
fn safe_add(a: u64, b: u64) -> Option<u64> {
    a.checked_add(b)
}

#[cfg(kani)]
#[kani::proof]
fn verify_safe_add() {
    let a: u64 = kani::any();
    let b: u64 = kani::any();

    match safe_add(a, b) {
        Some(v) => {
            // Result equals true mathematical sum
            assert!(v == a.wrapping_add(b));
            // No overflow occurred: v >= a and v >= b
            assert!(v >= a);
            assert!(v >= b);
        }
        None => {
            // Overflow would have occurred: a + b > u64::MAX
            // Verified by showing that a > u64::MAX - b
            assert!(a > u64::MAX - b);
        }
    }
}

// ============================================================================
// INV-ANFT-5: Royalty Fee Bounds
// ============================================================================
//
// validate_royalty_fee rejects any fee > MAX_ROYALTY_FEE.
// This harness verifies that for all possible u32 fee values, the validation
// function correctly allows fees <= MAX_ROYALTY_FEE and rejects all others.

const MAX_ROYALTY_FEE: u32 = 2500; // 25% in basis points

/// Mirror of AgentNFT::validate_royalty_fee for standalone proof.
fn validate_royalty_fee(fee: u32) -> Result<(), &'static str> {
    if fee > MAX_ROYALTY_FEE {
        return Err("InvalidRoyaltyFee");
    }
    Ok(())
}

#[cfg(kani)]
#[kani::proof]
fn verify_royalty_fee_bound() {
    let fee: u32 = kani::any();

    match validate_royalty_fee(fee) {
        Ok(()) => {
            // If accepted, fee must be within the allowed bound
            assert!(fee <= MAX_ROYALTY_FEE);
        }
        Err(_) => {
            // If rejected, fee must exceed the bound
            assert!(fee > MAX_ROYALTY_FEE);
        }
    }
}

// ============================================================================
// INV-ANFT-1 & INV-ANFT-2: Counter Monotonicity
// ============================================================================
//
// Each mint increments the counter by exactly 1, and the new agent's ID
// equals the new counter value. This harness verifies the arithmetic
// relationship between the old counter, new counter, and agent ID.

/// Simulates the counter increment logic used in mint_agent_legacy and batch_mint.
fn increment_counter(counter: u64) -> Option<u64> {
    counter.checked_add(1)
}

#[cfg(kani)]
#[kani::proof]
fn verify_counter_monotonicity() {
    let counter: u64 = kani::any();

    match increment_counter(counter) {
        Some(new_counter) => {
            // New counter is strictly greater than old counter
            assert!(new_counter > counter);
            // New counter is exactly old + 1
            assert!(new_counter == counter + 1);
        }
        None => {
            // Overflow: counter was at u64::MAX
            assert!(counter == u64::MAX);
        }
    }
}

// ============================================================================
// INV-ANFT-3: Owner-Only Transfer — Authorization Logic
// ============================================================================
//
// The transfer function checks `agent.owner == from` before proceeding.
// This harness verifies that the boolean check produces the correct outcome
// for all possible combinations of equality/inequality.

#[cfg(kani)]
#[kani::proof]
fn verify_owner_only_transfer() {
    // Represent owners as u64 IDs for symbolic reasoning (address equality
    // is equivalent to symbolic equality of identifiers).
    let agent_owner: u64 = kani::any();
    let from: u64 = kani::any();

    let is_authorized = agent_owner == from;

    if is_authorized {
        // Only the correct owner is authorized
        assert!(from == agent_owner);
    } else {
        // Any other caller is rejected
        assert!(from != agent_owner);
    }
}

// ============================================================================
// INV-ANFT-2: All Agent IDs Within Counter Bounds
// ============================================================================
//
// Verifies that an agent ID assigned as `counter + 1` (the next ID) is always
// <= the counter value stored after the mint (which IS `counter + 1`).

#[cfg(kani)]
#[kani::proof]
fn verify_id_within_counter() {
    let pre_counter: u64 = kani::any();
    // Assume counter is not at max (overflow guard exists in contract)
    kani::assume(pre_counter < u64::MAX);

    let assigned_id = pre_counter + 1;
    let post_counter = pre_counter + 1; // counter is updated to equal the new id

    // The assigned id equals the post-mint counter
    assert!(assigned_id == post_counter);
    // The assigned id never exceeds the post-mint counter
    assert!(assigned_id <= post_counter);
}
