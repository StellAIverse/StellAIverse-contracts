use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::types::{CollateralTypeConfig, DataKey, Loan, ProtocolParams, UserCollateral};

/* ──────────────────── INIT ──────────────────── */

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Initialized)
}

pub fn set_initialized(env: &Env) {
    env.storage().instance().set(&DataKey::Initialized, &true);
}

/* ──────────────────── ADMIN ──────────────────── */

#[allow(dead_code)]
pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("admin not set")
}

#[allow(dead_code)]
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

/* ──────────────────── ORACLE ──────────────────── */

pub fn get_oracle(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Oracle)
        .expect("oracle not set")
}

pub fn set_oracle(env: &Env, oracle: &Address) {
    env.storage().instance().set(&DataKey::Oracle, oracle);
}

/* ──────────────────── TREASURY ──────────────────── */

#[allow(dead_code)]
pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Treasury)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}

/* ──────────────────── PAUSE ──────────────────── */

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

/* ──────────────────── REENTRANCY ──────────────────── */

pub fn is_reentrancy_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::ReentrancyLock)
        .unwrap_or(false)
}

pub fn set_reentrancy_lock(env: &Env, locked: bool) {
    env.storage()
        .instance()
        .set(&DataKey::ReentrancyLock, &locked);
}

/* ──────────────────── LOAN COUNTER ──────────────────── */

pub fn get_loan_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::LoanCounter)
        .unwrap_or(0)
}

pub fn set_loan_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::LoanCounter, &counter);
}

/* ──────────────────── COLLATERAL TYPES ──────────────────── */

pub fn get_collateral_config(env: &Env, token: &Address) -> Option<CollateralTypeConfig> {
    env.storage()
        .persistent()
        .get(&DataKey::CollateralType(token.clone()))
}

pub fn set_collateral_config(env: &Env, config: &CollateralTypeConfig) {
    env.storage()
        .persistent()
        .set(&DataKey::CollateralType(config.token.clone()), config);
}

pub fn get_collateral_tokens(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::CollateralTokens)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_collateral_tokens(env: &Env, tokens: &Vec<Address>) {
    env.storage()
        .instance()
        .set(&DataKey::CollateralTokens, tokens);
}

pub fn add_collateral_token(env: &Env, token: &Address) {
    let mut tokens = get_collateral_tokens(env);
    tokens.push_back(token.clone());
    set_collateral_tokens(env, &tokens);
}

/* ──────────────────── USER COLLATERAL ──────────────────── */

pub fn get_user_collateral(env: &Env, user: &Address, token: &Address) -> UserCollateral {
    env.storage()
        .persistent()
        .get(&DataKey::UserCollateral(user.clone(), token.clone()))
        .unwrap_or(UserCollateral {
            user: user.clone(),
            token: token.clone(),
            amount: 0,
            last_updated: 0,
        })
}

pub fn set_user_collateral(env: &Env, collateral: &UserCollateral) {
    env.storage().persistent().set(
        &DataKey::UserCollateral(collateral.user.clone(), collateral.token.clone()),
        collateral,
    );
}

/* ──────────────────── LOANS ──────────────────── */

pub fn get_loan(env: &Env, loan_id: u64) -> Option<Loan> {
    env.storage().persistent().get(&DataKey::Loan(loan_id))
}

pub fn set_loan(env: &Env, loan: &Loan) {
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.loan_id), loan);
}

pub fn get_user_loan_ids(env: &Env, user: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::UserLoanIds(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_user_loan_ids(env: &Env, user: &Address, ids: &Vec<u64>) {
    env.storage()
        .persistent()
        .set(&DataKey::UserLoanIds(user.clone()), ids);
}

pub fn add_user_loan_id(env: &Env, user: &Address, loan_id: u64) {
    let mut ids = get_user_loan_ids(env, user);
    ids.push_back(loan_id);
    set_user_loan_ids(env, user, &ids);
}

/* ──────────────────── PROTOCOL PARAMS ──────────────────── */

pub fn get_protocol_params(env: &Env) -> ProtocolParams {
    env.storage()
        .instance()
        .get(&DataKey::ProtocolParams)
        .unwrap_or(ProtocolParams {
            debt_ceiling: 0,
            liq_health_threshold_bps: 10000,
            base_interest_rate_bps: 200,
            interest_slope1_bps: 400,
            interest_slope2_bps: 7500,
            optimal_utilization_bps: 8000,
            max_borrow_per_user: 0,
            max_collateral_per_user: 0,
        })
}

pub fn set_protocol_params(env: &Env, params: &ProtocolParams) {
    env.storage()
        .instance()
        .set(&DataKey::ProtocolParams, params);
}

/* ──────────────────── TOTAL DEBT ──────────────────── */

pub fn get_total_debt(env: &Env, token: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalDebt(token.clone()))
        .unwrap_or(0)
}

pub fn set_total_debt(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .instance()
        .set(&DataKey::TotalDebt(token.clone()), &amount);
}

/* ──────────────────── COLLATERAL / LENDING POOL TRACKING ──────────────────── */

pub fn get_total_collateral_for_type(env: &Env, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::TotalCollateral(token.clone()))
        .unwrap_or(0)
}

pub fn set_total_collateral_for_type(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::TotalCollateral(token.clone()), &amount);
}

pub fn get_lending_pool_deposits(env: &Env, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::LendingPoolDeposits(token.clone()))
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn set_lending_pool_deposits(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::LendingPoolDeposits(token.clone()), &amount);
}

/* ──────────────────── PRICE CACHE (for oracle integration / mock) ──────────────────── */

pub fn get_price_cache(env: &Env, feed_id: &Symbol) -> Option<i128> {
    env.storage()
        .persistent()
        .get(&DataKey::PriceCache(feed_id.clone()))
}

pub fn set_price_cache(env: &Env, feed_id: &Symbol, price: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::PriceCache(feed_id.clone()), &price);
}
