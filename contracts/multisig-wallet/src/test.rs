use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    Address, Env, String, Vec,
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

struct TestContext {
    env: Env,
    wallet: MultiSigWalletClient<'static>,
    token: MockTokenClient<'static>,
    admin: Address,
    signers: Vec<Address>,
    recipient: Address,
}

fn setup(required_confirmations: u32, daily_limit: i128) -> TestContext {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(DAY_SECONDS * 10);

    let admin = Address::generate(&env);
    let signer_one = Address::generate(&env);
    let signer_two = Address::generate(&env);
    let signer_three = Address::generate(&env);
    let signers = Vec::from_array(
        &env,
        [signer_one.clone(), signer_two.clone(), signer_three.clone()],
    );
    let depositor = Address::generate(&env);
    let recipient = Address::generate(&env);

    let wallet_id = env.register(MultiSigWallet, ());
    let wallet = MultiSigWalletClient::new(&env, &wallet_id);
    wallet.initialize(&admin, &signers, &required_confirmations, &daily_limit);

    let token_id = env.register(MockToken, ());
    let token = MockTokenClient::new(&env, &token_id);
    token.mint(&depositor, &10_000);
    wallet.deposit(&depositor, &token.address, &5_000);

    TestContext {
        env,
        wallet,
        token,
        admin,
        signers,
        recipient,
    }
}

fn submit_payment(ctx: &TestContext, amount: i128) -> u64 {
    ctx.wallet.submit_transaction(
        &ctx.signers.get_unchecked(0),
        &ctx.token.address,
        &ctx.recipient,
        &amount,
        &String::from_str(&ctx.env, "treasury payment"),
    )
}

#[test]
fn confirmed_transaction_executes_token_transfer_and_records_history() {
    let ctx = setup(2, 1_000);
    let tx_id = submit_payment(&ctx, 400);

    assert_eq!(ctx.wallet.confirmation_count(&tx_id), 1);
    assert_eq!(ctx.wallet.get_confirmations(&tx_id).len(), 1);

    assert_eq!(
        ctx.wallet
            .confirm_transaction(&ctx.signers.get_unchecked(1), &tx_id),
        2
    );
    let receipt = ctx
        .wallet
        .execute_transaction(&ctx.signers.get_unchecked(2), &tx_id);

    assert_eq!(receipt.tx_id, tx_id);
    assert_eq!(receipt.nonce, 1);
    assert_eq!(receipt.confirmation_count, 2);
    assert_eq!(ctx.token.balance(&ctx.recipient), 400);
    assert_eq!(ctx.token.balance(&ctx.wallet.address), 4_600);

    let transaction = ctx.wallet.get_transaction(&tx_id);
    assert!(transaction.executed);
    assert_eq!(transaction.executed_at, ctx.env.ledger().timestamp());
    assert_eq!(ctx.wallet.get_transaction_history(&10).len(), 1);
    assert_eq!(ctx.wallet.get_config().next_executable_nonce, 2);
}

#[test]
fn signer_can_revoke_confirmation_before_execution() {
    let ctx = setup(2, 1_000);
    let tx_id = submit_payment(&ctx, 300);

    ctx.wallet
        .confirm_transaction(&ctx.signers.get_unchecked(1), &tx_id);
    assert_eq!(ctx.wallet.confirmation_count(&tx_id), 2);

    assert_eq!(
        ctx.wallet
            .revoke_confirmation(&ctx.signers.get_unchecked(1), &tx_id),
        1
    );
    assert_eq!(ctx.wallet.confirmation_count(&tx_id), 1);

    ctx.wallet
        .confirm_transaction(&ctx.signers.get_unchecked(2), &tx_id);
    ctx.wallet
        .execute_transaction(&ctx.signers.get_unchecked(0), &tx_id);
    assert_eq!(ctx.token.balance(&ctx.recipient), 300);
}

