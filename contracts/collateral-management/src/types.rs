use soroban_sdk::{contracttype, Address, Symbol};

/// Storage keys for the collateral management contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Initialized,
    Admin,
    Oracle,
    Treasury,
    LoanCounter,
    /// Collateral type configuration keyed by token address.
    CollateralType(Address),
    /// List of all registered collateral token addresses.
    CollateralTokens,
    /// User collateral deposit: (user, token_address).
    UserCollateral(Address, Address),
    ProtocolParams,
    /// Total debt for a borrow token.
    TotalDebt(Address),
    Paused,
    ReentrancyLock,
    /// Total collateral deposited for a token.
    TotalCollateral(Address),
    /// Total deposits in lending pool for a token.
    LendingPoolDeposits(Address),
    /// Loan record keyed by loan_id.
    Loan(u64),
    /// User loan IDs list: (user).
    UserLoanIds(Address),
    /// Price cache for a feed: keyed by Symbol (feed_id).
    PriceCache(Symbol),
}

/// Configuration for a supported collateral type.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CollateralTypeConfig {
    /// The token contract address.
    pub token: Address,
    /// Oracle feed identifier (Symbol used by the oracle contract).
    pub oracle_feed: Symbol,
    /// Loan-to-value ratio in basis points (e.g. 8000 = 80%).
    pub ltv_bps: u32,
    /// Liquidation threshold in bps (e.g. 8500 = 85%).
    pub liq_threshold_bps: u32,
    /// Liquidation bonus in bps (e.g. 500 = 5%).
    pub liq_bonus_bps: u32,
    /// Max total deposit across all users (0 = unlimited).
    pub collateral_cap: i128,
    pub is_active: bool,
    /// Price scale factor for oracle prices (e.g. 1_000_000 for 6-decimal).
    pub price_scale: i128,
}

/// A user's deposited collateral for a specific asset.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserCollateral {
    pub user: Address,
    pub token: Address,
    pub amount: i128,
    pub last_updated: u64,
}

/// An active loan (debt position).
#[contracttype]
#[derive(Clone, Debug)]
pub struct Loan {
    pub loan_id: u64,
    pub borrower: Address,
    pub borrow_token: Address,
    pub principal: i128,
    pub accrued_interest: i128,
    pub total_debt: i128,
    pub interest_rate_bps: u32,
    pub created_at: u64,
    pub last_accrual_update: u64,
    pub is_liquidated: bool,
    pub is_repaid: bool,
}

/// Protocol-level risk parameters.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ProtocolParams {
    pub debt_ceiling: i128,
    /// Min health factor before liquidation (10000 = 1.0).
    pub liq_health_threshold_bps: u32,
    pub base_interest_rate_bps: u32,
    pub interest_slope1_bps: u32,
    pub interest_slope2_bps: u32,
    /// Optimal utilization rate in bps (e.g. 8000 = 80%).
    pub optimal_utilization_bps: u32,
    pub max_borrow_per_user: i128,
    pub max_collateral_per_user: i128,
}

/// Health factor result.
#[contracttype]
#[derive(Clone, Debug)]
pub struct HealthFactor {
    pub health_factor_bps: i128,
    pub total_collateral_value: i128,
    pub total_debt: i128,
    pub is_healthy: bool,
}
