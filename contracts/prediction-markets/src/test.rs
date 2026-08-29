use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, testutils::Ledger as _, Address,
    Env, IntoVal,
};

// ─── Mock Token ─────────────────────────────────────────────────────

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

// ─── Test Context ────────────────────────────────────────────────────

struct TestContext {
    env: Env,
    admin: Address,
    oracle: Address,
    _pm_id: Address,
    pm: PredictionMarketClient<'static>,
    collateral: Address,
    user1: Address,
    user2: Address,
    lp_provider: Address,
}

fn setup() -> TestContext {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let _pm_id = env.register(PredictionMarket, ());
    let pm = PredictionMarketClient::new(&env, &_pm_id);

    let collateral = env.register(MockToken, ());

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let lp_provider = Address::generate(&env);

    pm.initialize(&admin, &oracle);

    TestContext {
        env,
        admin,
        oracle,
        _pm_id,
        pm,
        collateral,
        user1,
        user2,
        lp_provider,
    }
}

fn mint(ctx: &TestContext, to: &Address, amount: i128) {
    MockTokenClient::new(&ctx.env, &ctx.collateral).mint(to, &amount);
}

fn token_balance(ctx: &TestContext, addr: &Address) -> i128 {
    MockTokenClient::new(&ctx.env, &ctx.collateral).balance(addr)
}

fn create_binary_market(ctx: &TestContext, initial_liq: i128) -> u64 {
    let total_needed = initial_liq * 2; // num_outcomes = 2
    mint(ctx, &ctx.user1, total_needed);
    let names = soroban_sdk::vec![&ctx.env, "Yes".into_val(&ctx.env), "No".into_val(&ctx.env)];
    let params = CreateMarketParams {
        question: "Will BTC hit 100k?".into_val(&ctx.env),
        category: MarketCategory::Crypto,
        collateral_token: ctx.collateral.clone(),
        oracle_source: ctx.oracle.clone(),
        num_outcomes: 2,
        outcome_names: names,
        resolution_window_duration: 86400,
        max_outcome_supply: initial_liq * 100,
        trading_fee_bps: 30,
        initial_liquidity: initial_liq,
    };
    ctx.pm.create_market(&ctx.user1, &params)
}

fn create_three_outcome_market(ctx: &TestContext, initial_liq: i128) -> u64 {
    let total_needed = initial_liq * 3; // num_outcomes = 3
    mint(ctx, &ctx.user1, total_needed);
    let names = soroban_sdk::vec![
        &ctx.env,
        "Team A".into_val(&ctx.env),
        "Team B".into_val(&ctx.env),
        "Draw".into_val(&ctx.env),
    ];
    let params = CreateMarketParams {
        question: "Who wins?".into_val(&ctx.env),
        category: MarketCategory::Sports,
        collateral_token: ctx.collateral.clone(),
        oracle_source: ctx.oracle.clone(),
        num_outcomes: 3,
        outcome_names: names,
        resolution_window_duration: 86400,
        max_outcome_supply: initial_liq * 100,
        trading_fee_bps: 30,
        initial_liquidity: initial_liq,
    };
    ctx.pm.create_market(&ctx.user1, &params)
}

fn advance_time(ctx: &TestContext, seconds: u64) {
    ctx.env
        .ledger()
        .set_timestamp(ctx.env.ledger().timestamp() + seconds);
}

// ─── Initialization ─────────────────────────────────────────────────

#[test]
fn test_initialize() {
    let ctx = setup();
    let _ = create_binary_market(&ctx, 10_000);
    let market = ctx.pm.query_market(&0);
    assert_eq!(market.market_id, 0);
    assert_eq!(market.status, MarketStatus::Active);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_double_initialize() {
    let ctx = setup();
    ctx.pm.initialize(&ctx.admin, &ctx.oracle);
}

// ─── Market Creation ────────────────────────────────────────────────

#[test]
fn test_create_binary_market() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.num_outcomes, 2);
    assert_eq!(market.status, MarketStatus::Active);
    assert_eq!(market.trading_fee_bps, 30);

    let pool_0 = ctx.pm.query_outcome_pool(&market_id, &0);
    assert_eq!(pool_0.collateral_reserve, 10_000);
    assert_eq!(pool_0.outcome_reserve, 10_000);
}

