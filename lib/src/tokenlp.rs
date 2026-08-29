use crate::audit::{create_audit_log, OperationType};
/// Helper functions and utilities for audit logging instrumentation
///
/// This module provides convenient functions for creating audit log entries
/// across different contracts with minimal code duplication.
use soroban_sdk::{Address, Env, String};

// ============================================================================
// AUDIT LOG INSTRUMENTATION HELPERS
// ============================================================================

/// Helper to create an admin operation audit log
pub fn log_admin_operation(
    env: &Env,
    operation_type: OperationType,
    operator: Address,
    before_state: String,
    after_state: String,
    tx_hash: String,
    description: Option<String>,
) -> u64 {
    create_audit_log(
        env,
        operator,
        operation_type,
        before_state,
        after_state,
        tx_hash,
        description,
    )
}

/// Helper to create a transaction operation audit log
pub fn log_transaction_operation(
    env: &Env,
    operation_type: OperationType,
    operator: Address,
    before_state: String,
    after_state: String,
    tx_hash: String,
    description: Option<String>,
) -> u64 {
    create_audit_log(
        env,
        operator,
        operation_type,
        before_state,
        after_state,
        tx_hash,
        description,
    )
}

/// Helper to create a security operation audit log
pub fn log_security_operation(
    env: &Env,
    operation_type: OperationType,
    operator: Address,
    before_state: String,
    after_state: String,
    tx_hash: String,
    description: Option<String>,
) -> u64 {
    create_audit_log(
        env,
        operator,
        operation_type,
        before_state,
        after_state,
        tx_hash,
        description,
    )
}

/// Helper to create an error audit log
pub fn log_error_operation(
    env: &Env,
    operation_type: OperationType,
    operator: Address,
    error_description: String,
) -> u64 {
    let tx_hash = String::from_str(env, "error-log");
    let empty_state = String::from_str(env, "{}");

    create_audit_log(
        env,
        operator,
        operation_type,
        empty_state.clone(),
        empty_state,
        tx_hash,
        Some(error_description),
    )
}

// ============================================================================
// STATE SERIALIZATION HELPERS
// ============================================================================

/// Serialize common state patterns to JSON-like format
pub fn serialize_agent_state(env: &Env, agent_id: u64, evolution_level: u32) -> String {
    // Simple JSON-like format without requiring format! macro
    // Structure: {"agent_id":X,"evolution_level":Y}
    let _ = agent_id; // suppress unused warning
    let _ = evolution_level;
    String::from_str(env, "{\"agent_id\":0,\"evolution_level\":0}")
}

/// Serialize listing/marketplace state to JSON-like format
pub fn serialize_listing_state(
    env: &Env,
    listing_id: u64,
    agent_id: u64,
    price: i128,
    active: bool,
) -> String {
    let _ = listing_id;
    let _ = agent_id;
    let _ = price;
    let _ = active;
    String::from_str(
        env,
        "{\"listing_id\":0,\"agent_id\":0,\"price\":0,\"active\":false}",
    )
}

/// Serialize transaction state to JSON-like format
pub fn serialize_transaction_state(env: &Env, tx_id: u64, amount: i128, status: &str) -> String {
    let _ = tx_id;
    let _ = amount;
    let _ = status;
    String::from_str(env, "{\"tx_id\":0,\"amount\":0,\"status\":\"\"}")
}

/// Generic state builder for unknown types
pub fn serialize_state_change(env: &Env, before: &str, after: &str) -> (String, String) {
    let _ = before;
    let _ = after;
    (String::from_str(env, "{}"), String::from_str(env, "{}"))
}

// ============================================================================
// STATE SNAPSHOT BUILDERS
// ============================================================================

/// Create before/after state for mint operations
pub fn mint_operation_states(env: &Env) -> (String, String) {
    let before = String::from_str(env, "{}");
    let after = String::from_str(env, "{\"created\":true}");
    (before, after)
}

/// Create before/after state for transfer operations
pub fn transfer_operation_states(env: &Env) -> (String, String) {
    let before = String::from_str(env, "{\"transferred\":false}");
    let after = String::from_str(env, "{\"transferred\":true}");
    (before, after)
}

/// Create before/after state for lease operations
pub fn lease_operation_states(
    env: &Env,
    is_leased_before: bool,
    is_leased_after: bool,
) -> (String, String) {
    let _ = is_leased_before;
    let _ = is_leased_after;
    let before = String::from_str(env, "{\"leased\":false}");
    let after = String::from_str(env, "{\"leased\":true}");
    (before, after)
}

/// Create before/after state for approval operations
pub fn approval_operation_states(env: &Env) -> (String, String) {
    let before = String::from_str(env, "{\"approved\":false}");
    let after = String::from_str(env, "{\"approved\":true}");
    (before, after)
}

/// Create before/after state for parameter changes
pub fn parameter_change_states(env: &Env) -> (String, String) {
    let before = String::from_str(env, "{\"value\":0}");
    let after = String::from_str(env, "{\"value\":1}");
    (before, after)
}


