use super::*;
use crate::storage::set_reentrancy_lock;
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

struct TestContext {
    env: Env,
    admin: Address,
    amm_id: Address,
    amm: AmmClient<'static>,
    token_a: Address,
    token_b: Address,
}

fn setup_env() -> TestContext {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let governance = Address::generate(&env);

    let amm_id = env.register(Amm, ());
    let amm = AmmClient::new(&env, &amm_id);

    let token_a = env.register(MockToken, ());
    let token_b = env.register(MockToken, ());

    amm.initialize(&admin, &Some(governance));

    TestContext {
        env,
        admin,
        amm_id,
        amm,
        token_a,
        token_b,
    }
}

fn mint_tokens(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    MockTokenClient::new(env, token_id).mint(to, &amount);
}

fn create_funded_pool(ctx: &TestContext, amount_a: i128, amount_b: i128) -> (u64, Address) {
    let provider = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);
    mint_tokens(&ctx.env, &ctx.token_a, &provider, amount_a);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, amount_b);
    ctx.amm
        .add_liquidity(&provider, &pool_id, &amount_a, &amount_b);
    (pool_id, provider)
}

// ─── Initialization & Pool Creation ──────────────────────────────

#[test]
fn test_initialize_and_create_pool() {
    let ctx = setup_env();
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);
    assert_eq!(pool_id, 0);

    let pool = ctx.amm.get_pool(&pool_id);
    assert_eq!(pool.token_a, ctx.token_a);
    assert_eq!(pool.token_b, ctx.token_b);
    assert_eq!(pool.fee_bps, 30);
    assert_eq!(pool.reserve_a, 0);
    assert_eq!(pool.reserve_b, 0);
}

#[test]
#[should_panic(expected = "Token addresses must differ")]
fn test_create_pool_same_tokens() {
    let ctx = setup_env();
    ctx.amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_a, &30);
}

#[test]
#[should_panic(expected = "Fee cannot exceed 10%")]
fn test_create_pool_excessive_fee() {
    let ctx = setup_env();
    ctx.amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &1001);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_create_pool_unauthorized() {
    let ctx = setup_env();
    let attacker = Address::generate(&ctx.env);
    ctx.amm
        .create_pool(&attacker, &ctx.token_a, &ctx.token_b, &30);
}

// ─── Liquidity ─────────────────────────────────────────────────────

#[test]
fn test_add_liquidity_initial() {
    let ctx = setup_env();
    let provider = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 10_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 20_000);

    let lp = ctx.amm.add_liquidity(&provider, &pool_id, &10_000, &20_000);
    assert!(lp > 0);

    let pool = ctx.amm.get_pool(&pool_id);
    assert_eq!(pool.reserve_a, 10_000);
    assert_eq!(pool.reserve_b, 20_000);
    assert_eq!(pool.lp_total_supply, lp);
    assert_eq!(ctx.amm.get_lp_balance(&pool_id, &provider), lp);
}

#[test]
fn test_add_liquidity_subsequent() {
    let ctx = setup_env();
    let provider1 = Address::generate(&ctx.env);
    let provider2 = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider1, 10_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider1, 10_000);
    let lp1 = ctx
        .amm
        .add_liquidity(&provider1, &pool_id, &10_000, &10_000);

    mint_tokens(&ctx.env, &ctx.token_a, &provider2, 5_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider2, 5_000);
    let lp2 = ctx.amm.add_liquidity(&provider2, &pool_id, &5_000, &5_000);

    assert_eq!(lp2, lp1 / 2);
}

#[test]
fn test_remove_liquidity() {
    let ctx = setup_env();
    let provider = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 10_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 10_000);
    let lp = ctx.amm.add_liquidity(&provider, &pool_id, &10_000, &10_000);

    let (out_a, out_b) = ctx.amm.remove_liquidity(&provider, &pool_id, &(lp / 2));
    assert_eq!(out_a, 5_000);
    assert_eq!(out_b, 5_000);

    let pool = ctx.amm.get_pool(&pool_id);
    assert_eq!(pool.reserve_a, 5_000);
    assert_eq!(pool.reserve_b, 5_000);
}

