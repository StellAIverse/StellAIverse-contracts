use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    Address, Env, String,
};

#[contract]
pub struct MockToken;

#[derive(Clone)]
#[contracttype]
pub enum MockTokenKey {
    Balance(Address),
    Allowance(Address, Address),
}

#[contractimpl]
impl MockToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = MockTokenKey::Balance(to);
        let current: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&key, &(current.checked_add(amount).unwrap()));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockTokenKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockTokenKey::Allowance(from, spender))
            .unwrap_or(0)
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiration: u32) {
        from.require_auth();
        env.storage()
            .instance()
            .set(&MockTokenKey::Allowance(from, spender), &amount);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::move_balance(&env, from, to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let key = MockTokenKey::Allowance(from.clone(), spender);
        let allowance: i128 = env.storage().instance().get(&key).unwrap_or(0);
        if allowance < amount {
            panic!("Insufficient allowance");
        }
        env.storage().instance().set(&key, &(allowance - amount));
        Self::move_balance(&env, from, to, amount);
    }

    fn move_balance(env: &Env, from: Address, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("Transfer amount must be positive");
        }
        let from_key = MockTokenKey::Balance(from);
        let from_balance: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        if from_balance < amount {
            panic!("Insufficient balance");
        }
        env.storage()
            .instance()
            .set(&from_key, &(from_balance - amount));

        let to_key = MockTokenKey::Balance(to);
        let to_balance: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&to_key, &(to_balance.checked_add(amount).unwrap()));
    }
}

struct TestContext {
    env: Env,
    membership: MembershipSubscriptionClient<'static>,
    token: MockTokenClient<'static>,
    member: Address,
    referrer: Address,
}

fn setup() -> TestContext {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let admin = Address::generate(&env);
    let member = Address::generate(&env);
    let referrer = Address::generate(&env);

    let membership_id = env.register(MembershipSubscription, ());
    let membership = MembershipSubscriptionClient::new(&env, &membership_id);
    membership.initialize(&admin);

    let token_id = env.register(MockToken, ());
    let token = MockTokenClient::new(&env, &token_id);
    token.mint(&member, &1_000);

    membership.create_tier(
        &admin,
        &1,
        &String::from_str(&env, "Basic"),
        &100,
        &1_000,
        &100,
        &0b001,
        &500,
    );
    membership.create_tier(
        &admin,
        &2,
        &String::from_str(&env, "Pro"),
        &300,
        &1_000,
        &0,
        &0b111,
        &500,
    );
    membership.create_tier(
        &admin,
        &3,
        &String::from_str(&env, "Paid Basic"),
        &100,
        &1_000,
        &0,
        &0b001,
        &500,
    );

    TestContext {
        env,
        membership,
        token,
        member,
        referrer,
    }
}

#[test]
fn trial_subscription_unlocks_benefits_and_renews_with_pull_payment() {
    let ctx = setup();
    let subscription = ctx.membership.subscribe(
        &ctx.member,
        &ctx.token.address,
        &1,
        &true,
        &Some(ctx.referrer.clone()),
    );

    assert_eq!(subscription.status, SubscriptionStatus::Trial);
    assert_eq!(ctx.token.balance(&ctx.member), 1_000);
    assert!(ctx.membership.has_benefit(&ctx.member, &0b001));

    ctx.env.ledger().set_timestamp(1_100);
    ctx.token
        .approve(&ctx.member, &ctx.membership.address, &100, &10_000);
    let result = ctx.membership.renew_subscription(&ctx.member);

    assert_eq!(result.charged_amount, 100);
    assert_eq!(result.referral_reward, 5);
    assert_eq!(result.status, SubscriptionStatus::Active);
    assert_eq!(ctx.token.balance(&ctx.member), 900);
    assert_eq!(ctx.token.balance(&ctx.referrer), 5);
    assert_eq!(ctx.token.balance(&ctx.membership.address), 95);
    assert_eq!(ctx.membership.referral_rewards(&ctx.referrer), 5);

    let analytics = ctx.membership.get_analytics();
    assert_eq!(analytics.total_subscriptions, 1);
    assert_eq!(analytics.active_subscriptions, 1);
    assert_eq!(analytics.total_revenue, 100);
    assert_eq!(analytics.total_referral_rewards, 5);
}

