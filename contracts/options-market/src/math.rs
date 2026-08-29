use crate::errors::OptionsError;
use crate::types::{OptionType, PRECISION, SECONDS_PER_YEAR};

// ── Constants ───────────────────────────────────────────────────────────────

/// Default risk-free interest rate: 5% annualized (scaled by PRECISION).
pub const DEFAULT_RISK_FREE_RATE: i128 = 500; // 5.00%

/// Default implied volatility: 50% annualized (scaled by PRECISION).
pub const DEFAULT_VOLATILITY: i128 = 5_000; // 50.00%

/// Maximum allowed volatility: 500% annualized.
pub const MAX_VOLATILITY: i128 = 50_000; // 500.00%

/// Minimum allowed volatility: 1%.
pub const MIN_VOLATILITY: i128 = 100; // 1.00%

/// Maximum time to expiry: 365 days.
pub const MAX_TIME_TO_EXPIRY_SECONDS: u64 = 365 * 24 * 60 * 60;

// ── Helper Functions ────────────────────────────────────────────────────────

/// Integer square root using Newton's method (no_std compatible).
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

/// Approximation of the standard normal CDF using Abramowitz & Stegun formula.
/// Input: x scaled by PRECISION (e.g., 10000 = 1.0).
/// Output: probability scaled by PRECISION (e.g., 8413 = 0.8413).
pub fn normal_cdf_approx(x: i128) -> i128 {
    // For large negative x, CDF ≈ 0
    if x < -6 * PRECISION {
        return 0;
    }
    // For large positive x, CDF ≈ 1
    if x > 6 * PRECISION {
        return PRECISION;
    }

    // Abramowitz & Stegun approximation 26.2.17
    // CDF(x) ≈ 1 - φ(x)(a1*t + a2*t² + a3*t³)
    // where t = 1/(1 + 0.2316419*|x|)
    let abs_x = if x < 0 { -x } else { x };
    let t_denom = 10_000 + (2316 * abs_x) / PRECISION; // 1 + 0.2316419 * |x|
    let t = (PRECISION * PRECISION) / t_denom; // 1 / t_denom * PRECISION

    // Coefficients scaled by PRECISION
    let a1: i128 = 3193; // 0.319381530
    let a2: i128 = -3566; // -0.356563782
    let a3: i128 = 1781; // 1.781477937
    let a4: i128 = -1821; // -1.821255978
    let a5: i128 = 1330; // 1.330274429

    // Polynomial: a1*t + a2*t² + a3*t³ + a4*t⁴ + a5*t⁵
    let poly = (a1 * t
        + a2 * t * t / PRECISION / PRECISION * PRECISION
        + a3 * t * t * t / PRECISION / PRECISION / PRECISION * PRECISION * PRECISION
        + a4 * t * t * t * t / PRECISION / PRECISION / PRECISION / PRECISION
            * PRECISION
            * PRECISION
            * PRECISION
        + a5 * t * t * t * t * t / PRECISION / PRECISION / PRECISION / PRECISION / PRECISION
            * PRECISION
            * PRECISION
            * PRECISION
            * PRECISION)
        / PRECISION;

    // Standard normal PDF: φ(x) = exp(-x²/2) / sqrt(2π)
    let x_squared = x * x / PRECISION;
    let half_x_sq = x_squared / 2;

    // Approximation of exp(-x²/2) using Taylor expansion
    let exp_val = approx_exp(-half_x_sq);

    // 1/sqrt(2π) ≈ 0.39894228
    let inv_sqrt_2pi: i128 = 3989; // 0.3989 * PRECISION / 1000
    let pdf = exp_val * inv_sqrt_2pi / 1000 / PRECISION;

    // CDF = 1 - pdf * poly (when x >= 0)
    // CDF = pdf * poly (when x < 0)
    let one_minus = PRECISION - pdf * poly / PRECISION;

    if x >= 0 {
        one_minus
    } else {
        PRECISION - one_minus
    }
}

