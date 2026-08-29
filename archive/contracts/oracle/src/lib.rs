#![no_std]

#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "testutils"))]
mod testutils;
mod types;

use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String, Symbol, Val, Vec};
use stellai_lib::{
    audit::{create_audit_log, OperationType},
    rbac,
    storage_keys::{PROVIDER_LIST_KEY, SUB_PLAN_KEY_PREFIX, SUB_USAGE_KEY_PREFIX, SUBSCRIPTION_KEY_PREFIX},
    types::OracleData,
    ADMIN_KEY,
};

pub use types::*;

// ── storage key helpers ────────────────────────────────────────────────────

fn plan_key(env: &Env, feed_key: &Symbol, tier: SubscriptionTier) -> String {
    let tier_u32: u32 = tier as u32;
    // encode as "sub_plan_<feed>_<tier>"
    let mut key = soroban_sdk::String::from_str(env, SUB_PLAN_KEY_PREFIX);
    let feed_str = feed_key.to_string();
    key = concat_strings(env, &key, &feed_str);
    key = concat_strings(env, &key, &soroban_sdk::String::from_str(env, "_"));
    key = concat_strings(env, &key, &u32_to_string(env, tier_u32));
    key
}

fn subscription_key(env: &Env, subscriber: &Address, feed_key: &Symbol) -> String {
    let mut key = soroban_sdk::String::from_str(env, SUBSCRIPTION_KEY_PREFIX);
    let feed_str = feed_key.to_string();
    key = concat_strings(env, &key, &feed_str);
    key = concat_strings(env, &key, &soroban_sdk::String::from_str(env, "_"));
    // use subscriber address bytes as discriminator via a simple hash
    let addr_bytes = subscriber.to_string();
    key = concat_strings(env, &key, &addr_bytes);
    key
}

fn usage_key(env: &Env, subscriber: &Address, feed_key: &Symbol) -> String {
    let mut key = soroban_sdk::String::from_str(env, SUB_USAGE_KEY_PREFIX);
    let feed_str = feed_key.to_string();
    key = concat_strings(env, &key, &feed_str);
    key = concat_strings(env, &key, &soroban_sdk::String::from_str(env, "_"));
    key = concat_strings(env, &key, &subscriber.to_string());
    key
}

fn concat_strings(env: &Env, a: &String, b: &String) -> String {
    let a_len = a.len();
    let b_len = b.len();
    let mut buf = soroban_sdk::Bytes::new(env);
    for i in 0..a_len {
        buf.push_back(a.get(i).unwrap());
    }
    for i in 0..b_len {
        buf.push_back(b.get(i).unwrap());
    }
    String::from_bytes(env, &buf)
}