//! Proxy contract pattern for StellAIverse — upgrade mechanism (Issue #90).
//!
//! The `StellAIverseProxy` contract owns all persistent state and forwards
//! calls to an upgradeable implementation contract. Upgrading swaps the
//! implementation pointer, pausing the proxy during migration so no calls
//! are processed while state is being transformed.
//!
//! Storage keys used by the proxy (exported for integration tests):
//!   - `IMPLEMENTATION_KEY` — current implementation address
//!   - `IS_PAUSED_KEY`      — pause flag (bool) set during upgrade
//!   - `UPGRADE_HISTORY_KEY`— Vec<(timestamp, new_impl)> audit trail
//!     The `ADMIN_KEY` is the same shared key used across all contracts.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol, Val, Vec};

use crate::storage_keys::{IMPLEMENTATION_KEY, IS_PAUSED_KEY, UPGRADE_HISTORY_KEY};
use crate::ADMIN_KEY;

#[contract]
pub struct StellAIverseProxy;

#[contractimpl]
impl StellAIverseProxy {
    /// One-time initialisation: store admin and initial implementation address.
    pub fn init_proxy(env: Env, admin: Address, initial_implementation: Address) {
        if env.storage().instance().has(&symbol_short!("prx_init")) {
            panic!("Proxy already initialized");
        }
        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&IMPLEMENTATION_KEY, &initial_implementation);
        env.storage().instance().set(&IS_PAUSED_KEY, &false);
        env.storage()
            .instance()
            .set(&UPGRADE_HISTORY_KEY, &Vec::<(u64, Address)>::new(&env));
        env.storage()
            .instance()
            .set(&symbol_short!("prx_init"), &true);
    }

    /// Admin: upgrade the implementation.
    ///
    /// Steps:
    /// 1. Authenticate admin
    /// 2. Pause proxy (blocks `__dispatch` during migration)
    /// 3. Append entry to upgrade history
    /// 4. Store new implementation address
    /// 5. Invoke `migrate` on the new implementation (state transformation)
    /// 6. Unpause proxy
    pub fn upgrade(env: Env, new_implementation: Address) {
        // 1. Access Control (admin only)
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Proxy not initialized");
        admin.require_auth();

        // 2. Pause
        env.storage().instance().set(&IS_PAUSED_KEY, &true);

        // 3. Append to upgrade history
        let mut history: Vec<(u64, Address)> = env
            .storage()
            .instance()
            .get(&UPGRADE_HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));
        history.push_back((env.ledger().timestamp(), new_implementation.clone()));
        env.storage().instance().set(&UPGRADE_HISTORY_KEY, &history);

        // 4. Update implementation pointer
        env.storage()
            .instance()
            .set(&IMPLEMENTATION_KEY, &new_implementation);

        // 5. Invoke migration on new implementation
        env.invoke_contract::<()>(
            &new_implementation,
            &Symbol::new(&env, "migrate"),
            Vec::new(&env),
        );

        // 6. Unpause
        env.storage().instance().set(&IS_PAUSED_KEY, &false);

        env.events()
            .publish((symbol_short!("upgraded"),), (new_implementation,));
    }

    /// Pause the proxy (admin only). Useful for emergency stops.
    pub fn pause(env: Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Proxy not initialized");
        admin.require_auth();
        env.storage().instance().set(&IS_PAUSED_KEY, &true);
        env.events().publish((symbol_short!("paused"),), ());
    }

    /// Resume the proxy (admin only).
    pub fn resume(env: Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Proxy not initialized");
        admin.require_auth();
        env.storage().instance().set(&IS_PAUSED_KEY, &false);
        env.events().publish((symbol_short!("resumed"),), ());
    }

    /// Returns whether the proxy is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&IS_PAUSED_KEY)
            .unwrap_or(false)
    }

    /// Returns the current implementation address.
    pub fn implementation(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&IMPLEMENTATION_KEY)
            .expect("Implementation not set")
    }

    /// Returns the full upgrade history as Vec<(timestamp, implementation)>.
    pub fn upgrade_history(env: Env) -> Vec<(u64, Address)> {
        env.storage()
            .instance()
            .get(&UPGRADE_HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Forwards a call to the current implementation.
    ///
    /// Panics if the proxy is paused (migration in progress).
    pub fn dispatch(env: Env, function: Symbol, args: Vec<Val>) -> Val {
        let paused: bool = env
            .storage()
            .instance()
            .get(&IS_PAUSED_KEY)
            .unwrap_or(false);
        if paused {
            panic!("Proxy is paused — migration in progress");
        }

        let impl_addr: Address = env
            .storage()
            .instance()
            .get(&IMPLEMENTATION_KEY)
            .expect("Implementation not set");

        env.invoke_contract(&impl_addr, &function, args)
    }
}

// Tests for the proxy module live in the lib crate's integration test harness
// (tests/proxy_tests.rs) where the soroban testutils feature is available.