/// Approximation of exp(x) for small |x| using Taylor series.
/// Input and output scaled by PRECISION.
pub fn approx_exp(x: i128) -> i128 {
    // For very negative x, exp(x) ≈ 0
    if x < -10 * PRECISION {
        return 0;
    }
    // For very positive x, use bounds
    if x > 10 * PRECISION {
        // Return a large number bounded to prevent overflow
        return 22_026 * PRECISION; // e^10 ≈ 22026
    }

    // Taylor series: exp(x) = 1 + x + x²/2! + x³/3! + ...
    // We iterate enough terms for good precision
    let mut result: i128 = PRECISION; // 1.0
    let mut term: i128 = PRECISION; // Current term
    let mut i: i128 = 1;

    // Limit iterations to prevent overflow and ensure convergence
    while i <= 20 {
        term = term * x / (i * PRECISION);
        result += term;
        i += 1;
        // Early termination if term becomes negligible
        if term < 1 && term > -1 {
            break;
        }
    }

    result
}

/// Approximation of ln(x) for x > 0 using the identity ln(x) = 2 * atanh((x-1)/(x+1)).
/// Input and output scaled by PRECISION.
pub fn approx_ln(x: i128) -> i128 {
    if x <= 0 {
        return i128::MIN / 2; // Error case
    }

    // For values close to PRECISION (i.e., close to 1.0), ln(x) ≈ x - PRECISION
    if x > PRECISION - 100 && x < PRECISION + 100 {
        return x - PRECISION;
    }

    // Normalize: find k such that x ≈ PRECISION * e^k
    // Use repeated division by e ≈ 2.71828
    let e_val: i128 = 27183; // e * PRECISION / 10000
    let mut k: i128 = 0;
    let mut normalized = x;

    // Scale up for large values
    while normalized > 4 * PRECISION {
        normalized = normalized * 10_000 / e_val;
        k += PRECISION;
    }
    // Scale down for small values
    while normalized < PRECISION / 4 {
        normalized = normalized * e_val / 10_000;
        k -= PRECISION;
    }

    // Now normalized is roughly in [0.25, 4.0]
    // Use Padé approximation for ln around 1.0
    // ln(x) ≈ (x-1) - (x-1)²/2 + (x-1)³/3 - ...
    let t = normalized - PRECISION; // t = x - 1, scaled by PRECISION
    let mut result = t;
    let mut term = t;
    let mut sign: i128 = -1;

    for n in 2..=15 {
        term = term * t / PRECISION;
        result += sign * term / n;
        sign = -sign;
    }

    result + k
}

/// Calculate time to expiration in years, scaled by PRECISION.
pub fn time_to_expiry_years(current_time: u64, expiration: u64) -> i128 {
    if expiration <= current_time {
        return 0;
    }
    let seconds = (expiration - current_time) as i128;
    (seconds * PRECISION) / SECONDS_PER_YEAR
}

// ── Black-Scholes ───────────────────────────────────────────────────────────

/// Calculate d1 in the Black-Scholes model.
/// All inputs scaled by PRECISION.
pub fn calculate_d1(
    spot: i128,
    strike: i128,
    time_to_expiry: i128, // in years, scaled by PRECISION
    risk_free_rate: i128, // annualized, scaled by PRECISION
    volatility: i128,     // annualized, scaled by PRECISION
) -> i128 {
    if strike <= 0 || time_to_expiry <= 0 || volatility <= 0 {
        return 0;
    }

    // d1 = [ln(S/K) + (r + σ²/2) * T] / (σ * √T)

    // ln(S/K)
    let ln_sk = if spot > 0 && strike > 0 {
        let ratio = (spot * PRECISION) / strike;
        approx_ln(ratio)
    } else {
        return 0;
    };

    // σ²/2
    let vol_sq_half = (volatility * volatility / PRECISION) / 2;

    // (r + σ²/2) * T
    let drift = ((risk_free_rate + vol_sq_half) * time_to_expiry) / PRECISION;

    // Numerator: ln(S/K) + drift
    let numerator = ln_sk + drift;

    // Denominator: σ * √T
    let sqrt_t = isqrt(time_to_expiry * PRECISION);
    let denominator = volatility * sqrt_t / PRECISION;

    if denominator <= 0 {
        return 0;
    }

    (numerator * PRECISION) / denominator
}

