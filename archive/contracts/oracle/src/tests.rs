#![cfg(test)]
extern crate alloc;

use crate::{Oracle, OracleClient, RelayRequest};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::xdr::{self, Limited, Limits, WriteXdr};
use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, BytesN, Env, Symbol, TryIntoVal, Val, Vec,
};

#[contract]
pub struct Receiver;

#[contractimpl]
impl Receiver {
    pub fn ping(env: Env, input: u32) -> u32 {
        env.storage().instance().set(&symbol_short!("last"), &input);
        input + 1
    }

    pub fn last(env: Env) -> Option<u32> {
        env.storage().instance().get(&symbol_short!("last"))
    }
}

fn build_signed_payload(
    env: &Env,
    oracle_contract: &Address,
    oracle_pubkey: &BytesN<32>,
    target_contract: &Address,
    function: &Symbol,
    args: &Vec<Val>,
    nonce: u64,
    deadline: u64,
    signing_key: &SigningKey,
) -> BytesN<64> {
    let req = RelayRequest {
        relay_contract: oracle_contract.clone(),
        oracle_pubkey: oracle_pubkey.clone(),
        target_contract: target_contract.clone(),
        function: function.clone(),
        args: args.clone(),
        nonce,
        deadline,
    };

    let scval: xdr::ScVal = req.try_into().unwrap();
    let mut buf: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
    scval
        .write_xdr(&mut Limited::new(&mut buf, Limits::none()))
        .unwrap();

    let sig = signing_key.sign(&buf);
    BytesN::from_array(env, &sig.to_bytes())
}

fn setup() -> (
    Env,
    OracleClient<'static>,
    Address,
    BytesN<32>,
    SigningKey,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_contract_id = env.register_contract(None, Oracle);
    let oracle_client = OracleClient::new(&env, &oracle_contract_id);
    let admin = Address::generate(&env);
    oracle_client.init_contract(&admin);

    let receiver_id = env.register_contract(None, Receiver);

    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let pk_bytes: [u8; 32] = sk.verifying_key().to_bytes();
    let pk = BytesN::from_array(&env, &pk_bytes);

    (env, oracle_client, admin, pk, sk, receiver_id)
}

#[test]
fn test_relay_signed_success_forwards_payload() {
    let (env, oracle, admin, pk, sk, receiver_id) = setup();
    oracle.register_oracle_key(&admin, &pk);

    let function = Symbol::new(&env, "ping");
    let args: Vec<Val> = (123u32,).try_into_val(&env).unwrap();
    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 100;
    let signature = build_signed_payload(
        &env,
        &oracle.address,
        &pk,
        &receiver_id,
        &function,
        &args,
        nonce,
        deadline,
        &sk,
    );

    let res = oracle.relay_signed(
        &pk,
        &receiver_id,
        &function,
        &args,
        &nonce,
        &deadline,
        &signature,
    );
    let res_u32: u32 = res.try_into_val(&env).unwrap();
    assert_eq!(res_u32, 124);

    // Verify target contract state updated
    let last: Option<u32> =
        env.invoke_contract(&receiver_id, &Symbol::new(&env, "last"), Vec::new(&env));
    assert_eq!(last, Some(123));
}

#[test]
#[should_panic(expected = "Oracle not approved")]
fn test_relay_signed_rejects_unapproved_oracle() {
    let (env, oracle, _admin, pk, sk, receiver_id) = setup();

    let function = Symbol::new(&env, "ping");
    let args: Vec<Val> = (1u32,).try_into_val(&env).unwrap();
    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 100;
    let signature = build_signed_payload(
        &env,
        &oracle.address,
        &pk,
        &receiver_id,
        &function,
        &args,
        nonce,
        deadline,
        &sk,
    );

    oracle.relay_signed(
        &pk,
        &receiver_id,
        &function,
        &args,
        &nonce,
        &deadline,
        &signature,
    );
}

#[test]
#[should_panic]
fn test_relay_signed_rejects_bad_signature() {
    let (env, oracle, admin, pk, _sk, receiver_id) = setup();
    oracle.register_oracle_key(&admin, &pk);

    let function = Symbol::new(&env, "ping");
    let args: Vec<Val> = (1u32,).try_into_val(&env).unwrap();
    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 100;

    // Wrong signature
    let signature = BytesN::from_array(&env, &[0u8; 64]);
    oracle.relay_signed(
        &pk,
        &receiver_id,
        &function,
        &args,
        &nonce,
        &deadline,
        &signature,
    );
}

#[test]
#[should_panic(expected = "Invalid nonce: replay protection triggered")]
fn test_relay_signed_prevents_replay() {
    // This test simulates a replay attack by submitting the same signed request twice.
    // The second call must panic due to nonce reuse, confirming replay protection works.
    let (env, oracle, admin, pk, sk, receiver_id) = setup();
    oracle.register_oracle_key(&admin, &pk);

    let function = Symbol::new(&env, "ping");
    let args: Vec<Val> = (5u32,).try_into_val(&env).unwrap();
    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 100;
    let signature = build_signed_payload(
        &env,
        &oracle.address,
        &pk,
        &receiver_id,
        &function,
        &args,
        nonce,
        deadline,
        &sk,
    );

    oracle.relay_signed(
        &pk,
        &receiver_id,
        &function,
        &args,
        &nonce,
        &deadline,
        &signature,
    );
    // The following call should panic, as the nonce has already been used
    oracle.relay_signed(
        &pk,
        &receiver_id,
        &function,
        &args,
        &nonce,
        &deadline,
        &signature,
    );
}

