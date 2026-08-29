use soroban_sdk::Env;

use crate::types::{
    DataKey, Dispute, LimitOrder, OutcomePool, PredictionMarketV2, UserOutcomePosition,
};
use soroban_sdk::Address;

// ── Helpers ─────────────────────────────────────────────────────────────────

// ── Global State ────────────────────────────────────────────────────────────

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Admin)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn is_trading_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TradingPaused)
        .unwrap_or(false)
}

pub fn set_trading_paused(env: &Env, paused: bool) {
    env.storage()
        .instance()
        .set(&DataKey::TradingPaused, &paused);
}

pub fn get_market_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::MarketCounter)
        .unwrap_or(0)
}

pub fn set_market_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::MarketCounter, &counter);
}

pub fn get_default_oracle(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::OracleAddress)
}

pub fn set_default_oracle(env: &Env, oracle: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::OracleAddress, oracle);
}

#[allow(dead_code)]
pub fn get_fee_share_bps(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::FeeShareBps)
        .unwrap_or(0i128)
}

pub fn set_fee_share_bps(env: &Env, bps: i128) {
    env.storage().instance().set(&DataKey::FeeShareBps, &bps);
}

pub fn get_governance_collector(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::GovernanceCollector)
}

pub fn set_governance_collector(env: &Env, collector: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::GovernanceCollector, collector);
}

// ── Market ──────────────────────────────────────────────────────────────────

pub fn get_market(env: &Env, market_id: u64) -> Option<PredictionMarketV2> {
    env.storage().persistent().get(&DataKey::Market(market_id))
}

pub fn set_market(env: &Env, market: &PredictionMarketV2) {
    env.storage()
        .persistent()
        .set(&DataKey::Market(market.market_id), market);
}

#[allow(dead_code)]
pub fn require_market(env: &Env, market_id: u64) -> PredictionMarketV2 {
    get_market(env, market_id).expect("Market not found")
}

// ── Outcome Pools ───────────────────────────────────────────────────────────

pub fn get_outcome_pool(env: &Env, market_id: u64, outcome_index: u32) -> Option<OutcomePool> {
    env.storage()
        .persistent()
        .get(&DataKey::OutcomePool(market_id, outcome_index))
}

pub fn set_outcome_pool(env: &Env, market_id: u64, outcome_index: u32, pool: &OutcomePool) {
    env.storage()
        .persistent()
        .set(&DataKey::OutcomePool(market_id, outcome_index), pool);
}

#[allow(dead_code)]
pub fn require_outcome_pool(env: &Env, market_id: u64, outcome_index: u32) -> OutcomePool {
    get_outcome_pool(env, market_id, outcome_index).expect("Outcome pool not found")
}

// ── Outcome Balances ────────────────────────────────────────────────────────

pub fn get_outcome_balance(env: &Env, market_id: u64, outcome_index: u32, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::OutcomeBalance(
            market_id,
            outcome_index,
            user.clone(),
        ))
        .unwrap_or(0)
}

pub fn set_outcome_balance(
    env: &Env,
    market_id: u64,
    outcome_index: u32,
    user: &Address,
    amount: i128,
) {
    if amount <= 0 {
        env.storage().persistent().remove(&DataKey::OutcomeBalance(
            market_id,
            outcome_index,
            user.clone(),
        ));
    } else {
        env.storage().persistent().set(
            &DataKey::OutcomeBalance(market_id, outcome_index, user.clone()),
            &amount,
        );
    }
}

// ── Collateral ──────────────────────────────────────────────────────────────

pub fn get_market_collateral(env: &Env, market_id: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::MarketCollateral(market_id))
        .unwrap_or(0)
}

pub fn set_market_collateral(env: &Env, market_id: u64, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::MarketCollateral(market_id), &amount);
}

// ── LP Tokens ───────────────────────────────────────────────────────────────

pub fn get_lp_shares(env: &Env, market_id: u64, provider: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::LpShares(market_id, provider.clone()))
        .unwrap_or(0)
}

