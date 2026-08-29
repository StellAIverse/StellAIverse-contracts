use super::*;
use soroban_sdk::{contract, contractimpl, contracttype, testutils::Address as _, Address, Env};

// ─── Mock Token ────────────────────────────────────────────────────

#[contract]
pub struct MockToken;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MockDataKey {
    Balances(Address),
}

#[contractimpl]
impl MockToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = MockDataKey::Balances(to.clone());
        let current: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(current + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        let key = MockDataKey::Balances(id);
        env.storage().instance().get(&key).unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_key = MockDataKey::Balances(from.clone());
        let from_bal: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        assert!(from_bal >= amount, "Insufficient balance");
        env.storage()
            .instance()
            .set(&from_key, &(from_bal - amount));

        let to_key = MockDataKey::Balances(to.clone());
        let to_bal: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        env.storage().instance().set(&to_key, &(to_bal + amount));
    }
}

// ─── Helpers ───────────────────────────────────────────────────────

fn setup_env() -> (Env, Address, AmmClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let amm_id = env.register(Amm, ());
    let amm_client = AmmClient::new(&env, &amm_id);

    let token_a_id = env.register(MockToken, ());
    let token_b_id = env.register(MockToken, ());

    amm_client.init_contract(&admin);

    (env, admin, amm_client, token_a_id, token_b_id)
}

fn mint_tokens(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    let client = MockTokenClient::new(env, token_id);
    client.mint(to, &amount);
}

// ─── Tests ─────────────────────────────────────────────────────────

#[test]
fn test_create_pool() {
    let (_env, admin, amm, token_a, token_b) = setup_env();

    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);
    assert_eq!(pool_id, 0);

    let pool = amm.get_pool(&pool_id);
    assert_eq!(pool.token_a, token_a);
    assert_eq!(pool.token_b, token_b);
    assert_eq!(pool.fee_bps, 30);
    assert_eq!(pool.reserve_a, 0);
    assert_eq!(pool.reserve_b, 0);
}

#[test]
#[should_panic(expected = "Token addresses must differ")]
fn test_create_pool_same_tokens() {
    let (_env, admin, amm, token_a, _) = setup_env();
    amm.create_pool(&admin, &token_a, &token_a, &30);
}

#[test]
#[should_panic(expected = "Fee cannot exceed 10%")]
fn test_create_pool_excessive_fee() {
    let (_env, admin, amm, token_a, token_b) = setup_env();
    amm.create_pool(&admin, &token_a, &token_b, &1001);
}

#[test]
fn test_add_liquidity_initial() {
    let (env, _admin, amm, _token_a, _token_b) = setup_env();

    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Mint tokens to provider.
    mint_tokens(&env, &token_a, &provider, 10_000);
    mint_tokens(&env, &token_b, &provider, 20_000);

    let lp = amm.add_liquidity(&provider, &pool_id, &10_000, &20_000);
    assert!(lp > 0);

    let pool = amm.get_pool(&pool_id);
    assert_eq!(pool.reserve_a, 10_000);
    assert_eq!(pool.reserve_b, 20_000);
    assert_eq!(pool.lp_total_supply, lp);

    let balance = amm.get_lp_balance(&pool_id, &provider);
    assert_eq!(balance, lp);
}

#[test]
fn test_add_liquidity_subsequent() {
    let (env, _admin, amm, _token_a, _token_b) = setup_env();

    let provider1 = Address::generate(&env);
    let provider2 = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // First provider.
    mint_tokens(&env, &token_a, &provider1, 10_000);
    mint_tokens(&env, &token_b, &provider1, 10_000);
    let lp1 = amm.add_liquidity(&provider1, &pool_id, &10_000, &10_000);

    // Second provider.
    mint_tokens(&env, &token_a, &provider2, 5_000);
    mint_tokens(&env, &token_b, &provider2, 5_000);
    let lp2 = amm.add_liquidity(&provider2, &pool_id, &5_000, &5_000);

    // Second provider should get half the LP tokens of first.
    assert_eq!(lp2, lp1 / 2);
}

#[test]
fn test_remove_liquidity() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    mint_tokens(&env, &token_a, &provider, 10_000);
    mint_tokens(&env, &token_b, &provider, 10_000);

    let lp = amm.add_liquidity(&provider, &pool_id, &10_000, &10_000);

    // Withdraw half.
    let (out_a, out_b) = amm.remove_liquidity(&provider, &pool_id, &(lp / 2));
    assert_eq!(out_a, 5_000);
    assert_eq!(out_b, 5_000);

    let pool = amm.get_pool(&pool_id);
    assert_eq!(pool.reserve_a, 5_000);
    assert_eq!(pool.reserve_b, 5_000);
}

