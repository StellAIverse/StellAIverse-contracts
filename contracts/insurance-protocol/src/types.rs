use soroban_sdk::{contracttype, Address, String, Symbol};

// ═══════════════════════════════════════════════════════════════
//  COVERAGE TYPES
// ═══════════════════════════════════════════════════════════════

/// Supported insurance coverage types
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum CoverageType {
    SmartContractRisk = 0,
    OracleFailure = 1,
    LiquidationFailure = 2,
}

/// Coverage tier affecting deductibles and payout limits
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum CoverageTier {
    Basic = 0,
    Standard = 1,
    Premium = 2,
}

// ═══════════════════════════════════════════════════════════════
//  CLAIM STATUS
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ClaimStatus {
    Pending = 0,
    UnderReview = 1,
    Approved = 2,
    Denied = 3,
    Paid = 4,
}

// ═══════════════════════════════════════════════════════════════
//  UNDERWRITER
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct Underwriter {
    pub address: Address,
    pub shares: i128,
    pub total_deposited: i128,
    pub total_withdrawn: i128,
    pub deposit_timestamp: u64,
    pub last_withdrawal_timestamp: u64,
}

// ═══════════════════════════════════════════════════════════════
//  INSURANCE POOL
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct InsurancePool {
    pub pool_id: Symbol,
    pub coverage_type: CoverageType,
    pub total_assets: i128,
    pub total_shares: i128,
    pub reserve_ratio_bps: u32,
    pub reserve_amount: i128,
    pub active_policies: u32,
    pub total_premiums_collected: i128,
    pub total_claims_paid: i128,
    pub is_active: bool,
    pub created_at: u64,
}

// ═══════════════════════════════════════════════════════════════
//  COVERAGE POLICY
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct CoveragePolicy {
    pub policy_id: u64,
    pub pool_id: Symbol,
    pub holder: Address,
    pub coverage_type: CoverageType,
    pub tier: CoverageTier,
    pub coverage_limit: i128,
    pub deductible: i128,
    pub premium_paid: i128,
    pub is_active: bool,
    pub purchased_at: u64,
    pub expires_at: u64,
}

// ═══════════════════════════════════════════════════════════════
//  CLAIM
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct Claim {
    pub claim_id: u64,
    pub policy_id: u64,
    pub pool_id: Symbol,
    pub claimant: Address,
    pub amount: i128,
    pub evidence: String,
    pub status: ClaimStatus,
    pub submitted_at: u64,
    pub voting_deadline: u64,
    pub votes_for: u32,
    pub votes_against: u32,
    pub total_voters: u32,
    pub requires_voting: bool,
}

// ═══════════════════════════════════════════════════════════════
//  PAYOUT QUEUE
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct PayoutQueueItem {
    pub claim_id: u64,
    pub pool_id: Symbol,
    pub amount: i128,
    pub priority: u32,
    pub queued_at: u64,
    pub processed: bool,
}

// ═══════════════════════════════════════════════════════════════
//  RISK PARAMETERS
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct RiskParameters {
    pub coverage_type: CoverageType,
    pub base_rate_bps: u32,
    pub risk_multiplier: u32,
    pub max_coverage_limit: i128,
    pub min_coverage_limit: i128,
    pub default_deductible_bps: u32,
    pub max_payout_bps: u32,
    pub timelock_period: u64,
    pub voting_threshold_bps: u32,
}

// ═══════════════════════════════════════════════════════════════
//  COVERAGE TIER CONFIG
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct CoverageTierConfig {
    pub tier: CoverageTier,
    pub deductible_bps: u32,
    pub max_payout_multiplier: u32,
    pub premium_multiplier_bps: u32,
}

// ═══════════════════════════════════════════════════════════════
//  VOTE RECORD
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct VoteRecord {
    pub voter: Address,
    pub claim_id: u64,
    pub approve: bool,
    pub timestamp: u64,
}

// ═══════════════════════════════════════════════════════════════
//  POOL INFO (view helper)
// ═══════════════════════════════════════════════════════════════

#[contracttype]
#[derive(Clone, Debug)]
pub struct PoolInfo {
    pub pool_id: Symbol,
    pub coverage_type: CoverageType,
    pub total_assets: i128,
    pub total_shares: i128,
    pub share_price: i128,
    pub reserve_ratio_bps: u32,
    pub reserve_amount: i128,
    pub active_policies: u32,
    pub total_premiums_collected: i128,
    pub total_claims_paid: i128,
    pub is_active: bool,
}