#[test]
fn tier_changes_charge_or_refund_prorated_remaining_value() {
    let ctx = setup();
    ctx.membership
        .subscribe(&ctx.member, &ctx.token.address, &3, &true, &None::<Address>);
    assert_eq!(ctx.token.balance(&ctx.member), 900);

    ctx.env.ledger().set_timestamp(1_500);
    let upgrade_quote = ctx.membership.quote_tier_change(&ctx.member, &2);
    assert_eq!(upgrade_quote.remaining_credit, 50);
    assert_eq!(upgrade_quote.charge_amount, 250);

    let upgrade = ctx.membership.change_tier(&ctx.member, &2);
    assert_eq!(upgrade.old_tier_id, 3);
    assert_eq!(upgrade.new_tier_id, 2);
    assert_eq!(upgrade.charge_amount, 250);
    assert_eq!(ctx.token.balance(&ctx.member), 650);

    ctx.env.ledger().set_timestamp(1_750);
    let downgrade_quote = ctx.membership.quote_tier_change(&ctx.member, &3);
    assert_eq!(downgrade_quote.remaining_credit, 225);
    assert_eq!(downgrade_quote.refund_amount, 125);

    let downgrade = ctx.membership.change_tier(&ctx.member, &3);
    assert_eq!(downgrade.old_tier_id, 2);
    assert_eq!(downgrade.refund_amount, 125);
    assert_eq!(ctx.token.balance(&ctx.member), 775);
    assert_eq!(ctx.membership.get_subscription(&ctx.member).tier_id, 3);
}

#[test]
fn pause_and_resume_extend_entitlement_window() {
    let ctx = setup();
    ctx.membership
        .subscribe(&ctx.member, &ctx.token.address, &3, &true, &None::<Address>);

    ctx.env.ledger().set_timestamp(1_200);
    ctx.membership.pause_subscription(&ctx.member);
    assert!(!ctx.membership.has_benefit(&ctx.member, &0b001));
    assert_eq!(
        ctx.membership.get_subscription(&ctx.member).status,
        SubscriptionStatus::Paused
    );

    ctx.env.ledger().set_timestamp(1_500);
    let resumed = ctx.membership.resume_subscription(&ctx.member);
    assert_eq!(resumed.status, SubscriptionStatus::Active);
    assert_eq!(resumed.current_period_end, 2_300);
    assert!(ctx.membership.has_benefit(&ctx.member, &0b001));
}

#[test]
fn cancellation_refunds_unused_paid_period() {
    let ctx = setup();
    ctx.membership.subscribe(
        &ctx.member,
        &ctx.token.address,
        &3,
        &false,
        &None::<Address>,
    );

    ctx.env.ledger().set_timestamp(1_250);
    let refund = ctx.membership.cancel_subscription(&ctx.member);

    assert_eq!(refund, 75);
    assert_eq!(ctx.token.balance(&ctx.member), 975);
    assert_eq!(ctx.token.balance(&ctx.membership.address), 25);
    assert!(!ctx.membership.is_active(&ctx.member));

    let analytics = ctx.membership.get_analytics();
    assert_eq!(analytics.cancelled_subscriptions, 1);
    assert_eq!(analytics.active_subscriptions, 0);
    assert_eq!(analytics.total_refunds, 75);
}

#[test]
fn failed_pull_payment_expires_subscription_without_panic() {
    let ctx = setup();
    ctx.membership
        .subscribe(&ctx.member, &ctx.token.address, &1, &true, &None::<Address>);

    ctx.env.ledger().set_timestamp(2_000);
    let result = ctx.membership.renew_subscription(&ctx.member);

    assert_eq!(result.charged_amount, 0);
    assert_eq!(result.status, SubscriptionStatus::Expired);
    assert_eq!(
        ctx.membership.get_subscription(&ctx.member).status,
        SubscriptionStatus::Expired
    );
    assert!(!ctx.membership.get_subscription(&ctx.member).auto_renew);
    assert_eq!(ctx.membership.get_analytics().expired_subscriptions, 1);
}

#[test]
#[should_panic(expected = "Member cannot refer themselves")]
fn member_cannot_self_refer() {
    let ctx = setup();
    ctx.membership.subscribe(
        &ctx.member,
        &ctx.token.address,
        &1,
        &true,
        &Some(ctx.member.clone()),
    );
}