/// Calculate d2 from d1.
pub fn calculate_d2(d1: i128, time_to_expiry: i128, volatility: i128) -> i128 {
    // d2 = d1 - σ * √T
    let sqrt_t = isqrt(time_to_expiry * PRECISION);
    let vol_sqrt_t = volatility * sqrt_t / PRECISION;
    d1 - vol_sqrt_t
}

/// Calculate Black-Scholes option price.
/// Returns premium scaled by PRECISION (4 decimal places).
pub fn black_scholes_price(
    spot: i128,
    strike: i128,
    time_to_expiry: i128,
    risk_free_rate: i128,
    volatility: i128,
    option_type: OptionType,
) -> Result<i128, OptionsError> {
    if spot <= 0 || strike <= 0 || time_to_expiry <= 0 || volatility <= 0 {
        return Err(OptionsError::PricingFailed);
    }

    let d1 = calculate_d1(spot, strike, time_to_expiry, risk_free_rate, volatility);
    let d2 = calculate_d2(d1, time_to_expiry, volatility);

    let nd1 = normal_cdf_approx(d1);
    let nd2 = normal_cdf_approx(d2);

    // e^(-rT)
    let neg_rt = -(risk_free_rate * time_to_expiry) / PRECISION;
    let exp_neg_rt = approx_exp(neg_rt);

    match option_type {
        OptionType::Call => {
            // C = S * N(d1) - K * e^(-rT) * N(d2)
            let term1 = spot * nd1 / PRECISION;
            let term2 = strike * exp_neg_rt / PRECISION * nd2 / PRECISION;
            Ok(term1 - term2)
        }
        OptionType::Put => {
            // P = K * e^(-rT) * N(-d2) - S * N(-d1)
            let n_neg_d1 = PRECISION - nd1;
            let n_neg_d2 = PRECISION - nd2;
            let term1 = strike * exp_neg_rt / PRECISION * n_neg_d2 / PRECISION;
            let term2 = spot * n_neg_d1 / PRECISION;
            Ok(term1 - term2)
        }
    }
}

// ── Greeks Calculations ─────────────────────────────────────────────────────

/// Calculate Delta: rate of change of option price w.r.t. underlying price.
/// Delta ∈ [-1, 1] scaled by PRECISION.
pub fn calculate_delta(
    spot: i128,
    strike: i128,
    time_to_expiry: i128,
    risk_free_rate: i128,
    volatility: i128,
    option_type: OptionType,
) -> i128 {
    if spot <= 0 || strike <= 0 || time_to_expiry <= 0 || volatility <= 0 {
        return 0;
    }

    let d1 = calculate_d1(spot, strike, time_to_expiry, risk_free_rate, volatility);
    let nd1 = normal_cdf_approx(d1);

    match option_type {
        OptionType::Call => nd1,
        OptionType::Put => nd1 - PRECISION, // N(d1) - 1
    }
}

/// Calculate Gamma: rate of change of delta w.r.t. underlying price.
/// Gamma ∈ [0, ∞) scaled by PRECISION.
pub fn calculate_gamma(
    spot: i128,
    strike: i128,
    time_to_expiry: i128,
    risk_free_rate: i128,
    volatility: i128,
) -> i128 {
    if spot <= 0 || strike <= 0 || time_to_expiry <= 0 || volatility <= 0 {
        return 0;
    }

    let d1 = calculate_d1(spot, strike, time_to_expiry, risk_free_rate, volatility);

    // φ(d1) = exp(-d1²/2) / sqrt(2π)
    let d1_sq = d1 * d1 / PRECISION;
    let neg_half_d1_sq = -(d1_sq / 2);
    let exp_val = approx_exp(neg_half_d1_sq);
    let inv_sqrt_2pi: i128 = 3989; // 0.3989 * PRECISION / 1000
    let phi_d1 = exp_val * inv_sqrt_2pi / 1000 / PRECISION;

    // Denominator: S * σ * √T
    let sqrt_t = isqrt(time_to_expiry * PRECISION);
    let denominator = spot * volatility * sqrt_t / PRECISION / PRECISION;

    if denominator <= 0 {
        return 0;
    }

    (phi_d1 * PRECISION) / denominator
}

