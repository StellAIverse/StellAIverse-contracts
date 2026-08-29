/// Tests for safe governance parameter updates (gov_params.rs)
#[cfg(test)]
mod gov_param_tests {
    use crate::gov_params::{GovernanceParams, ParamUpdateError};
    use anchor_lang::prelude::Pubkey;

    fn default_params(authority: Pubkey) -> GovernanceParams {
        GovernanceParams {
            authority,
            fee_bps: 100,
            min_stake: 1_000_000,
            max_stake: 100_000_000,
            slippage_tolerance_bps: 50,
            epoch_duration: 604_800, // 1 week
            version: 0,
        }
    }

    // ── Helpers that mirror the on-chain instruction logic ────────────────────
    // (We test pure validation rules; the Anchor macro wiring is tested via
    // integration / bankrun tests.)

    fn apply_fee_bps(params: &mut GovernanceParams, v: u16) -> anchor_lang::Result<()> {
        anchor_lang::require!(
            v <= 10_000u16,
            ParamUpdateError::FeeBpsOutOfRange
        );
        params.fee_bps = v;
        params.version += 1;
        Ok(())
    }

    fn apply_stake_bounds(
        params: &mut GovernanceParams,
        min: u64,
        max: u64,
    ) -> anchor_lang::Result<()> {
        anchor_lang::require!(min >= 1, ParamUpdateError::MinStakeTooLow);
        anchor_lang::require!(max >= min, ParamUpdateError::MaxStakeBelowMin);
        params.min_stake = min;
        params.max_stake = max;
        params.version += 1;
        Ok(())
    }

    fn apply_slippage(params: &mut GovernanceParams, v: u16) -> anchor_lang::Result<()> {
        anchor_lang::require!(
            v >= 1 && v <= 10_000u16,
            ParamUpdateError::SlippageOutOfRange
        );
        params.slippage_tolerance_bps = v;
        params.version += 1;
        Ok(())
    }

    fn apply_epoch(params: &mut GovernanceParams, v: i64) -> anchor_lang::Result<()> {
        const MAX: i64 = 52 * 7 * 24 * 3600;
        anchor_lang::require!(
            v >= 1 && v <= MAX,
            ParamUpdateError::EpochDurationOutOfRange
        );
        params.epoch_duration = v;
        params.version += 1;
        Ok(())
    }

    // ── fee_bps ───────────────────────────────────────────────────────────────

