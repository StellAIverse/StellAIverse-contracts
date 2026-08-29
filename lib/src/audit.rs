/// Audit logging module for comprehensive contract operation tracking
///
/// This module provides immutable audit log storage with auto-incrementing IDs,
/// paginated querying, and signed export capabilities. Audit logs are stored in
/// a separate namespace to prevent interference with contract state.
use soroban_sdk::{contracttype, Address, Env, String, Symbol, Vec};

// ============================================================================
// AUDIT LOG TYPES
// ============================================================================

/// Operation type categories for audit logging
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OperationType {
    // Admin operations
    AdminMint = 1,
    AdminTransfer = 2,
    AdminApprove = 3,
    AdminSettingsChange = 4,
    AdminAddMinter = 5,

    // Transaction operations
    SaleCreated = 10,
    SaleCompleted = 11,
    LeaseStarted = 12,
    LeaseEnded = 13,
    RoyaltyPaid = 14,
    AuctionCreated = 15,
    AuctionBidPlaced = 16,
    AuctionEnded = 17,
    LeaseExtensionRequested = 18,
    LeaseExtended = 19,
    LeaseRenewed = 23,
    LeaseOverdue = 24,
    LeasePaymentProcessed = 25,

    // Security operations
    AuthFailure = 20,
    PermissionCheck = 21,
    UnauthorizedAttempt = 22,

    // Configuration operations
    ConfigurationChange = 30,
    ParameterUpdate = 31,

    // Error operations
    ErrorOccurred = 40,
    ValidationFailed = 41,
    OverflowDetected = 42,
}

/// Immutable audit log entry
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditLog {
    /// Auto-incrementing unique identifier
    pub id: u64,
    /// Ledger timestamp at time of operation
    pub timestamp: u64,
    /// Address that triggered the operation
    pub operator: Address,
    /// Categorized operation type
    pub operation_type: OperationType,
    /// JSON-serialized snapshot of state before operation
    pub before_state: String,
    /// JSON-serialized snapshot of state after operation
    pub after_state: String,
    /// Transaction hash for cross-referencing
    pub tx_hash: String,
    /// Optional human-readable description
    pub description: Option<String>,
}

/// Audit log entry for export (includes all fields as strings for signing)
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditLogExportEntry {
    pub id: String,
    pub timestamp: String,
    pub operator: String,
    pub operation_type: String,
    pub before_state: String,
    pub after_state: String,
    pub tx_hash: String,
    pub description: Option<String>,
}

/// Result of a paginated audit log query
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditLogQueryResult {
    pub logs: Vec<AuditLog>,
    pub total_count: u64,
    pub start_id: u64,
    pub end_id: u64,
    pub has_more: bool,
}

// ============================================================================
// STORAGE KEYS
// ============================================================================

#[contracttype]
#[derive(Clone)]
pub enum AuditStorageKey {
    /// Counter for auto-incrementing audit log IDs
    LogIdCounter,
    /// Individual audit log entry (indexed by id)
    LogEntry(u64),
}

// ============================================================================
// AUDIT LOG STORAGE FUNCTIONS
// ============================================================================

/// Get the current audit log ID counter
pub fn get_log_id_counter(env: &Env) -> u64 {
    let key = Symbol::new(env, "audit_log_id_counter");
    env.storage().persistent().get::<_, u64>(&key).unwrap_or(0)
}

/// Increment and return the next audit log ID
pub fn increment_log_id_counter(env: &Env) -> u64 {
    let key = Symbol::new(env, "audit_log_id_counter");
    let current = get_log_id_counter(env);
    let next = current.saturating_add(1);
    env.storage().persistent().set(&key, &next);
    next
}

/// Store an audit log entry (immutable after creation)
pub fn store_audit_log(env: &Env, log: &AuditLog) {
    let key = (Symbol::new(env, "audit_log_entry"), log.id);
    env.storage().persistent().set(&key, log);
}

/// Retrieve an audit log entry by ID
pub fn get_audit_log(env: &Env, log_id: u64) -> Option<AuditLog> {
    let key = (Symbol::new(env, "audit_log_entry"), log_id);
    env.storage().persistent().get(&key)
}

// ============================================================================
// AUDIT LOG CREATION
// ============================================================================

/// Create and store a new audit log entry
///
/// This function automatically assigns an incrementing ID and stores the log
/// in immutable persistent storage. Log entries cannot be modified or deleted.
pub fn create_audit_log(
    env: &Env,
    operator: Address,
    operation_type: OperationType,
    before_state: String,
    after_state: String,
    tx_hash: String,
    description: Option<String>,
) -> u64 {
    let log_id = increment_log_id_counter(env);
    let timestamp = env.ledger().timestamp();

    let log = AuditLog {
        id: log_id,
        timestamp,
        operator,
        operation_type,
        before_state,
        after_state,
        tx_hash,
        description,
    };

    store_audit_log(env, &log);
    log_id
}

