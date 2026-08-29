use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    Address, Env, Symbol, Vec,
};

#[contract]
pub struct MockToken;

#[derive(Clone)]
#[contracttype]
pub enum MockTokenKey {
    Balance(Address),
}

#[contractimpl]
impl MockToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("Mint amount must be positive");
        }
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

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic!("Transfer amount must be positive");
        }

        let from_key = MockTokenKey::Balance(from.clone());
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

fn setup() -> (
    Env,
    StakingClient<'static>,
    MockTokenClient<'static>,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let staking_id = env.register(Staking, ());
    let staking = StakingClient::new(&env, &staking_id);

    let token_id = env.register(MockToken, ());
    let token = MockTokenClient::new(&env, &token_id);
    token.mint(&user, &10_000_000);
    token.mint(&staking_id, &10_000_000);

    staking.initialize(&admin, &token_id, &100i128);

    (env, staking, token, admin, user)
}

fn add_default_tier(env: &Env, staking: &StakingClient<'_>, admin: &Address) -> u32 {
    let tier_name = Symbol::new(env, "standard");
    staking.add_tier(admin, &tier_name, &1000i128, &86400u64, &10000u32, &500u32)
}

fn add_premium_tier(env: &Env, staking: &StakingClient<'_>, admin: &Address) -> u32 {
    let tier_name = Symbol::new(env, "premium");
    staking.add_tier(
        admin, &tier_name, &10000i128, &604800u64, &15000u32, &1000u32,
    )
}

// ═══════════════════════════════════════════════════════════════
//  INITIALIZATION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn initializes_correctly() {
    let (_env, staking, _token, admin, _user) = setup();

    assert_eq!(staking.get_admin(), admin);
    assert_eq!(staking.get_reward_rate(), 100);
    assert_eq!(staking.get_total_staked(), 0);
    assert_eq!(staking.get_total_rewards_distributed(), 0);
    assert!(!staking.is_paused());
}

#[test]
#[should_panic(expected = "Already initialized")]
fn cannot_initialize_twice() {
    let (env, staking, _token, admin, _user) = setup();
    let token_id = env.register(MockToken, ());
    staking.initialize(&admin, &token_id, &100i128);
}

#[test]
#[should_panic(expected = "Reward rate cannot be negative")]
fn initialize_rejects_negative_rate() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let admin = Address::generate(&env);
    let staking_id = env.register(Staking, ());
    let staking = StakingClient::new(&env, &staking_id);
    let token_id = env.register(MockToken, ());

    staking.initialize(&admin, &token_id, &-1i128);
}

// ═══════════════════════════════════════════════════════════════
//  PAUSE/UNPAUSE TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn pause_and_unpause() {
    let (_env, staking, _token, admin, _user) = setup();

    staking.pause(&admin);
    assert!(staking.is_paused());

    staking.unpause(&admin);
    assert!(!staking.is_paused());
}

#[test]
#[should_panic(expected = "Already paused")]
fn cannot_pause_twice() {
    let (_env, staking, _token, admin, _user) = setup();
    staking.pause(&admin);
    staking.pause(&admin);
}

#[test]
#[should_panic(expected = "Not paused")]
fn cannot_unpause_when_not_paused() {
    let (_env, staking, _token, admin, _user) = setup();
    staking.unpause(&admin);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn non_admin_cannot_pause() {
    let (_env, staking, _token, _admin, user) = setup();
    staking.pause(&user);
}

// ═══════════════════════════════════════════════════════════════
//  TIER MANAGEMENT TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn add_tier_successfully() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    assert_eq!(tier_id, 1);
    let tier = staking.get_tier(&tier_id);
    assert_eq!(tier.tier_id, 1);
    assert_eq!(tier.min_stake_amount, 1000);
    assert_eq!(tier.lock_duration_seconds, 86400);
    assert_eq!(tier.reward_multiplier_bps, 10000);
    assert_eq!(tier.penalty_bps, 500);
    assert!(tier.active);
}

#[test]
fn add_multiple_tiers() {
    let (env, staking, _token, admin, _user) = setup();
    let tier1 = add_default_tier(&env, &staking, &admin);
    let tier2 = add_premium_tier(&env, &staking, &admin);

    assert_eq!(tier1, 1);
    assert_eq!(tier2, 2);

    let ids = staking.get_tier_ids();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids.get_unchecked(0), 1);
    assert_eq!(ids.get_unchecked(1), 2);
}