#[test]
fn test_swap() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let user = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Add liquidity.
    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_id, &100_000, &100_000);

    // User swaps token_a for token_b.
    mint_tokens(&env, &token_a, &user, 1_000);
    let out = amm.swap(&user, &pool_id, &token_a, &1_000, &0);

    // With 0.3% fee, output should be slightly less than ~990 (constant product).
    assert!(out > 0);
    assert!(out < 1_000); // Must be less due to price impact + fee.

    // Check user received tokens.
    let user_b = MockTokenClient::new(&env, &token_b).balance(&user);
    assert_eq!(user_b, out);
}

#[test]
#[should_panic(expected = "Slippage tolerance exceeded")]
fn test_swap_slippage_protection() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let user = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_id, &100_000, &100_000);

    mint_tokens(&env, &token_a, &user, 1_000);
    // Set min_amount_out unrealistically high.
    amm.swap(&user, &pool_id, &token_a, &1_000, &999);
}

#[test]
fn test_get_price() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    mint_tokens(&env, &token_a, &provider, 10_000);
    mint_tokens(&env, &token_b, &provider, 20_000);
    amm.add_liquidity(&provider, &pool_id, &10_000, &20_000);

    // Price of token_a in terms of token_b: 20000/10000 = 2.0 scaled by 1e6 = 2_000_000.
    let price_a = amm.get_price(&pool_id, &token_a);
    assert_eq!(price_a, 2_000_000);

    // Price of token_b in terms of token_a: 10000/20000 = 0.5 scaled by 1e6 = 500_000.
    let price_b = amm.get_price(&pool_id, &token_b);
    assert_eq!(price_b, 500_000);
}

#[test]
fn test_fee_distribution_via_reserves() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let user = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Add liquidity.
    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 100_000);
    let lp = amm.add_liquidity(&provider, &pool_id, &100_000, &100_000);

    // Execute a swap (fees stay in pool reserves).
    mint_tokens(&env, &token_a, &user, 10_000);
    amm.swap(&user, &pool_id, &token_a, &10_000, &0);

    // Withdraw all liquidity: provider should get back more value than deposited
    // because swap fees accumulated in the reserves.
    let (out_a, out_b) = amm.remove_liquidity(&provider, &pool_id, &lp);

    // Provider deposited 100k of each. After swaps, total value of reserves increased.
    // out_a should be > 100_000 (received swap input) and out_b < 100_000 (paid swap output).
    // But total value = out_a + out_b should be > 200_000 due to fees.
    assert!(out_a > 100_000);
    assert!(out_b < 100_000);
    assert!(out_a + out_b > 200_000); // Fee profit.
}

#[test]
fn test_multiple_pools() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let token_c_id = env.register(MockToken, ());

    let pool_0 = amm.create_pool(&admin, &token_a, &token_b, &30);
    let pool_1 = amm.create_pool(&admin, &token_a, &token_c_id, &50);

    assert_eq!(pool_0, 0);
    assert_eq!(pool_1, 1);

    let info_0 = amm.get_pool(&pool_0);
    let info_1 = amm.get_pool(&pool_1);

    assert_eq!(info_0.fee_bps, 30);
    assert_eq!(info_1.fee_bps, 50);
}

// ─── MULTI-HOP SWAP TESTS ────────────────────────────────────────

#[test]
fn test_find_best_route_direct() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Add liquidity
    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_id, &100_000, &100_000);

    // Find direct route
    let route = amm.find_best_route(&token_a, &token_b, &1_000, &2);

    assert_eq!(route.token_in, token_a);
    assert_eq!(route.token_out, token_b);
    assert_eq!(route.amount_in, 1_000);
    assert!(route.amount_out > 0);
    assert_eq!(route.hops.len(), 1);
    assert_eq!(route.hops.get(0).unwrap().pool_id, pool_id);
}

#[test]
fn test_execute_multi_hop_swap_single_hop() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let user = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Add liquidity
    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_id, &100_000, &100_000);

    // Find and execute route
    let route = amm.find_best_route(&token_a, &token_b, &1_000, &2);
    mint_tokens(&env, &token_a, &user, 1_000);

    let output = amm.execute_multi_hop_swap(&user, route, &0);
    assert!(output > 0);
    assert!(output < 1_000); // Less due to fees
}

#[test]
fn test_find_best_route_two_hop() {
    let (env, admin, amm, token_a, token_b) = setup_env();
    let token_c_id = env.register(MockToken, ());

    let provider = Address::generate(&env);

    // Create pools: A-B and B-C
    let pool_ab = amm.create_pool(&admin, &token_a, &token_b, &30);
    let pool_bc = amm.create_pool(&admin, &token_b, &token_c_id, &30);

    // Add liquidity to both pools
    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 200_000);
    amm.add_liquidity(&provider, &pool_ab, &100_000, &200_000);

    mint_tokens(&env, &token_b, &provider, 100_000);
    mint_tokens(&env, &token_c_id, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_bc, &100_000, &100_000);

    // Find 2-hop route from A to C
    let route = amm.find_best_route(&token_a, &token_c_id, &1_000, &3);

    assert_eq!(route.token_in, token_a);
    assert_eq!(route.token_out, token_c_id);
    assert_eq!(route.amount_in, 1_000);
    assert!(route.amount_out > 0);
    assert_eq!(route.hops.len(), 2);
}

