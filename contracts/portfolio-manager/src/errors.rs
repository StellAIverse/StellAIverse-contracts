//! Error constants for the portfolio manager contract.
//!
//! This contract uses panic!() with descriptive messages following the
//! same convention as staking/vault contracts.

#[allow(dead_code)]
pub const ALREADY_INITIALIZED: &str = "Already initialized";
#[allow(dead_code)]
pub const UNAUTHORIZED: &str = "Unauthorized";
#[allow(dead_code)]
pub const CONTRACT_PAUSED: &str = "Contract is paused";
#[allow(dead_code)]
pub const PORTFOLIO_NOT_FOUND: &str = "Portfolio not found";
#[allow(dead_code)]
pub const PORTFOLIO_NOT_ACTIVE: &str = "Portfolio is not active";
#[allow(dead_code)]
pub const INVALID_DEPOSIT: &str = "Deposit amount must be positive";
#[allow(dead_code)]
pub const INVALID_WITHDRAW: &str = "Shares must be positive";
#[allow(dead_code)]
pub const INSUFFICIENT_SHARES: &str = "Insufficient shares";
#[allow(dead_code)]
pub const NO_ASSETS: &str = "No assets in portfolio";
#[allow(dead_code)]
pub const WEIGHTS_INVALID: &str = "Weights must sum to 10000 BPS";
#[allow(dead_code)]
pub const TOO_MANY_ASSETS: &str = "Too many assets";
#[allow(dead_code)]
pub const SLIPPAGE_EXCEEDED: &str = "Slippage exceeded maximum";
#[allow(dead_code)]
pub const REBALANCE_TOO_FREQUENT: &str = "Rebalance too frequent";
#[allow(dead_code)]
pub const DIVIDENDS_UNAVAILABLE: &str = "No dividends to claim";
