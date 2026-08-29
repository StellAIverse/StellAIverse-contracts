use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, testutils::Ledger, Address, Env,
};

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
    cm_id: Address,
    cm: CollateralManagementClient<'static>,
    collateral_token: Address,
    borrow_token: Address,
    user: Address,
}

fn setup_env() -> TestContext {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let collateral_token = env.register(MockToken, ());
    let borrow_token = env.register(MockToken, ());

    let cm_id = env.register(CollateralManagement, ());
    let cm = CollateralManagementClient::new(&env, &cm_id);

    cm.initialize(&admin, &admin, &None);

    TestContext {
        env,
        admin,
        cm_id,
        cm,
        collateral_token,
        borrow_token,
        user,
    }
}

fn mint_tokens(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    MockTokenClient::new(env, token_id).mint(to, &amount);
}

fn set_price_and_register(ctx: &TestContext) {
    // Set oracle price: 1 collateral token = 2 borrow tokens (scaled by 1_000_000)
    let feed_id = Symbol::new(&ctx.env, "COLL_PRICE");
    ctx.cm.set_price(&ctx.admin, &feed_id, &2_000_000);

    // Register collateral type: 80% LTV, 85% liq threshold, 5% bonus
    ctx.cm.register_collateral_type(
        &ctx.admin,
        &ctx.collateral_token,
        &feed_id,
        &8000,      // ltv_bps
        &8500,      // liq_threshold_bps
        &500,       // liq_bonus_bps
        &0,         // no cap
        &1_000_000, // price_scale
    );
}

// ─── Initialization Tests ──────────────────────────────────────────

#[test]
fn test_initialize() {
    let ctx = setup_env();
    let params = ctx.cm.get_protocol_parameters();
    assert_eq!(params.liq_health_threshold_bps, 10000);
    assert_eq!(params.base_interest_rate_bps, 200);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_double_initialize() {
    let ctx = setup_env();
    ctx.cm.initialize(&ctx.admin, &ctx.admin, &None);
}

// ─── Collateral Type Management Tests ──────────────────────────────

#[test]
fn test_register_collateral_type() {
    let ctx = setup_env();
    let feed_id = Symbol::new(&ctx.env, "BTC_USD");
    ctx.cm.set_price(&ctx.admin, &feed_id, &50_000_000);

    ctx.cm.register_collateral_type(
        &ctx.admin,
        &ctx.collateral_token,
        &feed_id,
        &8000,
        &8500,
        &500,
        &1_000_000,
        &1_000_000,
    );

    let config = ctx.cm.get_collateral_type(&ctx.collateral_token);
    assert_eq!(config.ltv_bps, 8000);
    assert_eq!(config.liq_threshold_bps, 8500);
    assert_eq!(config.liq_bonus_bps, 500);
    assert!(config.is_active);

    let tokens = ctx.cm.get_all_collateral_tokens();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens.get(0).unwrap(), ctx.collateral_token);
}

#[test]
#[should_panic(expected = "Collateral type already exists")]
fn test_duplicate_collateral_type() {
    let ctx = setup_env();
    set_price_and_register(&ctx);
    // Try to register again
    ctx.cm.register_collateral_type(
        &ctx.admin,
        &ctx.collateral_token,
        &Symbol::new(&ctx.env, "BTC2"),
        &8000,
        &8500,
        &500,
        &0,
        &1_000_000,
    );
}

#[test]
#[should_panic(expected = "Invalid LTV parameter")]
fn test_invalid_ltv() {
    let ctx = setup_env();
    ctx.cm.register_collateral_type(
        &ctx.admin,
        &ctx.collateral_token,
        &Symbol::new(&ctx.env, "BTC"),
        &9600, // > MAX_LTV_BPS (9500)
        &9700,
        &500,
        &0,
        &1_000_000,
    );
}

#[test]
fn test_update_collateral_type() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    ctx.cm.update_collateral_type(
        &ctx.admin,
        &ctx.collateral_token,
        &7500, // new ltv
        &8000, // new liq threshold
        &600,  // new bonus
        &500_000,
    );

    let config = ctx.cm.get_collateral_type(&ctx.collateral_token);
    assert_eq!(config.ltv_bps, 7500);
    assert_eq!(config.liq_threshold_bps, 8000);
    assert_eq!(config.liq_bonus_bps, 600);
    assert_eq!(config.collateral_cap, 500_000);
}

