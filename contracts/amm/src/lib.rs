#![no_std]

mod math;
mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, symbol_short, token::TokenClient, Address, Env, String, Symbol, Vec,
};
use stellai_lib::{errors::ContractError, rbac, ADMIN_KEY};

use math::{ceil_div, floor_div, get_amount_in, get_amount_out, isqrt};
use storage::*;
use types::*;

const BPS_DENOMINATOR: i128 = 10_000;
const MAX_FEE_BPS: u32 = 1_000;
const MAX_PROTOCOL_FEE_SHARE_BPS: u32 = 5_000;
const DEPOSIT_RATIO_TOLERANCE_BPS: i128 = 500;
const PRICE_SCALE: i128 = 1_000_000;

#[contract]
pub struct Amm;

#[contractimpl]
impl Amm {
    /// Initialize the AMM contract with an admin and optional governance fee collector.
    pub fn initialize(env: Env, admin: Address, governance_collector: Option<Address>) {
        if env.storage().instance().has(&Symbol::new(&env, ADMIN_KEY)) {
            panic!("Contract already initialized");
        }
        admin.require_auth();

        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        set_pool_counter(&env, 0);
        set_trading_paused(&env, false);
        set_protocol_fee_share_bps(&env, 0);
        set_reentrancy_lock(&env, false);

        if let Some(collector) = governance_collector {
            set_governance_collector(&env, &collector);
        }

        env.events().publish((symbol_short!("amm_init"),), admin);
    }

    /// Create a new liquidity pool for a token pair.
    /// `fee_bps` is the swap fee in basis points (max 1000 = 10%).
    pub fn create_pool(
        env: Env,
        admin: Address,
        token_a: Address,
        token_b: Address,
        fee_bps: u32,
    ) -> u64 {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if token_a == token_b {
            panic!("Token addresses must differ");
        }
        if fee_bps > MAX_FEE_BPS {
            panic!("Fee cannot exceed 10%");
        }

        let pool_id = get_pool_counter(&env);
        let pool = Pool {
            pool_id,
            token_a,
            token_b,
            reserve_a: 0,
            reserve_b: 0,
            lp_total_supply: 0,
            fee_bps,
        };

        set_pool(&env, &pool);
        set_pool_counter(&env, pool_id + 1);

        env.events().publish(
            (Symbol::new(&env, "PoolCreated"),),
            (pool_id, &pool.token_a, &pool.token_b, fee_bps),
        );

        pool_id
    }

    /// Add liquidity to a pool. Returns the number of LP tokens minted.
    pub fn add_liquidity(
        env: Env,
        provider: Address,
        pool_id: u64,
        amount_a: i128,
        amount_b: i128,
    ) -> i128 {
        provider.require_auth();
        Self::with_reentrancy_guard(&env, || {
            Self::add_liquidity_internal(&env, &provider, pool_id, amount_a, amount_b)
        })
    }

    /// Remove liquidity from a pool by burning LP tokens.
    /// Returns (amount_a, amount_b) withdrawn.
    pub fn remove_liquidity(
        env: Env,
        provider: Address,
        pool_id: u64,
        lp_amount: i128,
    ) -> (i128, i128) {
        provider.require_auth();
        Self::with_reentrancy_guard(&env, || {
            Self::remove_liquidity_internal(&env, &provider, pool_id, lp_amount)
        })
    }

    /// Execute a swap on a pool with slippage protection.
    pub fn swap(
        env: Env,
        user: Address,
        pool_id: u64,
        token_in: Address,
        amount_in: i128,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();
        Self::check_trading_allowed(&env);
        Self::with_reentrancy_guard(&env, || {
            Self::swap_internal(
                &env,
                &user,
                pool_id,
                &token_in,
                amount_in,
                min_amount_out,
                true,
            )
        })
    }

    /// Quote the output amount for a swap without executing it.
    pub fn quote_swap(env: Env, pool_id: u64, token_in: Address, amount_in: i128) -> i128 {
        if amount_in <= 0 {
            panic!("Input amount must be positive");
        }
        let pool = get_pool(&env, pool_id);
        let (reserve_in, reserve_out) = Self::reserves_for_token(&pool, &token_in);
        get_amount_out(amount_in, reserve_in, reserve_out, pool.fee_bps)
    }