/// Calculate Vega: sensitivity to volatility changes.
/// Vega scaled by PRECISION.
pub fn calculate_vega(
    spot: i128,
    strike: i128,
    time_to_expiry: i128,
    risk_free_rate: i128,
    volatility: i128,
) -> i128 {
    if spot <= 0 || strike <= 0 || time_to_expiry <= 0 || volatility <= 0 {
        return 0;
    }

    let d1 = calculate_d1(spot, strike, time_to_expiry, risk_free_rate, volatility);

    // φ(d1) = exp(-d1²/2) / sqrt(2π)
    let d1_sq = d1 * d1 / PRECISION;
    let neg_half_d1_sq = -(d1_sq / 2);
    let exp_val = approx_exp(neg_half_d1_sq);
    let inv_sqrt_2pi: i128 = 3989;
    let phi_d1 = exp_val * inv_sqrt_2pi / 1000 / PRECISION;

    // Vega = S * φ(d1) * √T
    let sqrt_t = isqrt(time_to_expiry * PRECISION);
    let vega = spot * phi_d1 * sqrt_t / PRECISION / PRECISION;

    // Scale by 1% volatility change (standard Vega definition)
    vega / 100
}

/// Calculate Theta: time decay of the option.
/// Theta (annualized) scaled by PRECISION.
pub fn calculate_theta(
    spot: i128,
    strike: i128,
    time_to_expiry: i128,
    risk_free_rate: i128,
    volatility: i128,
    option_type: OptionType,
) -> i128 {
    if spot <= 0 || strike <= 0 || time_to_expiry <= 0 || volatility <= 0 {
        return 0;
    }

    let d1 = calculate_d1(spot, strike, time_to_expiry, risk_free_rate, volatility);
    let d2 = calculate_d2(d1, time_to_expiry, volatility);

    let nd1 = normal_cdf_approx(d1);
    let nd2 = normal_cdf_approx(d2);

    // φ(d1)
    let d1_sq = d1 * d1 / PRECISION;
    let neg_half_d1_sq = -(d1_sq / 2);
    let exp_val = approx_exp(neg_half_d1_sq);
    let inv_sqrt_2pi: i128 = 3989;
    let phi_d1 = exp_val * inv_sqrt_2pi / 1000 / PRECISION;

    let sqrt_t = isqrt(time_to_expiry * PRECISION);

    // e^(-rT)
    let neg_rt = -(risk_free_rate * time_to_expiry) / PRECISION;
    let exp_neg_rt = approx_exp(neg_rt);

    match option_type {
        OptionType::Call => {
            // Theta_call = [-S*φ(d1)*σ/(2*√T) - r*K*e^(-rT)*N(d2)] (daily)
            let vol_term = -(spot * phi_d1 * volatility) / (2 * sqrt_t * PRECISION * PRECISION);
            let rate_term = -(risk_free_rate * strike * exp_neg_rt * nd2)
                / (PRECISION * PRECISION * PRECISION * SECONDS_PER_YEAR / (24 * 3600));
            let annual_term =
                -(risk_free_rate * strike * exp_neg_rt * nd2) / (PRECISION * PRECISION);
            (vol_term + annual_term) * PRECISION / SECONDS_PER_YEAR * (24 * 3600)
        }
        OptionType::Put => {
            // Theta_put = [-S*φ(d1)*σ/(2*√T) + r*K*e^(-rT)*N(-d2)] (daily)
            let n_neg_d2 = PRECISION - nd2;
            let vol_term = -(spot * phi_d1 * volatility) / (2 * sqrt_t * PRECISION * PRECISION);
            let annual_term =
                (risk_free_rate * strike * exp_neg_rt * n_neg_d2) / (PRECISION * PRECISION);
            (vol_term + annual_term) * PRECISION / SECONDS_PER_YEAR * (24 * 3600)
        }
    }
}