#[test]
fn test_deactivate_reactivate_collateral_type() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    ctx.cm
        .deactivate_collateral_type(&ctx.admin, &ctx.collateral_token);
    let config = ctx.cm.get_collateral_type(&ctx.collateral_token);
    assert!(!config.is_active);

    ctx.cm
        .reactivate_collateral_type(&ctx.admin, &ctx.collateral_token);
    let config = ctx.cm.get_collateral_type(&ctx.collateral_token);
    assert!(config.is_active);
}

// ─── Deposit Tests ─────────────────────────────────────────────────

#[test]
fn test_deposit_collateral() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);

    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &5_000);

    let user_coll = ctx.cm.get_user_collateral(&ctx.user, &ctx.collateral_token);
    assert_eq!(user_coll.amount, 5_000);

    let total = ctx.cm.get_total_collateral(&ctx.collateral_token);
    assert_eq!(total, 5_000);
}

#[test]
fn test_deposit_multiple_collateral_types() {
    let ctx = setup_env();
    let collateral_token_2 = ctx.env.register(MockToken, ());

    // Register first collateral
    set_price_and_register(&ctx);

    // Register second collateral with different price
    let feed_id_2 = Symbol::new(&ctx.env, "ETH_PRICE");
    ctx.cm.set_price(&ctx.admin, &feed_id_2, &3_000_000);
    ctx.cm.register_collateral_type(
        &ctx.admin,
        &collateral_token_2,
        &feed_id_2,
        &7500,
        &8000,
        &500,
        &0,
        &1_000_000,
    );

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    mint_tokens(&ctx.env, &collateral_token_2, &ctx.user, 5_000);

    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &3_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &collateral_token_2, &2_000);

    let coll_1 = ctx.cm.get_user_collateral(&ctx.user, &ctx.collateral_token);
    let coll_2 = ctx.cm.get_user_collateral(&ctx.user, &collateral_token_2);
    assert_eq!(coll_1.amount, 3_000);
    assert_eq!(coll_2.amount, 2_000);
}

#[test]
#[should_panic(expected = "Invalid amount")]
fn test_deposit_zero_amount() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &0);
}

#[test]
#[should_panic(expected = "Collateral type inactive")]
fn test_deposit_inactive_collateral() {
    let ctx = setup_env();
    set_price_and_register(&ctx);
    ctx.cm
        .deactivate_collateral_type(&ctx.admin, &ctx.collateral_token);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &5_000);
}

#[test]
#[should_panic(expected = "Collateral cap exceeded")]
fn test_deposit_collateral_cap() {
    let ctx = setup_env();

    let feed_id = Symbol::new(&ctx.env, "BTC_USD");
    ctx.cm.set_price(&ctx.admin, &feed_id, &2_000_000);
    ctx.cm.register_collateral_type(
        &ctx.admin,
        &ctx.collateral_token,
        &feed_id,
        &8000,
        &8500,
        &500,
        &1_000, // cap at 1000
        &1_000_000,
    );

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &500);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &600); // exceeds cap
}

// ─── Borrow Tests ──────────────────────────────────────────────────

#[test]
fn test_borrow_basic() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    // Deposit 1000 collateral at price 2 => value = 2000
    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    // LTV 80% => max borrow = 1600
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);

    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &1000);

    let loan = ctx.cm.get_user_loan(&ctx.user, &0);
    assert_eq!(loan.principal, 1000);
    assert_eq!(loan.total_debt, 1000);
    assert!(!loan.is_liquidated);
    assert!(!loan.is_repaid);

    let user_borrow_balance = MockTokenClient::new(&ctx.env, &ctx.borrow_token).balance(&ctx.user);
    assert_eq!(user_borrow_balance, 1000);
}

#[test]
#[should_panic(expected = "Health factor below liquidation threshold")]
fn test_borrow_exceeds_ltv() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    // Value = 2000, LTV 80% => max 1600
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);

    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &2000); // exceeds max
}