// ─── Swaps ─────────────────────────────────────────────────────────

#[test]
fn test_swap_and_quote() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);

    let quoted = ctx.amm.quote_swap(&pool_id, &ctx.token_a, &1_000);
    assert!(quoted > 0);
    assert!(quoted < 1_000);

    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);
    let out = ctx.amm.swap(&user, &pool_id, &ctx.token_a, &1_000, &0);
    assert_eq!(out, quoted);

    let user_b = MockTokenClient::new(&ctx.env, &ctx.token_b).balance(&user);
    assert_eq!(user_b, out);
}

#[test]
#[should_panic(expected = "Slippage tolerance exceeded")]
fn test_swap_slippage_protection() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);

    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);
    ctx.amm.swap(&user, &pool_id, &ctx.token_a, &1_000, &999);
}

#[test]
fn test_get_price() {
    let ctx = setup_env();
    let provider = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 10_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 20_000);
    ctx.amm.add_liquidity(&provider, &pool_id, &10_000, &20_000);

    assert_eq!(ctx.amm.get_price(&pool_id, &ctx.token_a), 2_000_000);
    assert_eq!(ctx.amm.get_price(&pool_id, &ctx.token_b), 500_000);
}

#[test]
fn test_fee_distribution_via_reserves() {
    let ctx = setup_env();
    let provider = Address::generate(&ctx.env);
    let user = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 100_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 100_000);
    let lp = ctx
        .amm
        .add_liquidity(&provider, &pool_id, &100_000, &100_000);

    mint_tokens(&ctx.env, &ctx.token_a, &user, 10_000);
    ctx.amm.swap(&user, &pool_id, &ctx.token_a, &10_000, &0);

    let (out_a, out_b) = ctx.amm.remove_liquidity(&provider, &pool_id, &lp);
    assert!(out_a > 100_000);
    assert!(out_b < 100_000);
    assert!(out_a + out_b > 200_000);
}

#[test]
fn test_protocol_fee_to_governance() {
    let ctx = setup_env();
    let governance = Address::generate(&ctx.env);
    ctx.amm.set_fee_config(&ctx.admin, &governance, &2_000);

    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);
    mint_tokens(&ctx.env, &ctx.token_a, &user, 10_000);

    let gov_before = MockTokenClient::new(&ctx.env, &ctx.token_a).balance(&governance);
    ctx.amm.swap(&user, &pool_id, &ctx.token_a, &10_000, &0);
    let gov_after = MockTokenClient::new(&ctx.env, &ctx.token_a).balance(&governance);

    assert!(gov_after > gov_before);
}

// ─── Flash Swap ────────────────────────────────────────────────────

#[test]
fn test_flash_swap() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let borrower = Address::generate(&ctx.env);

    let max_in = 10_000i128;
    mint_tokens(&ctx.env, &ctx.token_a, &borrower, max_in);

    let bal_before = MockTokenClient::new(&ctx.env, &ctx.token_b).balance(&borrower);
    let amount_in = ctx.amm.flash_swap(
        &borrower,
        &pool_id,
        &ctx.token_b,
        &1_000,
        &ctx.token_a,
        &max_in,
    );
    let bal_after = MockTokenClient::new(&ctx.env, &ctx.token_b).balance(&borrower);

    assert!(amount_in > 0);
    assert_eq!(bal_after - bal_before, 1_000);

    let pool = ctx.amm.get_pool(&pool_id);
    assert!(pool.reserve_a > 100_000);
    assert!(pool.reserve_b < 100_000);
}

