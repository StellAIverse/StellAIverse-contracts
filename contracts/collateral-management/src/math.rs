use crate::types::{HealthFactor, ProtocolParams};

/// Basis points denominator: 10_000 = 100%.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Seconds in a year (365 days).
pub const SECONDS_PER_YEAR: i128 = 365 * 24 * 60 * 60;

/// Calculate the current utilization rate for a borrow token.
/// Returns utilization in basis points (0–10000).
pub fn calculate_utilization(total_borrowed: i128, total_deposits: i128) -> u32 {
    if total_deposits <= 0 {
        return 0;
    }
    let util_bps = (total_borrowed * BPS_DENOMINATOR) / total_deposits;
    if util_bps > 10_000 {
        10_000
    } else if util_bps < 0 {
        0
    } else {
        util_bps as u32
    }
}

/// Dynamic interest rate model based on utilization (kink model).
///
/// Below optimal utilization: rate = base + (slope1 * util / optimal).
/// Above optimal utilization: rate = base + slope1 + (slope2 * (util - optimal) / (10000 - optimal)).
///
/// Returns the annual interest rate in basis points.
pub fn calculate_interest_rate(params: &ProtocolParams, utilization_bps: u32) -> u32 {
    let base = params.base_interest_rate_bps as i128;
    let slope1 = params.interest_slope1_bps as i128;
    let slope2 = params.interest_slope2_bps as i128;
    let optimal = params.optimal_utilization_bps as i128;
    let util = utilization_bps as i128;

    if optimal <= 0 {
        return base as u32;
    }

    let rate_bps = if util <= optimal {
        base + (slope1 * util) / optimal
    } else {
        let excess = util - optimal;
        let max_excess = BPS_DENOMINATOR - optimal;
        if max_excess <= 0 {
            base + slope1 + slope2
        } else {
            base + slope1 + (slope2 * excess) / max_excess
        }
    };

    rate_bps.min(u32::MAX as i128) as u32
}

/// Calculate accrued interest for a loan since the last accrual update.
pub fn calculate_accrued_interest(
    principal: i128,
    annual_rate_bps: u32,
    time_delta_seconds: i128,
) -> i128 {
    if principal <= 0 || annual_rate_bps == 0 || time_delta_seconds <= 0 {
        return 0;
    }
    let rate = annual_rate_bps as i128;
    (principal * rate * time_delta_seconds) / (BPS_DENOMINATOR * SECONDS_PER_YEAR)
}

/// Calculate the health factor of a user's position.
///
/// A health factor >= 10000 means healthy.
/// A health factor < 10000 means undercollateralized.
pub fn calculate_health_factor(
    total_collateral_value: i128,
    total_debt: i128,
    weighted_threshold_bps: u32,
) -> HealthFactor {
    if total_debt <= 0 {
        return HealthFactor {
            health_factor_bps: i128::MAX / 2,
            total_collateral_value,
            total_debt: 0,
            is_healthy: true,
        };
    }

    if total_collateral_value <= 0 {
        return HealthFactor {
            health_factor_bps: 0,
            total_collateral_value: 0,
            total_debt,
            is_healthy: false,
        };
    }

    let threshold = weighted_threshold_bps as i128;
    let hf_bps = (total_collateral_value * threshold) / total_debt;

    HealthFactor {
        health_factor_bps: hf_bps,
        total_collateral_value,
        total_debt,
        is_healthy: hf_bps >= BPS_DENOMINATOR,
    }
}

/// Calculate the maximum borrow amount a user can take given their collateral.
#[allow(dead_code)]
pub fn calculate_max_borrow(collateral_value: i128, ltv_bps: u32) -> i128 {
    let ltv = ltv_bps as i128;
    (collateral_value * ltv) / BPS_DENOMINATOR
}

/// Calculate the liquidation amount that seizes enough collateral to cover debt + bonus.
///
/// Returns (collateral_seized, bonus_amount).
pub fn calculate_liquidation_seizure(
    debt_to_cover: i128,
    collateral_price: i128,
    liquidation_bonus_bps: u32,
) -> (i128, i128) {
    if debt_to_cover <= 0 || collateral_price <= 0 {
        return (0, 0);
    }

    let bonus_factor = BPS_DENOMINATOR + liquidation_bonus_bps as i128;
    let numerator = debt_to_cover * bonus_factor;
    let denominator = collateral_price * BPS_DENOMINATOR;

    let collateral_seized = (numerator + denominator - 1) / denominator;
    let bonus_amount = (debt_to_cover * liquidation_bonus_bps as i128) / BPS_DENOMINATOR;

    (collateral_seized, bonus_amount)
}

