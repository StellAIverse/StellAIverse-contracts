use soroban_sdk::{contracttype, Address, Symbol, Vec};

// ═══════════════════════════════════════════════════════════════
//  CONSTANTS
// ═══════════════════════════════════════════════════════════════

pub const BPS_DENOMINATOR: i128 = 10_000;
pub const PRECISION_FACTOR: i128 = 1_000_000_000_000_000_000; // 1e18
pub const MAX_ASSETS: u32 = 50;
pub const DRIFT_TOLERANCE_BPS: u32 = 200; // ±2% default
pub const MAX_REBALANCE_SLIPPAGE_BPS: u32 = 500; // 5% max slippage

// ═══════════════════════════════════════════════════════════════
//  ENUMS
// ═══════════════════════════════════════════════════════════════

/// Portfolio risk profile / template type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum PortfolioType {
    Conservative = 0, // 60% bonds / 40% equities
    Balanced = 1,     // 50% equities / 30% bonds / 20% alternatives
    Aggressive = 2,   // 80% equities / 20% alternatives
    Thematic = 3,     // User-defined theme
    Custom = 4,       // Fully custom weights
}

/// How asset target weights are determined
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum WeightingStrategy {
    EqualWeight = 0,       // All assets have equal weight
    CustomWeight = 1,      // Admin-defined fixed weights
    MarketCapWeighted = 2, // Weights updated based on market cap via oracle
}

/// Rebalancing interval
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum RebalanceFrequency {
    Monthly = 0,    // ~30 days
    Quarterly = 1,  // ~90 days
    SemiAnnual = 2, // ~180 days
    Annual = 3,     // ~365 days
}

/// Portfolio lifecycle status
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum PortfolioStatus {
    Active = 0,
    Paused = 1,
    Closed = 2,
}

// ═══════════════════════════════════════════════════════════════
//  STRUCTS
// ═══════════════════════════════════════════════════════════════

/// Core portfolio data
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Portfolio {
    pub portfolio_id: u64,
    pub creator: Address,
    pub name: Symbol,
    pub portfolio_type: PortfolioType,
    pub weighting_strategy: WeightingStrategy,
    pub status: PortfolioStatus,
    /// Target allocations in basis points. Must sum to BPS_DENOMINATOR.
    pub target_weights: Vec<AssetAllocation>,
    /// Denomination / deposit token (e.g., USDC)
    pub deposit_token: Address,
    /// Oracle contract for price feeds
    pub oracle_address: Address,
    /// Total assets under management (in deposit token units)
    pub total_assets: i128,
    /// Total portfolio tokens (shares) outstanding
    pub total_supply: i128,
    /// Accumulated dividends not yet compounded
    pub accumulated_dividends: i128,
    /// Rebalance frequency
    pub rebalance_frequency: RebalanceFrequency,
    /// Drift tolerance in BPS (default 200 = 2%)
    pub drift_tolerance_bps: u32,
    /// Max slippage allowed per rebalance in BPS
    pub max_slippage_bps: u32,
    /// Timestamp of last rebalance
    pub last_rebalance_time: u64,
    /// Timestamp of last dividend collection
    pub last_dividend_time: u64,
    /// Total number of rebalances performed
    pub rebalance_count: u32,
    /// Whether governance can update parameters
    pub governance_managed: bool,
    pub created_at: u64,
}

/// Asset allocation entry: a token and its target weight
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetAllocation {
    /// Address of the underlying token
    pub token: Address,
    /// Target weight in BPS (must sum to 10000 across portfolio)
    pub weight_bps: u32,
    /// Optional oracle feed ID for market-cap weighting
    pub feed_id: Option<Symbol>,
}

/// A single asset's position within the portfolio
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetPosition {
    pub token: Address,
    /// Current balance held by portfolio
    pub balance: i128,
    /// Target weight in BPS
    pub target_weight_bps: u32,
    /// Current weight in BPS (actual)
    pub current_weight_bps: u32,
    /// Last known price from oracle (scaled 1e18)
    pub last_price: i128,
    /// Timestamp of last price update
    pub last_price_update: u64,
}

/// User's deposit position in a portfolio
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct UserPosition {
    pub user: Address,
    pub portfolio_id: u64,
    /// Portfolio tokens (shares) held
    pub shares: i128,
    /// Total deposited in deposit token units
    pub total_deposited: i128,
    /// Total withdrawn in deposit token units
    pub total_withdrawn: i128,
    /// Accumulated dividends earned (not yet claimed)
    pub pending_dividends: i128,
    /// Timestamp of first deposit
    pub first_deposit_at: u64,
    /// Timestamp of last activity
    pub last_activity_at: u64,
}

