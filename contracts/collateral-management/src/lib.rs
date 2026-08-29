#![no_std]
#![allow(clippy::too_many_arguments)]

mod errors;
mod math;
mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, symbol_short, token::TokenClient, Address, Env, IntoVal, Symbol, Val,
    Vec,
};

use stellai_lib::{rbac, ADMIN_KEY};

use errors::*;
use math::{
    calculate_accrued_interest, calculate_health_factor, calculate_interest_rate,
    calculate_liquidation_seizure, calculate_utilization,
};
use storage::*;
use types::*;

const MAX_LTV_BPS: u32 = 9500;
const MAX_LIQ_THRESHOLD_BPS: u32 = 9800;
const MAX_LIQ_BONUS_BPS: u32 = 2000;

#[contract]
pub struct CollateralManagement;

#[contractimpl]
impl CollateralManagement {
    // ═══════════════════════════════════════════════════════════════
    //  INITIALIZATION
    // ═══════════════════════════════════════════════════════════════

    pub fn initialize(env: Env, admin: Address, oracle: Address, treasury: Option<Address>) {
        if is_initialized(&env) {
            already_initialized();
        }
        admin.require_auth();

        // Store admin under the RBAC-compatible key (same as stellai_lib ADMIN_KEY)
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        set_oracle(&env, &oracle);
        if let Some(t) = treasury {
            set_treasury(&env, &t);
        }
        set_initialized(&env);
        set_paused(&env, false);
        set_reentrancy_lock(&env, false);
        set_loan_counter(&env, 0);

        env.events().publish(
            (symbol_short!("cm_init"),),
            (admin, oracle, env.ledger().timestamp()),
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  COLLATERAL TYPE MANAGEMENT (Admin)
    // ═══════════════════════════════════════════════════════════════

    pub fn register_collateral_type(
        env: Env,
        admin: Address,
        token: Address,
        oracle_feed: Symbol,
        ltv_bps: u32,
        liq_threshold_bps: u32,
        liq_bonus_bps: u32,
        collateral_cap: i128,
        price_scale: i128,
    ) {
        admin.require_auth();
        assert_admin(&env, &admin);
        check_paused(&env);

        if get_collateral_config(&env, &token).is_some() {
            collateral_type_already_exists();
        }
        if ltv_bps > MAX_LTV_BPS {
            invalid_ltv();
        }
        if liq_threshold_bps > MAX_LIQ_THRESHOLD_BPS || liq_threshold_bps < ltv_bps {
            invalid_liquidation_params();
        }
        if liq_bonus_bps > MAX_LIQ_BONUS_BPS {
            invalid_liquidation_params();
        }
        if collateral_cap < 0 || price_scale <= 0 {
            invalid_amount();
        }

        let config = CollateralTypeConfig {
            token: token.clone(),
            oracle_feed,
            ltv_bps,
            liq_threshold_bps,
            liq_bonus_bps,
            collateral_cap,
            is_active: true,
            price_scale,
        };

        set_collateral_config(&env, &config);
        add_collateral_token(&env, &token);

        env.events().publish(
            (symbol_short!("col_reg"),),
            (token, ltv_bps, liq_threshold_bps, liq_bonus_bps),
        );
    }

    pub fn update_collateral_type(
        env: Env,
        admin: Address,
        token: Address,
        ltv_bps: u32,
        liq_threshold_bps: u32,
        liq_bonus_bps: u32,
        collateral_cap: i128,
    ) {
        admin.require_auth();
        assert_admin(&env, &admin);
        check_paused(&env);

        let mut config = match get_collateral_config(&env, &token) {
            Some(c) => c,
            None => collateral_type_not_found(),
        };

        if ltv_bps > MAX_LTV_BPS {
            invalid_ltv();
        }
        if liq_threshold_bps > MAX_LIQ_THRESHOLD_BPS || liq_threshold_bps < ltv_bps {
            invalid_liquidation_params();
        }
        if liq_bonus_bps > MAX_LIQ_BONUS_BPS {
            invalid_liquidation_params();
        }

        config.ltv_bps = ltv_bps;
        config.liq_threshold_bps = liq_threshold_bps;
        config.liq_bonus_bps = liq_bonus_bps;
        config.collateral_cap = collateral_cap;
        set_collateral_config(&env, &config);

        env.events().publish(
            (symbol_short!("col_upd"),),
            (token, ltv_bps, liq_threshold_bps),
        );
    }

    pub fn deactivate_collateral_type(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        let mut config = match get_collateral_config(&env, &token) {
            Some(c) => c,
            None => collateral_type_not_found(),
        };
        config.is_active = false;
        set_collateral_config(&env, &config);
        env.events().publish((symbol_short!("col_off"),), (token,));
    }

    pub fn reactivate_collateral_type(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        let mut config = match get_collateral_config(&env, &token) {
            Some(c) => c,
            None => collateral_type_not_found(),
        };
        config.is_active = true;
        set_collateral_config(&env, &config);
        env.events().publish((symbol_short!("col_on"),), (token,));
    }

    // ═══════════════════════════════════════════════════════════════
    //  PROTOCOL PARAMS (Admin)
    // ═══════════════════════════════════════════════════════════════

    pub fn set_protocol_parameters(env: Env, admin: Address, params: ProtocolParams) {
        admin.require_auth();
        assert_admin(&env, &admin);
        set_protocol_params(&env, &params);
        env.events().publish(
            (symbol_short!("proto_upd"),),
            (params.debt_ceiling, params.liq_health_threshold_bps),
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  ORACLE PRICE MANAGEMENT (Admin — for testing & manual feeds)
    // ═══════════════════════════════════════════════════════════════

    /// Admin sets a price for an oracle feed (used in testing or manual feeds).
    /// In production, prices would come from the oracle contract via `get_oracle_price`.
    pub fn set_price(env: Env, admin: Address, feed_id: Symbol, price: i128) {
        admin.require_auth();
        assert_admin(&env, &admin);
        if price <= 0 {
            invalid_amount();
        }
        set_price_cache(&env, &feed_id, price);
        env.events().publish(
            (symbol_short!("price_set"),),
            (feed_id, price, env.ledger().timestamp()),
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  DEPOSIT / WITHDRAW COLLATERAL
    // ═══════════════════════════════════════════════════════════════

    pub fn deposit_collateral(env: Env, user: Address, token: Address, amount: i128) {
        user.require_auth();
        check_paused(&env);

        if amount <= 0 {
            invalid_amount();
        }

        let config = match get_collateral_config(&env, &token) {
            Some(c) => c,
            None => collateral_type_not_found(),
        };
        if !config.is_active {
            collateral_type_inactive();
        }

        // Collateral cap
        if config.collateral_cap > 0 {
            let current = get_total_collateral_for_type(&env, &token);
            if current + amount > config.collateral_cap {
                collateral_cap_exceeded();
            }
        }

        // Per-user cap
        let params = get_protocol_params(&env);
        if params.max_collateral_per_user > 0 {
            let user_coll = get_user_collateral(&env, &user, &token);
            if user_coll.amount + amount > params.max_collateral_per_user {
                collateral_per_user_exceeded();
            }
        }

        // Transfer tokens from user to contract
        let contract_addr = env.current_contract_address();
        TokenClient::new(&env, &token).transfer(&user, &contract_addr, &amount);

        // Update user collateral
        let mut user_coll = get_user_collateral(&env, &user, &token);
        user_coll.amount += amount;
        user_coll.last_updated = env.ledger().timestamp();
        set_user_collateral(&env, &user_coll);

        // Update global total
        let total = get_total_collateral_for_type(&env, &token);
        set_total_collateral_for_type(&env, &token, total + amount);

        env.events().publish(
            (symbol_short!("col_dep"),),
            (user, token, amount, env.ledger().timestamp()),
        );
    }

    pub fn withdraw_collateral(env: Env, user: Address, token: Address, amount: i128) {
        user.require_auth();
        check_paused(&env);
        with_guard(&env, || {
            Self::withdraw_internal(&env, &user, &token, amount);
        });
    }

    // ═══════════════════════════════════════════════════════════════
    //  BORROW
    // ═══════════════════════════════════════════════════════════════

    pub fn borrow(env: Env, user: Address, borrow_token: Address, amount: i128) {
        user.require_auth();
        check_paused(&env);
        with_guard(&env, || {
            Self::borrow_internal(&env, &user, &borrow_token, amount);
        });
    }

    // ═══════════════════════════════════════════════════════════════
    //  REPAY
    // ═══════════════════════════════════════════════════════════════

    pub fn repay_loan(env: Env, user: Address, loan_id: u64, amount: i128) {
        user.require_auth();
        check_paused(&env);
        with_guard(&env, || {
            Self::repay_internal(&env, &user, loan_id, amount);
        });
    }

    // ═══════════════════════════════════════════════════════════════
    //  LIQUIDATE
    // ═══════════════════════════════════════════════════════════════

    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        loan_id: u64,
        debt_to_cover: i128,
    ) {
        liquidator.require_auth();
        check_paused(&env);
        with_guard(&env, || {
            Self::liquidate_internal(&env, &liquidator, &borrower, loan_id, debt_to_cover);
        });
    }

    // ═══════════════════════════════════════════════════════════════
    //  INTEREST ACCRUAL
    // ═══════════════════════════════════════════════════════════════

    pub fn accrue_interest(env: Env, borrower: Address, loan_id: u64) {
        Self::accrue_one(&env, &borrower, loan_id);
    }

    pub fn accrue_all_interest(env: Env, borrower: Address) {
        let ids = get_user_loan_ids(&env, &borrower);
        for i in 0..ids.len() {
            Self::accrue_one(&env, &borrower, ids.get(i).unwrap());
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  VIEW FUNCTIONS
    // ═══════════════════════════════════════════════════════════════

    pub fn get_interest_rate(env: Env, borrow_token: Address) -> u32 {
        let params = get_protocol_params(&env);
        let debt = get_total_debt(&env, &borrow_token);
        let deposits = get_lending_pool_deposits(&env, &borrow_token);
        let util = calculate_utilization(debt, deposits);
        calculate_interest_rate(&params, util)
    }

    pub fn get_health_factor(env: Env, user: Address) -> HealthFactor {
        Self::calc_user_health(&env, &user)
    }

    pub fn get_user_collateral(env: Env, user: Address, token: Address) -> UserCollateral {
        get_user_collateral(&env, &user, &token)
    }

    pub fn get_user_loan(env: Env, borrower: Address, loan_id: u64) -> Loan {
        Self::require_user_loan(&env, &borrower, loan_id)
    }

    pub fn get_user_loan_ids(env: Env, user: Address) -> Vec<u64> {
        get_user_loan_ids(&env, &user)
    }

    pub fn get_collateral_type(env: Env, token: Address) -> CollateralTypeConfig {
        match get_collateral_config(&env, &token) {
            Some(c) => c,
            None => collateral_type_not_found(),
        }
    }

    pub fn get_all_collateral_tokens(env: Env) -> Vec<Address> {
        get_collateral_tokens(&env)
    }

    pub fn get_protocol_parameters(env: Env) -> ProtocolParams {
        get_protocol_params(&env)
    }

    pub fn get_total_protocol_debt(env: Env, borrow_token: Address) -> i128 {
        get_total_debt(&env, &borrow_token)
    }

    pub fn get_total_collateral(env: Env, token: Address) -> i128 {
        get_total_collateral_for_type(&env, &token)
    }

    pub fn can_liquidate(env: Env, borrower: Address, loan_id: u64) -> bool {
        let loan = Self::require_user_loan(&env, &borrower, loan_id);
        if loan.is_liquidated || loan.is_repaid {
            return false;
        }
        let hf = Self::calc_user_health(&env, &borrower);
        let threshold = get_protocol_params(&env).liq_health_threshold_bps as i128;
        hf.health_factor_bps < threshold
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        set_paused(&env, true);
        env.events()
            .publish((symbol_short!("cm_pause"),), (env.ledger().timestamp(),));
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        set_paused(&env, false);
        env.events()
            .publish((symbol_short!("cm_unp"),), (env.ledger().timestamp(),));
    }

    // ═══════════════════════════════════════════════════════════════
    //  INTERNAL: WITHDRAW
    // ═══════════════════════════════════════════════════════════════

    fn withdraw_internal(env: &Env, user: &Address, token: &Address, amount: i128) {
        if amount <= 0 {
            invalid_amount();
        }

        let user_coll = get_user_collateral(env, user, token);
        if user_coll.amount < amount {
            insufficient_collateral();
        }

        // Check safety: withdrawal must not undercollateralize active loans
        let remaining = user_coll.amount - amount;
        let total_debt = Self::user_total_debt(env, user);

        if total_debt > 0 {
            let total_value = Self::total_collateral_value_override(env, user, token, remaining);
            let wt = Self::weighted_threshold_override(env, user, token, remaining);
            let hf = calculate_health_factor(total_value, total_debt, wt);
            let threshold = get_protocol_params(env).liq_health_threshold_bps as i128;
            if hf.health_factor_bps < threshold {
                withdrawal_would_undercollateralize();
            }
        }

        // Transfer tokens back
        let contract_addr = env.current_contract_address();
        TokenClient::new(env, token).transfer(&contract_addr, user, &amount);

        // Update user collateral
        let mut updated = user_coll;
        updated.amount = remaining;
        updated.last_updated = env.ledger().timestamp();
        set_user_collateral(env, &updated);

        // Update global total
        let total = get_total_collateral_for_type(env, token);
        set_total_collateral_for_type(env, token, total - amount);

        env.events().publish(
            (symbol_short!("col_wd"),),
            (
                user.clone(),
                token.clone(),
                amount,
                env.ledger().timestamp(),
            ),
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  INTERNAL: BORROW
    // ═══════════════════════════════════════════════════════════════

    fn borrow_internal(env: &Env, user: &Address, borrow_token: &Address, amount: i128) {
        if amount <= 0 {
            invalid_amount();
        }

        let params = get_protocol_params(env);

        // Per-user borrow cap
        if params.max_borrow_per_user > 0 {
            let existing = Self::user_debt_for_token(env, user, borrow_token);
            if existing + amount > params.max_borrow_per_user {
                borrow_cap_exceeded();
            }
        }

        // Debt ceiling
        if params.debt_ceiling > 0 {
            let current = get_total_debt(env, borrow_token);
            if current + amount > params.debt_ceiling {
                debt_ceiling_exceeded();
            }
        }

        // Accrue interest on existing loans first
        let ids = get_user_loan_ids(env, user);
        for i in 0..ids.len() {
            Self::accrue_one(env, user, ids.get(i).unwrap());
        }

        // Check health after new borrow
        let total_debt_after = Self::user_total_debt(env, user) + amount;
        let total_collateral_value = Self::total_collateral_value(env, user);

        if total_debt_after > 0 {
            if total_collateral_value <= 0 {
                health_factor_insufficient();
            }
            let wt = Self::weighted_threshold(env, user);
            let hf = calculate_health_factor(total_collateral_value, total_debt_after, wt);
            let threshold = params.liq_health_threshold_bps as i128;
            if hf.health_factor_bps < threshold {
                health_factor_insufficient();
            }
        }

        // Calculate interest rate based on current utilization
        let debt = get_total_debt(env, borrow_token);
        let deposits = get_lending_pool_deposits(env, borrow_token);
        let util = calculate_utilization(debt, deposits);
        let rate = calculate_interest_rate(&params, util);

        // Create loan
        let loan_id = get_loan_counter(env);
        let now = env.ledger().timestamp();

        let loan = Loan {
            loan_id,
            borrower: user.clone(),
            borrow_token: borrow_token.clone(),
            principal: amount,
            accrued_interest: 0,
            total_debt: amount,
            interest_rate_bps: rate,
            created_at: now,
            last_accrual_update: now,
            is_liquidated: false,
            is_repaid: false,
        };

        set_loan(env, &loan);
        add_user_loan_id(env, user, loan_id);
        set_loan_counter(env, loan_id + 1);
        set_total_debt(env, borrow_token, debt + amount);

        // Transfer borrowed tokens to user
        let contract_addr = env.current_contract_address();
        TokenClient::new(env, borrow_token).transfer(&contract_addr, user, &amount);

        env.events().publish(
            (symbol_short!("loan_new"),),
            (loan_id, user.clone(), borrow_token.clone(), amount, rate),
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  INTERNAL: REPAY
    // ═══════════════════════════════════════════════════════════════

    fn repay_internal(env: &Env, user: &Address, loan_id: u64, amount: i128) {
        if amount <= 0 {
            invalid_amount();
        }

        let mut loan = Self::require_user_loan(env, user, loan_id);

        if loan.is_liquidated {
            loan_already_liquidated();
        }
        if loan.is_repaid {
            loan_already_repaid();
        }

        // Accrue interest first
        Self::accrue_loan(env, &mut loan);

        if amount > loan.total_debt {
            repayment_exceeds_debt();
        }

        // Transfer tokens from user to contract
        let contract_addr = env.current_contract_address();
        TokenClient::new(env, &loan.borrow_token).transfer(user, &contract_addr, &amount);

        // Update loan
        let new_debt = loan.total_debt - amount;

        // Reduce accrued interest first, then principal
        let interest_paid = amount.min(loan.accrued_interest);
        let principal_paid = amount - interest_paid;

        loan.accrued_interest -= interest_paid;
        loan.principal -= principal_paid;
        loan.total_debt = new_debt;

        if new_debt <= 0 {
            loan.is_repaid = true;
            loan.total_debt = 0;
            loan.principal = 0;
            loan.accrued_interest = 0;
        }

        set_loan(env, &loan);

        // Update global total debt
        let current = get_total_debt(env, &loan.borrow_token);
        set_total_debt(env, &loan.borrow_token, current - amount);

        env.events().publish(
            (symbol_short!("loan_rpy"),),
            (
                loan_id,
                user.clone(),
                amount,
                new_debt <= 0,
                env.ledger().timestamp(),
            ),
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  INTERNAL: LIQUIDATE
    // ═══════════════════════════════════════════════════════════════

    fn liquidate_internal(
        env: &Env,
        liquidator: &Address,
        borrower: &Address,
        loan_id: u64,
        debt_to_cover: i128,
    ) {
        if debt_to_cover <= 0 {
            invalid_amount();
        }

        let mut loan = Self::require_user_loan(env, borrower, loan_id);

        if loan.is_liquidated {
            loan_already_liquidated();
        }
        if loan.is_repaid {
            loan_already_repaid();
        }

        // Accrue interest
        Self::accrue_loan(env, &mut loan);

        // Check undercollateralized
        let hf = Self::calc_user_health(env, borrower);
        let threshold = get_protocol_params(env).liq_health_threshold_bps as i128;
        if hf.health_factor_bps >= threshold {
            health_factor_insufficient();
        }

        // Cap at total debt
        let actual_covered = debt_to_cover.min(loan.total_debt);

        // Find best collateral to seize
        let (coll_token, coll_config) = match Self::best_seize_collateral(env, borrower) {
            Some(v) => v,
            None => no_collateral_to_seize(),
        };

        // Get oracle price
        let oracle_price =
            Self::get_oracle_price(env, &coll_config.oracle_feed, coll_config.price_scale);

        // Calculate seizure amount
        let (coll_seized, bonus) =
            calculate_liquidation_seizure(actual_covered, oracle_price, coll_config.liq_bonus_bps);

        // Check borrower has enough collateral
        let user_coll = get_user_collateral(env, borrower, &coll_token);
        if user_coll.amount < coll_seized {
            no_collateral_to_seize();
        }

        // Transfer repayment from liquidator
        let contract_addr = env.current_contract_address();
        TokenClient::new(env, &loan.borrow_token).transfer(
            liquidator,
            &contract_addr,
            &actual_covered,
        );

        // Transfer seized collateral to liquidator
        TokenClient::new(env, &coll_token).transfer(&contract_addr, liquidator, &coll_seized);

        // Update borrower's collateral
        let mut updated_coll = user_coll;
        updated_coll.amount -= coll_seized;
        updated_coll.last_updated = env.ledger().timestamp();
        set_user_collateral(env, &updated_coll);

        // Update global collateral total
        let total = get_total_collateral_for_type(env, &coll_token);
        set_total_collateral_for_type(env, &coll_token, total - coll_seized);

        // Update loan
        let new_debt = loan.total_debt - actual_covered;
        let interest_paid = actual_covered.min(loan.accrued_interest);
        let principal_paid = actual_covered - interest_paid;

        loan.accrued_interest -= interest_paid;
        loan.principal -= principal_paid;
        loan.total_debt = new_debt;

        if new_debt <= 0 {
            loan.is_liquidated = true;
            loan.total_debt = 0;
            loan.principal = 0;
            loan.accrued_interest = 0;
        }

        set_loan(env, &loan);

        // Update global total debt
        let current_debt = get_total_debt(env, &loan.borrow_token);
        set_total_debt(env, &loan.borrow_token, current_debt - actual_covered);

        env.events().publish(
            (symbol_short!("loan_liq"),),
            (
                loan_id,
                borrower.clone(),
                liquidator.clone(),
                actual_covered,
                coll_seized,
                bonus,
                env.ledger().timestamp(),
            ),
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  INTERNAL: INTEREST ACCRUAL
    // ═══════════════════════════════════════════════════════════════

    fn accrue_one(env: &Env, borrower: &Address, loan_id: u64) {
        let mut loan = Self::require_user_loan(env, borrower, loan_id);
        Self::accrue_loan(env, &mut loan);
        set_loan(env, &loan);
    }

    fn accrue_loan(env: &Env, loan: &mut Loan) {
        if loan.is_liquidated || loan.is_repaid {
            return;
        }

        let now = env.ledger().timestamp();
        let delta = now as i128 - loan.last_accrual_update as i128;
        if delta <= 0 {
            return;
        }

        let interest = calculate_accrued_interest(loan.principal, loan.interest_rate_bps, delta);
        if interest > 0 {
            loan.accrued_interest += interest;
            loan.total_debt = loan.principal + loan.accrued_interest;
            loan.last_accrual_update = now;

            // Update global total debt
            let current = get_total_debt(env, &loan.borrow_token);
            set_total_debt(env, &loan.borrow_token, current + interest);

            env.events().publish(
                (symbol_short!("int_accr"),),
                (loan.loan_id, interest, loan.total_debt, now),
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  INTERNAL: HEALTH FACTOR HELPERS
    // ═══════════════════════════════════════════════════════════════

    fn calc_user_health(env: &Env, user: &Address) -> HealthFactor {
        let debt = Self::user_total_debt(env, user);
        let value = Self::total_collateral_value(env, user);
        let wt = Self::weighted_threshold(env, user);
        calculate_health_factor(value, debt, wt)
    }

    fn user_total_debt(env: &Env, user: &Address) -> i128 {
        let ids = get_user_loan_ids(env, user);
        let mut total = 0i128;
        for i in 0..ids.len() {
            let loan = get_loan(env, ids.get(i).unwrap()).unwrap_or_else(|| loan_not_found());
            if !loan.is_liquidated && !loan.is_repaid && loan.borrower == *user {
                total += loan.total_debt;
            }
        }
        total
    }

    fn user_debt_for_token(env: &Env, user: &Address, token: &Address) -> i128 {
        let ids = get_user_loan_ids(env, user);
        let mut total = 0i128;
        for i in 0..ids.len() {
            let loan = get_loan(env, ids.get(i).unwrap()).unwrap_or_else(|| loan_not_found());
            if !loan.is_liquidated
                && !loan.is_repaid
                && loan.borrower == *user
                && loan.borrow_token == *token
            {
                total += loan.total_debt;
            }
        }
        total
    }

    fn total_collateral_value(env: &Env, user: &Address) -> i128 {
        let tokens = get_collateral_tokens(env);
        let mut total = 0i128;
        for i in 0..tokens.len() {
            let tok = tokens.get(i).unwrap();
            let uc = get_user_collateral(env, user, &tok);
            if uc.amount > 0 {
                if let Some(cfg) = get_collateral_config(env, &tok) {
                    let price = Self::get_oracle_price(env, &cfg.oracle_feed, cfg.price_scale);
                    total += (uc.amount * price) / cfg.price_scale;
                }
            }
        }
        total
    }

    fn total_collateral_value_override(
        env: &Env,
        user: &Address,
        override_token: &Address,
        override_amt: i128,
    ) -> i128 {
        let tokens = get_collateral_tokens(env);
        let mut total = 0i128;
        for i in 0..tokens.len() {
            let tok = tokens.get(i).unwrap();
            let amt = if tok == *override_token {
                override_amt
            } else {
                get_user_collateral(env, user, &tok).amount
            };
            if amt > 0 {
                if let Some(cfg) = get_collateral_config(env, &tok) {
                    let price = Self::get_oracle_price(env, &cfg.oracle_feed, cfg.price_scale);
                    total += (amt * price) / cfg.price_scale;
                }
            }
        }
        total
    }

    fn weighted_threshold(env: &Env, user: &Address) -> u32 {
        let tokens = get_collateral_tokens(env);
        let mut weighted_sum: i128 = 0;
        let mut total_value: i128 = 0;
        for i in 0..tokens.len() {
            let tok = tokens.get(i).unwrap();
            let uc = get_user_collateral(env, user, &tok);
            if uc.amount > 0 {
                if let Some(cfg) = get_collateral_config(env, &tok) {
                    let price = Self::get_oracle_price(env, &cfg.oracle_feed, cfg.price_scale);
                    let value = (uc.amount * price) / cfg.price_scale;
                    total_value += value;
                    weighted_sum += value * cfg.liq_threshold_bps as i128;
                }
            }
        }
        if total_value <= 0 {
            return 0;
        }
        (weighted_sum / total_value).min(u32::MAX as i128) as u32
    }

    fn weighted_threshold_override(
        env: &Env,
        user: &Address,
        override_token: &Address,
        override_amt: i128,
    ) -> u32 {
        let tokens = get_collateral_tokens(env);
        let mut weighted_sum: i128 = 0;
        let mut total_value: i128 = 0;
        for i in 0..tokens.len() {
            let tok = tokens.get(i).unwrap();
            let amt = if tok == *override_token {
                override_amt
            } else {
                get_user_collateral(env, user, &tok).amount
            };
            if amt > 0 {
                if let Some(cfg) = get_collateral_config(env, &tok) {
                    let price = Self::get_oracle_price(env, &cfg.oracle_feed, cfg.price_scale);
                    let value = (amt * price) / cfg.price_scale;
                    total_value += value;
                    weighted_sum += value * cfg.liq_threshold_bps as i128;
                }
            }
        }
        if total_value <= 0 {
            return 0;
        }
        (weighted_sum / total_value).min(u32::MAX as i128) as u32
    }

    /// Find best collateral to seize (highest value).
    fn best_seize_collateral(
        env: &Env,
        borrower: &Address,
    ) -> Option<(Address, CollateralTypeConfig)> {
        let tokens = get_collateral_tokens(env);
        let mut best: Option<(Address, CollateralTypeConfig, i128)> = None;

        for i in 0..tokens.len() {
            let tok = tokens.get(i).unwrap();
            let uc = get_user_collateral(env, borrower, &tok);
            if uc.amount > 0 {
                if let Some(cfg) = get_collateral_config(env, &tok) {
                    let price = Self::get_oracle_price(env, &cfg.oracle_feed, cfg.price_scale);
                    let value = (uc.amount * price) / cfg.price_scale;
                    match &best {
                        None => best = Some((tok, cfg, value)),
                        Some((_, _, best_val)) if value > *best_val => {
                            best = Some((tok, cfg, value));
                        }
                        _ => {}
                    }
                }
            }
        }

        best.map(|(tok, cfg, _)| (tok, cfg))
    }

    fn require_user_loan(env: &Env, borrower: &Address, loan_id: u64) -> Loan {
        let ids = get_user_loan_ids(env, borrower);
        for i in 0..ids.len() {
            if ids.get(i).unwrap() == loan_id {
                return get_loan(env, loan_id).unwrap_or_else(|| loan_not_found());
            }
        }
        loan_not_found();
    }

    /// Get oracle price for a feed.
    /// First checks the price cache (for admin-set prices / tests).
    /// Falls back to the oracle contract if a price is cached.
    fn get_oracle_price(env: &Env, feed_id: &Symbol, _price_scale: i128) -> i128 {
        // Check local price cache first (set by admin for testing)
        if let Some(price) = get_price_cache(env, feed_id) {
            return price;
        }

        // Try the oracle contract
        let oracle_addr = get_oracle(env);
        let mut args = Vec::<Val>::new(env);
        args.push_back(feed_id.clone().into_val(env));
        let result: i128 = env.invoke_contract(&oracle_addr, &symbol_short!("get_price"), args);
        if result <= 0 {
            oracle_price_unavailable();
        }
        result
    }
}

// ═══════════════════════════════════════════════════════════════
//  MODULE-LEVEL HELPERS
// ═══════════════════════════════════════════════════════════════

fn assert_admin(env: &Env, caller: &Address) {
    rbac::require_admin(env, caller).unwrap_or_else(|_| unauthorized());
}

fn check_paused(env: &Env) {
    if is_paused(env) {
        protocol_paused();
    }
}

fn with_guard<F, R>(env: &Env, f: F) -> R
where
    F: FnOnce() -> R,
{
    if is_reentrancy_locked(env) {
        reentrancy_detected();
    }
    set_reentrancy_lock(env, true);
    let result = f();
    set_reentrancy_lock(env, false);
    result
}