#[test]
fn admin_can_manage_signers_and_requirements() {
    let ctx = setup(2, 1_000);
    let signer_four = Address::generate(&ctx.env);

    ctx.wallet.add_signer(&ctx.admin, &signer_four);
    assert!(ctx.wallet.is_signer(&signer_four));
    assert_eq!(ctx.wallet.get_signers().len(), 4);

    ctx.wallet.change_requirement(&ctx.admin, &3);
    assert_eq!(ctx.wallet.get_config().required_confirmations, 3);

    let tx_id = ctx.wallet.submit_transaction(
        &signer_four,
        &ctx.token.address,
        &ctx.recipient,
        &250,
        &String::from_str(&ctx.env, "new signer payment"),
    );
    ctx.wallet
        .confirm_transaction(&ctx.signers.get_unchecked(0), &tx_id);
    ctx.wallet
        .confirm_transaction(&ctx.signers.get_unchecked(1), &tx_id);
    ctx.wallet
        .execute_transaction(&ctx.signers.get_unchecked(2), &tx_id);

    ctx.wallet.remove_signer(&ctx.admin, &signer_four);
    assert!(!ctx.wallet.is_signer(&signer_four));
    assert_eq!(ctx.wallet.get_signers().len(), 3);
    assert_eq!(ctx.token.balance(&ctx.recipient), 250);
}

#[test]
fn whitelist_bypasses_daily_spending_limit() {
    let ctx = setup(2, 500);
    ctx.wallet.set_whitelist(&ctx.admin, &ctx.recipient, &true);
    assert!(ctx.wallet.is_whitelisted(&ctx.recipient));

    let tx_id = submit_payment(&ctx, 2_000);
    ctx.wallet
        .confirm_transaction(&ctx.signers.get_unchecked(1), &tx_id);
    ctx.wallet
        .execute_transaction(&ctx.signers.get_unchecked(2), &tx_id);

    assert_eq!(ctx.token.balance(&ctx.recipient), 2_000);
    assert_eq!(
        ctx.wallet.daily_spent(
            &ctx.token.address,
            &(ctx.env.ledger().timestamp() / DAY_SECONDS)
        ),
        0
    );
}

#[test]
#[should_panic(expected = "Daily spending limit exceeded")]
fn non_whitelisted_recipient_is_limited_by_daily_spend() {
    let ctx = setup(2, 500);
    let tx_id = submit_payment(&ctx, 600);
    ctx.wallet
        .confirm_transaction(&ctx.signers.get_unchecked(1), &tx_id);
    ctx.wallet
        .execute_transaction(&ctx.signers.get_unchecked(2), &tx_id);
}

#[test]
#[should_panic(expected = "Transaction nonce is not next")]
fn nonce_ordering_blocks_later_transaction_execution() {
    let ctx = setup(2, 1_000);
    let first_tx = submit_payment(&ctx, 100);
    let second_tx = ctx.wallet.submit_transaction(
        &ctx.signers.get_unchecked(1),
        &ctx.token.address,
        &ctx.recipient,
        &100,
        &String::from_str(&ctx.env, "second payment"),
    );

    ctx.wallet
        .confirm_transaction(&ctx.signers.get_unchecked(2), &second_tx);
    assert_eq!(ctx.wallet.confirmation_count(&second_tx), 2);
    assert_eq!(ctx.wallet.confirmation_count(&first_tx), 1);

    ctx.wallet
        .execute_transaction(&ctx.signers.get_unchecked(0), &second_tx);
}

#[test]
#[should_panic(expected = "Caller is not signer")]
fn non_signer_cannot_submit_transaction() {
    let ctx = setup(2, 1_000);
    let attacker = Address::generate(&ctx.env);
    ctx.wallet.submit_transaction(
        &attacker,
        &ctx.token.address,
        &ctx.recipient,
        &100,
        &String::from_str(&ctx.env, "bad payment"),
    );
}