#[test]
#[should_panic(expected = "Flash swap repayment exceeds max_amount_in")]
fn test_flash_swap_max_amount_in() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let borrower = Address::generate(&ctx.env);

    mint_tokens(&ctx.env, &ctx.token_a, &borrower, 1);
    ctx.amm
        .flash_swap(&borrower, &pool_id, &ctx.token_b, &1_000, &ctx.token_a, &1);
}

// ─── LP Rewards ────────────────────────────────────────────────────

#[test]
fn test_lp_rewards_deposit_and_claim() {
    let ctx = setup_env();
    let (pool_id, provider) = create_funded_pool(&ctx, 10_000, 10_000);

    mint_tokens(&ctx.env, &ctx.token_a, &ctx.admin, 5_000);
    ctx.amm
        .deposit_lp_rewards(&ctx.admin, &pool_id, &ctx.token_a, &5_000);

    let (pending_a, pending_b) = ctx.amm.get_pending_lp_rewards(&pool_id, &provider);
    assert_eq!(pending_a, 5_000);
    assert_eq!(pending_b, 0);

    let (claimed_a, claimed_b) = ctx.amm.claim_lp_rewards(&provider, &pool_id);
    assert_eq!(claimed_a, 5_000);
    assert_eq!(claimed_b, 0);

    let (pending_a2, _) = ctx.amm.get_pending_lp_rewards(&pool_id, &provider);
    assert_eq!(pending_a2, 0);
}

// ─── Multi-hop ─────────────────────────────────────────────────────

#[test]
fn test_find_best_route_direct() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);

    let route = ctx
        .amm
        .find_best_route(&ctx.token_a, &ctx.token_b, &1_000, &2);

    assert_eq!(route.token_in, ctx.token_a);
    assert_eq!(route.token_out, ctx.token_b);
    assert_eq!(route.hops.len(), 1);
    assert_eq!(route.hops.get(0).unwrap().pool_id, pool_id);
}

#[test]
fn test_execute_multi_hop_swap_two_hop() {
    let ctx = setup_env();
    let token_c = ctx.env.register(MockToken, ());
    let provider = Address::generate(&ctx.env);
    let user = Address::generate(&ctx.env);

    let pool_ab = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);
    let pool_bc = ctx.amm.create_pool(&ctx.admin, &ctx.token_b, &token_c, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 100_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 200_000);
    ctx.amm
        .add_liquidity(&provider, &pool_ab, &100_000, &200_000);

    mint_tokens(&ctx.env, &ctx.token_b, &provider, 100_000);
    mint_tokens(&ctx.env, &token_c, &provider, 100_000);
    ctx.amm
        .add_liquidity(&provider, &pool_bc, &100_000, &100_000);

    let route = ctx.amm.find_best_route(&ctx.token_a, &token_c, &1_000, &3);
    let hop_count = route.hops.len();
    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);

    let output = ctx.amm.execute_multi_hop_swap(&user, &route, &0);
    assert!(output > 0);
    assert_eq!(hop_count, 2);
}

#[test]
fn test_multiple_pools() {
    let ctx = setup_env();
    let token_c = ctx.env.register(MockToken, ());

    let pool_0 = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);
    let pool_1 = ctx.amm.create_pool(&ctx.admin, &ctx.token_a, &token_c, &50);

    assert_eq!(pool_0, 0);
    assert_eq!(pool_1, 1);
    assert_eq!(ctx.amm.get_pool(&pool_0).fee_bps, 30);
    assert_eq!(ctx.amm.get_pool(&pool_1).fee_bps, 50);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_double_initialize_panics() {
    let ctx = setup_env();
    ctx.amm.initialize(&ctx.admin, &None);
}

#[test]
#[should_panic(expected = "No valid route found")]
fn test_find_best_route_no_route() {
    let ctx = setup_env();
    let token_c = ctx.env.register(MockToken, ());

    // Pool exists for A-B only; no path to token_c.
    let _pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    ctx.amm.find_best_route(&ctx.token_a, &token_c, &1_000, &3);
}