    /// Execute an atomic flash swap: borrow `amount_out`, then repay `token_in` in the same call.
    pub fn flash_swap(
        env: Env,
        borrower: Address,
        pool_id: u64,
        token_out: Address,
        amount_out: i128,
        token_in: Address,
        max_amount_in: i128,
    ) -> i128 {
        borrower.require_auth();
        Self::check_trading_allowed(&env);

        if amount_out <= 0 {
            panic!("Flash swap amount must be positive");
        }

        Self::with_reentrancy_guard(&env, || {
            let mut pool = get_pool(&env, pool_id);
            Self::validate_pool_tokens(&pool, &token_in, &token_out);

            let (reserve_in, reserve_out, out_is_a) =
                Self::reserves_for_pair(&pool, &token_in, &token_out);

            if reserve_out <= amount_out {
                panic!("Insufficient pool liquidity for flash swap");
            }

            let amount_in_required =
                get_amount_in(amount_out, reserve_in, reserve_out, pool.fee_bps);

            if amount_in_required > max_amount_in {
                panic!("Flash swap repayment exceeds max_amount_in");
            }

            let contract_addr = env.current_contract_address();
            let token_out_client = TokenClient::new(&env, &token_out);
            let token_in_client = TokenClient::new(&env, &token_in);

            // Lend tokens to borrower, then collect repayment atomically.
            token_out_client.transfer(&contract_addr, &borrower, &amount_out);
            token_in_client.transfer(&borrower, &contract_addr, &amount_in_required);

            let protocol_share_bps = get_protocol_fee_share_bps(&env);
            let fee_amount = Self::calculate_swap_fee(amount_in_required, pool.fee_bps);
            let protocol_fee = (fee_amount * protocol_share_bps as i128) / BPS_DENOMINATOR;

            if protocol_fee > 0 {
                if let Some(collector) = get_governance_collector(&env) {
                    token_in_client.transfer(&contract_addr, &collector, &protocol_fee);
                }
            }

            let net_in = amount_in_required - protocol_fee;
            if out_is_a {
                pool.reserve_a -= amount_out;
                pool.reserve_b += net_in;
            } else {
                pool.reserve_b -= amount_out;
                pool.reserve_a += net_in;
            }

            set_pool(&env, &pool);
            invalidate_query_cache(&env, pool_id);

            env.events().publish(
                (Symbol::new(&env, "FlashSwap"),),
                (
                    pool_id,
                    borrower.clone(),
                    token_out.clone(),
                    token_in.clone(),
                    amount_out,
                    amount_in_required,
                ),
            );

            amount_in_required
        })
    }

    /// Deposit bonus reward tokens for LP providers to claim proportionally.
    pub fn deposit_lp_rewards(
        env: Env,
        admin: Address,
        pool_id: u64,
        token: Address,
        amount: i128,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if amount <= 0 {
            panic!("Reward amount must be positive");
        }

        let pool = get_pool(&env, pool_id);
        if token != pool.token_a && token != pool.token_b {
            panic!("Token not in pool");
        }

        let contract_addr = env.current_contract_address();
        TokenClient::new(&env, &token).transfer(&admin, &contract_addr, &amount);

        let current = get_lp_reward_balance(&env, pool_id, &token);
        set_lp_reward_balance(&env, pool_id, &token, current + amount);

        env.events().publish(
            (Symbol::new(&env, "LpRewardsDeposited"),),
            (pool_id, token, amount),
        );
    }

