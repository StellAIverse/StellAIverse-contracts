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

fn setup() -> (
    Env,
    FeeDistributionClient<'static>,
    MockTokenClient<'static>,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);

    let fee_id = env.register(FeeDistribution, ());
    let fee = FeeDistributionClient::new(&env, &fee_id);

    let token_id = env.register(MockToken, ());
    let token = MockTokenClient::new(&env, &token_id);
    token.mint(&payer, &1_000_000);

    fee.initialize(&admin, &token_id, &500u32, &2_000u32);

    (env, fee, token, admin, payer)
}

fn add_default_categories(
    env: &Env,
    fee: &FeeDistributionClient<'_>,
    admin: &Address,
) -> (Symbol, Symbol, Address, Address) {
    let treasury_id = Symbol::new(env, "treasury");
    let stakers_id = Symbol::new(env, "stakers");
    let treasury = Address::generate(env);
    let stakers = Address::generate(env);

    fee.add_category(admin, &treasury_id, &treasury, &6_000u32);
    fee.add_category(admin, &stakers_id, &stakers, &4_000u32);

    (treasury_id, stakers_id, treasury, stakers)
}

#[test]
fn calculate_fee_applies_bps_rate() {
    let (_env, fee, _token, _admin, _payer) = setup();
    assert_eq!(fee.calculate_fee(&10_000), 500);
    assert_eq!(fee.calculate_fee(&1), 0);
    assert_eq!(fee.calculate_fee(&1_000_000), 50_000);
}

#[test]
fn collect_fee_pulls_tokens_and_tracks_totals() {
    let (_env, fee, token, _admin, payer) = setup();

    let collected = fee.collect_fee(&payer, &10_000);
    assert_eq!(collected, 500);
    assert_eq!(token.balance(&payer), 999_500);
    assert_eq!(token.balance(&fee.address), 500);
    assert_eq!(fee.get_total_collected(), 500);
    assert_eq!(fee.get_pending_distribution(), 500);
}

#[test]
fn exempt_accounts_are_not_charged() {
    let (_env, fee, token, admin, payer) = setup();

    fee.set_exempt(&admin, &payer, &true);
    assert!(fee.is_exempt(&payer));

    let collected = fee.collect_fee(&payer, &10_000);
    assert_eq!(collected, 0);
    assert_eq!(token.balance(&payer), 1_000_000);
    assert_eq!(fee.get_total_collected(), 0);
}

#[test]
fn distribute_splits_fees_by_share_and_handles_rounding_remainder() {
    let (env, fee, _token, admin, payer) = setup();
    let (treasury_id, stakers_id, _treasury, _stakers) = add_default_categories(&env, &fee, &admin);

    // 200_020 at 5% bps rate yields a fee whose 60/40 split doesn't divide evenly.
    fee.collect_fee(&payer, &200_020);
    let pending = fee.get_pending_distribution();
    assert_eq!(pending, 10_001);

    let record = fee.distribute();
    assert_eq!(record.total_amount, 10_001);
    assert_eq!(record.category_count, 2);

    let treasury_share = fee.get_claimable(&treasury_id);
    let stakers_share = fee.get_claimable(&stakers_id);
    // 60% of 10_001 = 6_000 (floor); remainder goes to the last active category.
    assert_eq!(treasury_share, 6_000);
    assert_eq!(stakers_share, 4_001);
    assert_eq!(treasury_share + stakers_share, pending);
}

#[test]
#[should_panic(expected = "Recipient shares are not fully configured")]
fn distribute_requires_full_share_allocation() {
    let (env, fee, _token, admin, payer) = setup();
    let treasury_id = Symbol::new(&env, "treasury");
    let treasury = Address::generate(&env);
    fee.add_category(&admin, &treasury_id, &treasury, &6_000u32);

    fee.collect_fee(&payer, &10_000);
    fee.distribute();
}

#[test]
fn recipients_can_claim_allocated_fees() {
    let (env, fee, token, admin, payer) = setup();
    let (treasury_id, stakers_id, treasury, stakers) = add_default_categories(&env, &fee, &admin);

    fee.collect_fee(&payer, &200_000);
    fee.distribute();

    let claimed = fee.claim(&treasury_id, &treasury);
    assert_eq!(claimed, 6_000);
    assert_eq!(token.balance(&treasury), 6_000);
    assert_eq!(fee.get_claimable(&treasury_id), 0);

    let claimed_stakers = fee.claim(&stakers_id, &stakers);
    assert_eq!(claimed_stakers, 4_000);
    assert_eq!(token.balance(&stakers), 4_000);
    assert_eq!(fee.get_total_claimed(), 10_000);
}

#[test]
fn claim_batch_pays_multiple_categories_in_one_call() {
    let (env, fee, token, admin, payer) = setup();
    let recipient = Address::generate(&env);

    let cat_a = Symbol::new(&env, "cat_a");
    let cat_b = Symbol::new(&env, "cat_b");
    fee.add_category(&admin, &cat_a, &recipient, &5_000u32);
    fee.add_category(&admin, &cat_b, &recipient, &5_000u32);

    fee.collect_fee(&payer, &200_000);
    fee.distribute();

    let ids = Vec::from_array(&env, [cat_a, cat_b]);
    let total = fee.claim_batch(&ids, &recipient);
    assert_eq!(total, 10_000);
    assert_eq!(token.balance(&recipient), 10_000);
}