#[test]
fn test_execute_multi_hop_swap_single_hop() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);

    let route = ctx
        .amm
        .find_best_route(&ctx.token_a, &ctx.token_b, &1_000, &2);
    assert_eq!(route.hops.len(), 1);
    assert_eq!(route.hops.get(0).unwrap().pool_id, pool_id);

    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);
    let quoted = ctx.amm.quote_swap(&pool_id, &ctx.token_a, &1_000);
    let output = ctx.amm.execute_multi_hop_swap(&user, &route, &0);

    assert_eq!(output, quoted);
    assert!(output > 0);
    assert!(output < 1_000);
}

#[test]
#[should_panic(expected = "Reentrancy detected")]
fn test_reentrancy_guard_blocks_swap() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);
    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);

    // Simulate a nested entry while the reentrancy lock is held.
    ctx.env.as_contract(&ctx.amm_id, || {
        set_reentrancy_lock(&ctx.env, true);
    });

    ctx.amm.swap(&user, &pool_id, &ctx.token_a, &1_000, &0);
}

#[test]
#[should_panic(expected = "Insufficient pool liquidity for flash swap")]
fn test_flash_swap_insufficient_liquidity() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let borrower = Address::generate(&ctx.env);

    mint_tokens(&ctx.env, &ctx.token_a, &borrower, 200_000);
    ctx.amm.flash_swap(
        &borrower,
        &pool_id,
        &ctx.token_b,
        &200_000,
        &ctx.token_a,
        &500_000,
    );
}

#[test]
fn test_get_pending_lp_rewards_without_lp() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 10_000, 10_000);
    let outsider = Address::generate(&ctx.env);

    let (pending_a, pending_b) = ctx.amm.get_pending_lp_rewards(&pool_id, &outsider);
    assert_eq!(pending_a, 0);
    assert_eq!(pending_b, 0);
}

// ─── Admin & Risk ──────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Trading is currently paused")]
fn test_swap_while_paused() {
    let ctx = setup_env();
    ctx.amm.pause_trading(&ctx.admin);

    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);
    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);
    ctx.amm.swap(&user, &pool_id, &ctx.token_a, &1_000, &0);
}

#[test]
fn test_pause_resume_trading() {
    let ctx = setup_env();
    ctx.amm.pause_trading(&ctx.admin);
    ctx.amm.resume_trading(&ctx.admin);

    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);
    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);

    let output = ctx.amm.swap(&user, &pool_id, &ctx.token_a, &1_000, &0);
    assert!(output > 0);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_pause_trading() {
    let ctx = setup_env();
    let attacker = Address::generate(&ctx.env);
    ctx.amm.pause_trading(&attacker);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_old_admin_cannot_resume() {
    let ctx = setup_env();
    let new_admin = Address::generate(&ctx.env);

    ctx.amm.set_admin(&ctx.admin, &new_admin);
    ctx.amm.pause_trading(&new_admin);
    ctx.amm.resume_trading(&ctx.admin);
}

#[test]
#[should_panic(expected = "Circuit breaker is active")]
fn test_swap_while_circuit_breaker_active() {
    let ctx = setup_env();
    ctx.amm.trigger_circuit_breaker(
        &ctx.admin,
        &soroban_sdk::String::from_str(&ctx.env, "Market volatility"),
    );

    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);
    mint_tokens(&ctx.env, &ctx.token_a, &user, 1_000);
    ctx.amm.swap(&user, &pool_id, &ctx.token_a, &1_000, &0);
}

#[test]
fn test_risk_parameters() {
    let ctx = setup_env();
    let params = RiskParams {
        max_position_per_user: 1_000_000,
        max_position_per_asset: 5_000_000,
        concentration_threshold_bps: 2500,
        circuit_breaker_threshold_bps: 1000,
        circuit_breaker_cooldown: 1800,
        min_lp_token_threshold: 500,
    };

    ctx.amm.set_risk_parameters(&ctx.admin, &params);
    let user = Address::generate(&ctx.env);
    let (total_pos, concentration, threshold) = ctx.amm.get_risk_metrics(&user);
    assert_eq!(total_pos, 0);
    assert_eq!(concentration, 0);
    assert_eq!(threshold, 2500);
}