/// Calculate Rho: sensitivity to interest rate changes.
/// Rho scaled by PRECISION.
pub fn calculate_rho(
    spot: i128,
    strike: i128,
    time_to_expiry: i128,
    risk_free_rate: i128,
    volatility: i128,
    option_type: OptionType,
) -> i128 {
    if spot <= 0 || strike <= 0 || time_to_expiry <= 0 || volatility <= 0 {
        return 0;
    }

    let d1 = calculate_d1(spot, strike, time_to_expiry, risk_free_rate, volatility);
    let d2 = calculate_d2(d1, time_to_expiry, volatility);
    let nd2 = normal_cdf_approx(d2);

    // e^(-rT)
    let neg_rt = -(risk_free_rate * time_to_expiry) / PRECISION;
    let exp_neg_rt = approx_exp(neg_rt);

    match option_type {
        OptionType::Call => {
            // Rho_call = K * T * e^(-rT) * N(d2)
            (strike * time_to_expiry * exp_neg_rt * nd2) / (PRECISION * PRECISION * PRECISION)
        }
        OptionType::Put => {
            // Rho_put = -K * T * e^(-rT) * N(-d2)
            let n_neg_d2 = PRECISION - nd2;
            -(strike * time_to_expiry * exp_neg_rt * n_neg_d2) / (PRECISION * PRECISION * PRECISION)
        }
    }
}

/// Calculate collateral required for writing a single option contract.
/// Uses worst-case scenario: full notional for calls, full notional for puts.
pub fn calculate_collateral_required(
    strike: i128,
    size: i128,
    option_type: OptionType,
    spot: i128,
) -> i128 {
    match option_type {
        // Call: collateral = strike * size (writer needs to be able to sell at strike)
        OptionType::Call => strike * size / PRECISION,
        // Put: collateral = strike * size (writer needs to be able to buy at strike)
        // In practice, could be reduced for cash-settled, but we use full collateral
        OptionType::Put => strike * size / PRECISION,
    }
}

/// Check if an option is in-the-money.
pub fn is_in_the_money(spot: i128, strike: i128, option_type: OptionType) -> bool {
    match option_type {
        OptionType::Call => spot > strike,
        OptionType::Put => spot < strike,
    }
}