#[test]
#[should_panic(expected = "Only recipient can claim")]
fn claim_rejects_non_recipient_caller() {
    let (env, fee, _token, admin, payer) = setup();
    let (treasury_id, _stakers_id, _treasury, _stakers) =
        add_default_categories(&env, &fee, &admin);
    let stranger = Address::generate(&env);

    fee.collect_fee(&payer, &200_000);
    fee.distribute();

    fee.claim(&treasury_id, &stranger);
}

#[test]
fn set_fee_rate_allows_small_steps_and_rejects_large_jumps() {
    let (_env, fee, _token, admin, _payer) = setup();
    fee.set_fee_rate(&admin, &900u32);
    assert_eq!(fee.get_fee_rate(), 900);
}

#[test]
#[should_panic(expected = "Rate change exceeds max step")]
fn set_fee_rate_rejects_jump_beyond_step_limit() {
    let (_env, fee, _token, admin, _payer) = setup();
    fee.set_fee_rate(&admin, &1_500u32);
}

#[test]
fn emergency_set_fee_rate_bypasses_step_limit_but_respects_max() {
    let (_env, fee, _token, admin, _payer) = setup();
    fee.emergency_set_fee_rate(&admin, &2_000u32);
    assert_eq!(fee.get_fee_rate(), 2_000);
}

#[test]
#[should_panic(expected = "Fee rate exceeds max fee rate")]
fn emergency_set_fee_rate_still_respects_configured_max() {
    let (_env, fee, _token, admin, _payer) = setup();
    fee.emergency_set_fee_rate(&admin, &2_001u32);
}

#[test]
#[should_panic(expected = "Max fee rate exceeds hard cap")]
fn set_max_fee_rate_rejects_beyond_hard_cap() {
    let (_env, fee, _token, admin, _payer) = setup();
    fee.set_max_fee_rate(&admin, &2_001u32);
}

#[test]
fn pause_blocks_collection_but_not_claims() {
    let (env, fee, token, admin, payer) = setup();
    let (treasury_id, _stakers_id, treasury, _stakers) = add_default_categories(&env, &fee, &admin);

    fee.collect_fee(&payer, &200_000);
    fee.distribute();

    fee.pause(&admin);
    assert!(fee.is_paused());

    let claimed = fee.claim(&treasury_id, &treasury);
    assert_eq!(claimed, 6_000);
    assert_eq!(token.balance(&treasury), 6_000);

    fee.unpause(&admin);
    assert!(!fee.is_paused());
    let _ = env;
}

#[test]
#[should_panic(expected = "Fee collection is paused")]
fn collect_fee_rejected_while_paused() {
    let (_env, fee, _token, admin, payer) = setup();
    fee.pause(&admin);
    fee.collect_fee(&payer, &10_000);
}

#[test]
fn remove_and_reactivate_category_updates_shares() {
    let (env, fee, _token, admin, _payer) = setup();
    let (treasury_id, _stakers_id, _treasury, _stakers) =
        add_default_categories(&env, &fee, &admin);

    assert_eq!(fee.get_total_shares_bps(), 10_000);
    fee.remove_category(&admin, &treasury_id);
    assert_eq!(fee.get_total_shares_bps(), 4_000);
    assert!(!fee.get_category(&treasury_id).active);

    fee.reactivate_category(&admin, &treasury_id, &3_000u32);
    assert_eq!(fee.get_total_shares_bps(), 7_000);
    assert!(fee.get_category(&treasury_id).active);
    let _ = env;
}

#[test]
#[should_panic(expected = "Total shares exceed 100%")]
fn add_category_rejects_shares_over_100_percent() {
    let (env, fee, _token, admin, _payer) = setup();
    add_default_categories(&env, &fee, &admin);

    let extra_id = Symbol::new(&env, "extra");
    let extra = Address::generate(&env);
    fee.add_category(&admin, &extra_id, &extra, &1u32);
}

#[test]
fn distribution_history_is_queryable_for_analytics() {
    let (env, fee, _token, admin, payer) = setup();
    add_default_categories(&env, &fee, &admin);

    fee.collect_fee(&payer, &200_000);
    let first = fee.distribute();

    fee.collect_fee(&payer, &200_000);
    let second = fee.distribute();

    assert_eq!(fee.get_distribution_counter(), 2);

    let history = fee.get_distribution_history(&0u32, &10u32);
    assert_eq!(history.len(), 2);
    assert_eq!(
        history.get_unchecked(0).distribution_id,
        second.distribution_id
    );
    assert_eq!(
        history.get_unchecked(1).distribution_id,
        first.distribution_id
    );
    let _ = env;
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn non_admin_cannot_add_category() {
    let (env, fee, _token, _admin, _payer) = setup();
    let stranger = Address::generate(&env);
    let treasury_id = Symbol::new(&env, "treasury");
    let treasury = Address::generate(&env);
    fee.add_category(&stranger, &treasury_id, &treasury, &6_000u32);
}