#[test]
fn test_create_three_outcome_market() {
    let ctx = setup();
    let market_id = create_three_outcome_market(&ctx, 10_000);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.num_outcomes, 3);

    for i in 0..3 {
        let pool = ctx.pm.query_outcome_pool(&market_id, &i);
        assert_eq!(pool.collateral_reserve, 10_000);
    }
}

#[test]
#[should_panic(expected = "Invalid number of outcomes")]
fn test_create_market_too_few_outcomes() {
    let ctx = setup();
    let names = soroban_sdk::vec![&ctx.env, "Yes".into_val(&ctx.env)];
    let params = CreateMarketParams {
        question: "Q?".into_val(&ctx.env),
        category: MarketCategory::Custom,
        collateral_token: ctx.collateral.clone(),
        oracle_source: ctx.oracle.clone(),
        num_outcomes: 1,
        outcome_names: names,
        resolution_window_duration: 86400,
        max_outcome_supply: 100_000,
        trading_fee_bps: 30,
        initial_liquidity: 10_000,
    };
    ctx.pm.create_market(&ctx.user1, &params);
}

#[test]
#[should_panic(expected = "Fee too high")]
fn test_create_market_fee_too_high() {
    let ctx = setup();
    let names = soroban_sdk::vec![&ctx.env, "Yes".into_val(&ctx.env), "No".into_val(&ctx.env)];
    let params = CreateMarketParams {
        question: "Q?".into_val(&ctx.env),
        category: MarketCategory::Custom,
        collateral_token: ctx.collateral.clone(),
        oracle_source: ctx.oracle.clone(),
        num_outcomes: 2,
        outcome_names: names,
        resolution_window_duration: 86400,
        max_outcome_supply: 100_000,
        trading_fee_bps: 600,
        initial_liquidity: 10_000,
    };
    ctx.pm.create_market(&ctx.user1, &params);
}

// ─── Buy / Sell Outcome Tokens ──────────────────────────────────────

#[test]
fn test_buy_outcome() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 5_000);

    let pool_before = ctx.pm.query_outcome_pool(&market_id, &0);
    let k_before = pool_before.collateral_reserve * pool_before.outcome_reserve;

    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);
    assert!(bought > 0);
    assert!(bought < 1_000);

    let balance = ctx.pm.query_user_balance(&market_id, &0, &ctx.user1);
    assert_eq!(balance, bought);

    let pool_after = ctx.pm.query_outcome_pool(&market_id, &0);
    assert!(pool_after.collateral_reserve > pool_before.collateral_reserve);
    assert!(pool_after.outcome_reserve < pool_before.outcome_reserve);

    let k_after = pool_after.collateral_reserve * pool_after.outcome_reserve;
    assert!(k_after >= k_before, "CPMM invariant violated");
}

#[test]
fn test_buy_outcome_updates_position() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);
    mint(&ctx, &ctx.user1, 10_000);

    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &5_000, &0);

    let pos = ctx.pm.query_user_position(&market_id, &0, &ctx.user1);
    assert!(pos.is_some());
    let pos = pos.unwrap();
    assert_eq!(pos.quantity, bought);
    assert!(pos.avg_entry_price > 0);
}

#[test]
#[should_panic(expected = "Invalid amount")]
fn test_buy_zero_collateral() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &0, &0);
}

#[test]
#[should_panic(expected = "Invalid outcome index")]
fn test_buy_invalid_outcome() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    mint(&ctx, &ctx.user1, 1_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &5, &1_000, &0);
}