#[test]
fn test_execute_multi_hop_swap_two_hop() {
    let (env, admin, amm, token_a, token_b) = setup_env();
    let token_c_id = env.register(MockToken, ());

    let provider = Address::generate(&env);
    let user = Address::generate(&env);

    // Create pools: A-B and B-C
    let pool_ab = amm.create_pool(&admin, &token_a, &token_b, &30);
    let pool_bc = amm.create_pool(&admin, &token_b, &token_c_id, &30);

    // Add liquidity to both pools
    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 200_000);
    amm.add_liquidity(&provider, &pool_ab, &100_000, &200_000);

    mint_tokens(&env, &token_b, &provider, 100_000);
    mint_tokens(&env, &token_c_id, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_bc, &100_000, &100_000);

    // Find and execute 2-hop route
    let route = amm.find_best_route(&token_a, &token_c_id, &1_000, &3);
    mint_tokens(&env, &token_a, &user, 1_000);

    let output = amm.execute_multi_hop_swap(&user, route, &0);
    assert!(output > 0);
}

// ─── ADMIN & RISK MANAGEMENT TESTS ────────────────────────────────

#[test]
fn test_pause_resume_trading() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    // Pause trading
    amm.pause_trading(&admin);

    // Try to swap while paused - should fail
    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_id, &100_000, &100_000);

    mint_tokens(&env, &token_a, &user, 1_000);

    // This should panic because trading is paused
    let result = std::panic::catch_unwind(|| {
        amm.swap(&user, &pool_id, &token_a, &1_000, &0);
    });
    assert!(result.is_err());

    // Resume trading
    amm.resume_trading(&admin);

    // Now swap should work
    let output = amm.swap(&user, &pool_id, &token_a, &1_000, &0);
    assert!(output > 0);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_pause_trading() {
    let (env, admin, amm, token_a, token_b) = setup_env();
    let unauthorized = Address::generate(&env);

    // This should panic because caller is not admin
    amm.pause_trading(&unauthorized);
}

#[test]
fn test_set_admin() {
    let (env, admin, amm, token_a, token_b) = setup_env();
    let new_admin = Address::generate(&env);

    // Set new admin
    amm.set_admin(&admin, &new_admin);

    // New admin should be able to pause trading
    amm.pause_trading(&new_admin);

    // Old admin should not be able to pause trading anymore
    let result = std::panic::catch_unwind(|| {
        amm.resume_trading(&admin);
    });
    assert!(result.is_err());
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_set_admin() {
    let (env, admin, amm, token_a, token_b) = setup_env();
    let unauthorized = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // This should panic because caller is not admin
    amm.set_admin(&unauthorized, &new_admin);
}

#[test]
fn test_risk_parameters() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let params = RiskParams {
        max_position_per_user: 1_000_000,
        max_position_per_asset: 5_000_000,
        concentration_threshold_bps: 2500,   // 25%
        circuit_breaker_threshold_bps: 1000, // 10%
        circuit_breaker_cooldown: 1800,      // 30 minutes
        min_lp_token_threshold: 500,
    };

    amm.set_risk_parameters(&admin, params);

    // Check that parameters were set
    let user = Address::generate(&env);
    let (total_pos, concentration, threshold) = amm.get_risk_metrics(&user);
    assert_eq!(total_pos, 0); // No position yet
    assert_eq!(concentration, 0); // No concentration
    assert_eq!(threshold, 2500); // New threshold
}

#[test]
fn test_circuit_breaker() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    // Trigger circuit breaker
    amm.trigger_circuit_breaker(&admin, String::from_str(&env, "Market volatility"));

    // Try to swap while circuit breaker is active - should fail
    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    mint_tokens(&env, &token_a, &provider, 100_000);
    mint_tokens(&env, &token_b, &provider, 100_000);
    amm.add_liquidity(&provider, &pool_id, &100_000, &100_000);

    mint_tokens(&env, &token_a, &user, 1_000);

    // This should panic because circuit breaker is active
    let result = std::panic::catch_unwind(|| {
        amm.swap(&user, &pool_id, &token_a, &1_000, &0);
    });
    assert!(result.is_err());
}

// ─── ROUNDING PROTECTION TESTS ──────────────────────────────────────

