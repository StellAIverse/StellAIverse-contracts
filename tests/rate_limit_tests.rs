/// Tests for per-user rate limiting (rate_limit.rs)
#[cfg(test)]
mod rate_limit_tests {
    use crate::rate_limit::{RateLimitConfig, RateLimitError, RateLimitState};
    use anchor_lang::prelude::Pubkey;

    const CFG: RateLimitConfig = RateLimitConfig {
        window_seconds: 60,
        max_actions: 3,
    };

    fn fresh_state() -> (Pubkey, RateLimitState) {
        let owner = Pubkey::new_unique();
        (owner, RateLimitState::default())
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    #[test]
    fn first_action_is_allowed() {
        let (owner, mut state) = fresh_state();
        assert!(state.check_and_record(&owner, &CFG, 1000).is_ok());
        assert_eq!(state.action_count, 1);
    }

    #[test]
    fn actions_up_to_limit_all_succeed() {
        let (owner, mut state) = fresh_state();
        for i in 1..=CFG.max_actions {
            assert!(
                state.check_and_record(&owner, &CFG, 1000).is_ok(),
                "action {i} should succeed"
            );
        }
        assert_eq!(state.action_count, CFG.max_actions);
    }

    // ── Rate limit enforcement ─────────────────────────────────────────────────

    #[test]
    fn exceeding_limit_fails_with_specific_error() {
        let (owner, mut state) = fresh_state();
        // Exhaust the window
        for _ in 0..CFG.max_actions {
            state.check_and_record(&owner, &CFG, 1000).unwrap();
        }
        // Next call must fail
        let err = state.check_and_record(&owner, &CFG, 1000).unwrap_err();
        assert_eq!(
            err,
            anchor_lang::error!(RateLimitError::RateLimitExceeded),
            "must return RateLimitExceeded"
        );
    }

    #[test]
    fn exact_boundary_one_over_limit_fails() {
        let (owner, mut state) = fresh_state();
        for _ in 0..CFG.max_actions {
            state.check_and_record(&owner, &CFG, 0).unwrap();
        }
        // Exactly one call over the limit
        assert!(
            state.check_and_record(&owner, &CFG, 0).is_err(),
            "call at max_actions+1 must be rejected"
        );
    }

    // ── Window reset ──────────────────────────────────────────────────────────

    #[test]
    fn counter_resets_after_window_expires() {
        let (owner, mut state) = fresh_state();
        // Exhaust window starting at t=0
        for _ in 0..CFG.max_actions {
            state.check_and_record(&owner, &CFG, 0).unwrap();
        }
        assert!(state.check_and_record(&owner, &CFG, 0).is_err());

        // Advance time past the window
        let next_window = CFG.window_seconds;
        assert!(
            state.check_and_record(&owner, &CFG, next_window).is_ok(),
            "counter should reset and allow actions in new window"
        );
        assert_eq!(state.action_count, 1);
    }

    #[test]
    fn window_resets_at_exact_boundary() {
        let (owner, mut state) = fresh_state();
        for _ in 0..CFG.max_actions {
            state.check_and_record(&owner, &CFG, 0).unwrap();
        }
        // One second before reset — still blocked
        assert!(state.check_and_record(&owner, &CFG, CFG.window_seconds - 1).is_err());
        // Exactly at reset — allowed
        assert!(state.check_and_record(&owner, &CFG, CFG.window_seconds).is_ok());
    }

    #[test]
    fn multiple_windows_work_correctly() {
        let (owner, mut state) = fresh_state();
        for window in 0i64..3 {
            let t = window * CFG.window_seconds;
            for _ in 0..CFG.max_actions {
                assert!(
                    state.check_and_record(&owner, &CFG, t).is_ok(),
                    "window {window} should allow up to max_actions"
                );
            }
            assert!(
                state.check_and_record(&owner, &CFG, t).is_err(),
                "window {window} should reject over-limit"
            );
        }
    }

    // ── Config validation ──────────────────────────────────────────────────────

    #[test]
    fn zero_max_actions_is_rejected() {
        let (owner, mut state) = fresh_state();
        let bad_cfg = RateLimitConfig {
            window_seconds: 60,
            max_actions: 0,
        };
        assert!(
            state.check_and_record(&owner, &bad_cfg, 0).is_err(),
            "max_actions=0 must be rejected"
        );
    }

    // ── seconds_until_reset helper ────────────────────────────────────────────

    #[test]
    fn seconds_until_reset_is_accurate() {
        let (owner, mut state) = fresh_state();
        state.check_and_record(&owner, &CFG, 1000).unwrap();
        // 30 s into the window → 30 s remaining
        assert_eq!(state.seconds_until_reset(&CFG, 1030), 30);
        // After window expires → 0
        assert_eq!(state.seconds_until_reset(&CFG, 1070), 0);
    }
}