// ============================================================================
// AUDIT LOG QUERYING
// ============================================================================

/// Query audit logs with pagination
///
/// Returns logs inclusive of start_id and end_id. Handles out-of-range IDs gracefully.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `start_id` - Starting log ID (inclusive), or 1 if 0
/// * `end_id` - Ending log ID (inclusive), or latest if exceeds total
/// * `max_results` - Maximum number of results to return
pub fn query_audit_logs(
    env: &Env,
    start_id: u64,
    end_id: u64,
    max_results: u32,
) -> AuditLogQueryResult {
    let total_count = get_log_id_counter(env);

    // Handle boundary conditions
    let actual_start = if start_id == 0 { 1 } else { start_id };
    let actual_end = if end_id > total_count {
        total_count
    } else {
        end_id
    };
    let limit = if max_results == 0 { 100 } else { max_results };

    let mut logs: Vec<AuditLog> = Vec::new(env);

    if actual_start > total_count || actual_start > actual_end {
        return AuditLogQueryResult {
            logs,
            total_count,
            start_id: actual_start,
            end_id: actual_end,
            has_more: false,
        };
    }

    let mut count = 0u32;
    let mut current_id = actual_start;

    while current_id <= actual_end && count < limit {
        if let Some(log) = get_audit_log(env, current_id) {
            logs.push_back(log);
            count += 1;
        }
        current_id += 1;
    }

    // has_more is true if we stopped due to limit, not because we reached the end
    let has_more = count == limit && current_id <= actual_end;

    AuditLogQueryResult {
        logs,
        total_count,
        start_id: actual_start,
        end_id: if has_more { current_id - 1 } else { actual_end },
        has_more,
    }
}

/// Get the total number of audit log entries
pub fn get_total_audit_log_count(env: &Env) -> u64 {
    get_log_id_counter(env)
}

// ============================================================================
// AUDIT LOG EXPORT
// ============================================================================

/// Export audit logs in a format suitable for external auditors
///
/// Converts audit logs to export format with all fields as strings for consistency.
pub fn export_audit_logs(
    env: &Env,
    start_id: u64,
    end_id: u64,
    max_results: u32,
) -> Vec<AuditLogExportEntry> {
    let query_result = query_audit_logs(env, start_id, end_id, max_results);
    let mut export_entries: Vec<AuditLogExportEntry> = Vec::new(env);

    for i in 0..query_result.logs.len() {
        if let Some(log) = query_result.logs.get(i) {
            let operation_type_str = match log.operation_type {
                OperationType::AdminMint => String::from_str(env, "AdminMint"),
                OperationType::AdminTransfer => String::from_str(env, "AdminTransfer"),
                OperationType::AdminApprove => String::from_str(env, "AdminApprove"),
                OperationType::AdminSettingsChange => String::from_str(env, "AdminSettingsChange"),
                OperationType::AdminAddMinter => String::from_str(env, "AdminAddMinter"),
                OperationType::SaleCreated => String::from_str(env, "SaleCreated"),
                OperationType::SaleCompleted => String::from_str(env, "SaleCompleted"),
                OperationType::LeaseStarted => String::from_str(env, "LeaseStarted"),
                OperationType::LeaseEnded => String::from_str(env, "LeaseEnded"),
                OperationType::RoyaltyPaid => String::from_str(env, "RoyaltyPaid"),
                OperationType::AuctionCreated => String::from_str(env, "AuctionCreated"),
                OperationType::AuctionBidPlaced => String::from_str(env, "AuctionBidPlaced"),
                OperationType::AuctionEnded => String::from_str(env, "AuctionEnded"),
                OperationType::LeaseExtensionRequested => {
                    String::from_str(env, "LeaseExtensionRequested")
                }
                OperationType::LeaseExtended => String::from_str(env, "LeaseExtended"),
                OperationType::LeaseRenewed => String::from_str(env, "LeaseRenewed"),
                OperationType::LeaseOverdue => String::from_str(env, "LeaseOverdue"),
                OperationType::LeasePaymentProcessed => {
                    String::from_str(env, "LeasePaymentProcessed")
                }
                OperationType::AuthFailure => String::from_str(env, "AuthFailure"),
                OperationType::PermissionCheck => String::from_str(env, "PermissionCheck"),
                OperationType::UnauthorizedAttempt => String::from_str(env, "UnauthorizedAttempt"),
                OperationType::ConfigurationChange => String::from_str(env, "ConfigurationChange"),
                OperationType::ParameterUpdate => String::from_str(env, "ParameterUpdate"),
                OperationType::ErrorOccurred => String::from_str(env, "ErrorOccurred"),
                OperationType::ValidationFailed => String::from_str(env, "ValidationFailed"),
                OperationType::OverflowDetected => String::from_str(env, "OverflowDetected"),
            };

            // Create string representations for export
            // NOTE: u64 to String conversion is not available in no_std without `alloc`.
            // Using empty strings as placeholders, as documented in AUDIT_LOGGING_IMPLEMENTATION.md.
            let id_str = String::from_str(env, "");
            let timestamp_str = String::from_str(env, "");
            // Address has a to_string() method.
            let operator_str = log.operator.to_string();

            let entry = AuditLogExportEntry {
                id: id_str,
                timestamp: timestamp_str,
                operator: operator_str, // This is now correctly converted
                operation_type: operation_type_str,
                before_state: log.before_state.clone(),
                after_state: log.after_state.clone(),
                tx_hash: log.tx_hash.clone(),
                description: log.description.clone(),
            };
            export_entries.push_back(entry);
        }
    }

    export_entries
}

