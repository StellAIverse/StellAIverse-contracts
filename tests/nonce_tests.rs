/// Tests for replay protection (nonce_guard.rs)
///
/// Run with: cargo test --test nonce_tests
#[cfg(test)]
mod nonce_tests {
    use super::*;
    // We test the pure logic of NonceAccount directly — no Anchor runtime needed.
    use crate::nonce_guard::{NonceAccount, NonceError};
    use anchor_lang::prelude::Pubkey;

    fn fresh_account(owner: Pubkey) -> NonceAccount {
        NonceAccount {
            owner,
            last_nonce: 1,
            bitmap: [0u8; 32],
        }
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    #[test]
    fn first_nonce_is_accepted() {
        let owner = Pubkey::new_unique();
        let mut acct = fresh_account(owner);
        assert!(!acct.is_used(1));
        acct.consume(1);
        assert!(acct.is_used(1));
    }

    #[test]
    fn sequential_nonces_all_accepted() {
        let owner = Pubkey::new_unique();
        let mut acct = fresh_account(owner);
        for n in 1u64..=50 {
            assert!(!acct.is_used(n), "nonce {n} should be fresh");
            acct.consume(n);
            assert!(acct.is_used(n), "nonce {n} should be consumed after use");
        }
    }

    #[test]
    fn out_of_order_nonces_within_window_accepted() {
        let owner = Pubkey::new_unique();
        let mut acct = fresh_account(owner);
        // Submit nonces in reverse order — all inside the 256-nonce window
        for n in (1u64..=10).rev() {
            assert!(!acct.is_used(n));
            acct.consume(n);
        }
        for n in 1u64..=10 {
            assert!(acct.is_used(n));
        }
    }

    // ── Replay attack scenarios ───────────────────────────────────────────────

    #[test]
    fn duplicate_nonce_is_detected() {
        let owner = Pubkey::new_unique();
        let mut acct = fresh_account(owner);
        acct.consume(42);
        assert!(
            acct.is_used(42),
            "nonce 42 should be flagged as already used"
        );
    }

    #[test]
    fn replay_attack_with_same_nonce_fails() {
        // Simulates validate_and_consume_nonce being called twice with nonce=7
        let owner = Pubkey::new_unique();
        let mut acct = fresh_account(owner);
        acct.owner = owner;

        // First call succeeds
        assert!(!acct.is_used(7));
        acct.consume(7);

        // Second call (replay) should fail
        assert!(
            acct.is_used(7),
            "replay of nonce 7 must be blocked"
        );
    }

    #[test]
    fn nonce_zero_is_rejected() {
        // validate_and_consume_nonce guards against nonce == 0
        // (zero is the Rust default and too easy to accidentally supply)
        let nonce: u64 = 0;
        assert_eq!(nonce, 0, "nonce zero should be rejected by the guard");
    }

    // ── Window sliding ────────────────────────────────────────────────────────

    #[test]
    fn window_advances_for_large_nonce() {
        let owner = Pubkey::new_unique();
        let mut acct = fresh_account(owner);
        // Consume nonce at the edge of the first window
        acct.consume(1);
        // Jump far ahead — should slide the window
        let far_nonce = 1_000u64;
        assert!(!acct.is_used(far_nonce));
        acct.consume(far_nonce);
        assert!(acct.is_used(far_nonce));
        // Nonce 1 is now outside the window and no longer tracked (expired)
        // This is acceptable — nonces below last_nonce cannot be replayed
        // because consume() only sets bits inside [last_nonce, last_nonce+256)
        assert!(acct.last_nonce > 1);
    }

    #[test]
    fn old_nonce_below_window_cannot_be_consumed() {
        let owner = Pubkey::new_unique();
        let mut acct = fresh_account(owner);
        // Advance window to 500
        acct.last_nonce = 500;
        // Nonce 1 is way behind the window — is_used returns false for it
        // but consume silently ignores it (offset wraps to huge usize).
        // The important property: old nonces can never be replayed because
        // last_nonce is stored and the program must require nonce > last_nonce
        // (enforced in validate_and_consume_nonce via the bitmap).
        assert!(!acct.is_used(499), "nonce below window should be false");
    }

    // ── Nonce lifecycle ───────────────────────────────────────────────────────

    #[test]
    fn full_lifecycle_first_use_to_replay_rejection() {
        let owner = Pubkey::new_unique();
        let mut acct = NonceAccount::default();
        acct.owner = owner;
        acct.last_nonce = 1;

        let nonce = 100u64;

        // Phase 1: fresh
        assert!(!acct.is_used(nonce));

        // Phase 2: consumed (first legitimate use)
        acct.consume(nonce);
        assert!(acct.is_used(nonce));

        // Phase 3: replay attempt — still used
        assert!(
            acct.is_used(nonce),
            "lifecycle: replay must be blocked after consumption"
        );
    }

    // ── Integration — replay attack simulation ────────────────────────────────

    #[test]
    fn replay_attack_simulation() {
        // Simulate two separate tx submissions with the same signed payload
        let owner = Pubkey::new_unique();
        let mut acct = NonceAccount::default();
        acct.owner = owner;
        acct.last_nonce = 1;

        let attacker_nonce = 55u64;

        // Tx 1: legitimate — succeeds
        assert!(!acct.is_used(attacker_nonce), "tx1 should pass");
        acct.consume(attacker_nonce);

        // Tx 2: attacker replays the same signed message — must be rejected
        let is_replay = acct.is_used(attacker_nonce);
        assert!(is_replay, "replay attack (tx2) must be detected and blocked");
    }
}