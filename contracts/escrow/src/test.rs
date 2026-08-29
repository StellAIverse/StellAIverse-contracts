use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    vec, Address, Env, String, Vec,
};

use access_control::AccessControlClient;

const HOUR: u64 = 3_600;
const DELAY: u64 = 100;
const GRACE: u64 = 1_000;

fn gen_addresses(env: &Env, n: u32) -> Vec<Address> {
    let mut v = Vec::new(env);
    for _ in 0..n {
        v.push_back(Address::generate(env));
    }
    v
}

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

struct Setup<'a> {
    env: Env,
    client: EscrowClient<'a>,
    admin: Address,
    signers: Vec<Address>,
    token_client: TokenClient<'a>,
    escrow_id: u64,
}

/// Deploys the escrow with a 2-of-3 signer set, 100s time-lock, 1000s grace
/// period and a rate limit of 2 executions per hour.
fn setup() -> Setup<'static> {
    setup_with(None)
}

fn setup_with(ac: Option<Address>) -> Setup<'static> {
    let s = setup_raw(ac, false);
    // Token escrow pre-funded.
    MockTokenClient::new(&s.env, &s.token_client.address).mint(&s.signers.get_unchecked(0), &1_000);
    s.client
        .deposit(&s.escrow_id, &s.signers.get_unchecked(0), &1_000);
    assert_eq!(s.client.escrow_balance(&s.escrow_id), 1_000);
    s
}

fn setup_raw(ac: Option<Address>, native: bool) -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let admin = Address::generate(&env);
    let signers = gen_addresses(&env, 3);
    let token_id = env.register(MockToken, ());
    let escrow_id_contract = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_id_contract);

    client.initialize(&admin, &ac, &signers, &2, &DELAY, &GRACE, &HOUR, &2u32);

    let creator = signers.get_unchecked(0);
    let escrow_id = client.create_escrow(
        &creator,
        &token_id,
        &native,
        &String::from_str(&env, "treasury"),
    );

    Setup {
        token_client: TokenClient::new(&env, &token_id),
        env,
        client,
        admin,
        signers,
        escrow_id,
    }
}

/// Queues and fully approves a withdrawal of `amount` to `recipient`.
fn queue_approved(s: &Setup, recipient: &Address, amount: i128) -> u64 {
    let submitter = s.signers.get_unchecked(0);
    let tx_id = s
        .client
        .submit_transaction(&submitter, &s.escrow_id, recipient, &amount);
    let second = s.signers.get_unchecked(1);
    s.client.approve_transaction(&second, &tx_id);
    tx_id
}

#[test]
fn initialize_sets_config_and_rejects_duplicates() {
    let (env, admin, signers, _token) = {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let signers = gen_addresses(&env, 3);
        let token = env.register(MockToken, ());
        (env, admin, signers, token)
    };
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);

    client.initialize(&admin, &None, &signers, &2, &50, &500, &HOUR, &3u32);

    let cfg = client.get_config().unwrap();
    assert_eq!(cfg.admin, admin);
    assert_eq!(cfg.required_approvals, 2);
    assert_eq!(cfg.signer_count, 3);
    assert_eq!(cfg.timelock_delay, 50);
    assert_eq!(cfg.grace_period, 500);
    assert_eq!(cfg.rate_limit_window, HOUR);
    assert_eq!(cfg.rate_limit_max, 3);
    assert!(!cfg.paused);
    assert_eq!(client.get_signers().len(), 3);

    // Duplicate initialization is rejected.
    let err = client
        .try_initialize(&admin, &None, &signers, &2, &50, &500, &HOUR, &3u32)
        .unwrap_err();
    assert_eq!(err.unwrap(), EscrowError::AlreadyInitialized);
}