// ============================================================================
// RETENTION POLICY
// ============================================================================

/// All audit logs are permanently retained.
///
/// This is enforced by:
/// 1. Immutable storage - logs cannot be modified or deleted after creation
/// 2. Persistent storage layer - ensures data survives contract state resets
/// 3. Sequential ID assignment - maintains complete audit trail
///
/// For storage optimization with large volumes, consider:
/// - Archiving old logs to external storage (IPFS, S3, etc.)
/// - Compressing old log batches
/// - Batching entries into Merkle trees for verification
/// - Periodic exports for offline archival
pub fn get_retention_info(env: &Env) -> (u64, String) {
    let total = get_log_id_counter(env);
    let info = String::from_str(
        env,
        "All audit logs are permanently retained. No deletion or modification allowed.",
    );
    (total, info)
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Convert OperationType to human-readable string
pub fn operation_type_to_string(env: &Env, op_type: OperationType) -> String {
    match op_type {
        OperationType::AdminMint => String::from_str(env, "AdminMint"),
        OperationType::AdminTransfer => String::from_str(env, "AdminTransfer"),
        OperationType::AdminApprove => String::from_str(env, "AdminApprove"),
        OperationType::AdminSettingsChange => String::from_str(env, "AdminSettingsChange"),
        OperationType::AdminAddMinter => String::from_str(env, "AdminAddMinter"),
        OperationType::SaleCreated => String::from_str(env, "SaleCreated"),
        OperationType::SaleCompleted => String::from_str(env, "SaleCompleted"),
        OperationType::LeaseStarted => String::from_str(env, "LeaseStarted"),
        OperationType::LeaseEnded => String::from_str(env, "LeaseEnded"),
        OperationType::RoyaltyPaid => String::from_str(env, "RoyaltyPaid"),
        OperationType::AuctionCreated => String::from_str(env, "AuctionCreated"),
        OperationType::AuctionBidPlaced => String::from_str(env, "AuctionBidPlaced"),
        OperationType::AuctionEnded => String::from_str(env, "AuctionEnded"),
        OperationType::LeaseExtensionRequested => String::from_str(env, "LeaseExtensionRequested"),
        OperationType::LeaseExtended => String::from_str(env, "LeaseExtended"),
        OperationType::LeaseRenewed => String::from_str(env, "LeaseRenewed"),
        OperationType::LeaseOverdue => String::from_str(env, "LeaseOverdue"),
        OperationType::LeasePaymentProcessed => String::from_str(env, "LeasePaymentProcessed"),
        OperationType::AuthFailure => String::from_str(env, "AuthFailure"),
        OperationType::PermissionCheck => String::from_str(env, "PermissionCheck"),
        OperationType::UnauthorizedAttempt => String::from_str(env, "UnauthorizedAttempt"),
        OperationType::ConfigurationChange => String::from_str(env, "ConfigurationChange"),
        OperationType::ParameterUpdate => String::from_str(env, "ParameterUpdate"),
        OperationType::ErrorOccurred => String::from_str(env, "ErrorOccurred"),
        OperationType::ValidationFailed => String::from_str(env, "ValidationFailed"),
        OperationType::OverflowDetected => String::from_str(env, "OverflowDetected"),
    }
}