fn u32_to_string(env: &Env, mut n: u32) -> String {
    if n == 0 {
        return String::from_str(env, "0");
    }
    let mut digits: [u8; 10] = [0u8; 10];
    let mut idx = 10usize;
    while n > 0 {
        idx -= 1;
        digits[idx] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let slice = &digits[idx..];
    let bytes = soroban_sdk::Bytes::from_slice(env, slice);
    String::from_bytes(env, &bytes)
}

// ── contract ──────────────────────────────────────────────────────────────

#[contract]
pub struct Oracle;

#[contractimpl]
impl Oracle {
    pub fn init_contract(env: Env, admin: Address) {
        let admin_data: Option<Address> =
            env.storage().instance().get(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);

        let providers: Vec<Address> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PROVIDER_LIST_KEY), &providers);
    }

    /// Verify admin — always re-reads from storage (Issue #152)
    fn verify_admin(env: &Env, caller: &Address) {
        rbac::require_admin(env, caller).unwrap_or_else(|_| panic!("Caller is not admin"));
    }

    /// Check provider is registered — always re-reads from storage (Issue #152)
    fn is_authorized_provider(env: &Env, provider: &Address) -> bool {
        rbac::require_oracle_provider(env, provider, PROVIDER_LIST_KEY).is_ok()
    }

    pub fn register_provider(env: Env, admin: Address, provider: Address) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        let mut providers: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, PROVIDER_LIST_KEY))
            .unwrap_or_else(|| Vec::new(&env));

        for p in providers.iter() {
            if p == provider {
                panic!("Provider already registered");
            }
        }

        providers.push_back(provider.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PROVIDER_LIST_KEY), &providers);

        env.events().publish(
            (Symbol::new(&env, "provider_registered"),),
            (admin, provider),
        );
    }

    pub fn submit_data(env: Env, provider: Address, key: Symbol, value: i128) {
        provider.require_auth();

        if !Self::is_authorized_provider(&env, &provider) {
            panic!("Unauthorized: provider not registered");
        }

        let timestamp = env.ledger().timestamp();

        let oracle_data = OracleData {
            key: key.clone(),
            value,
            timestamp,
            provider: provider.clone(),
            signature: None,
            source: None,
        };

        env.storage().instance().set(&key, &oracle_data);

        env.events().publish(
            (Symbol::new(&env, "data_submitted"),),
            (key.clone(), timestamp, provider.clone()),
        );

        // Log audit entry for oracle data submission
        let before_state = String::from_str(&env, "{}"); // No specific 'before' state for new data
                                                         // A simple after state, could be more detailed in a real scenario
        let after_state = String::from_str(&env, "{\"status\":\"submitted\"}");
        // In a real scenario, this would be the actual transaction hash
        let tx_hash = String::from_str(&env, "0x_placeholder_tx_hash");
        let description = Some(String::from_str(&env, "Oracle data submitted."));

        create_audit_log(
            &env,
            provider.clone(),
            OperationType::ConfigurationChange, // Using a general type as no specific one exists
            before_state,
            after_state,
            tx_hash,
            description,
        );
    }

    pub fn get_data(env: Env, key: Symbol) -> Option<OracleData> {
        env.storage().instance().get(&key)
    }

    pub fn deregister_provider(env: Env, admin: Address, provider: Address) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        let providers: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, PROVIDER_LIST_KEY))
            .unwrap_or_else(|| Vec::new(&env));

        let mut updated_providers = Vec::new(&env);
        let mut found = false;

        for p in providers.iter() {
            if p != provider {
                updated_providers.push_back(p.clone());
            } else {
                found = true;
            }
        }

        if !found {
            panic!("Provider not found");
        }

        env.storage()
            .instance()
            .set(&Symbol::new(&env, PROVIDER_LIST_KEY), &updated_providers);

        env.events().publish(
            (Symbol::new(&env, "provider_deregistered"),),
            (admin, provider),
        );
    }

    fn is_approved_oracle_key(env: &Env, oracle_pubkey: &BytesN<32>) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Oracle(oracle_pubkey.clone()))
            .unwrap_or(false)
    }

    pub fn register_oracle_key(env: Env, admin: Address, oracle_pubkey: BytesN<32>) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        if Self::is_approved_oracle_key(&env, &oracle_pubkey) {
            panic!("Oracle key already registered");
        }

        env.storage()
            .instance()
            .set(&DataKey::Oracle(oracle_pubkey.clone()), &true);

        env.events().publish(
            (Symbol::new(&env, "oracle_key_registered"),),
            (admin, oracle_pubkey),
        );
    }

    pub fn deregister_oracle_key(env: Env, admin: Address, oracle_pubkey: BytesN<32>) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        if !Self::is_approved_oracle_key(&env, &oracle_pubkey) {
            panic!("Oracle key not found");
        }

        env.storage()
            .instance()
            .remove(&DataKey::Oracle(oracle_pubkey.clone()));
        env.storage()
            .instance()
            .remove(&DataKey::OracleNonce(oracle_pubkey.clone()));

        env.events().publish(
            (Symbol::new(&env, "oracle_key_deregistered"),),
            (admin, oracle_pubkey),
        );
    }

    pub fn is_registered_oracle_key(env: Env, oracle_pubkey: BytesN<32>) -> bool {
        Self::is_approved_oracle_key(&env, &oracle_pubkey)
    }

    fn get_oracle_nonce(env: &Env, oracle_pubkey: &BytesN<32>) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::OracleNonce(oracle_pubkey.clone()))
            .unwrap_or(0u64)
    }

    fn set_oracle_nonce(env: &Env, oracle_pubkey: &BytesN<32>, nonce: u64) {
        env.storage()
            .instance()
            .set(&DataKey::OracleNonce(oracle_pubkey.clone()), &nonce);
    }

    fn build_relay_message(env: &Env, req: &RelayRequest) -> Bytes {
        // Simplified implementation - just create a hash from the deadline and nonce
        let deadline_bytes = req.deadline.to_be_bytes();
        let nonce_bytes = req.nonce.to_be_bytes();
        let mut combined = [0u8; 16];
        combined[..8].copy_from_slice(&deadline_bytes);
        combined[8..].copy_from_slice(&nonce_bytes);
        let data_bytes = Bytes::from_array(env, &combined);
        let hash = env.crypto().sha256(&data_bytes);
        Bytes::from_array(env, &hash.to_array())
    }

    pub fn relay_signed(
        env: Env,
        oracle_pubkey: BytesN<32>,
        target_contract: Address,
        function: Symbol,
        args: Vec<Val>,
        nonce: u64,
        deadline: u64,
        signature: BytesN<64>,
    ) -> Val {
        // --- REPLAY PROTECTION: Ensure each signed request uses a unique, increasing nonce ---
        // This prevents replay attacks by rejecting any duplicate or stale nonce values.
        if !Self::is_approved_oracle_key(&env, &oracle_pubkey) {
            panic!("Oracle not approved");
        }

        if env.ledger().timestamp() > deadline {
            panic!("Signature expired");
        }

        let stored_nonce = Self::get_oracle_nonce(&env, &oracle_pubkey);
        if nonce <= stored_nonce {
            panic!("Invalid nonce: replay protection triggered");
        }

        let req = RelayRequest {
            relay_contract: env.current_contract_address(),
            oracle_pubkey: oracle_pubkey.clone(),
            target_contract: target_contract.clone(),
            function: function.clone(),
            args: args.clone(),
            nonce,
            deadline,
        };

        let message = Self::build_relay_message(&env, &req);
        env.crypto()
            .ed25519_verify(&oracle_pubkey, &message, &signature);

        // Store the new nonce to prevent future replays
        Self::set_oracle_nonce(&env, &oracle_pubkey, nonce);

        let result: Val = env.invoke_contract(&target_contract, &function, args);

        env.events().publish(
            (Symbol::new(&env, "payload_relayed"),),
            (oracle_pubkey, target_contract, function, nonce),
        );

        result
    }

    // ── Subscription system ─────────────────────────────────────────────────────

    /// Admin: create or update a subscription plan for a feed/tier.
    pub fn create_plan(
        env: Env,
        admin: Address,
        feed_key: Symbol,
        tier: SubscriptionTier,
        price_per_period: i128,
        period_seconds: u64,
        max_calls_per_period: u32,
    ) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        if price_per_period < 0 {
            panic!("Invalid price");
        }
        if period_seconds == 0 {
            panic!("Invalid period");
        }

        let plan = SubscriptionPlan {
            feed_key: feed_key.clone(),
            tier,
            price_per_period,
            period_seconds,
            max_calls_per_period,
            active: true,
        };

        let key = plan_key(&env, &feed_key, tier);
        env.storage().instance().set(&key, &plan);

        env.events().publish(
            (Symbol::new(&env, "plan_created"),),
            (feed_key, tier as u32, price_per_period, period_seconds),
        );
    }

    /// Admin: deactivate a subscription plan.
    pub fn deactivate_plan(env: Env, admin: Address, feed_key: Symbol, tier: SubscriptionTier) {
        admin.require_auth();
        Self::verify_admin(&env, &admin);

        let key = plan_key(&env, &feed_key, tier);
        let mut plan: SubscriptionPlan = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("Plan not found"));
        plan.active = false;
        env.storage().instance().set(&key, &plan);
    }

    /// Subscribe (or renew) to an oracle feed for one period.
    /// Caller must have already transferred `plan.price_per_period` to this contract.
    /// For simplicity on Soroban, payment is verified off-chain / via token contract
    /// before calling; the contract records the subscription state.
    pub fn subscribe(
        env: Env,
        subscriber: Address,
        feed_key: Symbol,
        tier: SubscriptionTier,
        auto_renew: bool,
    ) {
        subscriber.require_auth();

        let key = plan_key(&env, &feed_key, tier);
        let plan: SubscriptionPlan = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("Plan not found"));

        if !plan.active {
            panic!("Plan is not active");
        }

        let now = env.ledger().timestamp();
        let sub_key = subscription_key(&env, &subscriber, &feed_key);

        // If an active subscription exists, extend from its expiry; else start now.
        let existing: Option<Subscription> = env.storage().instance().get(&sub_key);
        let base = match &existing {
            Some(s) if s.expires_at > now => s.expires_at,
            _ => now,
        };

        let sub = Subscription {
            subscriber: subscriber.clone(),
            feed_key: feed_key.clone(),
            tier,
            expires_at: base + plan.period_seconds,
            calls_used: 0,
            calls_limit: plan.max_calls_per_period,
            auto_renew,
            created_at: existing.as_ref().map(|s| s.created_at).unwrap_or(now),
        };

        env.storage().instance().set(&sub_key, &sub);
        // Reset usage counter for the new period
        let u_key = usage_key(&env, &subscriber, &feed_key);
        env.storage().instance().set(&u_key, &0u32);

        env.events().publish(
            (Symbol::new(&env, "subscribed"),),
            (subscriber, feed_key, tier as u32, sub.expires_at),
        );
    }

    /// Renew an existing subscription for one more period (same tier).
    /// Mirrors subscribe but requires the subscription to already exist.
    pub fn renew(
        env: Env,
        subscriber: Address,
        feed_key: Symbol,
    ) {
        subscriber.require_auth();

        let sub_key = subscription_key(&env, &subscriber, &feed_key);
        let sub: Subscription = env
            .storage()
            .instance()
            .get(&sub_key)
            .unwrap_or_else(|| panic!("No subscription found"));

        // Delegate to subscribe with same tier and auto_renew setting
        Self::subscribe(env, subscriber, feed_key, sub.tier, sub.auto_renew);
    }

    /// Cancel a subscription (disables auto-renew; access valid until expiry).
    pub fn cancel(env: Env, subscriber: Address, feed_key: Symbol) {
        subscriber.require_auth();

        let sub_key = subscription_key(&env, &subscriber, &feed_key);
        let mut sub: Subscription = env
            .storage()
            .instance()
            .get(&sub_key)
            .unwrap_or_else(|| panic!("No subscription found"));

        sub.auto_renew = false;
        env.storage().instance().set(&sub_key, &sub);

        env.events().publish(
            (Symbol::new(&env, "sub_cancelled"),),
            (subscriber, feed_key),
        );
    }

    /// Check whether a subscriber currently has valid access to a feed.
    pub fn check_access(env: Env, subscriber: Address, feed_key: Symbol) -> bool {
        let sub_key = subscription_key(&env, &subscriber, &feed_key);
        let sub: Option<Subscription> = env.storage().instance().get(&sub_key);
        match sub {
            Some(s) => {
                let now = env.ledger().timestamp();
                s.expires_at > now && s.calls_used < s.calls_limit
            }
            None => false,
        }
    }

    /// Increment usage counter for a subscriber's feed; panics if no valid access.
    /// Called by oracle data consumers to track billing usage.
    pub fn track_usage(env: Env, subscriber: Address, feed_key: Symbol) {
        subscriber.require_auth();

        if !Self::check_access(env.clone(), subscriber.clone(), feed_key.clone()) {
            panic!("No active subscription or quota exceeded");
        }

        let sub_key = subscription_key(&env, &subscriber, &feed_key);
        let mut sub: Subscription = env.storage().instance().get(&sub_key).unwrap();
        sub.calls_used += 1;
        env.storage().instance().set(&sub_key, &sub);

        let u_key = usage_key(&env, &subscriber, &feed_key);
        let current: u32 = env.storage().instance().get(&u_key).unwrap_or(0);
        env.storage().instance().set(&u_key, &(current + 1));
    }

    /// Return the current subscription record for a subscriber/feed pair.
    pub fn get_subscription(env: Env, subscriber: Address, feed_key: Symbol) -> Option<Subscription> {
        let sub_key = subscription_key(&env, &subscriber, &feed_key);
        env.storage().instance().get(&sub_key)
    }

    /// Return the subscription plan for a feed/tier.
    pub fn get_plan(
        env: Env,
        feed_key: Symbol,
        tier: SubscriptionTier,
    ) -> Option<SubscriptionPlan> {
        let key = plan_key(&env, &feed_key, tier);
        env.storage().instance().get(&key)
    }
}
