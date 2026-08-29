use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec,
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
    TokenVestingClient<'static>,
    MockTokenClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let admin = Address::generate(&env);
    let funder = Address::generate(&env);
    let beneficiary = Address::generate(&env);

    let vesting_id = env.register(TokenVesting, ());
    let vesting = TokenVestingClient::new(&env, &vesting_id);
    vesting.initialize(&admin);

    let token_id = env.register(MockToken, ());
    let token = MockTokenClient::new(&env, &token_id);
    token.mint(&funder, &1_000_000);

    (env, vesting, token, admin, funder, beneficiary)
}

#[test]
fn linear_vesting_claim_respects_cliff_and_releases_pro_rata() {
    let (env, vesting, token, _admin, funder, beneficiary) = setup();
    let token_id = token.address.clone();

    let schedule_id = vesting.create_schedule(
        &funder,
        &token_id,
        &beneficiary,
        &1_000,
        &1_000,
        &200,
        &1_000,
        &true,
        &true,
        &1_000,
    );

    assert_eq!(token.balance(&funder), 999_000);
    assert_eq!(token.balance(&vesting.address), 1_000);

    env.ledger().set_timestamp(1_100);
    assert_eq!(vesting.claimable(&schedule_id), 0);

    env.ledger().set_timestamp(1_500);
    assert_eq!(vesting.vested_amount(&schedule_id, &1_500), 500);
    assert_eq!(vesting.claim(&schedule_id, &beneficiary), 500);
    assert_eq!(token.balance(&beneficiary), 500);

    env.ledger().set_timestamp(2_000);
    assert_eq!(vesting.claim(&schedule_id, &beneficiary), 500);
    assert_eq!(token.balance(&beneficiary), 1_000);
    assert_eq!(token.balance(&vesting.address), 0);
    assert!(!vesting.get_schedule(&schedule_id).active);
}

#[test]
fn batch_creation_tracks_multiple_beneficiaries_and_claims() {
    let (env, vesting, token, _admin, funder, beneficiary_one) = setup();
    let beneficiary_two = Address::generate(&env);
    let token_id = token.address.clone();

    let beneficiaries = Vec::from_array(&env, [beneficiary_one.clone(), beneficiary_two.clone()]);
    let amounts = Vec::from_array(&env, [200i128, 300i128]);
    let schedule_ids = vesting.create_schedules_batch(
        &funder,
        &token_id,
        &beneficiaries,
        &amounts,
        &1_000,
        &0,
        &500,
        &false,
        &false,
        &0,
    );

    assert_eq!(schedule_ids.len(), 2);
    assert_eq!(token.balance(&vesting.address), 500);
    assert_eq!(
        vesting
            .get_schedules_for_beneficiary(&beneficiary_one)
            .len(),
        1
    );
    assert_eq!(
        vesting
            .get_schedules_for_beneficiary(&beneficiary_two)
            .len(),
        1
    );

    env.ledger().set_timestamp(1_500);
    assert_eq!(
        vesting.claim(&schedule_ids.get_unchecked(0), &beneficiary_one),
        200
    );
    assert_eq!(
        vesting.claim(&schedule_ids.get_unchecked(1), &beneficiary_two),
        300
    );
    assert_eq!(token.balance(&beneficiary_one), 200);
    assert_eq!(token.balance(&beneficiary_two), 300);
    assert_eq!(token.balance(&vesting.address), 0);
}

#[test]
fn early_withdrawal_pays_vested_tokens_and_returns_penalty_to_funder() {
    let (env, vesting, token, _admin, funder, beneficiary) = setup();
    let token_id = token.address.clone();

    let schedule_id = vesting.create_schedule(
        &funder,
        &token_id,
        &beneficiary,
        &1_000,
        &1_000,
        &0,
        &1_000,
        &false,
        &true,
        &2_500,
    );

    env.ledger().set_timestamp(1_250);
    let result = vesting.early_withdraw(&schedule_id, &beneficiary);

    assert_eq!(result.vested_paid, 250);
    assert_eq!(result.unvested_paid, 563);
    assert_eq!(result.penalty_amount, 187);
    assert_eq!(result.total_paid, 813);
    assert_eq!(token.balance(&beneficiary), 813);
    assert_eq!(token.balance(&funder), 999_187);
    assert_eq!(token.balance(&vesting.address), 0);
    assert!(!vesting.get_schedule(&schedule_id).active);
}

#[test]
fn admin_can_modify_beneficiary_terms_and_revoke_unvested_amount() {
    let (env, vesting, token, admin, funder, old_beneficiary) = setup();
    let new_beneficiary = Address::generate(&env);
    let token_id = token.address.clone();

    let schedule_id = vesting.create_schedule(
        &funder,
        &token_id,
        &old_beneficiary,
        &1_000,
        &1_000,
        &500,
        &1_000,
        &true,
        &true,
        &2_500,
    );

    let modified = vesting.modify_schedule(
        &admin,
        &schedule_id,
        &Some(new_beneficiary.clone()),
        &Some(2_000),
        &Some(100),
        &Some(1_000),
        &Some(false),
        &Some(1_000),
    );

    assert_eq!(modified.beneficiary, new_beneficiary);
    assert_eq!(modified.start_time, 2_000);
    assert_eq!(modified.cliff_time, 2_100);
    assert_eq!(modified.end_time, 3_000);
    assert_eq!(modified.early_penalty_bps, 1_000);
    assert!(!modified.early_withdrawal_allowed);
    assert_eq!(
        vesting
            .get_schedules_for_beneficiary(&old_beneficiary)
            .len(),
        0
    );
    assert_eq!(
        vesting
            .get_schedules_for_beneficiary(&new_beneficiary)
            .len(),
        1
    );

    env.ledger().set_timestamp(2_500);
    assert_eq!(vesting.revoke_schedule(&admin, &schedule_id), 500);
    assert_eq!(token.balance(&funder), 999_500);
    assert_eq!(vesting.claim(&schedule_id, &new_beneficiary), 500);
    assert_eq!(token.balance(&new_beneficiary), 500);
    assert_eq!(token.balance(&vesting.address), 0);
}

#[test]
#[should_panic(expected = "Early withdrawal disabled")]
fn early_withdrawal_rejects_disabled_schedules() {
    let (env, vesting, token, _admin, funder, beneficiary) = setup();
    let token_id = token.address.clone();
    let schedule_id = vesting.create_schedule(
        &funder,
        &token_id,
        &beneficiary,
        &1_000,
        &1_000,
        &0,
        &1_000,
        &false,
        &false,
        &0,
    );

    env.ledger().set_timestamp(1_100);
    vesting.early_withdraw(&schedule_id, &beneficiary);
}
