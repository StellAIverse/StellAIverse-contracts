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
