use crate::types::{OutcomePool, DECIMAL_FACTOR};

/// Integer square root (Newton's method, no_std compatible).
pub fn isqrt(n: i128) -> i128 {
    if n < 0 {
        panic!("Cannot take square root of negative number");
    }
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Ceiling division for positive numbers.
#[allow(dead_code)]
pub fn ceil_div(a: i128, b: i128) -> i128 {
    if b <= 0 {
        panic!("Divisor must be positive");
    }
    if a < 0 {
        panic!("Dividend must be non-negative for ceiling division");
    }
    (a + b - 1) / b
}

/// Compute the amount of outcome tokens received for buying with `collateral_in`.
/// CPMM: x * y = k → new_x = x + dx → new_y = k / new_x
/// Amount out = y - k / (x + dx)
/// Fee is deducted from collateral_in before the swap.
pub fn calculate_buy_amount(
    collateral_in: i128,
    collateral_reserve: i128,
    outcome_reserve: i128,
    fee_bps: u32,
) -> i128 {
    if collateral_in <= 0 || collateral_reserve <= 0 || outcome_reserve <= 0 {
        return 0;
    }
    let fee_factor = 10_000 - fee_bps as i128;
    let net_collateral = (collateral_in * fee_factor) / 10_000;
    if net_collateral <= 0 {
        return 0;
    }
    // k = x * y
    // new_y = (x * y) / (x + dx)
    // dy = y - (x * y) / (x + dx) = y * dx / (x + dx)
    let numerator = outcome_reserve * net_collateral;
    let denominator = collateral_reserve + net_collateral;
    numerator / denominator
}

/// Compute the amount of collateral received for selling `outcome_amount` tokens.
/// CPMM: new_x = k / (new_y) where new_y = y - dy
/// Collateral out = new_x - x = k / (y - dy) - x
/// Fee is deducted from the collateral out.
pub fn calculate_sell_amount(
    outcome_amount: i128,
    collateral_reserve: i128,
    outcome_reserve: i128,
    fee_bps: u32,
) -> i128 {
    if outcome_amount <= 0 || collateral_reserve <= 0 || outcome_reserve <= 0 {
        return 0;
    }
    if outcome_amount >= outcome_reserve {
        return 0; // Cannot sell all or more than reserve
    }
    // When selling outcome tokens INTO the pool:
    // new_y = y + dy (outcome reserve increases)
    // new_x = k / new_y (collateral reserve decreases)
    // collateral_out = x - new_x
    let k = collateral_reserve * outcome_reserve;
    let new_outcome_reserve = outcome_reserve + outcome_amount;
    let new_collateral_reserve = k / new_outcome_reserve;
    let gross_collateral = collateral_reserve - new_collateral_reserve;
    let fee_factor = 10_000 - fee_bps as i128;
    (gross_collateral * fee_factor) / 10_000
}

/// Compute the collateral required to buy a desired `outcome_amount`.
/// Reverse of calculate_buy_amount (rounds up to ensure sufficient collateral).
#[allow(dead_code)]
pub fn calculate_buy_collateral_required(
    outcome_amount: i128,
    collateral_reserve: i128,
    outcome_reserve: i128,
    fee_bps: u32,
) -> i128 {
    if outcome_amount <= 0 || collateral_reserve <= 0 || outcome_reserve <= 0 {
        return 0;
    }
    if outcome_amount >= outcome_reserve {
        return i128::MAX; // Impossible to buy this much
    }
    // dy = y * dx / (x + dx) → dx = x * dy / (y - dy)
    let numerator = collateral_reserve * outcome_amount;
    let denominator = outcome_reserve - outcome_amount;
    let net_collateral = ceil_div(numerator, denominator);
    // Apply fee: net_collateral = collateral * (1 - fee/10000)
    // → collateral = net_collateral * 10000 / (10000 - fee)
    let fee_factor = 10_000 - fee_bps as i128;
    ceil_div(net_collateral * 10_000, fee_factor)
}

/// Calculate LP shares to mint when adding liquidity to an outcome pool.
/// For initial liquidity: shares = sqrt(collateral * outcome)
/// For subsequent: shares = min(collateral / reserve_c * total, outcome / reserve_o * total)
pub fn calculate_lp_shares_add(collateral_in: i128, outcome_in: i128, pool: &OutcomePool) -> i128 {
    if pool.lp_total_supply == 0 {
        // Initial liquidity
        isqrt(collateral_in * outcome_in)
    } else {
        let share_c = (collateral_in * pool.lp_total_supply) / pool.collateral_reserve;
        let share_o = (outcome_in * pool.lp_total_supply) / pool.outcome_reserve;
        if share_c < share_o {
            share_c
        } else {
            share_o
        }
    }
}

/// Calculate collateral and outcome amounts to withdraw for burning `lp_amount` shares.
pub fn calculate_lp_withdraw(lp_amount: i128, pool: &OutcomePool) -> (i128, i128) {
    let collateral_out = (lp_amount * pool.collateral_reserve) / pool.lp_total_supply;
    let outcome_out = (lp_amount * pool.outcome_reserve) / pool.lp_total_supply;
    (collateral_out, outcome_out)
}

/// Get the implied probability (price) of an outcome, scaled by DECIMAL_FACTOR.
/// price = collateral_reserve / (collateral_reserve + outcome_reserve)
pub fn get_outcome_price(collateral_reserve: i128, outcome_reserve: i128) -> i128 {
    if collateral_reserve + outcome_reserve == 0 {
        return 0;
    }
    (collateral_reserve * DECIMAL_FACTOR) / (collateral_reserve + outcome_reserve)
}

/// Check that x * y = k invariant holds (approximately, allowing for fee accumulation).
#[allow(dead_code)]
pub fn check_cpmm_invariant(old_k: i128, new_collateral: i128, new_outcome: i128) -> bool {
    let new_k = new_collateral * new_outcome;
    // k should increase or stay the same (fees accumulate in pool)
    new_k >= old_k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(10), 3);
        assert_eq!(isqrt(100), 10);
        assert_eq!(isqrt(1_000_000), 1_000);
    }

    #[test]
    fn test_ceil_div() {
        assert_eq!(ceil_div(10, 3), 4);
        assert_eq!(ceil_div(9, 3), 3);
        assert_eq!(ceil_div(1, 1), 1);
    }

    #[test]
    fn test_buy_amount_basic() {
        // Pool: 10000 collateral, 10000 outcome, 30 bps fee
        let out = calculate_buy_amount(1_000, 10_000, 10_000, 30);
        // net_collateral = 1000 * 9970/10000 = 997
        // dy = 10000 * 997 / (10000 + 997) = 9970000 / 10997 ≈ 906
        assert!(out > 900);
        assert!(out < 1_000);
    }

    #[test]
    fn test_buy_amount_zero_collateral() {
        assert_eq!(calculate_buy_amount(0, 10_000, 10_000, 30), 0);
    }

    #[test]
    fn test_sell_amount_basic() {
        let out = calculate_sell_amount(1_000, 10_000, 10_000, 30);
        // Selling 1000 outcome into pool:
        // new_y = 11000, k = 100_000_000, new_x = 100_000_000 / 11000 ≈ 9091
        // gross = 10000 - 9091 = 909, net = 909 * 9970/10000 ≈ 906
        assert!(out > 900);
        assert!(out < 910);
    }

    #[test]
    fn test_sell_amount_full_reserve() {
        // Cannot sell all tokens in reserve
        assert_eq!(calculate_sell_amount(10_000, 10_000, 10_000, 30), 0);
    }

    #[test]
    fn test_buy_collateral_required_basic() {
        let required = calculate_buy_collateral_required(1_000, 10_000, 10_000, 30);
        // Should require slightly more than 1_000 due to fee
        assert!(required >= 1_000);
        assert!(required < 2_000);
    }

    #[test]
    fn test_lp_shares_initial() {
        let pool = OutcomePool {
            collateral_reserve: 0,
            outcome_reserve: 0,
            lp_total_supply: 0,
        };
        let shares = calculate_lp_shares_add(10_000, 10_000, &pool);
        assert_eq!(shares, isqrt(10_000 * 10_000)); // 10_000
    }

    #[test]
    fn test_lp_shares_subsequent() {
        let pool = OutcomePool {
            collateral_reserve: 10_000,
            outcome_reserve: 10_000,
            lp_total_supply: 10_000,
        };
        let shares = calculate_lp_shares_add(5_000, 5_000, &pool);
        assert_eq!(shares, 5_000);
    }

    #[test]
    fn test_lp_withdraw() {
        let pool = OutcomePool {
            collateral_reserve: 10_000,
            outcome_reserve: 10_000,
            lp_total_supply: 10_000,
        };
        let (c, o) = calculate_lp_withdraw(5_000, &pool);
        assert_eq!(c, 5_000);
        assert_eq!(o, 5_000);
    }

    #[test]
    fn test_get_outcome_price() {
        // 50/50 pool → price = 0.5 * DECIMAL_FACTOR
        let price = get_outcome_price(10_000, 10_000);
        assert_eq!(price, DECIMAL_FACTOR / 2);
    }

    #[test]
    fn test_cpmm_invariant_holds() {
        let old_k = 100_000i128 * 100_000i128;
        // After a buy with fee, collateral increases more than outcome decreases
        // because fees stay in pool. Simulate a small buy:
        let collateral_in = 1_000i128;
        let fee_bps = 30u32;
        let dy = calculate_buy_amount(collateral_in, 100_000, 100_000, fee_bps);
        let fee_factor = 10_000 - fee_bps as i128;
        let net = (collateral_in * fee_factor) / 10_000;
        let new_x = 100_000 + net;
        let new_y = 100_000 - dy;
        let new_k = new_x * new_y;
        assert!(
            new_k >= old_k,
            "CPMM invariant violated: new_k={new_k} < old_k={old_k}",
        );
    }

    #[test]
    fn test_buy_conserves_value() {
        // After a buy, new_x * new_y should be >= old_x * old_y
        let old_x = 100_000i128;
        let old_y = 100_000i128;
        let old_k = old_x * old_y;

        let collateral_in = 1_000i128;
        let fee_bps = 30u32;
        let dy = calculate_buy_amount(collateral_in, old_x, old_y, fee_bps);

        let fee_factor = 10_000 - fee_bps as i128;
        let net = (collateral_in * fee_factor) / 10_000;
        let new_x = old_x + net;
        let new_y = old_y - dy;
        let new_k = new_x * new_y;

        assert!(
            new_k >= old_k,
            "CPMM invariant violated: new_k={new_k} < old_k={old_k}",
        );
    }

    #[test]
    fn test_sell_conserves_value() {
        let old_x = 100_000i128;
        let old_y = 100_000i128;
        let old_k = old_x * old_y;

        let dy = 1_000i128; // outcome tokens sold into pool
        let fee_bps = 30u32;

        // When selling outcome tokens into pool:
        // new_y = y + dy (outcome reserve increases)
        // new_x = k / new_y (collateral reserve decreases)
        // collateral_out = x - new_x (before fee)
        let new_y = old_y + dy;
        let new_x_from_k = old_k / new_y;
        let gross = old_x - new_x_from_k; // collateral taken out
        let fee_factor = 10_000 - fee_bps as i128;
        let net_out = (gross * fee_factor) / 10_000;

        let final_x = old_x - net_out; // less collateral in pool
        let final_y = new_y; // more outcome in pool
        let final_k = final_x * final_y;

        // k should grow due to fees staying in pool
        assert!(final_k >= old_k, "CPMM invariant violated after sell");
    }
}