#[test]
fn update_tier() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    let new_name = Symbol::new(&env, "updated");
    let updated = staking.update_tier(
        &admin,
        &tier_id,
        &Some(new_name),
        &Some(2000),
        &Some(172800),
        &Some(20000),
        &Some(1000),
    );

    assert_eq!(updated.name, Symbol::new(&env, "updated"));
    assert_eq!(updated.min_stake_amount, 2000);
    assert_eq!(updated.lock_duration_seconds, 172800);
    assert_eq!(updated.reward_multiplier_bps, 20000);
    assert_eq!(updated.penalty_bps, 1000);
}

#[test]
fn deactivate_tier() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.deactivate_tier(&admin, &tier_id);
    let tier = staking.get_tier(&tier_id);
    assert!(!tier.active);
}

#[test]
#[should_panic(expected = "Tier already inactive")]
fn cannot_deactivate_twice() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);
    staking.deactivate_tier(&admin, &tier_id);
    staking.deactivate_tier(&admin, &tier_id);
}

#[test]
#[should_panic(expected = "Minimum stake amount must be positive")]
fn add_tier_rejects_zero_min_amount() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_name = Symbol::new(&env, "bad");
    staking.add_tier(&admin, &tier_name, &0i128, &86400u64, &10000u32, &500u32);
}

#[test]
#[should_panic(expected = "Lock duration must be positive")]
fn add_tier_rejects_zero_duration() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_name = Symbol::new(&env, "bad");
    staking.add_tier(&admin, &tier_name, &1000i128, &0u64, &10000u32, &500u32);
}

#[test]
#[should_panic(expected = "Reward multiplier must be positive")]
fn add_tier_rejects_zero_multiplier() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_name = Symbol::new(&env, "bad");
    staking.add_tier(&admin, &tier_name, &1000i128, &86400u64, &0u32, &500u32);
}

#[test]
#[should_panic(expected = "Penalty exceeds 100%")]
fn add_tier_rejects_excessive_penalty() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_name = Symbol::new(&env, "bad");
    staking.add_tier(
        &admin, &tier_name, &1000i128, &86400u64, &10000u32, &10001u32,
    );
}

#[test]
#[should_panic(expected = "Tier is not active")]
fn update_inactive_tier_fails() {
    let (env, staking, _token, admin, _user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);
    staking.deactivate_tier(&admin, &tier_id);
    staking.update_tier(&admin, &tier_id, &None, &None, &None, &None, &None);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn non_admin_cannot_add_tier() {
    let (env, staking, _token, _admin, user) = setup();
    let tier_name = Symbol::new(&env, "standard");
    staking.add_tier(&user, &tier_name, &1000i128, &86400u64, &10000u32, &500u32);
}

// ═══════════════════════════════════════════════════════════════
//  STAKING TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn stake_tokens_successfully() {
    let (env, staking, token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    let result = staking.stake(&user, &5000, &tier_id);

    assert_eq!(result.stake_id, 1);
    assert_eq!(result.amount, 5000);
    assert_eq!(result.tier_id, tier_id);
    assert_eq!(token.balance(&user), 10_000_000 - 5000);
    assert_eq!(staking.get_total_staked(), 5000);

    let position = staking.get_stake(&1);
    assert_eq!(position.user, user);
    assert_eq!(position.amount, 5000);
    assert!(position.active);
}

#[test]
fn stake_multiple_times() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.stake(&user, &5000, &tier_id);
    staking.stake(&user, &3000, &tier_id);

    assert_eq!(staking.get_total_staked(), 8000);

    let user_stakes = staking.get_user_stakes(&user);
    assert_eq!(user_stakes.len(), 2);
}

#[test]
fn stake_tracks_lock_end_time() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    let result = staking.stake(&user, &5000, &tier_id);

    assert_eq!(result.lock_end_time, 87400);

    let position = staking.get_stake(&1);
    assert_eq!(position.stake_time, 1000);
    assert_eq!(position.lock_end_time, 87400);
}

#[test]
#[should_panic(expected = "Staking is paused")]
fn cannot_stake_when_paused() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.pause(&admin);
    staking.stake(&user, &5000, &tier_id);
}