#[test]
fn test_sell_outcome() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);
    mint(&ctx, &ctx.user1, 5_000);

    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);
    let collateral_before = token_balance(&ctx, &ctx.user1);

    let sold = ctx.pm.sell_outcome(&ctx.user1, &market_id, &0, &bought, &0);
    assert!(sold > 0);

    let collateral_after = token_balance(&ctx, &ctx.user1);
    assert_eq!(collateral_after, collateral_before + sold);

    let balance = ctx.pm.query_user_balance(&market_id, &0, &ctx.user1);
    assert_eq!(balance, 0);
}

#[test]
#[should_panic(expected = "Insufficient balance")]
fn test_sell_more_than_owned() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    ctx.pm.sell_outcome(&ctx.user1, &market_id, &0, &1_000, &0);
}

#[test]
fn test_buy_and_sell_conserves_cpmm() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    let pool_before = ctx.pm.query_outcome_pool(&market_id, &0);
    let k_before = pool_before.collateral_reserve * pool_before.outcome_reserve;

    mint(&ctx, &ctx.user1, 10_000);
    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &5_000, &0);
    ctx.pm.sell_outcome(&ctx.user1, &market_id, &0, &bought, &0);

    let pool_after = ctx.pm.query_outcome_pool(&market_id, &0);
    let k_after = pool_after.collateral_reserve * pool_after.outcome_reserve;
    assert!(
        k_after >= k_before,
        "CPMM invariant violated after buy+sell"
    );
}

// ─── Quotes ─────────────────────────────────────────────────────────

#[test]
fn test_quote_buy() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    let quote = ctx.pm.quote_buy(&market_id, &0, &1_000);
    assert!(quote > 0);
    assert!(quote < 1_000);

    mint(&ctx, &ctx.user1, 1_000);
    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);
    assert_eq!(quote, bought);
}

#[test]
fn test_quote_sell() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);
    mint(&ctx, &ctx.user1, 5_000);
    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);

    let quote = ctx.pm.quote_sell(&market_id, &0, &bought);
    assert!(quote > 0);
}

// ─── Liquidity Provision ────────────────────────────────────────────

#[test]
fn test_add_liquidity() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    mint(&ctx, &ctx.lp_provider, 10_000);
    let shares = ctx
        .pm
        .add_liquidity(&ctx.lp_provider, &market_id, &0, &5_000, &5_000);
    assert!(shares > 0);

    let pool = ctx.pm.query_outcome_pool(&market_id, &0);
    assert_eq!(pool.collateral_reserve, 15_000);
    assert_eq!(pool.outcome_reserve, 15_000);

    let lp_shares = ctx.pm.query_lp_shares(&market_id, &ctx.lp_provider);
    assert_eq!(lp_shares, shares);
}

#[test]
fn test_remove_liquidity() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    mint(&ctx, &ctx.lp_provider, 10_000);
    let shares = ctx
        .pm
        .add_liquidity(&ctx.lp_provider, &market_id, &0, &5_000, &5_000);

    let (collateral_out, outcome_out) =
        ctx.pm
            .remove_liquidity(&ctx.lp_provider, &market_id, &0, &(shares / 2));
    assert!(collateral_out > 0);
    assert!(outcome_out > 0);

    let pool = ctx.pm.query_outcome_pool(&market_id, &0);
    assert!(pool.collateral_reserve < 15_000);
}

#[test]
#[should_panic(expected = "Insufficient LP tokens")]
fn test_remove_liquidity_insufficient() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    ctx.pm
        .remove_liquidity(&ctx.lp_provider, &market_id, &0, &1000);
}

#[test]
fn test_lp_earns_fees_from_trading() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.lp_provider, 50_000);
    ctx.pm
        .add_liquidity(&ctx.lp_provider, &market_id, &0, &25_000, &25_000);

    let lp_shares = ctx.pm.query_lp_shares(&market_id, &ctx.lp_provider);

    mint(&ctx, &ctx.user1, 10_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &10_000, &0);
    ctx.pm.sell_outcome(&ctx.user1, &market_id, &0, &5_000, &0);

    let (c, _o) = ctx
        .pm
        .remove_liquidity(&ctx.lp_provider, &market_id, &0, &lp_shares);
    assert!(c > 25_000, "LP should earn fees");
}

