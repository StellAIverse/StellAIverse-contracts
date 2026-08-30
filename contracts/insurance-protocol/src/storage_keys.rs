use soroban_sdk::{contracttype, Address, Symbol};

/// Storage keys for the insurance protocol contract.
///
/// Using typed enum variants for collision-free key layout, following
/// the pattern established by the oracle contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    // ── Instance Configuration ───────────────────────────────
    Admin,
    Token,
    Oracle,
    Paused,
    PolicyCounter,
    ClaimCounter,
    PayoutQueueCounter,
    PoolCounter,
    UnderwriterCount,

    // ── Pool Records ────────────────────────────────────────
    Pool(Symbol),
    PoolIds,

    // ── Underwriter Records ──────────────────────────────────
    Underwriter(Symbol, Address),

    // ── Policy Records ──────────────────────────────────────
    Policy(u64),
    PolicyIds,
    HolderPolicies(Address),

    // ── Claim Records ───────────────────────────────────────
    Claim(u64),
    ClaimIds,
    PoolClaims(Symbol),

    // ── Payout Queue ────────────────────────────────────────
    PayoutQueueItem(u64),
    PayoutQueueIds,

    // ── Risk Parameters ─────────────────────────────────────
    RiskParameters(CoverageTypeKey),
    TierConfig(CoverageTypeKey, CoverageTierKey),

    // ── Vote Records ────────────────────────────────────────
    VoteRecord(u64, Address),
    ClaimVoters(u64),
}

/// Helper for using CoverageType as a storage key component.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverageTypeKey {
    SmartContractRisk,
    OracleFailure,
    LiquidationFailure,
}

/// Helper for using CoverageTier as a storage key component.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverageTierKey {
    Basic,
    Standard,
    Premium,
}

// ── Key Helper Functions ───────────────────────────────────────

pub fn get_admin_key() -> StorageKey {
    StorageKey::Admin
}

pub fn get_token_key() -> StorageKey {
    StorageKey::Token
}

pub fn get_oracle_key() -> StorageKey {
    StorageKey::Oracle
}

pub fn get_paused_key() -> StorageKey {
    StorageKey::Paused
}

pub fn get_policy_counter_key() -> StorageKey {
    StorageKey::PolicyCounter
}

pub fn get_claim_counter_key() -> StorageKey {
    StorageKey::ClaimCounter
}

pub fn get_payout_queue_counter_key() -> StorageKey {
    StorageKey::PayoutQueueCounter
}

pub fn get_pool_counter_key() -> StorageKey {
    StorageKey::PoolCounter
}

pub fn get_underwriter_count_key() -> StorageKey {
    StorageKey::UnderwriterCount
}

pub fn get_pool_key(pool_id: &Symbol) -> StorageKey {
    StorageKey::Pool(pool_id.clone())
}

pub fn get_pool_ids_key() -> StorageKey {
    StorageKey::PoolIds
}

pub fn get_underwriter_key(pool_id: &Symbol, address: &Address) -> StorageKey {
    StorageKey::Underwriter(pool_id.clone(), address.clone())
}

pub fn get_policy_key(policy_id: u64) -> StorageKey {
    StorageKey::Policy(policy_id)
}

pub fn get_policy_ids_key() -> StorageKey {
    StorageKey::PolicyIds
}

pub fn get_holder_policies_key(holder: &Address) -> StorageKey {
    StorageKey::HolderPolicies(holder.clone())
}

pub fn get_claim_key(claim_id: u64) -> StorageKey {
    StorageKey::Claim(claim_id)
}

pub fn get_claim_ids_key() -> StorageKey {
    StorageKey::ClaimIds
}

pub fn get_pool_claims_key(pool_id: &Symbol) -> StorageKey {
    StorageKey::PoolClaims(pool_id.clone())
}

pub fn get_payout_queue_item_key(item_id: u64) -> StorageKey {
    StorageKey::PayoutQueueItem(item_id)
}

pub fn get_payout_queue_ids_key() -> StorageKey {
    StorageKey::PayoutQueueIds
}

pub fn get_risk_parameters_key(coverage_type: &CoverageTypeKey) -> StorageKey {
    StorageKey::RiskParameters(coverage_type.clone())
}

pub fn get_tier_config_key(coverage_type: &CoverageTypeKey, tier: &CoverageTierKey) -> StorageKey {
    StorageKey::TierConfig(coverage_type.clone(), tier.clone())
}

pub fn get_vote_record_key(claim_id: u64, voter: &Address) -> StorageKey {
    StorageKey::VoteRecord(claim_id, voter.clone())
}

pub fn get_claim_voters_key(claim_id: u64) -> StorageKey {
    StorageKey::ClaimVoters(claim_id)
}