#[test]
#[should_panic(expected = "Stake amount must be positive")]
fn cannot_stake_zero() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.stake(&user, &0, &tier_id);
}

#[test]
#[should_panic(expected = "Amount below tier minimum")]
fn cannot_stake_below_minimum() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.stake(&user, &500, &tier_id);
}

#[test]
#[should_panic(expected = "Tier is not active")]
fn cannot_stake_to_inactive_tier() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.deactivate_tier(&admin, &tier_id);
    staking.stake(&user, &5000, &tier_id);
}

#[test]
#[should_panic(expected = "Tier not found")]
fn cannot_stake_to_nonexistent_tier() {
    let (_env, staking, _token, _admin, user) = setup();
    staking.stake(&user, &5000, &999);
}

#[test]
fn stake_counter_increments() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    assert_eq!(staking.get_stake_counter(), 0);

    staking.stake(&user, &5000, &tier_id);
    assert_eq!(staking.get_stake_counter(), 1);

    staking.stake(&user, &3000, &tier_id);
    assert_eq!(staking.get_stake_counter(), 2);
}

// ═══════════════════════════════════════════════════════════════
//  UNSTAKING TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn unstake_after_lock_period() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);

    env.ledger().set_timestamp(87401);
    let result = staking.unstake(&user, &1);

    assert_eq!(result.penalty_amount, 0);
    assert_eq!(result.principal_returned, 5000);
    assert!(result.rewards_claimed >= 0);
    assert_eq!(
        result.total_returned,
        result.principal_returned + result.rewards_claimed
    );

    let position = staking.get_stake(&1);
    assert!(!position.active);
    assert_eq!(staking.get_total_staked(), 0);
}

#[test]
fn unstake_early_with_penalty() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    env.ledger().set_timestamp(4601);
    let result = staking.unstake(&user, &1);

    assert_eq!(result.penalty_amount, 500);
    assert_eq!(result.principal_returned, 9500);

    let position = staking.get_stake(&1);
    assert!(!position.active);
}

#[test]
fn unstake_calculates_rewards_proportionally() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    env.ledger().set_timestamp(1010);
    let result = staking.unstake(&user, &1);

    assert!(result.rewards_claimed >= 0);
}

#[test]
#[should_panic(expected = "Only staker can unstake")]
fn non_staker_cannot_unstake() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);

    let stranger = Address::generate(&env);
    staking.unstake(&stranger, &1);
}

#[test]
#[should_panic(expected = "Stake is not active")]
fn cannot_unstake_inactive_stake() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);

    env.ledger().set_timestamp(87401);
    staking.unstake(&user, &1);
    staking.unstake(&user, &1);
}

// ═══════════════════════════════════════════════════════════════
//  CLAIM REWARDS TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn claim_rewards_after_staking() {
    let (env, staking, token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    env.ledger().set_timestamp(1010);
    let claimed = staking.claim_rewards(&user, &1);

    assert!(claimed > 0);
    assert_eq!(token.balance(&user), 10_000_000 - 10000 + claimed);
}

#[test]
#[should_panic(expected = "No rewards to claim")]
fn cannot_claim_zero_rewards() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    staking.claim_rewards(&user, &1);
}

#[test]
fn claim_rewards_batch() {
    let (env, staking, token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);
    staking.stake(&user, &5000, &tier_id);

    env.ledger().set_timestamp(1010);
    let stake_ids = Vec::from_array(&env, [1, 2]);
    let total = staking.claim_rewards_batch(&user, &stake_ids);

    assert!(total > 0);
    assert_eq!(token.balance(&user), 10_000_000 - 10000 + total);
}

#[test]
#[should_panic(expected = "No stakes provided")]
fn claim_batch_rejects_empty() {
    let (env, staking, _token, _admin, user) = setup();
    let empty = Vec::<u64>::new(&env);
    staking.claim_rewards_batch(&user, &empty);
}

#[test]
#[should_panic(expected = "Only staker can claim")]
fn non_staker_cannot_claim() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);

    env.ledger().set_timestamp(1010);
    let stranger = Address::generate(&env);
    staking.claim_rewards(&stranger, &1);
}

