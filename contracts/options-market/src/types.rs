use soroban_sdk::{contracttype, Address, Symbol};

/// Precision factor: 4 decimal places (10^4).
pub const PRECISION: i128 = 10_000;

/// Basis points denominator (100%).
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Seconds in a year for Black-Scholes time component.
pub const SECONDS_PER_YEAR: i128 = 365 * 24 * 60 * 60;

// ── Enums ───────────────────────────────────────────────────────────────────

/// Option type: Call (right to buy) or Put (right to sell).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OptionType {
    Call = 0,
    Put = 1,
}

/// Option style: American (exercisable any time) or European (only at expiry).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OptionStyle {
    American = 0,
    European = 1,
}

/// Option lifecycle status.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OptionStatus {
    /// Option is active and can be traded.
    Active = 0,
    /// Option has expired and is awaiting settlement.
    Expired = 1,
    /// Option has been exercised.
    Exercised = 2,
    /// Option has been settled (payoffs distributed).
    Settled = 3,
    /// Option was cancelled before expiry.
    Cancelled = 4,
}

/// Underlying asset identifiers.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum UnderlyingAsset {
    XLM = 0,
    USDC = 1,
}

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum StorageKey {
    // Global
    Admin,
    Oracle,
    Treasury,
    Paused,
    CircuitBreakerActive,

    // Option series
    OptionCounter,
    OptionSeries(u64),

    // Option positions
    OptionPosition(u64, Address), // (option_id, holder) -> Position

    // Writer collateral
    WriterCollateral(Address, Symbol), // (writer, underlying) -> collateral amount

    // Pool collateral
    PoolCollateral(Symbol), // underlying -> total collateral

    // User option IDs
    UserOptionIds(Address), // user -> Vec<u64> of option ids they wrote

    // User held option IDs
    UserHeldOptionIds(Address),

    // Secondary market
    ListingCounter,
    Listing(u64),

    // Oracle price cache
    PriceCache(Symbol),
    VolatilityCache(Symbol),

    // Risk management
    MaxPositionPerUser,
    MaxTotalExposure,
    TotalExposure,
    UserExposure(Address),

    // Expiration tracking
    ExpirationQueue(u64), // expiry_timestamp -> Vec<u64> of option ids

    // Greeks cache
    GreeksCache(u64), // option_id -> Greeks

    // Multi-sig withdrawal
    WithdrawalRequestCounter,
    WithdrawalRequest(u64),
    WithdrawalApprovals(u64, Address),

    // Circuit breaker
    PriceHistory(Symbol), // underlying -> Vec<(timestamp, price)>
}

// ── Core Types ──────────────────────────────────────────────────────────────

/// Configuration for an option series (chain of options with same expiry).
#[contracttype]
#[derive(Clone, Debug)]
pub struct OptionSeriesConfig {
    pub series_id: u64,
    pub underlying: UnderlyingAsset,
    pub strike_price: i128, // Strike price scaled by PRECISION
    pub expiration: u64,    // Unix timestamp
    pub option_type: OptionType,
    pub option_style: OptionStyle,
    pub created_at: u64,
    pub total_open_interest: i128,
    pub max_open_interest: i128,
    pub is_active: bool,
}

/// An individual option contract position.
#[contracttype]
#[derive(Clone, Debug)]
pub struct OptionPosition {
    pub option_id: u64,
    pub series_id: u64,
    pub holder: Address,
    pub writer: Address,
    pub underlying: UnderlyingAsset,
    pub option_type: OptionType,
    pub option_style: OptionStyle,
    pub strike_price: i128,
    pub current_price: i128, // Current underlying price
    pub expiration: u64,
    pub premium_paid: i128,      // Premium paid by buyer (4-decimal)
    pub collateral_locked: i128, // Collateral locked by writer
    pub size: i128,              // Number of contracts
    pub status: OptionStatus,
    pub created_at: u64,
    pub exercised_at: Option<u64>,
    pub settled_at: Option<u64>,
}

/// Greeks calculation result for risk management.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Greeks {
    pub delta: i128, // Rate of change of option price w.r.t. underlying
    pub gamma: i128, // Rate of change of delta w.r.t. underlying
    pub vega: i128,  // Sensitivity to volatility changes
    pub theta: i128, // Time decay
    pub rho: i128,   // Sensitivity to interest rate changes
}

/// Secondary market listing for trading options.
#[contracttype]
#[derive(Clone, Debug)]
pub struct OptionListing {
    pub listing_id: u64,
    pub option_id: u64,
    pub seller: Address,
    pub price: i128, // Ask price per contract
    pub size: i128,  // Number of contracts for sale
    pub is_active: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

/// Multi-sig withdrawal request.
#[contracttype]
#[derive(Clone, Debug)]
pub struct WithdrawalRequest {
    pub request_id: u64,
    pub requester: Address,
    pub amount: i128,
    pub underlying: UnderlyingAsset,
    pub approvals: u32,
    pub required_approvals: u32,
    pub is_executed: bool,
    pub created_at: u64,
}

/// Portfolio risk snapshot.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PortfolioRisk {
    pub user: Address,
    pub total_exposure: i128,
    pub total_collateral_locked: i128,
    pub total_premium_paid: i128,
    pub max_position_size: i128,
    pub is_within_limits: bool,
    pub net_delta: i128,
    pub net_gamma: i128,
    pub net_vega: i128,
    pub net_theta: i128,
}

/// Parameters for creating option series (stays within Soroban's param limit).
#[contracttype]
#[derive(Clone, Debug)]
pub struct CreateSeriesParams {
    pub underlying: UnderlyingAsset,
    pub strike_prices: soroban_sdk::Vec<i128>,
    pub expiration: u64,
    pub option_type: OptionType,
    pub option_style: OptionStyle,
    pub max_open_interest_per_strike: i128,
}

/// Circuit breaker state for extreme volatility protection.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircuitBreakerState {
    pub triggered: bool,
    pub triggered_at: Option<u64>,
    pub price_at_trigger: Option<i128>,
    pub previous_price: Option<i128>,
    pub change_bps: Option<u32>,
    pub cooldown_seconds: u64,
    pub can_resume_at: Option<u64>,
}

/// Oracle volatility data.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VolatilityData {
    pub underlying: UnderlyingAsset,
    pub volatility: i128, // Annualized volatility scaled by PRECISION
    pub last_updated: u64,
    pub source: Address,
    pub is_fresh: bool,
}
