use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::types::*;

// ═══════════════════════════════════════════════════════════════
//  STORAGE KEYS
// ═══════════════════════════════════════════════════════════════

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    PortfolioCounter,
    Portfolio(u64),
    AssetPosition(u64, u32),    // (portfolio_id, asset_index)
    UserPosition(Address, u64), // (user, portfolio_id)
    UserPortfolioIds(Address),
    RebalanceCounter(u64),     // per-portfolio
    RebalanceRecord(u64, u64), // (portfolio_id, rebalance_id)
    DividendCounter(u64),
    DividendRecord(u64, u64), // (portfolio_id, record_id)
    SnapshotCounter(u64),
    PerformanceSnapshot(u64, u64), // (portfolio_id, snapshot_id)
    PerformanceAccumulator(u64),   // (portfolio_id)
    TotalDividendsCollected(u64),
    OraclePriceCache(u64, Address), // (portfolio_id, token) -> cached price
    ReentrancyLock,
}

// ═══════════════════════════════════════════════════════════════
//  STORAGE HELPERS
// ═══════════════════════════════════════════════════════════════

#[allow(dead_code)]
pub struct Storage;

#[allow(dead_code)]
impl Storage {
    // ── ADMIN ──────────────────────────────────────────────────

    pub fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    pub fn set_admin(env: &Env, admin: &Address) {
        env.storage().instance().set(&DataKey::Admin, admin);
    }

    pub fn is_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn set_paused(env: &Env, paused: bool) {
        env.storage().instance().set(&DataKey::Paused, &paused);
    }

    // ── PORTFOLIO COUNTER ──────────────────────────────────────