    /// Claim proportional LP rewards for a provider.
    pub fn claim_lp_rewards(env: Env, provider: Address, pool_id: u64) -> (i128, i128) {
        provider.require_auth();
        Self::with_reentrancy_guard(&env, || {
            let pool = get_pool(&env, pool_id);
            let lp_balance = get_lp_balance(&env, pool_id, &provider);

            if lp_balance <= 0 || pool.lp_total_supply <= 0 {
                panic!("No LP tokens to claim rewards");
            }

            let reward_a = get_lp_reward_balance(&env, pool_id, &pool.token_a);
            let reward_b = get_lp_reward_balance(&env, pool_id, &pool.token_b);

            let share_a = floor_div(reward_a * lp_balance, pool.lp_total_supply);
            let share_b = floor_div(reward_b * lp_balance, pool.lp_total_supply);

            if share_a <= 0 && share_b <= 0 {
                panic!("No rewards to claim");
            }

            let contract_addr = env.current_contract_address();
            if share_a > 0 {
                TokenClient::new(&env, &pool.token_a).transfer(&contract_addr, &provider, &share_a);
                set_lp_reward_balance(&env, pool_id, &pool.token_a, reward_a - share_a);
            }
            if share_b > 0 {
                TokenClient::new(&env, &pool.token_b).transfer(&contract_addr, &provider, &share_b);
                set_lp_reward_balance(&env, pool_id, &pool.token_b, reward_b - share_b);
            }

            env.events().publish(
                (Symbol::new(&env, "LpRewardsClaimed"),),
                (pool_id, provider, share_a, share_b),
            );

            (share_a, share_b)
        })
    }

    /// Get pool information.
    pub fn get_pool(env: Env, pool_id: u64) -> Pool {
        get_pool(&env, pool_id)
    }

    /// Get the current price of a token in the pool (scaled by 1_000_000).
    pub fn get_price(env: Env, pool_id: u64, token: Address) -> i128 {
        let pool = get_pool(&env, pool_id);
        if pool.reserve_a <= 0 || pool.reserve_b <= 0 {
            panic!("Pool has no liquidity");
        }

        if token == pool.token_a {
            (pool.reserve_b * PRICE_SCALE) / pool.reserve_a
        } else if token == pool.token_b {
            (pool.reserve_a * PRICE_SCALE) / pool.reserve_b
        } else {
            panic!("Token not in pool");
        }
    }

    /// Get the LP token balance for a provider in a pool.
    pub fn get_lp_balance(env: Env, pool_id: u64, provider: Address) -> i128 {
        get_lp_balance(&env, pool_id, &provider)
    }

    /// Get pending LP reward amounts for a provider.
    pub fn get_pending_lp_rewards(env: Env, pool_id: u64, provider: Address) -> (i128, i128) {
        let pool = get_pool(&env, pool_id);
        let lp_balance = get_lp_balance(&env, pool_id, &provider);

        if lp_balance <= 0 || pool.lp_total_supply <= 0 {
            return (0, 0);
        }

        let reward_a = get_lp_reward_balance(&env, pool_id, &pool.token_a);
        let reward_b = get_lp_reward_balance(&env, pool_id, &pool.token_b);

        (
            floor_div(reward_a * lp_balance, pool.lp_total_supply),
            floor_div(reward_b * lp_balance, pool.lp_total_supply),
        )
    }

    // ---------------- MULTI-HOP SWAP FUNCTIONALITY ----------------

    /// Find the best route for a multi-hop swap.
    pub fn find_best_route(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        max_hops: u32,
    ) -> Route {
        if amount_in <= 0 {
            panic!("Input amount must be positive");
        }
        if token_in == token_out {
            panic!("Tokens must be different");
        }
        if max_hops == 0 || max_hops > 5 {
            panic!("Invalid max hops (1-5)");
        }

        Self::check_trading_allowed(&env);

        let mut best_route: Option<Route> = None;
        let mut best_output = 0i128;
        let pool_counter = get_pool_counter(&env);

        for pool_id in 0..pool_counter {
            if let Some(route) =
                Self::try_direct_route(&env, pool_id, &token_in, &token_out, amount_in)
            {
                if route.amount_out > best_output {
                    best_output = route.amount_out;
                    best_route = Some(route);
                }
            }
        }

        if max_hops >= 2 {
            for pool_id_1 in 0..pool_counter {
                for pool_id_2 in 0..pool_counter {
                    if pool_id_1 != pool_id_2 {
                        if let Some(route) = Self::try_two_hop_route(
                            &env, pool_id_1, pool_id_2, &token_in, &token_out, amount_in,
                        ) {
                            if route.amount_out > best_output {
                                best_output = route.amount_out;
                                best_route = Some(route);
                            }
                        }
                    }
                }
            }
        }

        best_route.unwrap_or_else(|| panic!("No valid route found"))
    }