#[test]
#[should_panic(expected = "Liquidity too small")]
fn test_minimum_lp_token_threshold() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    // Set high minimum threshold
    let params = RiskParams {
        max_position_per_user: 1_000_000_000,
        max_position_per_asset: 10_000_000_000,
        concentration_threshold_bps: 3000,
        circuit_breaker_threshold_bps: 1500,
        circuit_breaker_cooldown: 3600,
        min_lp_token_threshold: 10_000, // High threshold
    };
    amm.set_risk_parameters(&admin, params);

    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Try to add very small liquidity - should fail
    mint_tokens(&env, &token_a, &provider, 100);
    mint_tokens(&env, &token_b, &provider, 100);
    amm.add_liquidity(&provider, &pool_id, &100, &100);
}

#[test]
#[should_panic(expected = "Deposit ratio deviates too much")]
fn test_liquidity_ratio_protection() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Add initial liquidity with balanced ratio
    mint_tokens(&env, &token_a, &provider, 10_000);
    mint_tokens(&env, &token_b, &provider, 10_000);
    amm.add_liquidity(&provider, &pool_id, &10_000, &10_000);

    // Try to add liquidity with very unbalanced ratio - should fail
    let provider2 = Address::generate(&env);
    mint_tokens(&env, &token_a, &provider2, 10_000);
    mint_tokens(&env, &token_b, &provider2, 100); // Very unbalanced
    amm.add_liquidity(&provider2, &pool_id, &10_000, &100);
}

#[test]
#[should_panic(expected = "Remaining LP tokens below minimum threshold")]
fn test_remove_liquidity_dust_protection() {
    let (env, admin, amm, token_a, token_b) = setup_env();

    // Set high minimum threshold
    let params = RiskParams {
        max_position_per_user: 1_000_000_000,
        max_position_per_asset: 10_000_000_000,
        concentration_threshold_bps: 3000,
        circuit_breaker_threshold_bps: 1500,
        circuit_breaker_cooldown: 3600,
        min_lp_token_threshold: 1000,
    };
    amm.set_risk_parameters(&admin, params);

    let provider = Address::generate(&env);
    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    // Add liquidity
    mint_tokens(&env, &token_a, &provider, 10_000);
    mint_tokens(&env, &token_b, &provider, 10_000);
    let lp_tokens = amm.add_liquidity(&provider, &pool_id, &10_000, &10_000);

    // Try to remove most but not all liquidity, leaving dust - should fail
    let dust_amount = lp_tokens - 500; // Leave 500 LP tokens (below threshold)
    amm.remove_liquidity(&provider, &pool_id, &dust_amount);
}

// Issue #215: Tests for cache invalidation
#[test]
fn test_cache_invalidation_on_add_liquidity() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let amm_id = env.register_contract(None, Amm);
    let amm = AmmClient::new(&env, &amm_id);

    amm.init_contract(&admin);

    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    let provider = Address::generate(&env);

    // Add liquidity - should trigger cache invalidation
    let lp_tokens = amm.add_liquidity(&provider, &pool_id, &1000, &2000);
    assert!(lp_tokens > 0);

    // Verify cache was invalidated by checking that subsequent reads get fresh data
    let pool = amm.get_pool(&pool_id);
    assert_eq!(pool.reserve_a, 1000);
    assert_eq!(pool.reserve_b, 2000);
}

#[test]
fn test_cache_invalidation_on_remove_liquidity() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let amm_id = env.register_contract(None, Amm);
    let amm = AmmClient::new(&env, &amm_id);

    amm.init_contract(&admin);

    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    let provider = Address::generate(&env);
    amm.add_liquidity(&provider, &pool_id, &1000, &2000);

    // Remove liquidity - should trigger cache invalidation
    let (amount_a, amount_b) = amm.remove_liquidity(&provider, &pool_id, &500);
    assert!(amount_a > 0);
    assert!(amount_b > 0);

    // Verify cache was invalidated and data is fresh
    let pool = amm.get_pool(&pool_id);
    assert!(pool.reserve_a < 1000);
    assert!(pool.reserve_b < 2000);
}

#[test]
fn test_cache_invalidation_on_swap() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let amm_id = env.register_contract(None, Amm);
    let amm = AmmClient::new(&env, &amm_id);

    amm.init_contract(&admin);

    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    let pool_id = amm.create_pool(&admin, &token_a, &token_b, &30);

    let provider = Address::generate(&env);
    amm.add_liquidity(&provider, &pool_id, &1000, &2000);

    // Execute swap - should trigger cache invalidation
    let user = Address::generate(&env);
    let amount_out = amm.swap(&user, &pool_id, &token_a, &100, &1);
    assert!(amount_out > 0);

    // Verify cache was invalidated and reserves updated
    let pool = amm.get_pool(&pool_id);
    assert!(pool.reserve_a > 1000); // Increased by 100
    assert!(pool.reserve_b < 2000); // Decreased by amount_out
}