    pub fn next_portfolio_id(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PortfolioCounter)
            .unwrap_or(0);
        let next = current + 1;
        env.storage()
            .instance()
            .set(&DataKey::PortfolioCounter, &next);
        next
    }

    // ── PORTFOLIO ──────────────────────────────────────────────

    pub fn get_portfolio(env: &Env, portfolio_id: u64) -> Portfolio {
        env.storage()
            .instance()
            .get(&DataKey::Portfolio(portfolio_id))
            .unwrap_or_else(|| panic!("Portfolio not found"))
    }

    pub fn set_portfolio(env: &Env, portfolio: &Portfolio) {
        env.storage()
            .instance()
            .set(&DataKey::Portfolio(portfolio.portfolio_id), portfolio);
    }

    pub fn has_portfolio(env: &Env, portfolio_id: u64) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::Portfolio(portfolio_id))
    }

    // ── ASSET POSITIONS ────────────────────────────────────────

    pub fn get_asset_position(env: &Env, portfolio_id: u64, asset_index: u32) -> AssetPosition {
        env.storage()
            .instance()
            .get(&DataKey::AssetPosition(portfolio_id, asset_index))
            .unwrap_or_else(|| panic!("Asset position not found"))
    }

    pub fn set_asset_position(env: &Env, portfolio_id: u64, asset_index: u32, pos: &AssetPosition) {
        env.storage()
            .instance()
            .set(&DataKey::AssetPosition(portfolio_id, asset_index), pos);
    }

    pub fn has_asset_position(env: &Env, portfolio_id: u64, asset_index: u32) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::AssetPosition(portfolio_id, asset_index))
    }

    // ── USER POSITIONS ─────────────────────────────────────────

    pub fn get_user_position(env: &Env, user: &Address, portfolio_id: u64) -> UserPosition {
        env.storage()
            .instance()
            .get(&DataKey::UserPosition(user.clone(), portfolio_id))
            .unwrap_or_else(|| panic!("User position not found"))
    }

    pub fn set_user_position(env: &Env, user: &Address, portfolio_id: u64, pos: &UserPosition) {
        env.storage()
            .instance()
            .set(&DataKey::UserPosition(user.clone(), portfolio_id), pos);
    }

    pub fn has_user_position(env: &Env, user: &Address, portfolio_id: u64) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::UserPosition(user.clone(), portfolio_id))
    }

    pub fn get_user_portfolio_ids(env: &Env, user: &Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::UserPortfolioIds(user.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn add_user_portfolio(env: &Env, user: &Address, portfolio_id: u64) {
        let mut ids = Self::get_user_portfolio_ids(env, user);
        ids.push_back(portfolio_id);
        env.storage()
            .instance()
            .set(&DataKey::UserPortfolioIds(user.clone()), &ids);
    }

    // ── REBALANCE ──────────────────────────────────────────────

    pub fn next_rebalance_id(env: &Env, portfolio_id: u64) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RebalanceCounter(portfolio_id))
            .unwrap_or(0);
        let next = current + 1;
        env.storage()
            .instance()
            .set(&DataKey::RebalanceCounter(portfolio_id), &next);
        next
    }

    pub fn get_rebalance_record(
        env: &Env,
        portfolio_id: u64,
        rebalance_id: u64,
    ) -> RebalanceRecord {
        env.storage()
            .instance()
            .get(&DataKey::RebalanceRecord(portfolio_id, rebalance_id))
            .unwrap_or_else(|| panic!("Rebalance record not found"))
    }

    pub fn set_rebalance_record(env: &Env, record: &RebalanceRecord) {
        env.storage().instance().set(
            &DataKey::RebalanceRecord(record.portfolio_id, record.rebalance_id),
            record,
        );
    }

    // ── DIVIDENDS ──────────────────────────────────────────────

    pub fn next_dividend_id(env: &Env, portfolio_id: u64) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DividendCounter(portfolio_id))
            .unwrap_or(0);
        let next = current + 1;
        env.storage()
            .instance()
            .set(&DataKey::DividendCounter(portfolio_id), &next);
        next
    }

    pub fn get_dividend_record(env: &Env, portfolio_id: u64, record_id: u64) -> DividendRecord {
        env.storage()
            .instance()
            .get(&DataKey::DividendRecord(portfolio_id, record_id))
            .unwrap_or_else(|| panic!("Dividend record not found"))
    }

    pub fn set_dividend_record(env: &Env, record: &DividendRecord) {
        env.storage().instance().set(
            &DataKey::DividendRecord(record.portfolio_id, record.record_id),
            record,
        );
    }

    pub fn total_dividends_collected(env: &Env, portfolio_id: u64) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalDividendsCollected(portfolio_id))
            .unwrap_or(0)
    }

    pub fn set_total_dividends_collected(env: &Env, portfolio_id: u64, total: i128) {
        env.storage()
            .instance()
            .set(&DataKey::TotalDividendsCollected(portfolio_id), &total);
    }

    // ── PERFORMANCE ────────────────────────────────────────────

    pub fn next_snapshot_id(env: &Env, portfolio_id: u64) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SnapshotCounter(portfolio_id))
            .unwrap_or(0);
        let next = current + 1;
        env.storage()
            .instance()
            .set(&DataKey::SnapshotCounter(portfolio_id), &next);
        next
    }

    pub fn get_performance_snapshot(
        env: &Env,
        portfolio_id: u64,
        snapshot_id: u64,
    ) -> PerformanceSnapshot {
        env.storage()
            .instance()
            .get(&DataKey::PerformanceSnapshot(portfolio_id, snapshot_id))
            .unwrap_or_else(|| panic!("Snapshot not found"))
    }

    pub fn set_performance_snapshot(env: &Env, snapshot: &PerformanceSnapshot) {
        env.storage().instance().set(
            &DataKey::PerformanceSnapshot(snapshot.portfolio_id, snapshot.snapshot_id),
            snapshot,
        );
    }

    pub fn get_performance_accumulator(env: &Env, portfolio_id: u64) -> PerformanceAccumulator {
        env.storage()
            .instance()
            .get(&DataKey::PerformanceAccumulator(portfolio_id))
            .unwrap_or(PerformanceAccumulator {
                portfolio_id,
                return_sum: 0,
                return_squared_sum: 0,
                observation_count: 0,
                previous_nav: PRECISION_FACTOR,
                previous_nav_time: 0,
                peak_nav: PRECISION_FACTOR,
                max_drawdown_bps: 0,
            })
    }

    pub fn set_performance_accumulator(env: &Env, acc: &PerformanceAccumulator) {
        env.storage()
            .instance()
            .set(&DataKey::PerformanceAccumulator(acc.portfolio_id), acc);
    }

    // ── REENTRANCY ─────────────────────────────────────────────

    pub fn enter_non_reentrant(env: &Env) {
        let locked: bool = env
            .storage()
            .instance()
            .get(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        if locked {
            panic!("Reentrant call blocked");
        }
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    }

    pub fn exit_non_reentrant(env: &Env) {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);
    }
}