#[test]
#[should_panic(expected = "Signature expired")]
fn test_relay_signed_rejects_expired_deadline() {
    let (env, oracle, admin, pk, sk, receiver_id) = setup();
    oracle.register_oracle_key(&admin, &pk);

    let function = Symbol::new(&env, "ping");
    let args: Vec<Val> = (1u32,).try_into_val(&env).unwrap();
    let nonce = 1u64;
    let deadline = env.ledger().timestamp();
    let signature = build_signed_payload(
        &env,
        &oracle.address,
        &pk,
        &receiver_id,
        &function,
        &args,
        nonce,
        deadline,
        &sk,
    );

    // Move ledger time forward
    env.ledger().set_timestamp(deadline + 1);
    oracle.relay_signed(
        &pk,
        &receiver_id,
        &function,
        &args,
        &nonce,
        &deadline,
        &signature,
    );
}

// ── Subscription tests ─────────────────────────────────────────────────────

use crate::SubscriptionTier;

fn setup_with_plan(
) -> (
    Env,
    OracleClient<'static>,
    Address,
    Symbol,
) {
    let (env, oracle, admin, _pk, _sk, _receiver_id) = setup();
    let feed = Symbol::new(&env, "BTC_USD");
    oracle.create_plan(
        &admin,
        &feed,
        &SubscriptionTier::Basic,
        &1_000_000i128, // 1 XLM
        &86_400u64,     // 1 day
        &100u32,        // 100 calls/period
    );
    (env, oracle, admin, feed)
}

#[test]
fn test_create_plan_and_get_plan() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let plan = oracle.get_plan(&feed, &SubscriptionTier::Basic).unwrap();
    assert_eq!(plan.price_per_period, 1_000_000);
    assert_eq!(plan.period_seconds, 86_400);
    assert_eq!(plan.max_calls_per_period, 100);
    assert!(plan.active);
}

#[test]
fn test_subscribe_creates_subscription() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let user = Address::generate(&env);

    oracle.subscribe(&user, &feed, &SubscriptionTier::Basic, &false);

    let sub = oracle.get_subscription(&user, &feed).unwrap();
    assert_eq!(sub.calls_used, 0);
    assert_eq!(sub.calls_limit, 100);
    assert!(!sub.auto_renew);
    assert!(sub.expires_at > env.ledger().timestamp());
}

#[test]
fn test_check_access_active_subscription() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let user = Address::generate(&env);

    assert!(!oracle.check_access(&user, &feed));
    oracle.subscribe(&user, &feed, &SubscriptionTier::Basic, &true);
    assert!(oracle.check_access(&user, &feed));
}

#[test]
fn test_track_usage_increments_counter() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let user = Address::generate(&env);
    oracle.subscribe(&user, &feed, &SubscriptionTier::Basic, &false);

    oracle.track_usage(&user, &feed);
    oracle.track_usage(&user, &feed);

    let sub = oracle.get_subscription(&user, &feed).unwrap();
    assert_eq!(sub.calls_used, 2);
}

#[test]
fn test_cancel_disables_auto_renew() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let user = Address::generate(&env);
    oracle.subscribe(&user, &feed, &SubscriptionTier::Basic, &true);

    oracle.cancel(&user, &feed);

    let sub = oracle.get_subscription(&user, &feed).unwrap();
    assert!(!sub.auto_renew);
    // Access still valid until expiry
    assert!(oracle.check_access(&user, &feed));
}

#[test]
fn test_renew_extends_expiry() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let user = Address::generate(&env);
    oracle.subscribe(&user, &feed, &SubscriptionTier::Basic, &false);

    let before = oracle.get_subscription(&user, &feed).unwrap().expires_at;
    oracle.renew(&user, &feed);
    let after = oracle.get_subscription(&user, &feed).unwrap().expires_at;

    assert_eq!(after, before + 86_400);
}

#[test]
#[should_panic(expected = "No active subscription or quota exceeded")]
fn test_track_usage_fails_without_subscription() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let user = Address::generate(&env);
    oracle.track_usage(&user, &feed);
}

#[test]
#[should_panic(expected = "Plan is not active")]
fn test_subscribe_fails_on_deactivated_plan() {
    let (env, oracle, admin, feed) = setup_with_plan();
    let user = Address::generate(&env);
    oracle.deactivate_plan(&admin, &feed, &SubscriptionTier::Basic);
    oracle.subscribe(&user, &feed, &SubscriptionTier::Basic, &false);
}

#[test]
fn test_access_expires_after_period() {
    let (env, oracle, _admin, feed) = setup_with_plan();
    let user = Address::generate(&env);
    oracle.subscribe(&user, &feed, &SubscriptionTier::Basic, &false);

    let sub = oracle.get_subscription(&user, &feed).unwrap();
    // Advance ledger past expiry
    env.ledger().set_timestamp(sub.expires_at + 1);
    assert!(!oracle.check_access(&user, &feed));
}