#[test]
fn initialize_validates_parameters() {
    let (env, admin, signers, _) = {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let signers = gen_addresses(&env, 3);
        (env, admin, signers, ())
    };
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);

    // Threshold above signer count.
    assert_eq!(
        client
            .try_initialize(&admin, &None, &signers, &4, &50, &500, &HOUR, &1u32)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
    // Zero threshold.
    assert_eq!(
        client
            .try_initialize(&admin, &None, &signers, &0, &50, &500, &HOUR, &1u32)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
    // Duplicate signers.
    let dupes = vec![&env, signers.get_unchecked(0), signers.get_unchecked(0)];
    assert_eq!(
        client
            .try_initialize(&admin, &None, &dupes, &1, &50, &500, &HOUR, &1u32)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
    // Zero grace period.
    assert_eq!(
        client
            .try_initialize(&admin, &None, &signers, &2, &50, &0, &HOUR, &1u32)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
    // Zero rate limit window.
    assert_eq!(
        client
            .try_initialize(&admin, &None, &signers, &2, &50, &500, &0, &1u32)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
}

#[test]
fn deposit_moves_tokens_and_tracks_balance() {
    let s = setup();
    let depositor = s.signers.get_unchecked(0);

    let before_depositor = s.token_client.balance(&depositor);
    let before_wallet = s.token_client.balance(&s.client.address);
    assert_eq!(before_wallet, 1_000); // from initial setup deposit

    MockTokenClient::new(&s.env, &s.token_client.address).mint(&depositor, &400);
    s.client.deposit(&s.escrow_id, &depositor, &400);
    assert_eq!(s.client.escrow_balance(&s.escrow_id), 1_400);
    // Minted 400 then deposited the same 400 away, so the depositor's
    // token balance nets back to what it was before this test's mint.
    assert_eq!(s.token_client.balance(&depositor), before_depositor);
    assert_eq!(
        s.token_client.balance(&s.client.address),
        before_wallet + 400
    );

    // Zero / negative deposits rejected.
    assert_eq!(
        s.client
            .try_deposit(&s.escrow_id, &depositor, &0)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
    // Unknown escrow rejected.
    let unknown = Address::generate(&s.env);
    assert_eq!(
        s.client
            .try_deposit(&99, &unknown, &5)
            .unwrap_err()
            .unwrap(),
        EscrowError::NotFound
    );
}

#[test]
fn native_escrow_accounts_internally() {
    let s = setup_raw(None, true);
    let depositor = s.signers.get_unchecked(0);
    s.client.deposit(&s.escrow_id, &depositor, &700);
    assert_eq!(s.client.escrow_balance(&s.escrow_id), 700);
}

#[test]
fn submit_requires_signer_or_admin_and_funded_escrow() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);

    assert_eq!(
        s.client
            .try_submit_transaction(&outsider, &s.escrow_id, &recipient, &10)
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );

    // Over the funded balance.
    let submitter = s.signers.get_unchecked(0);
    assert_eq!(
        s.client
            .try_submit_transaction(&submitter, &s.escrow_id, &recipient, &(1_001))
            .unwrap_err()
            .unwrap(),
        EscrowError::InsufficientBalance
    );

    // Admin may submit too.
    s.client
        .submit_transaction(&s.admin, &s.escrow_id, &recipient, &10);

    // Zero amounts rejected.
    assert_eq!(
        s.client
            .try_submit_transaction(&submitter, &s.escrow_id, &recipient, &0)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
}

#[test]
fn approvals_enforce_m_of_n() {
    let s = setup();
    let recipient = Address::generate(&s.env);
    let tx_id = queue_approved(&s, &recipient, 100);

    let tx = s.client.get_transaction(&tx_id).unwrap();
    assert_eq!(tx.approvers.len(), 2);
    assert_eq!(tx.amount, 100);
    assert_eq!(tx.escrow_id, s.escrow_id);

    // Double approval rejected.
    let second = s.signers.get_unchecked(1);
    assert_eq!(
        s.client
            .try_approve_transaction(&second, &tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::AlreadyApproved
    );

    // Non-signer approval rejected.
    let outsider = Address::generate(&s.env);
    assert_eq!(
        s.client
            .try_approve_transaction(&outsider, &tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::NotASigner
    );

    // Revocation drops below threshold; execution then fails until re-approved.
    s.client.revoke_approval(&second, &tx_id);
    assert_eq!(s.client.get_transaction(&tx_id).unwrap().approvers.len(), 1);
    assert_eq!(
        s.client
            .try_execute_transaction(&tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::NotApproved
    );

    // Revoking an approval one does not have fails.
    assert_eq!(
        s.client
            .try_revoke_approval(&second, &tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::NotApproved
    );
}

#[test]
fn timelock_blocks_then_execution_releases_funds() {
    let s = setup();
    let recipient = Address::generate(&s.env);
    let tx_id = queue_approved(&s, &recipient, 300);

    // Still time-locked right after queueing.
    assert_eq!(
        s.client
            .try_execute_transaction(&tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::TimeLocked
    );

    s.env.ledger().with_mut(|l| l.timestamp += DELAY - 1);
    assert_eq!(
        s.client
            .try_execute_transaction(&tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::TimeLocked
    );

    // Exactly at unlock time execution succeeds.
    s.env.ledger().with_mut(|l| l.timestamp += 1);
    s.client.execute_transaction(&tx_id);

    let wallet = s.client.address.clone();
    assert_eq!(s.token_client.balance(&recipient), 300);
    assert_eq!(s.token_client.balance(&wallet), 700);
    assert_eq!(s.client.escrow_balance(&s.escrow_id), 700);

    let tx = s.client.get_transaction(&tx_id).unwrap();
    assert!(tx.executed);
    assert_eq!(tx.executed_at, s.env.ledger().timestamp());

    // Re-execution rejected.
    assert_eq!(
        s.client
            .try_execute_transaction(&tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::AlreadyExecuted
    );
}

#[test]
fn grace_period_expiry_blocks_execution_and_allows_anyone_to_cancel() {
    let s = setup();
    let recipient = Address::generate(&s.env);
    let tx_id = queue_approved(&s, &recipient, 50);

    // Inside the grace window execution still works conceptually; jump past it.
    s.env
        .ledger()
        .with_mut(|l| l.timestamp += DELAY + GRACE + 1);

    assert_eq!(
        s.client
            .try_execute_transaction(&tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::GraceExpired
    );

    // Random third party can garbage-collect the expired transaction.
    let stranger = Address::generate(&s.env);
    s.client.cancel_transaction(&stranger, &tx_id);
    assert!(s.client.get_transaction(&tx_id).unwrap().cancelled);

    // Cancelling again fails.
    assert_eq!(
        s.client
            .try_cancel_transaction(&stranger, &tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::AlreadyCancelled
    );
}

#[test]
fn cancel_permissions_inside_grace_period() {
    let s = setup();
    let r1 = Address::generate(&s.env);
    let r2 = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    let tx_admin = queue_approved(&s, &r1, 10);
    let tx_submitter = {
        let second = s.signers.get_unchecked(1);
        let tx = s.client.submit_transaction(&second, &s.escrow_id, &r2, &20);
        s.client
            .approve_transaction(&s.signers.get_unchecked(0), &tx);
        tx
    };

    // Stranger cannot cancel inside the grace period.
    assert_eq!(
        s.client
            .try_cancel_transaction(&stranger, &tx_admin)
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );

    // Admin cancels their own queued transaction.
    s.client.cancel_transaction(&s.admin, &tx_admin);
    assert!(s.client.get_transaction(&tx_admin).unwrap().cancelled);

    // Submitter cancels their own queued transaction.
    let submitter = s.signers.get_unchecked(1);
    s.client.cancel_transaction(&submitter, &tx_submitter);
    assert!(s.client.get_transaction(&tx_submitter).unwrap().cancelled);
}

#[test]
fn rate_limits_block_rapid_cascading_executions() {
    let s = setup();
    let recipient = Address::generate(&s.env);

    let t1 = queue_approved(&s, &recipient, 10);
    s.env.ledger().with_mut(|l| l.timestamp += DELAY);
    s.client.execute_transaction(&t1);

    // Second execution for the same recipient within the hour succeeds
    // (cap = 2).
    let t2 = queue_approved(&s, &recipient, 10);
    s.env.ledger().with_mut(|l| l.timestamp += DELAY);
    s.client.execute_transaction(&t2);

    // Third is rate-limited.
    let t3 = queue_approved(&s, &recipient, 10);
    s.env.ledger().with_mut(|l| l.timestamp += DELAY);
    assert_eq!(
        s.client.try_execute_transaction(&t3).unwrap_err().unwrap(),
        EscrowError::RateLimited
    );

    // A different recipient also hits the global cap.
    let other = Address::generate(&s.env);
    let t4 = queue_approved(&s, &other, 10);
    s.env.ledger().with_mut(|l| l.timestamp += DELAY);
    assert_eq!(
        s.client.try_execute_transaction(&t4).unwrap_err().unwrap(),
        EscrowError::RateLimited
    );

    // After the rate-limit window passes, execution works again for a
    // freshly queued transaction. `t3` itself cannot be retried here: its
    // own grace window (1_000s) is shorter than the rate-limit window
    // (1 hour) it was blocked by, so by the time the rate limit clears it
    // has independently fallen outside its grace period and can only be
    // cancelled (garbage-collected), not executed.
    s.env.ledger().with_mut(|l| l.timestamp += HOUR);
    assert_eq!(
        s.client.try_execute_transaction(&t3).unwrap_err().unwrap(),
        EscrowError::GraceExpired
    );
    s.client.cancel_transaction(&s.admin, &t3);

    let t5 = queue_approved(&s, &recipient, 10);
    s.env.ledger().with_mut(|l| l.timestamp += DELAY);
    s.client.execute_transaction(&t5);
    assert!(s.client.get_transaction(&t5).unwrap().executed);
}

#[test]
fn pause_gates_state_changing_operations() {
    let s = setup();
    let recipient = Address::generate(&s.env);
    let tx_id = queue_approved(&s, &recipient, 25);
    let depositor = s.signers.get_unchecked(0);

    s.client.pause(&s.admin);

    assert_eq!(
        s.client
            .try_create_escrow(
                &depositor,
                &s.token_client.address,
                &false,
                &String::from_str(&s.env, "x")
            )
            .unwrap_err()
            .unwrap(),
        EscrowError::Paused
    );
    assert_eq!(
        s.client
            .try_submit_transaction(&depositor, &s.escrow_id, &recipient, &5)
            .unwrap_err()
            .unwrap(),
        EscrowError::Paused
    );
    s.env.ledger().with_mut(|l| l.timestamp += DELAY);
    assert_eq!(
        s.client
            .try_execute_transaction(&tx_id)
            .unwrap_err()
            .unwrap(),
        EscrowError::Paused
    );

    // Non-admin cannot pause or unpause.
    assert_eq!(
        s.client
            .try_pause(&s.signers.get_unchecked(0))
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );

    s.client.unpause(&s.admin);
    s.client.execute_transaction(&tx_id);
    assert!(s.client.get_transaction(&tx_id).unwrap().executed);
}

#[test]
fn signer_management_updates_threshold_bounds() {
    let s = setup();
    let newcomer = Address::generate(&s.env);

    // Duplicate add rejected.
    assert_eq!(
        s.client
            .try_add_signer(&s.admin, &s.signers.get_unchecked(0))
            .unwrap_err()
            .unwrap(),
        EscrowError::AlreadySigner
    );

    s.client.add_signer(&s.admin, &newcomer);
    assert!(s.client.is_signer_public(&newcomer));
    assert_eq!(s.client.get_signers().len(), 4);

    // Raise the threshold to 3, then remove one of the original signers:
    // signer_count drops to 3, exactly matching the threshold.
    s.client.set_threshold(&s.admin, &3);
    let first = s.signers.get_unchecked(0);
    let second = s.signers.get_unchecked(1);
    s.client.remove_signer(&s.admin, &first);
    assert_eq!(s.client.get_signers().len(), 3);

    // Removal never auto-lowers the threshold (unlike `add_signer`, which
    // auto-lowers it): a further removal that would bring signer_count to
    // or below `required_approvals` is refused until the admin explicitly
    // lowers the threshold first.
    assert_eq!(
        s.client
            .try_remove_signer(&s.admin, &second)
            .unwrap_err()
            .unwrap(),
        EscrowError::ThresholdTooHigh
    );

    s.client.set_threshold(&s.admin, &2);
    s.client.remove_signer(&s.admin, &second);
    assert_eq!(s.client.get_signers().len(), 2);
    let cfg = s.client.get_config().unwrap();
    assert_eq!(cfg.required_approvals, 2);
    assert_eq!(cfg.signer_count, 2);

    // Removing any further signer would drop below the threshold.
    let third = s.signers.get_unchecked(2);
    assert_eq!(
        s.client
            .try_remove_signer(&s.admin, &third)
            .unwrap_err()
            .unwrap(),
        EscrowError::ThresholdTooHigh
    );
    assert_eq!(
        s.client
            .try_remove_signer(&s.admin, &newcomer)
            .unwrap_err()
            .unwrap(),
        EscrowError::ThresholdTooHigh
    );

    // Removing an address that was never a signer is a different error.
    let unknown = Address::generate(&s.env);
    assert_eq!(
        s.client
            .try_remove_signer(&s.admin, &unknown)
            .unwrap_err()
            .unwrap(),
        EscrowError::NotASigner
    );

    // Only admins manage signers.
    assert_eq!(
        s.client
            .try_add_signer(&third, &Address::generate(&s.env))
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );
}

#[test]
fn config_setters_validate_and_persist() {
    let s = setup();

    s.client.set_threshold(&s.admin, &3);
    assert_eq!(s.client.get_config().unwrap().required_approvals, 3);
    assert_eq!(
        s.client
            .try_set_threshold(&s.admin, &0)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );
    assert_eq!(
        s.client
            .try_set_threshold(&s.admin, &9)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );

    s.client.set_timelock(&s.admin, &200, &2_000);
    let cfg = s.client.get_config().unwrap();
    assert_eq!(cfg.timelock_delay, 200);
    assert_eq!(cfg.grace_period, 2_000);
    assert_eq!(
        s.client
            .try_set_timelock(&s.admin, &0, &0)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );

    s.client.set_rate_limit(&s.admin, &60, &5);
    let cfg = s.client.get_config().unwrap();
    assert_eq!(cfg.rate_limit_window, 60);
    assert_eq!(cfg.rate_limit_max, 5);
    assert_eq!(
        s.client
            .try_set_rate_limit(&s.admin, &0, &0)
            .unwrap_err()
            .unwrap(),
        EscrowError::InvalidParam
    );

    // Non-admin rejected on every setter.
    let non_admin = s.signers.get_unchecked(2);
    assert_eq!(
        s.client
            .try_set_timelock(&non_admin, &1, &1)
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );
    assert_eq!(
        s.client
            .try_set_rate_limit(&non_admin, &1, &1)
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );
    assert_eq!(
        s.client
            .try_set_threshold(&non_admin, &1)
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );
}

#[test]
fn access_control_role_grants_admin_rights() {
    // Deploy AccessControl with its own admin, grant a delegate the Admin
    // role, and verify the delegate can administer the escrow through the
    // linked access-control contract.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let ac_id = env.register(access_control::AccessControl, ());
    let ac_admin = Address::generate(&env);
    AccessControlClient::new(&env, &ac_id).initialize(&ac_admin);
    let delegate = Address::generate(&env);
    AccessControlClient::new(&env, &ac_id).grant_role(
        &access_control::Role::Admin,
        &delegate,
        &ac_admin,
    );

    let signers = gen_addresses(&env, 2);
    let token_id = env.register(MockToken, ());
    let escrow_c = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_c);
    client.initialize(
        &ac_admin,
        &Some(ac_id.clone()),
        &signers,
        &1,
        &DELAY,
        &GRACE,
        &HOUR,
        &2u32,
    );

    // Delegate (Admin via AC) can change configuration...
    client.set_timelock(&delegate, &7, &70);
    assert_eq!(client.get_config().unwrap().timelock_delay, 7);

    // ...and a random account still cannot.
    let nobody = Address::generate(&env);
    assert_eq!(
        client
            .try_set_timelock(&nobody, &1, &1)
            .unwrap_err()
            .unwrap(),
        EscrowError::Unauthorized
    );
    let _ = token_id;
}

#[test]
fn close_escrow_blocks_future_deposits() {
    let s = setup();
    s.client.close_escrow(&s.admin, &s.escrow_id);
    assert!(!s.client.get_escrow(&s.escrow_id).unwrap().active);

    let depositor = s.signers.get_unchecked(0);
    assert_eq!(
        s.client
            .try_deposit(&s.escrow_id, &depositor, &10)
            .unwrap_err()
            .unwrap(),
        EscrowError::InactiveEscrow
    );
}

#[test]
fn multiple_escrow_instances_are_independent() {
    let s = setup();
    let creator = s.signers.get_unchecked(0);

    let second = s.client.create_escrow(
        &creator,
        &s.token_client.address,
        &true,
        &String::from_str(&s.env, "payroll"),
    );
    assert_ne!(second, s.escrow_id);
    assert_eq!(s.client.escrow_balance(&second), 0);

    // Funds are tracked per instance.
    s.client.deposit(&second, &creator, &123);
    assert_eq!(s.client.escrow_balance(&second), 123);
    assert_eq!(s.client.escrow_balance(&s.escrow_id), 1_000);
}

#[test]
fn views_return_none_for_unknown_ids() {
    let s = setup();
    assert!(s.client.get_escrow(&999).is_none());
    assert!(s.client.get_transaction(&999).is_none());
}