/// Calculate the weighted liquidation threshold across multiple collateral types.
/// Returns the effective threshold in basis points.
#[allow(dead_code)]
pub fn calculate_weighted_liquidation_threshold(
    collateral_values: &[(i128, u32)],
    total_collateral_value: i128,
) -> u32 {
    if total_collateral_value <= 0 {
        return 0;
    }

    let mut weighted_sum: i128 = 0;
    for (value, threshold_bps) in collateral_values.iter() {
        weighted_sum += value * *threshold_bps as i128;
    }

    let weighted = weighted_sum / total_collateral_value;
    weighted.min(u32::MAX as i128) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utilization_zero_deposits() {
        assert_eq!(calculate_utilization(0, 0), 0);
    }

    #[test]
    fn test_utilization_half() {
        assert_eq!(calculate_utilization(500, 1000), 5000);
    }

    #[test]
    fn test_utilization_full() {
        assert_eq!(calculate_utilization(1000, 1000), 10000);
    }

    #[test]
    fn test_utilization_over_100() {
        assert_eq!(calculate_utilization(1500, 1000), 10000);
    }

    fn default_params() -> ProtocolParams {
        ProtocolParams {
            debt_ceiling: 0,
            liq_health_threshold_bps: 10000,
            base_interest_rate_bps: 200,
            interest_slope1_bps: 400,
            interest_slope2_bps: 7500,
            optimal_utilization_bps: 8000,
            max_borrow_per_user: 0,
            max_collateral_per_user: 0,
        }
    }

    #[test]
    fn test_interest_rate_below_optimal() {
        let params = default_params();
        assert_eq!(calculate_interest_rate(&params, 0), 200);
        assert_eq!(calculate_interest_rate(&params, 4000), 400);
        assert_eq!(calculate_interest_rate(&params, 8000), 600);
    }

    #[test]
    fn test_interest_rate_above_optimal() {
        let params = default_params();
        assert_eq!(calculate_interest_rate(&params, 9000), 4350);
        assert_eq!(calculate_interest_rate(&params, 10000), 8100);
    }

    #[test]
    fn test_accrued_interest_basic() {
        let interest = calculate_accrued_interest(10_000, 1000, SECONDS_PER_YEAR);
        assert_eq!(interest, 1000);
    }

    #[test]
    fn test_accrued_interest_half_year() {
        let interest = calculate_accrued_interest(10_000, 1000, SECONDS_PER_YEAR / 2);
        assert_eq!(interest, 500);
    }

    #[test]
    fn test_accrued_interest_zero_principal() {
        assert_eq!(calculate_accrued_interest(0, 1000, SECONDS_PER_YEAR), 0);
    }

    #[test]
    fn test_accrued_interest_zero_rate() {
        assert_eq!(calculate_accrued_interest(10_000, 0, SECONDS_PER_YEAR), 0);
    }

    #[test]
    fn test_health_factor_healthy() {
        let hf = calculate_health_factor(15_000, 10_000, 10000);
        assert_eq!(hf.health_factor_bps, 15000);
        assert!(hf.is_healthy);
    }

    #[test]
    fn test_health_factor_exactly_1() {
        let hf = calculate_health_factor(10_000, 10_000, 10000);
        assert_eq!(hf.health_factor_bps, 10000);
        assert!(hf.is_healthy);
    }

    #[test]
    fn test_health_factor_unhealthy() {
        let hf = calculate_health_factor(8_000, 10_000, 10000);
        assert_eq!(hf.health_factor_bps, 8000);
        assert!(!hf.is_healthy);
    }

    #[test]
    fn test_health_factor_no_debt() {
        let hf = calculate_health_factor(10_000, 0, 10000);
        assert!(hf.is_healthy);
        assert!(hf.health_factor_bps > 100_000);
    }

    #[test]
    fn test_health_factor_no_collateral() {
        let hf = calculate_health_factor(0, 10_000, 10000);
        assert!(!hf.is_healthy);
        assert_eq!(hf.health_factor_bps, 0);
    }

    #[test]
    fn test_health_factor_with_threshold() {
        let hf = calculate_health_factor(10_000, 8_000, 8500);
        assert_eq!(hf.health_factor_bps, 10625);
        assert!(hf.is_healthy);
    }

    #[test]
    fn test_max_borrow() {
        assert_eq!(calculate_max_borrow(10_000, 7500), 7_500);
    }

    #[test]
    fn test_max_borrow_full_ltv() {
        assert_eq!(calculate_max_borrow(10_000, 10000), 10_000);
    }

    #[test]
    fn test_liquidation_seizure() {
        let (seized, bonus) = calculate_liquidation_seizure(1000, 2, 500);
        assert_eq!(seized, 525);
        assert_eq!(bonus, 50);
    }

    #[test]
    fn test_liquidation_seizure_zero_debt() {
        let (seized, bonus) = calculate_liquidation_seizure(0, 2, 500);
        assert_eq!(seized, 0);
        assert_eq!(bonus, 0);
    }

    #[test]
    fn test_weighted_liquidation_threshold() {
        let values = [(10_000i128, 8500u32), (5_000i128, 7500u32)];
        let total = 15_000;
        let wt = calculate_weighted_liquidation_threshold(&values, total);
        assert_eq!(wt, 8166);
    }

    #[test]
    fn test_weighted_threshold_zero_total() {
        let values = [(10_000i128, 8500u32)];
        let wt = calculate_weighted_liquidation_threshold(&values, 0);
        assert_eq!(wt, 0);
    }
}