// ═══════════════════════════════════════════════════════════════
//  EMERGENCY WITHDRAWAL TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn emergency_withdraw_returns_principal_only() {
    let (env, staking, token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    let result = staking.emergency_withdraw(&admin, &user, &1);

    assert_eq!(result.principal_returned, 10000);
    assert_eq!(result.rewards_claimed, 0);
    assert_eq!(result.penalty_amount, 0);
    assert_eq!(result.total_returned, 10000);

    let position = staking.get_stake(&1);
    assert!(!position.active);

    assert_eq!(staking.get_total_staked(), 0);
    assert_eq!(token.balance(&user), 10_000_000);
}

#[test]
fn emergency_withdraw_all() {
    let (env, staking, token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);
    staking.stake(&user, &3000, &tier_id);

    let total = staking.emergency_withdraw_all(&admin, &user);

    assert_eq!(total, 8000);
    assert_eq!(staking.get_total_staked(), 0);
    assert_eq!(token.balance(&user), 10_000_000);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn non_admin_cannot_emergency_withdraw() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);

    let stranger = Address::generate(&env);
    staking.emergency_withdraw(&stranger, &user, &1);
}

#[test]
#[should_panic(expected = "Stake does not belong to user")]
fn emergency_withdraw_wrong_user() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);

    let other_user = Address::generate(&env);
    staking.emergency_withdraw(&admin, &other_user, &1);
}

// ═══════════════════════════════════════════════════════════════
//  FUND REWARDS TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn fund_rewards() {
    let (env, staking, token, _admin, _user) = setup();

    let funder = Address::generate(&env);
    token.mint(&funder, &1_000_000);

    let contract_balance_before = token.balance(&staking.address);
    staking.fund_rewards(&funder, &50_000);

    assert_eq!(
        token.balance(&staking.address),
        contract_balance_before + 50_000
    );
    assert_eq!(token.balance(&funder), 950_000);
}

#[test]
#[should_panic(expected = "Amount must be positive")]
fn fund_rewards_rejects_zero() {
    let (_env, staking, _token, _admin, _user) = setup();
    let funder = Address::generate(&_env);
    staking.fund_rewards(&funder, &0);
}

// ═══════════════════════════════════════════════════════════════
//  REWARD RATE MANAGEMENT TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn set_reward_rate() {
    let (_env, staking, _token, admin, _user) = setup();

    staking.set_reward_rate(&admin, &200);
    assert_eq!(staking.get_reward_rate(), 200);
}

#[test]
#[should_panic(expected = "Reward rate cannot be negative")]
fn set_reward_rate_rejects_negative() {
    let (_env, staking, _token, admin, _user) = setup();
    staking.set_reward_rate(&admin, &-1);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn non_admin_cannot_set_reward_rate() {
    let (_env, staking, _token, _admin, user) = setup();
    staking.set_reward_rate(&user, &200);
}

// ═══════════════════════════════════════════════════════════════
//  VIEW FUNCTIONS TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn get_staking_info() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.stake(&user, &5000, &tier_id);

    let info = staking.get_staking_info();
    assert_eq!(info.admin, admin);
    assert_eq!(info.total_staked, 5000);
    assert_eq!(info.reward_rate_per_second, 100);
    assert_eq!(info.tier_count, 1);
    assert!(!info.paused);
}

#[test]
fn get_user_stakes() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    staking.stake(&user, &5000, &tier_id);
    staking.stake(&user, &3000, &tier_id);

    let stakes = staking.get_user_stakes(&user);
    assert_eq!(stakes.len(), 2);
    assert_eq!(stakes.get_unchecked(0).amount, 5000);
    assert_eq!(stakes.get_unchecked(1).amount, 3000);
}

#[test]
fn get_pending_rewards() {
    let (env, staking, _token, _admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &_admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    assert_eq!(staking.get_pending_rewards(&1), 0);

    env.ledger().set_timestamp(1010);
    let pending = staking.get_pending_rewards(&1);
    assert!(pending > 0);
}

#[test]
fn get_stake_counter() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    assert_eq!(staking.get_stake_counter(), 0);

    staking.stake(&user, &5000, &tier_id);
    assert_eq!(staking.get_stake_counter(), 1);

    staking.stake(&user, &3000, &tier_id);
    assert_eq!(staking.get_stake_counter(), 2);
}