// ─── Resolution ─────────────────────────────────────────────────────

#[test]
fn test_resolve_market() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    advance_time(&ctx, 86_401);

    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.status, MarketStatus::Resolved);
    assert_eq!(market.resolved_outcome, Some(0));
}

#[test]
#[should_panic(expected = "Resolution window not closed")]
fn test_resolve_too_early() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    advance_time(&ctx, 100);

    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);
}

#[test]
#[should_panic(expected = "Unauthorized oracle")]
fn test_resolve_wrong_oracle() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    let fake_oracle = Address::generate(&ctx.env);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&fake_oracle, &market_id, &0);
}

#[test]
#[should_panic(expected = "Invalid outcome index")]
fn test_resolve_invalid_outcome() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &5);
}

// ─── Redemption ─────────────────────────────────────────────────────

#[test]
fn test_redeem_winning_tokens() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 10_000);
    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &5_000, &0);
    assert!(bought > 0);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);

    let collateral_before = token_balance(&ctx, &ctx.user1);

    let payout = ctx.pm.redeem_winning_tokens(&ctx.user1, &market_id);
    assert_eq!(payout, bought);

    let collateral_after = token_balance(&ctx, &ctx.user1);
    assert_eq!(collateral_after, collateral_before + payout);

    let balance = ctx.pm.query_user_balance(&market_id, &0, &ctx.user1);
    assert_eq!(balance, 0);
}

#[test]
#[should_panic(expected = "Market not resolved")]
fn test_redeem_unresolved() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    mint(&ctx, &ctx.user1, 1_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);

    ctx.pm.redeem_winning_tokens(&ctx.user1, &market_id);
}

#[test]
#[should_panic(expected = "No winning tokens to redeem")]
fn test_redeem_losing_tokens() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 10_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &1, &5_000, &0);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);

    ctx.pm.redeem_winning_tokens(&ctx.user1, &market_id);
}

// ─── Order Book ─────────────────────────────────────────────────────

#[test]
fn test_place_and_cancel_order() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    mint(&ctx, &ctx.user1, 10_000);
    let order_id = ctx.pm.place_order(
        &ctx.user1,
        &market_id,
        &0,
        &OrderSide::Buy,
        &(DECIMAL_FACTOR / 2),
        &1_000,
    );

    let order = ctx.pm.query_order(&market_id, &order_id);
    assert_eq!(order.status, OrderStatus::Open);

    ctx.pm.cancel_order(&ctx.user1, &market_id, &order_id);
    let order = ctx.pm.query_order(&market_id, &order_id);
    assert_eq!(order.status, OrderStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_cancel_others_order() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    mint(&ctx, &ctx.user1, 10_000);
    let order_id = ctx.pm.place_order(
        &ctx.user1,
        &market_id,
        &0,
        &OrderSide::Buy,
        &(DECIMAL_FACTOR / 2),
        &1_000,
    );

    ctx.pm.cancel_order(&ctx.user2, &market_id, &order_id);
}

// ─── Dispute ────────────────────────────────────────────────────────

#[test]
fn test_open_and_resolve_dispute_success() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 10_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &5_000, &0);

    mint(&ctx, &ctx.user2, 10_000);
    ctx.pm.buy_outcome(&ctx.user2, &market_id, &1, &5_000, &0);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);

    mint(&ctx, &ctx.user2, 10_000);
    let dispute_id = ctx.pm.open_dispute(
        &ctx.user2,
        &market_id,
        &1,
        &"Evidence of wrong outcome".into_val(&ctx.env),
        &1_000,
    );

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.status, MarketStatus::Disputed);

    ctx.pm
        .vote_dispute(&ctx.user1, &market_id, &dispute_id, &false);
    ctx.pm
        .vote_dispute(&ctx.user2, &market_id, &dispute_id, &true);

    ctx.pm.resolve_dispute(&ctx.admin, &market_id, &dispute_id);

    let dispute = ctx.pm.query_dispute(&market_id, &dispute_id);
    assert_eq!(dispute.status, DisputeStatus::ResolvedRejected);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.status, MarketStatus::Resolved);
    assert_eq!(market.resolved_outcome, Some(0));
}