#[test]
#[should_panic(expected = "Invalid amount")]
fn test_borrow_zero_amount() {
    let ctx = setup_env();
    set_price_and_register(&ctx);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &0);
}

// ─── Repay Tests ───────────────────────────────────────────────────

#[test]
fn test_repay_full_loan() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &500);

    // Repay full amount
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.user, 10_000);
    ctx.cm.repay_loan(&ctx.user, &0, &500);

    let loan = ctx.cm.get_user_loan(&ctx.user, &0);
    assert!(loan.is_repaid);
    assert_eq!(loan.total_debt, 0);
}

#[test]
fn test_repay_partial() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.user, 10_000);
    ctx.cm.repay_loan(&ctx.user, &0, &500);

    let loan = ctx.cm.get_user_loan(&ctx.user, &0);
    assert!(!loan.is_repaid);
    assert_eq!(loan.total_debt, 500);
    assert_eq!(loan.principal, 500);
}

#[test]
#[should_panic(expected = "Repayment exceeds outstanding debt")]
fn test_repay_exceeds_debt() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &500);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.user, 10_000);
    ctx.cm.repay_loan(&ctx.user, &0, &1000); // exceeds 500 debt
}

#[test]
#[should_panic(expected = "Loan already repaid")]
fn test_repay_already_repaid() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &500);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.user, 10_000);
    ctx.cm.repay_loan(&ctx.user, &0, &500);
    ctx.cm.repay_loan(&ctx.user, &0, &1);
}

// ─── Withdraw Tests ────────────────────────────────────────────────

#[test]
fn test_withdraw_collateral() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &5_000);

    ctx.cm
        .withdraw_collateral(&ctx.user, &ctx.collateral_token, &2_000);

    let user_coll = ctx.cm.get_user_collateral(&ctx.user, &ctx.collateral_token);
    assert_eq!(user_coll.amount, 3_000);

    let balance = MockTokenClient::new(&ctx.env, &ctx.collateral_token).balance(&ctx.user);
    assert_eq!(balance, 7_000);
}

#[test]
#[should_panic(expected = "Insufficient collateral balance")]
fn test_withdraw_too_much() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1_000);

    ctx.cm
        .withdraw_collateral(&ctx.user, &ctx.collateral_token, &2_000);
}

#[test]
#[should_panic(expected = "Withdrawal would undercollateralize position")]
fn test_withdraw_undercollateralizes() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    // Deposit 1000 at price 2 => value = 2000
    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    // Borrow 1500 (under 80% LTV of 1600)
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &1500);

    // Withdrawing 400 would leave 600 collateral (value 1200), debt 1500 => unhealthy
    ctx.cm
        .withdraw_collateral(&ctx.user, &ctx.collateral_token, &400);
}

// ─── Liquidation Tests ─────────────────────────────────────────────

#[test]
fn test_liquidation() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    let borrower = Address::generate(&ctx.env);
    let liquidator = Address::generate(&ctx.env);

    // Deposit 1000 collateral at price 2 => value = 2000
    mint_tokens(&ctx.env, &ctx.collateral_token, &borrower, 10_000);
    ctx.cm
        .deposit_collateral(&borrower, &ctx.collateral_token, &1000);

    // Borrow 1500 (healthy with LTV 80%: max 1600)
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&borrower, &ctx.borrow_token, &1500);

    // Drop price to make position unhealthy:
    // Old price 2 => new price 1. value = 1000, debt = 1500. HF < 1
    ctx.cm
        .set_price(&ctx.admin, &Symbol::new(&ctx.env, "COLL_PRICE"), &1_000_000);

    // Verify position is now liquidatable
    let hf = ctx.cm.get_health_factor(&borrower);
    assert!(!hf.is_healthy);

    // Liquidator covers 500 debt
    mint_tokens(&ctx.env, &ctx.borrow_token, &liquidator, 10_000);
    ctx.cm.liquidate(&liquidator, &borrower, &0, &500);

    let loan = ctx.cm.get_user_loan(&borrower, &0);
    assert_eq!(loan.total_debt, 1000);

    // Liquidator should receive collateral tokens
    let liq_coll = MockTokenClient::new(&ctx.env, &ctx.collateral_token).balance(&liquidator);
    assert!(liq_coll > 0);
}