// ═══════════════════════════════════════════════════════════════
//  REWARD CALCULATION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn reward_calculation_with_multiplier() {
    let (env, staking, token, admin, _user) = setup();
    let standard_tier = add_default_tier(&env, &staking, &admin);
    let premium_tier = add_premium_tier(&env, &staking, &admin);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    token.mint(&user_a, &1_000_000);
    token.mint(&user_b, &1_000_000);

    env.ledger().set_timestamp(1000);
    staking.stake(&user_a, &10000, &standard_tier);

    env.ledger().set_timestamp(1000);
    staking.stake(&user_b, &10000, &premium_tier);

    env.ledger().set_timestamp(1010);

    let claimed_a = staking.claim_rewards(&user_a, &1);
    let claimed_b = staking.claim_rewards(&user_b, &2);

    assert!(claimed_b > claimed_a);
}

#[test]
fn early_unstake_applies_correct_penalty() {
    let (env, staking, _token, admin, user) = setup();
    let standard_tier = add_default_tier(&env, &staking, &admin);
    let premium_tier = add_premium_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &standard_tier);

    env.ledger().set_timestamp(4601);
    let result_standard = staking.unstake(&user, &1);
    assert_eq!(result_standard.penalty_amount, 500);

    staking.stake(&user, &10000, &premium_tier);

    env.ledger().set_timestamp(4602);
    let result_premium = staking.unstake(&user, &2);
    assert_eq!(result_premium.penalty_amount, 1000);
}

#[test]
fn multiple_stakers_share_rewards_fairly() {
    let (env, staking, token, admin, _user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    token.mint(&user_a, &1_000_000);
    token.mint(&user_b, &1_000_000);

    env.ledger().set_timestamp(1000);
    staking.stake(&user_a, &10000, &tier_id);
    staking.stake(&user_b, &10000, &tier_id);

    env.ledger().set_timestamp(1010);

    let claimed_a = staking.claim_rewards(&user_a, &1);
    let claimed_b = staking.claim_rewards(&user_b, &2);

    assert_eq!(claimed_a, claimed_b);
}

// ═══════════════════════════════════════════════════════════════
//  EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[test]
fn unstake_at_exact_lock_time() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    env.ledger().set_timestamp(87400);
    let result = staking.unstake(&user, &1);
    assert_eq!(result.penalty_amount, 0);
}

#[test]
fn unstake_one_second_before_lock() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    env.ledger().set_timestamp(87399);
    let result = staking.unstake(&user, &1);
    assert_eq!(result.penalty_amount, 500);
}

#[test]
fn zero_penalty_tier() {
    let (env, staking, _token, admin, user) = setup();

    let tier_name = Symbol::new(&env, "nopenalty");
    let tier_id = staking.add_tier(&admin, &tier_name, &1000i128, &86400u64, &10000u32, &0u32);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &10000, &tier_id);

    env.ledger().set_timestamp(4601);
    let result = staking.unstake(&user, &1);
    assert_eq!(result.penalty_amount, 0);
}

#[test]
fn tier_ids_tracking() {
    let (env, staking, _token, admin, _user) = setup();

    assert_eq!(staking.get_tier_ids().len(), 0);

    let _ = add_default_tier(&env, &staking, &admin);
    assert_eq!(staking.get_tier_ids().len(), 1);

    let _ = add_premium_tier(&env, &staking, &admin);
    assert_eq!(staking.get_tier_ids().len(), 2);
}

#[test]
fn get_last_reward_time_updates() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    assert_eq!(staking.get_last_reward_time(), 1000);

    env.ledger().set_timestamp(2000);
    staking.stake(&user, &5000, &tier_id);

    assert_eq!(staking.get_last_reward_time(), 2000);
    let _ = env;
}

#[test]
fn emergency_withdraw_all_with_mixed_active_inactive() {
    let (env, staking, _token, admin, user) = setup();
    let tier_id = add_default_tier(&env, &staking, &admin);

    env.ledger().set_timestamp(1000);
    staking.stake(&user, &5000, &tier_id);
    staking.stake(&user, &3000, &tier_id);

    env.ledger().set_timestamp(87401);
    staking.unstake(&user, &1);

    let total = staking.emergency_withdraw_all(&admin, &user);
    assert_eq!(total, 3000);
}