/// Calculate the intrinsic value (payoff) of an option at expiration.
pub fn calculate_payoff(spot: i128, strike: i128, size: i128, option_type: OptionType) -> i128 {
    match option_type {
        OptionType::Call => {
            if spot > strike {
                (spot - strike) * size / PRECISION
            } else {
                0
            }
        }
        OptionType::Put => {
            if strike > spot {
                (strike - spot) * size / PRECISION
            } else {
                0
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(100), 10);
        assert_eq!(isqrt(1_000_000), 1000);
    }

    #[test]
    fn test_normal_cdf_at_zero() {
        // CDF(0) should be approximately 0.5
        let cdf_0 = normal_cdf_approx(0);
        assert!(cdf_0 > 4800 && cdf_0 < 5200, "CDF(0) ≈ 0.5, got {}", cdf_0);
    }

    #[test]
    fn test_normal_cdf_positive() {
        // CDF(1) should be approximately 0.8413
        let cdf_1 = normal_cdf_approx(PRECISION);
        assert!(
            cdf_1 > 8300 && cdf_1 < 8600,
            "CDF(1) ≈ 0.8413, got {}",
            cdf_1
        );
    }

    #[test]
    fn test_normal_cdf_negative() {
        // CDF(-1) should be approximately 0.1587
        let cdf_neg1 = normal_cdf_approx(-PRECISION);
        assert!(
            cdf_neg1 > 1400 && cdf_neg1 < 1700,
            "CDF(-1) ≈ 0.1587, got {}",
            cdf_neg1
        );
    }

    #[test]
    fn test_normal_cdf_extreme_positive() {
        // CDF(6) should be approximately 1.0
        let cdf_6 = normal_cdf_approx(6 * PRECISION);
        assert!(cdf_6 >= PRECISION - 1, "CDF(6) ≈ 1.0");
    }

    #[test]
    fn test_normal_cdf_extreme_negative() {
        // CDF(-6) should be approximately 0.0
        let cdf_neg6 = normal_cdf_approx(-6 * PRECISION);
        assert!(cdf_neg6 <= 1, "CDF(-6) ≈ 0.0");
    }

    #[test]
    fn test_black_scholes_call_basic() {
        // S=100, K=100, T=1 year, r=5%, σ=20%
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION; // 1 year
        let rate = 500; // 5%
        let vol = 2000; // 20%

        let price = black_scholes_price(spot, strike, tte, rate, vol, OptionType::Call);
        assert!(price.is_ok());
        let premium = price.unwrap();
        // BS call price for ATM should be ~10.45 for these params
        assert!(
            premium > 800 && premium < 1500,
            "ATM call premium ~10.45, got {}",
            premium
        );
    }

    #[test]
    fn test_black_scholes_put_basic() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let price = black_scholes_price(spot, strike, tte, rate, vol, OptionType::Put);
        assert!(price.is_ok());
        let premium = price.unwrap();
        // Put-call parity: P = C - S + K*e^(-rT)
        // ATM put should be ~5.57 for these params
        assert!(
            premium > 300 && premium < 900,
            "ATM put premium ~5.57, got {}",
            premium
        );
    }

    #[test]
    fn test_black_scholes_deep_itm_call() {
        // Deep in-the-money call should have high premium
        let spot = 200 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let price = black_scholes_price(spot, strike, tte, rate, vol, OptionType::Call);
        assert!(price.is_ok());
        let premium = price.unwrap();
        // Deep ITM call: premium ≈ S - K*e^(-rT) ≈ 200 - 100*e^(-0.05) ≈ 200 - 95.12 ≈ 104.88
        assert!(
            premium > 9000 && premium < 11000,
            "Deep ITM call premium should be ~104.88, got {}",
            premium
        );
    }

    #[test]
    fn test_black_scholes_otm_put() {
        // Out-of-the-money put
        let spot = 200 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let price = black_scholes_price(spot, strike, tte, rate, vol, OptionType::Put);
        assert!(price.is_ok());
        let premium = price.unwrap();
        // OTM put should have very low premium
        assert!(
            premium < 50,
            "Deep OTM put should be near zero, got {}",
            premium
        );
    }

    #[test]
    fn test_black_scholes_invalid_inputs() {
        let result =
            black_scholes_price(0, 100 * PRECISION, PRECISION, 500, 2000, OptionType::Call);
        assert!(result.is_err());

        let result =
            black_scholes_price(100 * PRECISION, 0, PRECISION, 500, 2000, OptionType::Call);
        assert!(result.is_err());

        let result = black_scholes_price(
            100 * PRECISION,
            100 * PRECISION,
            0,
            500,
            2000,
            OptionType::Call,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_put_call_parity() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 3000;

        let call = black_scholes_price(spot, strike, tte, rate, vol, OptionType::Call).unwrap();
        let put = black_scholes_price(spot, strike, tte, rate, vol, OptionType::Put).unwrap();

        // Put-Call Parity: C - P = S - K*e^(-rT)
        let exp_neg_rt = approx_exp(-(rate * tte) / PRECISION);
        let lhs = call - put;
        let rhs = spot - (strike * exp_neg_rt / PRECISION);

        // Allow ±5% tolerance due to approximation
        let tolerance = (rhs.abs() * 5) / 100;
        assert!(
            (lhs - rhs).abs() <= tolerance,
            "Put-Call parity: C-P={}, S-K*e^(-rT)={}, diff={}",
            lhs,
            rhs,
            (lhs - rhs).abs()
        );
    }

    #[test]
    fn test_delta_call_atm() {
        // ATM call delta should be close to 0.5
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let delta = calculate_delta(spot, strike, tte, rate, vol, OptionType::Call);
        assert!(
            delta > 4500 && delta < 5500,
            "ATM call delta ≈ 0.5, got {}",
            delta
        );
    }

    #[test]
    fn test_delta_put_atm() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let delta = calculate_delta(spot, strike, tte, rate, vol, OptionType::Put);
        // ATM put delta should be close to -0.5
        assert!(
            delta > -5500 && delta < -4500,
            "ATM put delta ≈ -0.5, got {}",
            delta
        );
    }

    #[test]
    fn test_gamma_positive() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let gamma = calculate_gamma(spot, strike, tte, rate, vol);
        assert!(gamma > 0, "Gamma should be positive, got {}", gamma);
    }

    #[test]
    fn test_vega_positive() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let vega = calculate_vega(spot, strike, tte, rate, vol);
        assert!(vega > 0, "Vega should be positive, got {}", vega);
    }

    #[test]
    fn test_theta_negative() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let theta = calculate_theta(spot, strike, tte, rate, vol, OptionType::Call);
        // Theta should be negative for long options (time decay)
        assert!(theta < 0, "Theta should be negative, got {}", theta);
    }

    #[test]
    fn test_rho_call_positive() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let rho = calculate_rho(spot, strike, tte, rate, vol, OptionType::Call);
        assert!(rho > 0, "Call Rho should be positive, got {}", rho);
    }

    #[test]
    fn test_rho_put_negative() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;
        let vol = 2000;

        let rho = calculate_rho(spot, strike, tte, rate, vol, OptionType::Put);
        assert!(rho < 0, "Put Rho should be negative, got {}", rho);
    }

    #[test]
    fn test_is_in_the_money() {
        // Call: ITM when spot > strike
        assert!(is_in_the_money(
            110 * PRECISION,
            100 * PRECISION,
            OptionType::Call
        ));
        assert!(!is_in_the_money(
            90 * PRECISION,
            100 * PRECISION,
            OptionType::Call
        ));

        // Put: ITM when spot < strike
        assert!(is_in_the_money(
            90 * PRECISION,
            100 * PRECISION,
            OptionType::Put
        ));
        assert!(!is_in_the_money(
            110 * PRECISION,
            100 * PRECISION,
            OptionType::Put
        ));
    }

    #[test]
    fn test_payoff() {
        // Call payoff: max(S - K, 0) * size
        let call_payoff = calculate_payoff(120 * PRECISION, 100 * PRECISION, 1, OptionType::Call);
        assert_eq!(call_payoff, 20);

        let call_otm = calculate_payoff(80 * PRECISION, 100 * PRECISION, 1, OptionType::Call);
        assert_eq!(call_otm, 0);

        // Put payoff: max(K - S, 0) * size
        let put_payoff = calculate_payoff(80 * PRECISION, 100 * PRECISION, 1, OptionType::Put);
        assert_eq!(put_payoff, 20);

        let put_otm = calculate_payoff(120 * PRECISION, 100 * PRECISION, 1, OptionType::Put);
        assert_eq!(put_otm, 0);
    }

    #[test]
    fn test_collateral_required() {
        // Call collateral = strike * size
        let call_coll =
            calculate_collateral_required(100 * PRECISION, 10, OptionType::Call, 110 * PRECISION);
        assert_eq!(call_coll, 1000);

        // Put collateral = strike * size
        let put_coll =
            calculate_collateral_required(100 * PRECISION, 10, OptionType::Put, 90 * PRECISION);
        assert_eq!(put_coll, 1000);
    }

    #[test]
    fn test_time_to_expiry_years() {
        let now = 1_000_000;
        let expiry = now + 365 * 24 * 60 * 60; // 1 year later
        let tte = time_to_expiry_years(now, expiry);
        assert_eq!(tte, PRECISION); // 1.0 year

        let short = now + 24 * 60 * 60; // 1 day later
        let tte_short = time_to_expiry_years(now, short);
        assert!(tte_short > 0 && tte_short < PRECISION / 10);

        // Past expiry
        let tte_past = time_to_expiry_years(now + 1, now);
        assert_eq!(tte_past, 0);
    }

    #[test]
    fn test_volatility_bounds() {
        let spot = 100 * PRECISION;
        let strike = 100 * PRECISION;
        let tte = PRECISION;
        let rate = 500;

        // Very low volatility
        let price_low =
            black_scholes_price(spot, strike, tte, rate, MIN_VOLATILITY, OptionType::Call).unwrap();
        // Very high volatility
        let price_high =
            black_scholes_price(spot, strike, tte, rate, MAX_VOLATILITY, OptionType::Call).unwrap();

        assert!(
            price_low < price_high,
            "Higher volatility should give higher premium"
        );
    }
}