#[test]
#[should_panic(expected = "Health factor below liquidation threshold")]
fn test_liquidation_not_undercollateralized() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    let borrower = Address::generate(&ctx.env);
    let liquidator = Address::generate(&ctx.env);

    mint_tokens(&ctx.env, &ctx.collateral_token, &borrower, 10_000);
    ctx.cm
        .deposit_collateral(&borrower, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&borrower, &ctx.borrow_token, &500);

    // Position is healthy (value 2000, debt 500)
    mint_tokens(&ctx.env, &ctx.borrow_token, &liquidator, 10_000);
    ctx.cm.liquidate(&liquidator, &borrower, &0, &500);
}

#[test]
#[should_panic(expected = "Loan already repaid")]
fn test_liquidation_already_repaid() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    let borrower = Address::generate(&ctx.env);
    let liquidator = Address::generate(&ctx.env);

    mint_tokens(&ctx.env, &ctx.collateral_token, &borrower, 10_000);
    ctx.cm
        .deposit_collateral(&borrower, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&borrower, &ctx.borrow_token, &500);

    // Repay first
    mint_tokens(&ctx.env, &ctx.borrow_token, &borrower, 10_000);
    ctx.cm.repay_loan(&borrower, &0, &500);

    // Then try to liquidate
    mint_tokens(&ctx.env, &ctx.borrow_token, &liquidator, 10_000);
    ctx.cm.liquidate(&liquidator, &borrower, &0, &500);
}

// ─── Health Factor Tests ───────────────────────────────────────────

#[test]
fn test_health_factor_no_debt() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    let hf = ctx.cm.get_health_factor(&ctx.user);
    assert!(hf.is_healthy);
}

#[test]
fn test_health_factor_healthy() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &500);

    let hf = ctx.cm.get_health_factor(&ctx.user);
    assert!(hf.is_healthy);
    // Value = 2000 (1000 * 2), debt = 500
    // hf_bps = 2000 * 8500 / 500 = 34000
    assert_eq!(hf.health_factor_bps, 34_000);
}

#[test]
fn test_health_factor_unhealthy_after_price_drop() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &1500);

    // Drop price: value goes from 2000 to 1000, debt 1500
    ctx.cm
        .set_price(&ctx.admin, &Symbol::new(&ctx.env, "COLL_PRICE"), &1_000_000);

    let hf = ctx.cm.get_health_factor(&ctx.user);
    assert!(!hf.is_healthy);
    // Value = 1000 (1000 * 1), debt = 1500
    // hf_bps = 1000 * 8500 / 1500 = 5666
    assert_eq!(hf.health_factor_bps, 5666);
}

// ─── Interest Rate Tests ───────────────────────────────────────────

#[test]
fn test_interest_rate_zero_utilization() {
    let ctx = setup_env();
    set_price_and_register(&ctx);
    let rate = ctx.cm.get_interest_rate(&ctx.borrow_token);
    assert_eq!(rate, 200); // base rate
}

#[test]
fn test_can_liquidate() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    let borrower = Address::generate(&ctx.env);

    mint_tokens(&ctx.env, &ctx.collateral_token, &borrower, 10_000);
    ctx.cm
        .deposit_collateral(&borrower, &ctx.collateral_token, &1000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&borrower, &ctx.borrow_token, &1500);

    // Healthy
    assert!(!ctx.cm.can_liquidate(&borrower, &0));

    // Drop price
    ctx.cm
        .set_price(&ctx.admin, &Symbol::new(&ctx.env, "COLL_PRICE"), &1_000_000);
    assert!(ctx.cm.can_liquidate(&borrower, &0));
}

// ─── Pause/Unpause Tests ───────────────────────────────────────────