    /// Execute a multi-hop swap atomically.
    pub fn execute_multi_hop_swap(
        env: Env,
        user: Address,
        route: Route,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();

        if route.amount_in <= 0 {
            panic!("Invalid route amount");
        }
        if route.amount_out < min_amount_out {
            panic!("Route output below slippage tolerance");
        }

        Self::check_trading_allowed(&env);
        Self::check_position_limits(&env, &user, &route.token_in, route.amount_in);

        let mut current_amount = route.amount_in;
        let mut current_token = route.token_in.clone();
        let mut total_fees = 0u32;

        for (i, hop) in route.hops.iter().enumerate() {
            let hop_index = i as u32;
            if hop.amount_in != current_amount {
                panic!("Hop amount mismatch at hop {}", hop_index);
            }
            if hop.token_in != current_token {
                panic!("Hop token mismatch at hop {}", hop_index);
            }

            let pool = get_pool(&env, hop.pool_id);
            let actual_output = Self::swap_internal(
                &env,
                &user,
                hop.pool_id,
                &hop.token_in,
                hop.amount_in,
                hop.min_amount_out,
                false,
            );

            current_amount = actual_output;
            current_token = hop.token_out.clone();
            total_fees += pool.fee_bps;

            env.events().publish(
                (Symbol::new(&env, "HopCompleted"),),
                (
                    hop_index,
                    hop.pool_id,
                    hop.token_in.clone(),
                    hop.token_out.clone(),
                    actual_output,
                ),
            );
        }

        if current_amount < min_amount_out {
            panic!("Final output below slippage tolerance");
        }
        if current_token != route.token_out {
            panic!("Final token mismatch");
        }

        Self::update_user_position(&env, &user, &route.token_in, -route.amount_in);
        Self::update_user_position(&env, &user, &route.token_out, current_amount);

        env.events().publish(
            (Symbol::new(&env, "MultiHopSwapCompleted"),),
            (
                user,
                route.token_in,
                route.token_out,
                route.amount_in,
                current_amount,
                route.hops.len(),
                total_fees,
            ),
        );

        current_amount
    }

    // ---------------- ADMIN & RISK MANAGEMENT ----------------