/// Record of a single rebalance operation
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RebalanceRecord {
    pub portfolio_id: u64,
    pub rebalance_id: u64,
    pub timestamp: u64,
    /// Assets that were bought (token, amount)
    pub buys: Vec<SwapRecord>,
    /// Assets that were sold (token, amount)
    pub sells: Vec<SwapRecord>,
    /// Total slippage incurred in BPS
    pub slippage_bps: u32,
    /// Whether triggered by time or drift
    pub trigger: RebalanceTrigger,
    /// NAV before rebalance
    pub nav_before: i128,
    /// NAV after rebalance
    pub nav_after: i128,
}

/// A single swap within a rebalance
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SwapRecord {
    pub token: Address,
    pub amount_in: i128,
    pub amount_out: i128,
    pub price_impact_bps: u32,
}

/// What triggered a rebalance
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum RebalanceTrigger {
    TimeBased = 0,
    DriftTriggered = 1,
    GovernanceForced = 2,
}

/// Record of a dividend distribution event
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct DividendRecord {
    pub portfolio_id: u64,
    pub record_id: u64,
    pub timestamp: u64,
    /// Total dividend amount collected from assets
    pub total_collected: i128,
    /// Amount compounded back into portfolio
    pub compounded: i128,
    /// Per-share dividend amount
    pub per_share_amount: i128,
}

/// Portfolio performance snapshot
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PerformanceSnapshot {
    pub portfolio_id: u64,
    pub snapshot_id: u64,
    pub timestamp: u64,
    /// Net asset value per share (scaled 1e18)
    pub nav_per_share: i128,
    /// Total assets under management
    pub total_assets: i128,
    /// Sharpe ratio (annualized, scaled 1e18)
    pub sharpe_ratio: i128,
    /// Maximum drawdown in BPS from peak
    pub max_drawdown_bps: u32,
    /// Peak NAV before current drawdown
    pub peak_nav: i128,
    /// Time-weighted return since inception in BPS
    pub twr_bps: i32,
    /// Annualized return in BPS
    pub annualized_return_bps: i32,
}

/// Performance tracking accumulator (stored between snapshots)
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PerformanceAccumulator {
    pub portfolio_id: u64,
    /// Running sum of periodic returns for Sharpe calculation
    pub return_sum: i128,
    /// Running sum of squared returns for volatility
    pub return_squared_sum: i128,
    /// Number of return observations
    pub observation_count: u32,
    /// Previous period NAV for return calculation
    pub previous_nav: i128,
    /// Timestamp of previous observation
    pub previous_nav_time: u64,
    /// All-time peak NAV
    pub peak_nav: i128,
    /// Current max drawdown in BPS
    pub max_drawdown_bps: u32,
}

/// Fork configuration for creating customized portfolios
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ForkConfig {
    pub source_portfolio_id: u64,
    pub fork_creator: Address,
    pub custom_weights: Option<Vec<AssetAllocation>>,
    pub custom_name: Option<Symbol>,
}

/// Governance proposal for parameter changes
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct GovernanceProposal {
    pub proposal_id: u64,
    pub portfolio_id: u64,
    pub proposer: Address,
    pub proposal_type: ProposalType,
    pub executed: bool,
    pub executed_at: Option<u64>,
    pub created_at: u64,
}

/// Types of governance proposals
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum ProposalType {
    UpdateDriftTolerance = 0,
    UpdateRebalanceFrequency = 1,
    UpdateMaxSlippage = 2,
    UpdateWeights = 3,
    PausePortfolio = 4,
    UnpausePortfolio = 5,
    ClosePortfolio = 6,
}

/// Portfolio information summary for view functions
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PortfolioInfo {
    pub portfolio_id: u64,
    pub name: Symbol,
    pub portfolio_type: PortfolioType,
    pub weighting_strategy: WeightingStrategy,
    pub status: PortfolioStatus,
    pub total_assets: i128,
    pub total_supply: i128,
    pub nav_per_share: i128,
    pub rebalance_frequency: RebalanceFrequency,
    pub drift_tolerance_bps: u32,
    pub asset_count: u32,
    pub rebalance_count: u32,
    pub last_rebalance_time: u64,
    pub created_at: u64,
}
