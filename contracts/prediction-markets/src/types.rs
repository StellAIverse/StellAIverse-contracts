use soroban_sdk::{contracttype, Address, String, Vec};

// ── Constants ───────────────────────────────────────────────────────────────

/// Maximum number of outcomes per market.
pub const MAX_OUTCOMES: usize = 10;

/// 18-decimal precision factor (1e18).
pub const DECIMAL_FACTOR: i128 = 1_000_000_000_000_000_000;

/// Basis points denominator (100%).
#[allow(dead_code)]
pub const BPS_DENOMINATOR: i128 = 10_000;

// ── Enums ───────────────────────────────────────────────────────────────────

/// Current lifecycle status of a prediction market.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum MarketStatus {
    /// Market is accepting trades.
    Active = 0,
    /// Resolution window has closed; awaiting oracle.
    PendingResolution = 1,
    /// Oracle has resolved the market.
    Resolved = 2,
    /// Market is under dispute.
    Disputed = 3,
    /// Market was closed early by admin or creator.
    Closed = 4,
}

/// Category for filtering prediction markets.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum MarketCategory {
    Sports = 0,
    Crypto = 1,
    Politics = 2,
    Events = 3,
    Custom = 4,
}

/// Status of a dispute.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DisputeStatus {
    Open = 0,
    Voted = 1,
    ResolvedUpheld = 2,
    ResolvedRejected = 3,
}

/// Status of an order on the order book.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OrderStatus {
    Open = 1,
    Filled = 2,
    Cancelled = 3,
}

/// Direction of a limit order.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OrderSide {
    Buy = 0,
    Sell = 1,
}

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    // Global
    MarketCounter,
    Admin,
    TradingPaused,
    OracleAddress,
    FeeShareBps,
    GovernanceCollector,

    // Market
    Market(u64),

    // Outcome token pools (CPMM reserves per outcome)
    OutcomePool(u64, u32), // (market_id, outcome_index) -> OutcomePool

    // Outcome token balances (user -> outcome -> amount)
    OutcomeBalance(u64, u32, Address), // (market_id, outcome_index, user)

    // Collateral balance per market
    MarketCollateral(u64),

    // LP token data
    LpShares(u64, Address),   // (market_id, provider) -> lp_shares
    LpTotalSupply(u64),       // market_id -> total_lp_shares
    LpRewardAccumulator(u64), // market_id -> accumulated fee per lp_share

    // Order book
    OrderCounter(u64), // market_id -> next order_id
    Order(u64, u64),   // (market_id, order_id) -> Order

    // User positions
    UserPosition(u64, u32, Address), // (market_id, outcome_index, user) -> Position

    // Disputes
    DisputeCounter(u64),
    Dispute(u64, u64),              // (market_id, dispute_id) -> Dispute
    DisputeVote(u64, u64, Address), // (market_id, dispute_id, voter) -> vote_weight

    // Market cap enforcement
    TotalOutcomeSupply(u64, u32), // (market_id, outcome_index) -> total minted

    // Price oracle cache
    OracleResultCache(u64), // market_id -> cached oracle result
}

// ── Core Market Types ───────────────────────────────────────────────────────

/// A prediction market.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PredictionMarketV2 {
    pub market_id: u64,
    pub question: String,
    pub category: MarketCategory,
    pub creator: Address,
    pub collateral_token: Address,
    pub oracle_source: Address,
    pub num_outcomes: u32,
    pub outcome_names: Vec<String>,
    pub status: MarketStatus,
    pub created_at: u64,
    pub resolution_window_start: u64,
    pub resolution_window_end: u64,
    pub resolved_outcome: Option<u32>,
    pub total_collateral: i128,
    pub max_outcome_supply: i128,
    pub trading_fee_bps: u32,
}

/// CPMM reserve pool for a single outcome token.
#[contracttype]
#[derive(Clone, Debug)]
pub struct OutcomePool {
    /// Collateral reserve (x in x*y=k).
    pub collateral_reserve: i128,
    /// Outcome token reserve (y in x*y=k).
    pub outcome_reserve: i128,
    /// Total LP share supply for this outcome pool.
    pub lp_total_supply: i128,
}

/// User's position in a specific outcome of a market.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserOutcomePosition {
    pub market_id: u64,
    pub outcome_index: u32,
    pub owner: Address,
    pub quantity: i128,
    pub avg_entry_price: i128, // scaled by DECIMAL_FACTOR
    pub realized_pnl: i128,
    pub created_at: u64,
    pub updated_at: u64,
}

/// A limit order on the order book.
#[contracttype]
#[derive(Clone, Debug)]
pub struct LimitOrder {
    pub order_id: u64,
    pub market_id: u64,
    pub owner: Address,
    pub outcome_index: u32,
    pub side: OrderSide,
    pub price: i128, // price per outcome token scaled by DECIMAL_FACTOR
    pub quantity: i128,
    pub filled_quantity: i128,
    pub status: OrderStatus,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

/// A dispute against a market resolution.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Dispute {
    pub dispute_id: u64,
    pub market_id: u64,
    pub challenger: Address,
    pub claimed_outcome: u32,
    pub evidence: String,
    pub stake_amount: i128,
    pub status: DisputeStatus,
    pub votes_for: i128,
    pub votes_against: i128,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}

/// Snapshot of a market's CPMM state for querying.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MarketSnapshot {
    pub market: PredictionMarketV2,
    pub pools: Vec<OutcomePool>,
    pub total_collateral: i128,
    pub lp_total_supply: i128,
}

/// Parameters for creating a new prediction market.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CreateMarketParams {
    pub question: String,
    pub category: MarketCategory,
    pub collateral_token: Address,
    pub oracle_source: Address,
    pub num_outcomes: u32,
    pub outcome_names: Vec<String>,
    pub resolution_window_duration: u64,
    pub max_outcome_supply: i128,
    pub trading_fee_bps: u32,
    pub initial_liquidity: i128,
}

/// Summary of a user's positions across a market.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserPortfolioEntry {
    pub market_id: u64,
    pub outcome_index: u32,
    pub quantity: i128,
    pub avg_entry_price: i128,
    pub unrealized_value: i128,
    pub realized_pnl: i128,
}