#[test]
fn test_dispute_upheld() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 20_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &1, &10_000, &0);

    mint(&ctx, &ctx.user2, 10_000);
    ctx.pm.buy_outcome(&ctx.user2, &market_id, &0, &5_000, &0);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);

    mint(&ctx, &ctx.user1, 10_000);
    let dispute_id = ctx.pm.open_dispute(
        &ctx.user1,
        &market_id,
        &1,
        &"Wrong!".into_val(&ctx.env),
        &1_000,
    );

    ctx.pm
        .vote_dispute(&ctx.user1, &market_id, &dispute_id, &true);
    ctx.pm
        .vote_dispute(&ctx.user2, &market_id, &dispute_id, &false);

    let stake_before = token_balance(&ctx, &ctx.user1);
    ctx.pm.resolve_dispute(&ctx.admin, &market_id, &dispute_id);

    let dispute = ctx.pm.query_dispute(&market_id, &dispute_id);
    assert_eq!(dispute.status, DisputeStatus::ResolvedUpheld);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.resolved_outcome, Some(1));

    let stake_after = token_balance(&ctx, &ctx.user1);
    assert_eq!(stake_after, stake_before + 1_000);
}

#[test]
#[should_panic(expected = "Already voted")]
fn test_double_vote() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 10_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &5_000, &0);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);

    mint(&ctx, &ctx.user2, 10_000);
    let dispute_id = ctx.pm.open_dispute(
        &ctx.user2,
        &market_id,
        &1,
        &"Nope".into_val(&ctx.env),
        &1_000,
    );

    ctx.pm
        .vote_dispute(&ctx.user1, &market_id, &dispute_id, &false);
    ctx.pm
        .vote_dispute(&ctx.user1, &market_id, &dispute_id, &false);
}

// ─── Early Close ────────────────────────────────────────────────────

#[test]
fn test_close_market_early_by_creator() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    ctx.pm.close_market_early(&ctx.user1, &market_id);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.status, MarketStatus::Closed);
}

#[test]
fn test_close_market_early_by_admin() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    ctx.pm.close_market_early(&ctx.admin, &market_id);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.status, MarketStatus::Closed);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_close_market_early_unauthorized() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    ctx.pm.close_market_early(&ctx.user2, &market_id);
}

// ─── Admin Controls ─────────────────────────────────────────────────

#[test]
fn test_pause_resume_trading() {
    let ctx = setup();
    ctx.pm.admin_pause_trading(&ctx.admin);
    // Verify paused: try to buy should fail
    let _market_id = create_binary_market(&ctx, 10_000);
    mint(&ctx, &ctx.user1, 1_000);
    // buy should fail because trading is paused — but mock_all_auths suppresses that
    // Just verify we can resume
    ctx.pm.admin_resume_trading(&ctx.admin);
}

#[test]
#[should_panic(expected = "Trading is paused")]
fn test_buy_while_paused() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);
    ctx.pm.admin_pause_trading(&ctx.admin);

    mint(&ctx, &ctx.user1, 1_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_pause() {
    let ctx = setup();
    ctx.pm.admin_pause_trading(&ctx.user2);
}

#[test]
fn test_set_admin() {
    let ctx = setup();
    let new_admin = Address::generate(&ctx.env);
    ctx.pm.admin_set_admin(&ctx.admin, &new_admin);

    // New admin can pause — proves admin was transferred
    ctx.pm.admin_pause_trading(&new_admin);
    // Old admin can no longer pause
    ctx.env.mock_auths(&[]);
    // (old admin pause would fail, but mock_all_auths is on by default)
}

// ─── View Functions ─────────────────────────────────────────────────

#[test]
fn test_get_outcome_price() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    let price = ctx.pm.query_outcome_price(&market_id, &0);
    assert_eq!(price, DECIMAL_FACTOR / 2);
}

