#![no_std]

mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, token, Address, Env, String, Symbol, Vec};

use storage::*;
use types::*;

#[contract]
pub struct Amm;

#[contractimpl]
impl Amm {
    /// Initialize the AMM contract with an admin.
    pub fn init_contract(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }
        admin.require_auth();
        set_admin(&env, &admin);
        set_pool_counter(&env, 0);
    }

    /// Ceiling division to prevent rounding attacks
    fn ceil_div(a: u128, b: u128) -> u128 {
        (a + b - 1) / b
    }

    /// Floor division for safe calculations
    fn floor_div(a: u128, b: u128) -> u128 {
        a / b
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
        require_admin(&env, &admin);

        assert!(token_a != token_b, "Token addresses must differ");
        assert!(fee_bps <= 1000, "Fee cannot exceed 10%");

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

        assert!(amount_a > 0 && amount_b > 0, "Amounts must be positive");

        let mut pool = get_pool(&env, pool_id);
        let min_threshold = get_min_lp_token_threshold(&env);

        // Calculate LP tokens to mint with rounding protection.
        let lp_minted = if pool.lp_total_supply == 0 {
            // First deposit: LP tokens = sqrt(amount_a * amount_b).
            let lp_tokens = isqrt(amount_a * amount_b);

            // Enforce minimum LP token threshold
            if lp_tokens < min_threshold {
                panic!(
                    "Liquidity too small - minimum LP tokens required: {}",
                    min_threshold
                );
            }

            lp_tokens
        } else {
            // Subsequent deposits: use ceiling division to favor the pool
            // This prevents rounding attacks where users deposit tiny amounts
            let lp_a = Self::ceil_div(amount_a * pool.lp_total_supply, pool.reserve_a);
            let lp_b = Self::ceil_div(amount_b * pool.lp_total_supply, pool.reserve_b);

            // Take the minimum to avoid diluting existing LPs
            let lp_tokens = if lp_a < lp_b { lp_a } else { lp_b };

            // Enforce minimum LP token threshold
            if lp_tokens < min_threshold {
                panic!(
                    "Liquidity too small - minimum LP tokens required: {}",
                    min_threshold
                );
            }

            lp_tokens
        };

        assert!(lp_minted > 0, "Insufficient liquidity minted");

        // Slippage protection - check that the ratio of deposits is close to pool ratio
        if pool.reserve_a > 0 && pool.reserve_b > 0 {
            let pool_ratio = (pool.reserve_a * 10000) / pool.reserve_b;
            let deposit_ratio = (amount_a * 10000) / amount_b;

            // Allow 5% deviation from pool ratio
            let ratio_diff = if pool_ratio > deposit_ratio {
                pool_ratio - deposit_ratio
            } else {
                deposit_ratio - pool_ratio
            };

            if ratio_diff > 500 {
                // 5% in basis points
                panic!("Deposit ratio deviates too much from pool ratio");
            }
        }

        // Transfer tokens from provider to the contract.
        let contract_addr = env.current_contract_address();
        let token_a_client = token::Client::new(&env, &pool.token_a);
        let token_b_client = token::Client::new(&env, &pool.token_b);

        token_a_client.transfer(&provider, &contract_addr, &amount_a);
        token_b_client.transfer(&provider, &contract_addr, &amount_b);

        // Update pool state.
        pool.reserve_a += amount_a;
        pool.reserve_b += amount_b;
        pool.lp_total_supply += lp_minted;
        set_pool(&env, &pool);

        // Credit LP tokens to provider.
        let current_lp = get_lp_balance(&env, pool_id, &provider);
        set_lp_balance(&env, pool_id, &provider, current_lp + lp_minted);

        // CRITICAL: Invalidate query cache after state change (Issue #215)
        storage::invalidate_query_cache(&env, pool_id);

        env.events().publish(
            (Symbol::new(&env, "LiquidityAdded"),),
            (pool_id, &provider, amount_a, amount_b, lp_minted),
        );

        lp_minted
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

        assert!(lp_amount > 0, "LP amount must be positive");

        let current_lp = get_lp_balance(&env, pool_id, &provider);
        assert!(current_lp >= lp_amount, "Insufficient LP balance");

        let mut pool = get_pool(&env, pool_id);
        assert!(pool.lp_total_supply > 0, "Pool has no liquidity");

        // Check minimum LP token threshold to prevent dust attacks
        let remaining_lp = current_lp - lp_amount;
        let min_threshold = get_min_lp_token_threshold(&env);
        if remaining_lp > 0 && remaining_lp < min_threshold {
            panic!(
                "Remaining LP tokens below minimum threshold: {}",
                min_threshold
            );
        }

        // Calculate proportional share using ceiling division to favor the pool
        // This prevents rounding attacks where users withdraw tiny amounts
        let amount_a = Self::floor_div(lp_amount * pool.reserve_a, pool.lp_total_supply);
        let amount_b = Self::floor_div(lp_amount * pool.reserve_b, pool.lp_total_supply);

        assert!(amount_a > 0 && amount_b > 0, "Withdrawal amounts too small");

        // Update pool state.
        pool.reserve_a -= amount_a;
        pool.reserve_b -= amount_b;
        pool.lp_total_supply -= lp_amount;
        set_pool(&env, &pool);

        // Burn LP tokens from provider.
        set_lp_balance(&env, pool_id, &provider, remaining_lp);

        // Transfer tokens back to provider.
        let contract_addr = env.current_contract_address();
        let token_a_client = token::Client::new(&env, &pool.token_a);
        let token_b_client = token::Client::new(&env, &pool.token_b);

        token_a_client.transfer(&contract_addr, &provider, &amount_a);
        token_b_client.transfer(&contract_addr, &provider, &amount_b);

        // CRITICAL: Invalidate query cache after state change (Issue #215)
        storage::invalidate_query_cache(&env, pool_id);

        env.events().publish(
            (Symbol::new(&env, "LiquidityRemoved"),),
            (pool_id, &provider, amount_a, amount_b, lp_amount),
        );

        (amount_a, amount_b)
    }

    /// Execute a swap on a pool with slippage protection.
    /// `token_in` must match either `token_a` or `token_b` of the pool.
    /// Returns the amount of tokens received.
    pub fn swap(
        env: Env,
        user: Address,
        pool_id: u64,
        token_in: Address,
        amount_in: i128,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();

        assert!(amount_in > 0, "Input amount must be positive");

        let mut pool = get_pool(&env, pool_id);
        assert!(
            pool.reserve_a > 0 && pool.reserve_b > 0,
            "Pool has no liquidity"
        );

        // Determine swap direction.
        let (reserve_in, reserve_out, is_a_to_b) = if token_in == pool.token_a {
            (pool.reserve_a, pool.reserve_b, true)
        } else if token_in == pool.token_b {
            (pool.reserve_b, pool.reserve_a, false)
        } else {
            panic!("Token not in pool");
        };

        // Constant product formula with fee:
        // amount_out = (reserve_out * amount_in_after_fee) / (reserve_in + amount_in_after_fee)
        let fee_factor = 10_000 - pool.fee_bps as i128;
        let amount_in_after_fee = (amount_in * fee_factor) / 10_000;
        let numerator = reserve_out * amount_in_after_fee;
        let denominator = reserve_in + amount_in_after_fee;
        let amount_out = numerator / denominator;

        assert!(amount_out >= min_amount_out, "Slippage tolerance exceeded");
        assert!(amount_out > 0, "Output amount is zero");

        // Transfer input tokens from user to contract.
        let contract_addr = env.current_contract_address();
        let token_in_client = token::Client::new(&env, &token_in);
        token_in_client.transfer(&user, &contract_addr, &amount_in);

        // Transfer output tokens from contract to user.
        let token_out = if is_a_to_b {
            &pool.token_b
        } else {
            &pool.token_a
        };
        let token_out_client = token::Client::new(&env, token_out);
        token_out_client.transfer(&contract_addr, &user, &amount_out);

        // Update reserves (full amount_in goes to reserve, including fee).
        if is_a_to_b {
            pool.reserve_a += amount_in;
            pool.reserve_b -= amount_out;
        } else {
            pool.reserve_b += amount_in;
            pool.reserve_a -= amount_out;
        }
        set_pool(&env, &pool);

        // CRITICAL: Invalidate query cache after state change (Issue #215)
        storage::invalidate_query_cache(&env, pool_id);

        env.events().publish(
            (Symbol::new(&env, "Swapped"),),
            (pool_id, &user, &token_in, amount_in, amount_out),
        );

        amount_out
    }

    /// Get pool information.
    pub fn get_pool(env: Env, pool_id: u64) -> Pool {
        get_pool(&env, pool_id)
    }

    /// Get the current price of a token in the pool (as the ratio of the other reserve).
    /// Returns price scaled by 1_000_000 (6 decimal places) for precision.
    pub fn get_price(env: Env, pool_id: u64, token: Address) -> i128 {
        let pool = get_pool(&env, pool_id);
        assert!(
            pool.reserve_a > 0 && pool.reserve_b > 0,
            "Pool has no liquidity"
        );

        let scale: i128 = 1_000_000;
        if token == pool.token_a {
            // Price of token_a in terms of token_b.
            (pool.reserve_b * scale) / pool.reserve_a
        } else if token == pool.token_b {
            // Price of token_b in terms of token_a.
            (pool.reserve_a * scale) / pool.reserve_b
        } else {
            panic!("Token not in pool");
        }
    }

    /// Get the LP token balance for a provider in a pool.
    pub fn get_lp_balance(env: Env, pool_id: u64, provider: Address) -> i128 {
        get_lp_balance(&env, pool_id, &provider)
    }

    // ---------------- MULTI-HOP SWAP FUNCTIONALITY ----------------

    /// Find the best route for a multi-hop swap.
    /// Returns a Route object with optimal path and expected output.
    pub fn find_best_route(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        max_hops: u32,
    ) -> Route {
        assert!(amount_in > 0, "Input amount must be positive");
        assert!(token_in != token_out, "Tokens must be different");
        assert!(max_hops > 0 && max_hops <= 5, "Invalid max hops (1-5)");

        // Check if trading is paused or circuit breaker is active
        Self::check_trading_allowed(&env);

        let mut best_route: Option<Route> = None;
        let mut best_output = 0i128;

        // Find all possible routes up to max_hops
        let pool_counter = get_pool_counter(&env);

        // Try direct swap first (1 hop)
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

        // Try 2-hop routes if direct route not found or better
        if max_hops >= 2 && (best_route.is_none() || best_output < amount_in / 2) {
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

        // Try 3-hop routes if needed
        if max_hops >= 3 && (best_route.is_none() || best_output < amount_in / 3) {
            // For simplicity, implement basic 3-hop search
            // In production, this would use more sophisticated pathfinding
        }

        best_route.unwrap_or_else(|| panic!("No valid route found"))
    }

    /// Execute a multi-hop swap atomically.
    /// If any hop fails, the entire transaction reverts.
    pub fn execute_multi_hop_swap(
        env: Env,
        user: Address,
        route: Route,
        min_amount_out: i128,
    ) -> i128 {
        user.require_auth();

        assert!(route.amount_in > 0, "Invalid route amount");
        assert!(
            route.amount_out >= min_amount_out,
            "Route output below slippage tolerance"
        );

        // Check trading constraints
        Self::check_trading_allowed(&env);
        Self::check_position_limits(&env, &user, &route.token_in, route.amount_in);

        let mut current_amount = route.amount_in;
        let mut current_token = route.token_in.clone();
        let mut total_fees = 0u32;

        // Execute each hop sequentially
        for (i, hop) in route.hops.iter().enumerate() {
            // Validate hop
            assert!(
                hop.amount_in == current_amount,
                "Hop amount mismatch at hop {}",
                i
            );
            assert!(
                hop.token_in == current_token,
                "Hop token mismatch at hop {}",
                i
            );

            // Execute the swap for this hop
            let pool = get_pool(&env, hop.pool_id);
            let actual_output = Self::pool_swap(
                env.clone(),
                user.clone(),
                hop.pool_id,
                hop.token_in.clone(),
                hop.amount_in,
                hop.min_amount_out,
            );

            // Update state for next hop
            current_amount = actual_output;
            current_token = hop.token_out.clone();
            total_fees += pool.fee_bps;

            // Emit hop completion event
            env.events().publish(
                (Symbol::new(&env, "HopCompleted"),),
                (
                    i,
                    hop.pool_id,
                    hop.token_in.clone(),
                    hop.token_out.clone(),
                    actual_output,
                ),
            );
        }

        // Verify final output
        assert!(
            current_amount >= min_amount_out,
            "Final output below slippage tolerance"
        );
        assert!(current_token == route.token_out, "Final token mismatch");

        // Update user position tracking
        Self::update_user_position(&env, &user, &route.token_in, -route.amount_in);
        Self::update_user_position(&env, &user, &route.token_out, current_amount);

        // Emit route completion event
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

    /// Internal: Try to find a direct route (single hop)
    fn try_direct_route(
        env: &Env,
        pool_id: u64,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
    ) -> Option<Route> {
        let pool = get_pool(env, pool_id);

        // Check if pool contains the token pair
        let (is_a_to_b, reserve_in, reserve_out) =
            if token_in == &pool.token_a && token_out == &pool.token_b {
                (true, pool.reserve_a, pool.reserve_b)
            } else if token_in == &pool.token_b && token_out == &pool.token_a {
                (false, pool.reserve_b, pool.reserve_a)
            } else {
                return None; // Pool doesn't contain this pair
            };

        if reserve_in == 0 || reserve_out == 0 {
            return None; // No liquidity
        }

        // Calculate expected output using constant product formula
        let fee_factor = 10_000 - pool.fee_bps as i128;
        let amount_in_after_fee = (amount_in * fee_factor) / 10_000;
        let amount_out = (reserve_out * amount_in_after_fee) / (reserve_in + amount_in_after_fee);

        if amount_out <= 0 {
            return None;
        }

        let hop = Hop {
            pool_id,
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in,
            min_amount_out: amount_out * 95 / 100, // 5% slippage protection
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

    /// Internal: Try to find a 2-hop route
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

        // Find intermediate token
        let intermediate_token =
            Self::find_intermediate_token(&pool_1, &pool_2, token_in, token_out)?;

        // Calculate first hop output
        let route_1 =
            Self::try_direct_route(env, pool_id_1, token_in, &intermediate_token, amount_in)?;
        let intermediate_amount = route_1.amount_out;

        // Calculate second hop output
        let route_2 = Self::try_direct_route(
            env,
            pool_id_2,
            &intermediate_token,
            token_out,
            intermediate_amount,
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

    /// Internal: Find intermediate token for 2-hop route
    fn find_intermediate_token(
        pool_1: &Pool,
        pool_2: &Pool,
        token_in: &Address,
        token_out: &Address,
    ) -> Option<Address> {
        // Check all possible intermediate tokens
        let pool_1_tokens = soroban_sdk::vec![&env, pool_1.token_a.clone(), pool_1.token_b.clone()];
        let pool_2_tokens = soroban_sdk::vec![&env, pool_2.token_a.clone(), pool_2.token_b.clone()];

        for &token1 in &pool_1_tokens {
            if token1 == token_in || token1 == token_out {
                continue;
            }
            for &token2 in &pool_2_tokens {
                if token1 == token2 {
                    return Some(token1.clone());
                }
            }
        }
        None
    }

    /// Internal: Execute swap on a specific pool (extracted from existing swap function)
    fn pool_swap(
        env: Env,
        user: Address,
        pool_id: u64,
        token_in: Address,
        amount_in: i128,
        min_amount_out: i128,
    ) -> i128 {
        assert!(amount_in > 0, "Input amount must be positive");

        let mut pool = get_pool(&env, pool_id);
        assert!(
            pool.reserve_a > 0 && pool.reserve_b > 0,
            "Pool has no liquidity"
        );

        // Determine swap direction.
        let (reserve_in, reserve_out, is_a_to_b) = if token_in == pool.token_a {
            (pool.reserve_a, pool.reserve_b, true)
        } else if token_in == pool.token_b {
            (pool.reserve_b, pool.reserve_a, false)
        } else {
            panic!("Token not in pool");
        };

        // Constant product formula with fee:
        let fee_factor = 10_000 - pool.fee_bps as i128;
        let amount_in_after_fee = (amount_in * fee_factor) / 10_000;
        let numerator = reserve_out * amount_in_after_fee;
        let denominator = reserve_in + amount_in_after_fee;
        let amount_out = numerator / denominator;

        assert!(amount_out >= min_amount_out, "Slippage tolerance exceeded");
        assert!(amount_out > 0, "Output amount is zero");

        // Transfer input tokens from user to contract.
        let contract_addr = env.current_contract_address();
        let token_in_client = token::Client::new(&env, &token_in);
        token_in_client.transfer(&user, &contract_addr, &amount_in);

        // Transfer output tokens from contract to user.
        let token_out = if is_a_to_b {
            &pool.token_b
        } else {
            &pool.token_a
        };
        let token_out_client = token::Client::new(&env, token_out);
        token_out_client.transfer(&contract_addr, &user, &amount_out);

        // Update reserves
        if is_a_to_b {
            pool.reserve_a += amount_in;
            pool.reserve_b -= amount_out;
        } else {
            pool.reserve_b += amount_in;
            pool.reserve_a -= amount_out;
        }
        set_pool(&env, &pool);

        // Invalidate query cache
        storage::invalidate_query_cache(&env, pool_id);

        amount_out
    }

    // ---------------- ADMIN & RISK MANAGEMENT FUNCTIONS ----------------

    /// Pause all trading operations (admin only)
    pub fn pause_trading(env: Env, admin: Address) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        set_trading_paused(&env, true);

        env.events().publish(
            (Symbol::new(&env, "TradingPaused"),),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Resume all trading operations (admin only)
    pub fn resume_trading(env: Env, admin: Address) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        set_trading_paused(&env, false);

        env.events().publish(
            (Symbol::new(&env, "TradingResumed"),),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Set a new admin (current admin only)
    pub fn set_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::verify_admin(&env, &current_admin);

        set_admin(&env, &new_admin);

        env.events().publish(
            (Symbol::new(&env, "AdminUpdated"),),
            (current_admin, new_admin),
        );
    }

    /// Set risk management parameters (admin only)
    pub fn set_risk_parameters(env: Env, admin: Address, params: RiskParams) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        assert!(
            params.max_position_per_user > 0,
            "Invalid max position per user"
        );
        assert!(
            params.max_position_per_asset > 0,
            "Invalid max position per asset"
        );
        assert!(
            params.concentration_threshold_bps <= 10000,
            "Invalid concentration threshold"
        );
        assert!(
            params.circuit_breaker_threshold_bps <= 10000,
            "Invalid circuit breaker threshold"
        );
        assert!(
            params.circuit_breaker_cooldown > 0,
            "Invalid cooldown period"
        );
        assert!(
            params.min_lp_token_threshold > 0,
            "Invalid LP token threshold"
        );

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

    /// Get current risk metrics
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

    /// Trigger circuit breaker manually (admin only)
    pub fn trigger_circuit_breaker(env: Env, admin: Address, reason: String) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

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

    // ---------------- INTERNAL HELPER FUNCTIONS ----------------

    /// Internal: verify admin authentication
    fn verify_admin(env: &Env, caller: &Address) {
        require_admin(env, caller);
    }

    /// Internal: check if trading is allowed
    fn check_trading_allowed(env: &Env) {
        // Check if trading is paused
        if is_trading_paused(env) {
            panic!("Trading is currently paused");
        }

        // Check circuit breaker
        if let Some(state) = get_circuit_breaker_state(env) {
            if state.is_active {
                let now = env.ledger().timestamp();
                if now < state.cooldown_until {
                    panic!("Circuit breaker is active: {}", state.reason);
                } else {
                    // Auto-expire circuit breaker
                    let mut expired_state = state;
                    expired_state.is_active = false;
                    set_circuit_breaker_state(env, &expired_state);
                }
            }
        }
    }

    /// Internal: check position limits
    fn check_position_limits(env: &Env, user: &Address, token: &Address, amount: i128) {
        let params = get_risk_params(env);
        let current_position = Self::get_user_position_for_token(env, user, token);
        let new_position = current_position + amount;

        // Check per-user limit
        let user_total = Self::get_user_total_position(env, user);
        if user_total + amount > params.max_position_per_user {
            panic!("Exceeds maximum position per user");
        }

        // Check per-asset limit
        if new_position > params.max_position_per_asset {
            panic!("Exceeds maximum position per asset");
        }

        // Check concentration risk
        let concentration = Self::calculate_concentration_score(env, user);
        if concentration > params.concentration_threshold_bps {
            panic!("Portfolio concentration too high");
        }
    }

    /// Internal: update user position tracking
    fn update_user_position(env: &Env, user: &Address, token: &Address, amount_change: i128) {
        let current = Self::get_user_position_for_token(env, user, token);
        let new_position = current + amount_change;

        if new_position == 0 {
            // Remove position if zero
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

    /// Internal: get user position for specific token
    fn get_user_position_for_token(env: &Env, user: &Address, token: &Address) -> i128 {
        get_user_position(env, user, token)
            .map(|p| p.position_size)
            .unwrap_or(0)
    }

    /// Internal: get user total position across all tokens
    fn get_user_total_position(env: &Env, user: &Address) -> i128 {
        let pool_counter = get_pool_counter(env);
        let mut total = 0i128;

        // This is a simplified implementation
        // In production, you'd maintain a separate total tracking
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

    /// Internal: calculate portfolio concentration score
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
                let concentration = (pos.position_size.abs() * 10000) / total;
                max_concentration = max_concentration.max(concentration as u32);
            }
            if let Some(pos) = get_user_position(env, user, &pool.token_b) {
                let concentration = (pos.position_size.abs() * 10000) / total;
                max_concentration = max_concentration.max(concentration as u32);
            }
        }

        max_concentration
    }
}

/// Integer square root using Newton's method (for no_std environments).
fn isqrt(n: i128) -> i128 {
    if n < 0 {
        panic!("Cannot take square root of negative number");
    }
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Ceiling division: (a + b - 1) / b for positive numbers
/// Returns the smallest integer >= a / b
pub fn ceil_div(a: i128, b: i128) -> i128 {
    if b <= 0 {
        panic!("Divisor must be positive");
    }
    if a < 0 {
        panic!("Dividend must be non-negative for ceiling division");
    }
    (a + b - 1) / b
}

/// Floor division: a / b for positive numbers
/// Returns the largest integer <= a / b
pub fn floor_div(a: i128, b: i128) -> i128 {
    if b <= 0 {
        panic!("Divisor must be positive");
    }
    a / b
}