#[test]
fn test_pause_unpause() {
    let ctx = setup_env();
    ctx.cm.pause(&ctx.admin);

    let result = ctx
        .cm
        .try_deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);
    assert!(result.is_err()); // should fail

    ctx.cm.unpause(&ctx.admin);
    // After unpausing, deposit should work (but collateral type needs to exist)
    set_price_and_register(&ctx);
    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_admin_cannot_pause() {
    let ctx = setup_env();
    let non_admin = Address::generate(&ctx.env);
    ctx.cm.pause(&non_admin);
}

// ─── Protocol Parameters Tests ─────────────────────────────────────

#[test]
fn test_set_protocol_parameters() {
    let ctx = setup_env();
    let params = ProtocolParams {
        debt_ceiling: 1_000_000,
        liq_health_threshold_bps: 11000,
        base_interest_rate_bps: 500,
        interest_slope1_bps: 300,
        interest_slope2_bps: 8000,
        optimal_utilization_bps: 7500,
        max_borrow_per_user: 100_000,
        max_collateral_per_user: 200_000,
    };
    ctx.cm.set_protocol_parameters(&ctx.admin, &params);

    let stored = ctx.cm.get_protocol_parameters();
    assert_eq!(stored.debt_ceiling, 1_000_000);
    assert_eq!(stored.liq_health_threshold_bps, 11000);
    assert_eq!(stored.base_interest_rate_bps, 500);
    assert_eq!(stored.max_borrow_per_user, 100_000);
}

// ─── Debt Ceiling Tests ────────────────────────────────────────────

#[test]
#[should_panic(expected = "Protocol debt ceiling exceeded")]
fn test_debt_ceiling_enforced() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    // Set debt ceiling of 500
    let params = ProtocolParams {
        debt_ceiling: 500,
        ..ctx.cm.get_protocol_parameters()
    };
    ctx.cm.set_protocol_parameters(&ctx.admin, &params);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &10_000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &600);
}

// ─── Interest Accrual Tests ────────────────────────────────────────

#[test]
fn test_interest_accrual() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &10_000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &1000);

    // Advance ledger time by 1 year
    ctx.env.ledger().with_mut(|li| {
        li.timestamp += 365 * 24 * 60 * 60;
    });

    ctx.cm.accrue_all_interest(&ctx.user);

    let loan = ctx.cm.get_user_loan(&ctx.user, &0);
    // With base rate 200bps (2%) + some slope, should have accrued interest
    assert!(loan.accrued_interest > 0);
    assert!(loan.total_debt > loan.principal);
}

// ─── View Function Tests ───────────────────────────────────────────

#[test]
fn test_get_user_loan_ids() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &10_000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);

    let ids = ctx.cm.get_user_loan_ids(&ctx.user);
    assert_eq!(ids.len(), 0);

    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &100);

    let ids = ctx.cm.get_user_loan_ids(&ctx.user);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids.get(0).unwrap(), 0);

    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &200);

    let ids = ctx.cm.get_user_loan_ids(&ctx.user);
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_get_total_protocol_debt() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &10_000);

    assert_eq!(ctx.cm.get_total_protocol_debt(&ctx.borrow_token), 0);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &500);

    assert_eq!(ctx.cm.get_total_protocol_debt(&ctx.borrow_token), 500);
}

// ─── Multiple Loan Tests ───────────────────────────────────────────

#[test]
fn test_multiple_loans() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &5_000);

    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);

    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &1_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &2_000);

    let ids = ctx.cm.get_user_loan_ids(&ctx.user);
    assert_eq!(ids.len(), 2);

    let loan_0 = ctx.cm.get_user_loan(&ctx.user, &0);
    let loan_1 = ctx.cm.get_user_loan(&ctx.user, &1);
    assert_eq!(loan_0.principal, 1_000);
    assert_eq!(loan_1.principal, 2_000);

    // Total debt
    assert_eq!(ctx.cm.get_total_protocol_debt(&ctx.borrow_token), 3_000);

    // Repay first loan
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.user, 10_000);
    ctx.cm.repay_loan(&ctx.user, &0, &1_000);

    let loan_0 = ctx.cm.get_user_loan(&ctx.user, &0);
    assert!(loan_0.is_repaid);
    assert_eq!(ctx.cm.get_total_protocol_debt(&ctx.borrow_token), 2_000);
}