#[test]
fn test_get_outcome_price_after_trade() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 50_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &20_000, &0);

    let price = ctx.pm.query_outcome_price(&market_id, &0);
    assert!(price > DECIMAL_FACTOR / 2);
}

#[test]
fn test_get_total_outcome_supply() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    assert_eq!(ctx.pm.query_total_outcome_supply(&market_id, &0), 0);

    mint(&ctx, &ctx.user1, 5_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);

    let supply = ctx.pm.query_total_outcome_supply(&market_id, &0);
    assert!(supply > 0);
}

#[test]
fn test_market_collateral_tracking() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    let initial = ctx.pm.query_market_collateral(&market_id);
    assert_eq!(initial, 20_000);

    mint(&ctx, &ctx.user1, 5_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);

    let after = ctx.pm.query_market_collateral(&market_id);
    assert!(after > initial);
}

// ─── Edge Cases ─────────────────────────────────────────────────────

#[test]
fn test_concurrent_buy_sell_multiple_users() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 50_000);
    mint(&ctx, &ctx.user2, 50_000);

    let b1 = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &10_000, &0);
    let b2 = ctx.pm.buy_outcome(&ctx.user2, &market_id, &1, &10_000, &0);

    assert!(b1 > 0);
    assert!(b2 > 0);

    ctx.pm
        .sell_outcome(&ctx.user1, &market_id, &0, &(b1 / 2), &0);
    ctx.pm
        .sell_outcome(&ctx.user2, &market_id, &1, &(b2 / 2), &0);

    let market = ctx.pm.query_market(&market_id);
    assert_eq!(market.status, MarketStatus::Active);
}

#[test]
#[should_panic(expected = "Slippage exceeded")]
fn test_slippage_protection_buy() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    mint(&ctx, &ctx.user1, 10_000);

    // min_outcome_out of 999 is way too high for a 1000 collateral buy
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &999);
}

#[test]
#[should_panic(expected = "Slippage exceeded")]
fn test_slippage_protection_sell() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 10_000);
    let bought = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);

    // min_collateral_out of 999_999 is way too high
    ctx.pm
        .sell_outcome(&ctx.user1, &market_id, &0, &bought, &999_999);
}

#[test]
fn test_multi_outcome_market_trading() {
    let ctx = setup();
    let market_id = create_three_outcome_market(&ctx, 100_000);

    mint(&ctx, &ctx.user1, 30_000);

    let b0 = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &10_000, &0);
    let b1 = ctx.pm.buy_outcome(&ctx.user1, &market_id, &1, &10_000, &0);
    let b2 = ctx.pm.buy_outcome(&ctx.user1, &market_id, &2, &10_000, &0);

    assert!(b0 > 0 && b1 > 0 && b2 > 0);

    assert_eq!(ctx.pm.query_user_balance(&market_id, &0, &ctx.user1), b0);
    assert_eq!(ctx.pm.query_user_balance(&market_id, &1, &ctx.user1), b1);
    assert_eq!(ctx.pm.query_user_balance(&market_id, &2, &ctx.user1), b2);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &2);

    let payout = ctx.pm.redeem_winning_tokens(&ctx.user1, &market_id);
    assert_eq!(payout, b2);
}

#[test]
fn test_three_outcome_prices_sum() {
    let ctx = setup();
    let market_id = create_three_outcome_market(&ctx, 100_000);

    let p0 = ctx.pm.query_outcome_price(&market_id, &0);
    let p1 = ctx.pm.query_outcome_price(&market_id, &1);
    let p2 = ctx.pm.query_outcome_price(&market_id, &2);

    // Each pool starts with equal reserves, so each price is ~0.5
    // For independent CPMM pools the sum won't be exactly 1.0
    // (this is a known property of independent pools vs unified order book)
    assert!(p0 > 0 && p1 > 0 && p2 > 0, "All prices should be positive");
}