// ─── Rounding Protection ───────────────────────────────────────────

#[test]
#[should_panic(expected = "Liquidity too small")]
fn test_minimum_lp_token_threshold() {
    let ctx = setup_env();
    let params = RiskParams {
        max_position_per_user: 1_000_000_000,
        max_position_per_asset: 10_000_000_000,
        concentration_threshold_bps: 3000,
        circuit_breaker_threshold_bps: 1500,
        circuit_breaker_cooldown: 3600,
        min_lp_token_threshold: 10_000,
    };
    ctx.amm.set_risk_parameters(&ctx.admin, &params);

    let provider = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 100);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 100);
    ctx.amm.add_liquidity(&provider, &pool_id, &100, &100);
}

#[test]
#[should_panic(expected = "Deposit ratio deviates too much")]
fn test_liquidity_ratio_protection() {
    let ctx = setup_env();
    let params = RiskParams {
        max_position_per_user: 1_000_000_000,
        max_position_per_asset: 10_000_000_000,
        concentration_threshold_bps: 3000,
        circuit_breaker_threshold_bps: 1500,
        circuit_breaker_cooldown: 3600,
        min_lp_token_threshold: 10,
    };
    ctx.amm.set_risk_parameters(&ctx.admin, &params);

    let provider = Address::generate(&ctx.env);
    let provider2 = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 10_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 10_000);
    ctx.amm.add_liquidity(&provider, &pool_id, &10_000, &10_000);

    mint_tokens(&ctx.env, &ctx.token_a, &provider2, 10_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider2, 100);
    ctx.amm.add_liquidity(&provider2, &pool_id, &10_000, &100);
}

#[test]
#[should_panic(expected = "Remaining LP tokens below minimum threshold")]
fn test_remove_liquidity_dust_protection() {
    let ctx = setup_env();
    let params = RiskParams {
        max_position_per_user: 1_000_000_000,
        max_position_per_asset: 10_000_000_000,
        concentration_threshold_bps: 3000,
        circuit_breaker_threshold_bps: 1500,
        circuit_breaker_cooldown: 3600,
        min_lp_token_threshold: 1000,
    };
    ctx.amm.set_risk_parameters(&ctx.admin, &params);

    let provider = Address::generate(&ctx.env);
    let pool_id = ctx
        .amm
        .create_pool(&ctx.admin, &ctx.token_a, &ctx.token_b, &30);

    mint_tokens(&ctx.env, &ctx.token_a, &provider, 10_000);
    mint_tokens(&ctx.env, &ctx.token_b, &provider, 10_000);
    let lp = ctx.amm.add_liquidity(&provider, &pool_id, &10_000, &10_000);

    ctx.amm.remove_liquidity(&provider, &pool_id, &(lp - 500));
}

#[test]
fn test_constant_product_invariant_after_swap() {
    let ctx = setup_env();
    let (pool_id, _) = create_funded_pool(&ctx, 100_000, 100_000);
    let user = Address::generate(&ctx.env);

    let pool_before = ctx.amm.get_pool(&pool_id);
    let k_before = pool_before.reserve_a * pool_before.reserve_b;

    mint_tokens(&ctx.env, &ctx.token_a, &user, 5_000);
    ctx.amm.swap(&user, &pool_id, &ctx.token_a, &5_000, &0);

    let pool_after = ctx.amm.get_pool(&pool_id);
    let k_after = pool_after.reserve_a * pool_after.reserve_b;

    // k should increase due to fees staying in the pool.
    assert!(k_after >= k_before);
}