pub fn set_lp_shares(env: &Env, market_id: u64, provider: &Address, shares: i128) {
    if shares <= 0 {
        env.storage()
            .persistent()
            .remove(&DataKey::LpShares(market_id, provider.clone()));
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::LpShares(market_id, provider.clone()), &shares);
    }
}

pub fn get_lp_total_supply(env: &Env, market_id: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::LpTotalSupply(market_id))
        .unwrap_or(0)
}

pub fn set_lp_total_supply(env: &Env, market_id: u64, supply: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::LpTotalSupply(market_id), &supply);
}

#[allow(dead_code)]
pub fn get_lp_reward_accumulator(env: &Env, market_id: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::LpRewardAccumulator(market_id))
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn set_lp_reward_accumulator(env: &Env, market_id: u64, accumulator: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::LpRewardAccumulator(market_id), &accumulator);
}

// ── Order Book ──────────────────────────────────────────────────────────────

pub fn get_order_counter(env: &Env, market_id: u64) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::OrderCounter(market_id))
        .unwrap_or(0)
}

pub fn set_order_counter(env: &Env, market_id: u64, counter: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::OrderCounter(market_id), &counter);
}

pub fn get_order(env: &Env, market_id: u64, order_id: u64) -> Option<LimitOrder> {
    env.storage()
        .persistent()
        .get(&DataKey::Order(market_id, order_id))
}

pub fn set_order(env: &Env, order: &LimitOrder) {
    env.storage()
        .persistent()
        .set(&DataKey::Order(order.market_id, order.order_id), order);
}

// ── User Positions ──────────────────────────────────────────────────────────

pub fn get_user_position(
    env: &Env,
    market_id: u64,
    outcome_index: u32,
    user: &Address,
) -> Option<UserOutcomePosition> {
    env.storage().persistent().get(&DataKey::UserPosition(
        market_id,
        outcome_index,
        user.clone(),
    ))
}

pub fn set_user_position(env: &Env, position: &UserOutcomePosition) {
    env.storage().persistent().set(
        &DataKey::UserPosition(
            position.market_id,
            position.outcome_index,
            position.owner.clone(),
        ),
        position,
    );
}

// ── Disputes ────────────────────────────────────────────────────────────────

pub fn get_dispute_counter(env: &Env, market_id: u64) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::DisputeCounter(market_id))
        .unwrap_or(0)
}

pub fn set_dispute_counter(env: &Env, market_id: u64, counter: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::DisputeCounter(market_id), &counter);
}

pub fn get_dispute(env: &Env, market_id: u64, dispute_id: u64) -> Option<Dispute> {
    env.storage()
        .persistent()
        .get(&DataKey::Dispute(market_id, dispute_id))
}

pub fn set_dispute(env: &Env, dispute: &Dispute) {
    env.storage().persistent().set(
        &DataKey::Dispute(dispute.market_id, dispute.dispute_id),
        dispute,
    );
}

pub fn has_voted(env: &Env, market_id: u64, dispute_id: u64, voter: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::DisputeVote(market_id, dispute_id, voter.clone()))
}

pub fn set_vote(env: &Env, market_id: u64, dispute_id: u64, voter: &Address, weight: i128) {
    env.storage().persistent().set(
        &DataKey::DisputeVote(market_id, dispute_id, voter.clone()),
        &weight,
    );
}

// ── Total Outcome Supply ────────────────────────────────────────────────────

pub fn get_total_outcome_supply(env: &Env, market_id: u64, outcome_index: u32) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::TotalOutcomeSupply(market_id, outcome_index))
        .unwrap_or(0)
}

pub fn set_total_outcome_supply(env: &Env, market_id: u64, outcome_index: u32, supply: i128) {
    env.storage().persistent().set(
        &DataKey::TotalOutcomeSupply(market_id, outcome_index),
        &supply,
    );
}

// ── Oracle Cache ────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn get_oracle_result_cache(env: &Env, market_id: u64) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::OracleResultCache(market_id))
}

#[allow(dead_code)]
pub fn set_oracle_result_cache(env: &Env, market_id: u64, outcome: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::OracleResultCache(market_id), &outcome);
}

#[allow(dead_code)]
pub fn clear_oracle_result_cache(env: &Env, market_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::OracleResultCache(market_id));
}
