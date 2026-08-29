#![no_std]
pub mod admin;
pub mod atomic;
pub mod audit;
pub mod audit_helpers;
pub mod errors;
pub mod proxy;
pub mod rbac;
pub mod storage_keys;
pub mod types;
pub mod validation;

pub use storage_keys::*;
pub use types::*;

/// Constants for security hardening
// Config
pub const ADMIN_KEY: &str = "admin";
pub const MAX_STRING_LENGTH: u32 = 256;
pub const MAX_ROYALTY_FEE: u32 = 10000;
pub const MAX_DATA_SIZE: u32 = 65536;
pub const MAX_HISTORY_SIZE: u32 = 1000;
pub const MAX_HISTORY_QUERY_LIMIT: u32 = 500;
pub const DEFAULT_RATE_LIMIT_OPERATIONS: u32 = 100;
pub const DEFAULT_RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_ROYALTY_PERCENTAGE: u32 = 10000; // 100%
pub const MIN_ROYALTY_PERCENTAGE: u32 = 0;
pub const SAFE_ARITHMETIC_CHECK_OVERFLOW: u128 = u128::MAX;
pub const PRICE_UPPER_BOUND: i128 = i128::MAX / 2; // Prevent overflow in calculations
pub const PRICE_LOWER_BOUND: i128 = 0; // Prevent negative prices
pub const MAX_DURATION_DAYS: u64 = 36500; // ~100 years max lease duration
pub const MAX_AGE_SECONDS: u64 = 365 * 24 * 60 * 60; // ~1 year max data age
pub const ATTESTATION_SIGNATURE_SIZE: usize = 64; // Ed25519 signature size
pub const MAX_ATTESTATION_DATA_SIZE: usize = 1024; // Max size for attestation data

// Approval constants
pub const DEFAULT_APPROVAL_THRESHOLD: i128 = 10_000_000_000; // 10,000 USDC in stroops (assuming 7 decimals)
pub const DEFAULT_APPROVERS_REQUIRED: u32 = 2; // N of M
pub const DEFAULT_TOTAL_APPROVERS: u32 = 3; // Total authorized approvers
pub const DEFAULT_APPROVAL_TTL_SECONDS: u64 = 604800; // 7 days

// Lease config: basis points (bps). 1000 bps = 10%.
pub const DEFAULT_LEASE_DEPOSIT_BPS: u32 = 1000; // 10% of lease value
pub const DEFAULT_EARLY_TERMINATION_PENALTY_BPS: u32 = 2000; // 20% of remaining value
pub const LEASE_EXTENSION_REQUEST_TTL_SECONDS: u64 = 604_800; // 7 days

// Transaction constants
pub const TRANSACTION_TIMEOUT_SECONDS: u64 = 300; // 5 minutes
pub const MAX_TRANSACTION_STEPS: u32 = 10; // Prevent DoS
pub const TRANSACTION_COUNTER_KEY: &str = "tx_counter";
pub const TRANSACTION_KEY_PREFIX: &str = "tx_";
pub const TRANSACTION_JOURNAL_KEY_PREFIX: &str = "tx_journal_";
pub const MAX_ROLLBACK_ATTEMPTS: u32 = 3;