// ─── Price Management Tests ────────────────────────────────────────

#[test]
fn test_set_price() {
    let ctx = setup_env();
    let feed_id = Symbol::new(&ctx.env, "TEST_FEED");
    ctx.cm.set_price(&ctx.admin, &feed_id, &42_000_000);

    // Price should be used for collateral valuation
    set_price_and_register(&ctx);
    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &1000);

    let hf = ctx.cm.get_health_factor(&ctx.user);
    assert!(hf.is_healthy);
}

// ─── Full Lifecycle Test ───────────────────────────────────────────

#[test]
fn test_full_lifecycle_deposit_borrow_repay_withdraw() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    // 1. Deposit collateral
    mint_tokens(&ctx.env, &ctx.collateral_token, &ctx.user, 10_000);
    ctx.cm
        .deposit_collateral(&ctx.user, &ctx.collateral_token, &2_000);
    assert_eq!(
        ctx.cm
            .get_user_collateral(&ctx.user, &ctx.collateral_token)
            .amount,
        2_000
    );

    // 2. Borrow
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&ctx.user, &ctx.borrow_token, &1_000);
    assert_eq!(ctx.cm.get_user_loan_ids(&ctx.user).len(), 1);

    // 3. Check health factor
    let hf = ctx.cm.get_health_factor(&ctx.user);
    assert!(hf.is_healthy);

    // 4. Repay
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.user, 10_000);
    ctx.cm.repay_loan(&ctx.user, &0, &1_000);
    assert!(ctx.cm.get_user_loan(&ctx.user, &0).is_repaid);

    // 5. Withdraw collateral
    ctx.cm
        .withdraw_collateral(&ctx.user, &ctx.collateral_token, &2_000);
    assert_eq!(
        ctx.cm
            .get_user_collateral(&ctx.user, &ctx.collateral_token)
            .amount,
        0
    );

    let balance = MockTokenClient::new(&ctx.env, &ctx.collateral_token).balance(&ctx.user);
    assert_eq!(balance, 10_000);
}

// ─── Liquidation Lifecycle Test ────────────────────────────────────

#[test]
fn test_liquidation_lifecycle() {
    let ctx = setup_env();
    set_price_and_register(&ctx);

    let borrower = Address::generate(&ctx.env);
    let liquidator = Address::generate(&ctx.env);

    // 1. Borrower deposits 1000 collateral (value = 2000)
    mint_tokens(&ctx.env, &ctx.collateral_token, &borrower, 10_000);
    ctx.cm
        .deposit_collateral(&borrower, &ctx.collateral_token, &1000);

    // 2. Borrower borrows 1500 (under 80% LTV)
    mint_tokens(&ctx.env, &ctx.borrow_token, &ctx.cm_id, 10_000);
    ctx.cm.borrow(&borrower, &ctx.borrow_token, &1500);

    // 3. Price drops, making position unhealthy
    ctx.cm
        .set_price(&ctx.admin, &Symbol::new(&ctx.env, "COLL_PRICE"), &1_000_000);

    let hf = ctx.cm.get_health_factor(&borrower);
    assert!(!hf.is_healthy);
    assert!(ctx.cm.can_liquidate(&borrower, &0));

    // 4. Liquidator covers 750 debt
    let liq_token_before =
        MockTokenClient::new(&ctx.env, &ctx.collateral_token).balance(&liquidator);
    mint_tokens(&ctx.env, &ctx.borrow_token, &liquidator, 10_000);

    ctx.cm.liquidate(&liquidator, &borrower, &0, &750);

    // 5. Check results
    let loan = ctx.cm.get_user_loan(&borrower, &0);
    assert_eq!(loan.total_debt, 750);

    let liq_token_after =
        MockTokenClient::new(&ctx.env, &ctx.collateral_token).balance(&liquidator);
    assert!(liq_token_after > liq_token_before); // Received collateral

    let borrower_coll = ctx.cm.get_user_collateral(&borrower, &ctx.collateral_token);
    assert!(borrower_coll.amount < 1000); // Lost some collateral
}