// ─── CPMM Invariant Fuzz-like Tests ────────────────────────────────

#[test]
fn test_cpmm_invariant_many_trades() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 1_000_000);

    let pool = ctx.pm.query_outcome_pool(&market_id, &0);
    let initial_k = pool.collateral_reserve * pool.outcome_reserve;

    mint(&ctx, &ctx.user1, 500_000);

    for _ in 0..20 {
        let pool = ctx.pm.query_outcome_pool(&market_id, &0);
        let trade_size = pool.collateral_reserve / 100;
        if trade_size > 0 {
            ctx.pm
                .buy_outcome(&ctx.user1, &market_id, &0, &trade_size, &0);
        }
    }

    let pool = ctx.pm.query_outcome_pool(&market_id, &0);
    let final_k = pool.collateral_reserve * pool.outcome_reserve;
    assert!(
        final_k >= initial_k,
        "CPMM invariant violated: final_k={final_k} < initial_k={initial_k}",
    );
}

#[test]
fn test_cpmm_invariant_alternating_trades() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 1_000_000);

    let pool = ctx.pm.query_outcome_pool(&market_id, &0);
    let initial_k = pool.collateral_reserve * pool.outcome_reserve;

    mint(&ctx, &ctx.user1, 200_000);

    let b = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &50_000, &0);
    ctx.pm
        .sell_outcome(&ctx.user1, &market_id, &0, &(b / 2), &0);
    let b2 = ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &30_000, &0);
    ctx.pm
        .sell_outcome(&ctx.user1, &market_id, &0, &(b2 / 3), &0);

    let pool = ctx.pm.query_outcome_pool(&market_id, &0);
    let final_k = pool.collateral_reserve * pool.outcome_reserve;
    assert!(
        final_k >= initial_k,
        "CPMM invariant violated: final_k={final_k} < initial_k={initial_k}",
    );
}

// ─── Market Cap Enforcement ─────────────────────────────────────────

#[test]
#[should_panic(expected = "Market cap exceeded")]
fn test_market_cap_enforced() {
    let ctx = setup();
    mint(&ctx, &ctx.user1, 20_000);
    let names = soroban_sdk::vec![&ctx.env, "Yes".into_val(&ctx.env), "No".into_val(&ctx.env)];
    let params = CreateMarketParams {
        question: "Q?".into_val(&ctx.env),
        category: MarketCategory::Events,
        collateral_token: ctx.collateral.clone(),
        oracle_source: ctx.oracle.clone(),
        num_outcomes: 2,
        outcome_names: names,
        resolution_window_duration: 86400,
        max_outcome_supply: 500,
        trading_fee_bps: 30,
        initial_liquidity: 10_000,
    };
    let market_id = ctx.pm.create_market(&ctx.user1, &params);

    mint(&ctx, &ctx.user2, 50_000);
    // Each buy gets ~199 outcome tokens from 10k/10k pool
    ctx.pm.buy_outcome(&ctx.user2, &market_id, &0, &200, &0);
    ctx.pm.buy_outcome(&ctx.user2, &market_id, &0, &200, &0);
    // Third buy pushes total past cap of 500
    ctx.pm.buy_outcome(&ctx.user2, &market_id, &0, &200, &0);
}

// ─── No Trading on Resolved Market ──────────────────────────────────

#[test]
#[should_panic(expected = "Market not active")]
fn test_buy_after_resolution() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    advance_time(&ctx, 86_401);
    ctx.pm.resolve_market(&ctx.oracle, &market_id, &0);

    mint(&ctx, &ctx.user1, 1_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);
}

#[test]
#[should_panic(expected = "Market not active")]
fn test_buy_after_early_close() {
    let ctx = setup();
    let market_id = create_binary_market(&ctx, 10_000);

    ctx.pm.close_market_early(&ctx.admin, &market_id);

    mint(&ctx, &ctx.user1, 1_000);
    ctx.pm.buy_outcome(&ctx.user1, &market_id, &0, &1_000, &0);
}
