// Error macros for the collateral management contract.
// Uses simple panic! strings consistent with the AMM contract pattern.

macro_rules! err {
    ($msg:expr) => {
        panic!($msg)
    };
}

#[inline(always)]
pub fn already_initialized() -> ! {
    err!("Contract already initialized")
}

#[inline(always)]
#[allow(dead_code)]
pub fn not_initialized() -> ! {
    err!("Contract not initialized")
}

#[inline(always)]
pub fn unauthorized() -> ! {
    err!("Unauthorized")
}

#[inline(always)]
pub fn collateral_type_not_found() -> ! {
    err!("Collateral type not found")
}

#[inline(always)]
pub fn collateral_type_inactive() -> ! {
    err!("Collateral type inactive")
}

#[inline(always)]
pub fn collateral_type_already_exists() -> ! {
    err!("Collateral type already exists")
}

#[inline(always)]
pub fn insufficient_collateral() -> ! {
    err!("Insufficient collateral balance")
}

#[inline(always)]
pub fn collateral_cap_exceeded() -> ! {
    err!("Collateral cap exceeded")
}

#[inline(always)]
pub fn collateral_per_user_exceeded() -> ! {
    err!("Per-user collateral cap exceeded")
}

#[inline(always)]
pub fn debt_ceiling_exceeded() -> ! {
    err!("Protocol debt ceiling exceeded")
}

#[inline(always)]
pub fn borrow_cap_exceeded() -> ! {
    err!("Per-user borrow cap exceeded")
}

#[inline(always)]
pub fn loan_not_found() -> ! {
    err!("Loan not found")
}

#[inline(always)]
pub fn loan_already_liquidated() -> ! {
    err!("Loan already liquidated")
}

#[inline(always)]
pub fn loan_already_repaid() -> ! {
    err!("Loan already repaid")
}

#[inline(always)]
pub fn health_factor_insufficient() -> ! {
    err!("Health factor below liquidation threshold")
}

#[inline(always)]
pub fn invalid_amount() -> ! {
    err!("Invalid amount")
}

#[inline(always)]
pub fn withdrawal_would_undercollateralize() -> ! {
    err!("Withdrawal would undercollateralize position")
}

#[inline(always)]
pub fn repayment_exceeds_debt() -> ! {
    err!("Repayment exceeds outstanding debt")
}

#[inline(always)]
pub fn no_collateral_to_seize() -> ! {
    err!("No collateral available to seize")
}

#[inline(always)]
pub fn oracle_price_unavailable() -> ! {
    err!("Oracle price unavailable")
}

#[inline(always)]
pub fn protocol_paused() -> ! {
    err!("Protocol is paused")
}

#[inline(always)]
pub fn reentrancy_detected() -> ! {
    err!("Reentrancy detected")
}

#[inline(always)]
pub fn invalid_ltv() -> ! {
    err!("Invalid LTV parameter")
}

#[inline(always)]
pub fn invalid_liquidation_params() -> ! {
    err!("Invalid liquidation parameters")
}

#[inline(always)]
#[allow(dead_code)]
pub fn borrow_token_not_accepted() -> ! {
    err!("Borrow token not accepted")
}

#[inline(always)]
#[allow(dead_code)]
pub fn zero_debt_position() -> ! {
    err!("Zero debt position")
}

#[inline(always)]
#[allow(dead_code)]
pub fn no_deposits() -> ! {
    err!("No deposits found")
}