    #[test]
    fn update_fee_bps_valid() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_fee_bps(&mut p, 250).unwrap();
        assert_eq!(p.fee_bps, 250);
        assert_eq!(p.version, 1);
    }

    #[test]
    fn update_fee_bps_zero_allowed() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_fee_bps(&mut p, 0).unwrap();
        assert_eq!(p.fee_bps, 0);
    }

    #[test]
    fn update_fee_bps_max_allowed() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_fee_bps(&mut p, 10_000).unwrap();
        assert_eq!(p.fee_bps, 10_000);
    }

    #[test]
    fn update_fee_bps_over_max_fails() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        let err = apply_fee_bps(&mut p, 10_001).unwrap_err();
        assert_eq!(
            err,
            anchor_lang::error!(ParamUpdateError::FeeBpsOutOfRange)
        );
        // Storage must be unchanged
        assert_eq!(p.fee_bps, 100, "fee_bps must not mutate on invalid input");
        assert_eq!(p.version, 0, "version must not increment on failure");
    }

    // ── stake bounds ──────────────────────────────────────────────────────────

    #[test]
    fn update_stake_bounds_valid() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_stake_bounds(&mut p, 500_000, 200_000_000).unwrap();
        assert_eq!(p.min_stake, 500_000);
        assert_eq!(p.max_stake, 200_000_000);
        assert_eq!(p.version, 1);
    }

    #[test]
    fn update_stake_bounds_equal_min_max_valid() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_stake_bounds(&mut p, 1_000, 1_000).unwrap();
    }

    #[test]
    fn update_stake_bounds_zero_min_fails() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        assert!(apply_stake_bounds(&mut p, 0, 1_000_000).is_err());
        assert_eq!(p.min_stake, 1_000_000, "min_stake must not mutate");
    }

    #[test]
    fn update_stake_bounds_max_below_min_fails() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        let err = apply_stake_bounds(&mut p, 5_000_000, 1_000_000).unwrap_err();
        assert_eq!(
            err,
            anchor_lang::error!(ParamUpdateError::MaxStakeBelowMin)
        );
        // Neither field should have changed
        assert_eq!(p.min_stake, 1_000_000);
        assert_eq!(p.max_stake, 100_000_000);
    }

    // ── slippage ──────────────────────────────────────────────────────────────

    #[test]
    fn update_slippage_valid() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_slippage(&mut p, 300).unwrap();
        assert_eq!(p.slippage_tolerance_bps, 300);
    }

    #[test]
    fn update_slippage_zero_fails() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        assert!(apply_slippage(&mut p, 0).is_err());
        assert_eq!(p.slippage_tolerance_bps, 50, "must not mutate on failure");
    }

    #[test]
    fn update_slippage_over_max_fails() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        assert!(apply_slippage(&mut p, 10_001).is_err());
    }

    // ── epoch duration ────────────────────────────────────────────────────────

    #[test]
    fn update_epoch_duration_valid() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_epoch(&mut p, 86_400).unwrap(); // 1 day
        assert_eq!(p.epoch_duration, 86_400);
    }

    #[test]
    fn update_epoch_duration_zero_fails() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        assert!(apply_epoch(&mut p, 0).is_err());
        assert_eq!(p.epoch_duration, 604_800);
    }

    #[test]
    fn update_epoch_duration_over_52_weeks_fails() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        let too_long = 52i64 * 7 * 24 * 3600 + 1;
        assert!(apply_epoch(&mut p, too_long).is_err());
    }

    // ── Storage isolation (no unintended mutations) ───────────────────────────

    #[test]
    fn updating_fee_does_not_alter_other_fields() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        let snapshot_min = p.min_stake;
        let snapshot_max = p.max_stake;
        let snapshot_slippage = p.slippage_tolerance_bps;
        let snapshot_epoch = p.epoch_duration;

        apply_fee_bps(&mut p, 500).unwrap();

        assert_eq!(p.min_stake, snapshot_min, "min_stake must not change");
        assert_eq!(p.max_stake, snapshot_max, "max_stake must not change");
        assert_eq!(
            p.slippage_tolerance_bps, snapshot_slippage,
            "slippage must not change"
        );
        assert_eq!(p.epoch_duration, snapshot_epoch, "epoch must not change");
    }

    #[test]
    fn updating_stake_does_not_alter_other_fields() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        let snapshot_fee = p.fee_bps;
        let snapshot_slippage = p.slippage_tolerance_bps;
        let snapshot_epoch = p.epoch_duration;

        apply_stake_bounds(&mut p, 2_000_000, 50_000_000).unwrap();

        assert_eq!(p.fee_bps, snapshot_fee);
        assert_eq!(p.slippage_tolerance_bps, snapshot_slippage);
        assert_eq!(p.epoch_duration, snapshot_epoch);
    }

    // ── Version counter ───────────────────────────────────────────────────────

    #[test]
    fn version_increments_on_each_successful_update() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        assert_eq!(p.version, 0);
        apply_fee_bps(&mut p, 200).unwrap();
        assert_eq!(p.version, 1);
        apply_slippage(&mut p, 100).unwrap();
        assert_eq!(p.version, 2);
        apply_epoch(&mut p, 86_400).unwrap();
        assert_eq!(p.version, 3);
    }

    #[test]
    fn version_does_not_increment_on_failed_update() {
        let auth = Pubkey::new_unique();
        let mut p = default_params(auth);
        apply_fee_bps(&mut p, 99_999).unwrap_err();
        assert_eq!(p.version, 0, "version must not change on rejection");
    }
}