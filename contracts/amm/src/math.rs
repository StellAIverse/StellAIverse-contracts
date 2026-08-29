/// Integer square root using Newton's method (for no_std environments).
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

/// Ceiling division: smallest integer >= a / b for positive numbers.
pub fn ceil_div(a: i128, b: i128) -> i128 {
    if b <= 0 {
        panic!("Divisor must be positive");
    }
    if a < 0 {
        panic!("Dividend must be non-negative for ceiling division");
    }
    (a + b - 1) / b
}

/// Floor division: largest integer <= a / b for positive numbers.
pub fn floor_div(a: i128, b: i128) -> i128 {
    if b <= 0 {
        panic!("Divisor must be positive");
    }
    a / b
}

/// Constant-product swap output: amount_out given amount_in and fee.
pub fn get_amount_out(amount_in: i128, reserve_in: i128, reserve_out: i128, fee_bps: u32) -> i128 {
    if amount_in <= 0 || reserve_in <= 0 || reserve_out <= 0 {
        return 0;
    }
    let fee_factor = 10_000 - fee_bps as i128;
    let amount_in_after_fee = (amount_in * fee_factor) / 10_000;
    let numerator = reserve_out * amount_in_after_fee;
    let denominator = reserve_in + amount_in_after_fee;
    numerator / denominator
}

/// Constant-product input required for a desired output (flash swap repayment).
pub fn get_amount_in(amount_out: i128, reserve_in: i128, reserve_out: i128, fee_bps: u32) -> i128 {
    if amount_out <= 0 || reserve_in <= 0 || reserve_out <= amount_out {
        panic!("Invalid flash swap amount");
    }
    let numerator = reserve_in * amount_out * 10_000;
    let denominator = (reserve_out - amount_out) * (10_000 - fee_bps as i128);
    ceil_div(numerator, denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_basic() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(10), 3);
        assert_eq!(isqrt(100), 10);
    }

    #[test]
    fn ceil_div_basic() {
        assert_eq!(ceil_div(10, 3), 4);
        assert_eq!(ceil_div(9, 3), 3);
    }

    #[test]
    fn get_amount_out_matches_constant_product() {
        let out = get_amount_out(1_000, 100_000, 100_000, 30);
        assert!(out > 980);
        assert!(out < 1_000);
    }

    #[test]
    fn get_amount_in_rounds_up() {
        let amount_in = get_amount_in(1_000, 100_000, 100_000, 30);
        let out = get_amount_out(amount_in, 100_000, 100_000, 30);
        assert!(out >= 999);
    }
}