    /// Pause all trading operations (admin only).
    pub fn pause_trading(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        set_trading_paused(&env, true);
        env.events().publish(
            (Symbol::new(&env, "TradingPaused"),),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Resume all trading operations (admin only).
    pub fn resume_trading(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        set_trading_paused(&env, false);
        env.events().publish(
            (Symbol::new(&env, "TradingResumed"),),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Set a new admin (current admin only).
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::assert_admin(&env, &current_admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &new_admin);
        env.events().publish(
            (Symbol::new(&env, "AdminUpdated"),),
            (current_admin, new_admin),
        );
    }

    /// Configure governance fee collector and protocol fee share.
    pub fn set_fee_config(
        env: Env,
        admin: Address,
        governance_collector: Address,
        protocol_fee_share_bps: u32,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if protocol_fee_share_bps > MAX_PROTOCOL_FEE_SHARE_BPS {
            panic!("Protocol fee share cannot exceed 50%");
        }

        set_governance_collector(&env, &governance_collector);
        set_protocol_fee_share_bps(&env, protocol_fee_share_bps);

        env.events().publish(
            (Symbol::new(&env, "FeeConfigUpdated"),),
            (governance_collector, protocol_fee_share_bps),
        );
    }

    /// Set risk management parameters (admin only).
    pub fn set_risk_parameters(env: Env, admin: Address, params: RiskParams) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if params.max_position_per_user <= 0 {
            panic!("Invalid max position per user");
        }
        if params.max_position_per_asset <= 0 {
            panic!("Invalid max position per asset");
        }
        if params.concentration_threshold_bps > 10_000 {
            panic!("Invalid concentration threshold");
        }
        if params.circuit_breaker_threshold_bps > 10_000 {
            panic!("Invalid circuit breaker threshold");
        }
        if params.circuit_breaker_cooldown == 0 {
            panic!("Invalid cooldown period");
        }
        if params.min_lp_token_threshold <= 0 {
            panic!("Invalid LP token threshold");
        }

        set_risk_params(&env, &params);

        env.events().publish(
            (Symbol::new(&env, "RiskParamsUpdated"),),
            (
                params.max_position_per_user,
                params.max_position_per_asset,
                params.concentration_threshold_bps,
                params.circuit_breaker_threshold_bps,
            ),
        );
    }

    /// Get current risk metrics for a user.
    pub fn get_risk_metrics(env: Env, user: Address) -> (i128, i128, u32) {
        let params = get_risk_params(&env);
        let user_total_position = Self::get_user_total_position(&env, &user);
        let concentration_score = Self::calculate_concentration_score(&env, &user);

        (
            user_total_position,
            concentration_score.into(),
            params.concentration_threshold_bps,
        )
    }

    /// Trigger circuit breaker manually (admin only).
    pub fn trigger_circuit_breaker(env: Env, admin: Address, reason: String) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let params = get_risk_params(&env);
        let now = env.ledger().timestamp();

        let state = CircuitBreakerState {
            is_active: true,
            triggered_at: now,
            reason: reason.clone(),
            cooldown_until: now + params.circuit_breaker_cooldown,
        };

        set_circuit_breaker_state(&env, &state);

        env.events().publish(
            (Symbol::new(&env, "CircuitBreakerTriggered"),),
            (reason, now),
        );
    }

    // ---------------- INTERNAL HELPERS ----------------

    fn assert_admin(env: &Env, caller: &Address) {
        if let Err(err) = rbac::require_admin(env, caller) {
            match err {
                ContractError::RoleEscalationAttempt | ContractError::Unauthorized => {
                    panic!("Unauthorized");
                }
                _ => panic!("Unauthorized"),
            }
        }
    }

    fn with_reentrancy_guard<F, R>(env: &Env, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if is_reentrancy_locked(env) {
            panic!("Reentrancy detected");
        }
        set_reentrancy_lock(env, true);
        let result = f();
        set_reentrancy_lock(env, false);
        result
    }

    fn add_liquidity_internal(
        env: &Env,
        provider: &Address,
        pool_id: u64,
        amount_a: i128,
        amount_b: i128,
    ) -> i128 {
        if amount_a <= 0 || amount_b <= 0 {
            panic!("Amounts must be positive");
        }

        let mut pool = get_pool(env, pool_id);
        let min_threshold = get_min_lp_token_threshold(env);

        let lp_minted = if pool.lp_total_supply == 0 {
            let lp_tokens = isqrt(amount_a * amount_b);
            if lp_tokens < min_threshold {
                panic!("Liquidity too small - minimum LP tokens required");
            }
            lp_tokens
        } else {
            let lp_a = ceil_div(amount_a * pool.lp_total_supply, pool.reserve_a);
            let lp_b = ceil_div(amount_b * pool.lp_total_supply, pool.reserve_b);
            let lp_tokens = if lp_a < lp_b { lp_a } else { lp_b };

            if lp_tokens < min_threshold {
                panic!("Liquidity too small - minimum LP tokens required");
            }
            lp_tokens
        };

        if lp_minted <= 0 {
            panic!("Insufficient liquidity minted");
        }

        if pool.reserve_a > 0 && pool.reserve_b > 0 {
            let pool_ratio = (pool.reserve_a * 10_000) / pool.reserve_b;
            let deposit_ratio = (amount_a * 10_000) / amount_b;
            let ratio_diff = if pool_ratio > deposit_ratio {
                pool_ratio - deposit_ratio
            } else {
                deposit_ratio - pool_ratio
            };
            if ratio_diff > DEPOSIT_RATIO_TOLERANCE_BPS {
                panic!("Deposit ratio deviates too much from pool ratio");
            }
        }

        let contract_addr = env.current_contract_address();
        TokenClient::new(env, &pool.token_a).transfer(provider, &contract_addr, &amount_a);
        TokenClient::new(env, &pool.token_b).transfer(provider, &contract_addr, &amount_b);

        pool.reserve_a += amount_a;
        pool.reserve_b += amount_b;
        pool.lp_total_supply += lp_minted;
        set_pool(env, &pool);

        let current_lp = get_lp_balance(env, pool_id, provider);
        set_lp_balance(env, pool_id, provider, current_lp + lp_minted);
        invalidate_query_cache(env, pool_id);

        env.events().publish(
            (Symbol::new(env, "LiquidityAdded"),),
            (pool_id, provider.clone(), amount_a, amount_b, lp_minted),
        );

        lp_minted
    }

    fn remove_liquidity_internal(
        env: &Env,
        provider: &Address,
        pool_id: u64,
        lp_amount: i128,
    ) -> (i128, i128) {
        if lp_amount <= 0 {
            panic!("LP amount must be positive");
        }

        let current_lp = get_lp_balance(env, pool_id, provider);
        if current_lp < lp_amount {
            panic!("Insufficient LP balance");
        }

        let mut pool = get_pool(env, pool_id);
        if pool.lp_total_supply <= 0 {
            panic!("Pool has no liquidity");
        }

        let remaining_lp = current_lp - lp_amount;
        let min_threshold = get_min_lp_token_threshold(env);
        if remaining_lp > 0 && remaining_lp < min_threshold {
            panic!("Remaining LP tokens below minimum threshold");
        }

        let amount_a = floor_div(lp_amount * pool.reserve_a, pool.lp_total_supply);
        let amount_b = floor_div(lp_amount * pool.reserve_b, pool.lp_total_supply);

        if amount_a <= 0 || amount_b <= 0 {
            panic!("Withdrawal amounts too small");
        }

        pool.reserve_a -= amount_a;
        pool.reserve_b -= amount_b;
        pool.lp_total_supply -= lp_amount;
        set_pool(env, &pool);

        set_lp_balance(env, pool_id, provider, remaining_lp);

        let contract_addr = env.current_contract_address();
        TokenClient::new(env, &pool.token_a).transfer(&contract_addr, provider, &amount_a);
        TokenClient::new(env, &pool.token_b).transfer(&contract_addr, provider, &amount_b);

        invalidate_query_cache(env, pool_id);

        env.events().publish(
            (Symbol::new(env, "LiquidityRemoved"),),
            (pool_id, provider.clone(), amount_a, amount_b, lp_amount),
        );

        (amount_a, amount_b)
    }

    fn swap_internal(
        env: &Env,
        user: &Address,
        pool_id: u64,
        token_in: &Address,
        amount_in: i128,
        min_amount_out: i128,
        collect_protocol_fee: bool,
    ) -> i128 {
        if amount_in <= 0 {
            panic!("Input amount must be positive");
        }

        let mut pool = get_pool(env, pool_id);
        if pool.reserve_a <= 0 || pool.reserve_b <= 0 {
            panic!("Pool has no liquidity");
        }

        let (reserve_in, reserve_out, is_a_to_b) = if token_in == &pool.token_a {
            (pool.reserve_a, pool.reserve_b, true)
        } else if token_in == &pool.token_b {
            (pool.reserve_b, pool.reserve_a, false)
        } else {
            panic!("Token not in pool");
        };

        let amount_out = get_amount_out(amount_in, reserve_in, reserve_out, pool.fee_bps);

        if amount_out < min_amount_out {
            panic!("Slippage tolerance exceeded");
        }
        if amount_out <= 0 {
            panic!("Output amount is zero");
        }

        let contract_addr = env.current_contract_address();
        let token_in_client = TokenClient::new(env, token_in);
        token_in_client.transfer(user, &contract_addr, &amount_in);

        let token_out = if is_a_to_b {
            &pool.token_b
        } else {
            &pool.token_a
        };

        let protocol_share_bps = get_protocol_fee_share_bps(env);
        let fee_amount = Self::calculate_swap_fee(amount_in, pool.fee_bps);
        let protocol_fee = if collect_protocol_fee {
            (fee_amount * protocol_share_bps as i128) / BPS_DENOMINATOR
        } else {
            0
        };

        if protocol_fee > 0 {
            if let Some(collector) = get_governance_collector(env) {
                token_in_client.transfer(&contract_addr, &collector, &protocol_fee);
            }
        }

        let net_amount_in = amount_in - protocol_fee;
        TokenClient::new(env, token_out).transfer(&contract_addr, user, &amount_out);

        if is_a_to_b {
            pool.reserve_a += net_amount_in;
            pool.reserve_b -= amount_out;
        } else {
            pool.reserve_b += net_amount_in;
            pool.reserve_a -= amount_out;
        }
        set_pool(env, &pool);
        invalidate_query_cache(env, pool_id);

        env.events().publish(
            (Symbol::new(env, "Swapped"),),
            (
                pool_id,
                user.clone(),
                token_in.clone(),
                amount_in,
                amount_out,
            ),
        );

        amount_out
    }

    fn calculate_swap_fee(amount_in: i128, fee_bps: u32) -> i128 {
        (amount_in * fee_bps as i128) / BPS_DENOMINATOR
    }

    fn reserves_for_token(pool: &Pool, token_in: &Address) -> (i128, i128) {
        if token_in == &pool.token_a {
            (pool.reserve_a, pool.reserve_b)
        } else if token_in == &pool.token_b {
            (pool.reserve_b, pool.reserve_a)
        } else {
            panic!("Token not in pool");
        }
    }

    fn reserves_for_pair(
        pool: &Pool,
        token_in: &Address,
        token_out: &Address,
    ) -> (i128, i128, bool) {
        Self::validate_pool_tokens(pool, token_in, token_out);
        if token_in == &pool.token_a && token_out == &pool.token_b {
            (pool.reserve_a, pool.reserve_b, false)
        } else {
            (pool.reserve_b, pool.reserve_a, true)
        }
    }

    fn validate_pool_tokens(pool: &Pool, token_a: &Address, token_b: &Address) {
        let valid = (token_a == &pool.token_a && token_b == &pool.token_b)
            || (token_a == &pool.token_b && token_b == &pool.token_a);
        if !valid {
            panic!("Token not in pool");
        }
    }

    fn try_direct_route(
        env: &Env,
        pool_id: u64,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
    ) -> Option<Route> {
        let pool = get_pool(env, pool_id);

        let (reserve_in, reserve_out) = if token_in == &pool.token_a && token_out == &pool.token_b {
            (pool.reserve_a, pool.reserve_b)
        } else if token_in == &pool.token_b && token_out == &pool.token_a {
            (pool.reserve_b, pool.reserve_a)
        } else {
            return None;
        };

        if reserve_in <= 0 || reserve_out <= 0 {
            return None;
        }

        let amount_out = get_amount_out(amount_in, reserve_in, reserve_out, pool.fee_bps);
        if amount_out <= 0 {
            return None;
        }

        let hop = Hop {
            pool_id,
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in,
            min_amount_out: amount_out * 95 / 100,
        };

        let mut hops = Vec::new(env);
        hops.push_back(hop);

        Some(Route {
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in,
            amount_out,
            total_fee_bps: pool.fee_bps,
            hops,
            created_at: env.ledger().timestamp(),
        })
    }

    fn try_two_hop_route(
        env: &Env,
        pool_id_1: u64,
        pool_id_2: u64,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
    ) -> Option<Route> {
        let pool_1 = get_pool(env, pool_id_1);
        let pool_2 = get_pool(env, pool_id_2);

        let intermediate_token =
            Self::find_intermediate_token(&pool_1, &pool_2, token_in, token_out)?;

        let route_1 =
            Self::try_direct_route(env, pool_id_1, token_in, &intermediate_token, amount_in)?;
        let route_2 = Self::try_direct_route(
            env,
            pool_id_2,
            &intermediate_token,
            token_out,
            route_1.amount_out,
        )?;

        let mut hops = Vec::new(env);
        hops.push_back(route_1.hops.get(0).unwrap().clone());
        hops.push_back(route_2.hops.get(0).unwrap().clone());

        Some(Route {
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in,
            amount_out: route_2.amount_out,
            total_fee_bps: pool_1.fee_bps + pool_2.fee_bps,
            hops,
            created_at: env.ledger().timestamp(),
        })
    }

    fn find_intermediate_token(
        pool_1: &Pool,
        pool_2: &Pool,
        token_in: &Address,
        token_out: &Address,
    ) -> Option<Address> {
        let candidates = [pool_1.token_a.clone(), pool_1.token_b.clone()];
        for t1 in candidates.iter() {
            if t1 == token_in || t1 == token_out {
                continue;
            }
            if t1 == &pool_2.token_a || t1 == &pool_2.token_b {
                return Some(t1.clone());
            }
        }
        None
    }

    fn check_trading_allowed(env: &Env) {
        if is_trading_paused(env) {
            panic!("Trading is currently paused");
        }

        if let Some(state) = get_circuit_breaker_state(env) {
            if state.is_active {
                let now = env.ledger().timestamp();
                if now < state.cooldown_until {
                    panic!("Circuit breaker is active");
                } else {
                    let mut expired_state = state;
                    expired_state.is_active = false;
                    set_circuit_breaker_state(env, &expired_state);
                }
            }
        }
    }

    fn check_position_limits(env: &Env, user: &Address, token: &Address, amount: i128) {
        let params = get_risk_params(env);
        let current_position = Self::get_user_position_for_token(env, user, token);
        let new_position = current_position + amount;

        let user_total = Self::get_user_total_position(env, user);
        if user_total + amount > params.max_position_per_user {
            panic!("Exceeds maximum position per user");
        }

        if new_position > params.max_position_per_asset {
            panic!("Exceeds maximum position per asset");
        }

        let concentration = Self::calculate_concentration_score(env, user);
        if concentration > params.concentration_threshold_bps {
            panic!("Portfolio concentration too high");
        }
    }

    fn update_user_position(env: &Env, user: &Address, token: &Address, amount_change: i128) {
        let current = Self::get_user_position_for_token(env, user, token);
        let new_position = current + amount_change;

        if new_position == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::UserPosition(user.clone(), token.clone()));
        } else {
            let position = UserPosition {
                user: user.clone(),
                token: token.clone(),
                position_size: new_position,
                last_updated: env.ledger().timestamp(),
            };
            set_user_position(env, user, token, &position);
        }
    }

    fn get_user_position_for_token(env: &Env, user: &Address, token: &Address) -> i128 {
        get_user_position(env, user, token)
            .map(|p| p.position_size)
            .unwrap_or(0)
    }

    fn get_user_total_position(env: &Env, user: &Address) -> i128 {
        let pool_counter = get_pool_counter(env);
        let mut total = 0i128;

        for pool_id in 0..pool_counter {
            let pool = get_pool(env, pool_id);
            if let Some(pos) = get_user_position(env, user, &pool.token_a) {
                total += pos.position_size.abs();
            }
            if let Some(pos) = get_user_position(env, user, &pool.token_b) {
                total += pos.position_size.abs();
            }
        }

        total
    }

    fn calculate_concentration_score(env: &Env, user: &Address) -> u32 {
        let total = Self::get_user_total_position(env, user);
        if total == 0 {
            return 0;
        }

        let pool_counter = get_pool_counter(env);
        let mut max_concentration = 0u32;

        for pool_id in 0..pool_counter {
            let pool = get_pool(env, pool_id);
            if let Some(pos) = get_user_position(env, user, &pool.token_a) {
                let concentration = (pos.position_size.abs() * 10_000) / total;
                max_concentration = max_concentration.max(concentration as u32);
            }
            if let Some(pos) = get_user_position(env, user, &pool.token_b) {
                let concentration = (pos.position_size.abs() * 10_000) / total;
                max_concentration = max_concentration.max(concentration as u32);
            }
        }

        max_concentration
    }
}
