# Audit Log Format Specification

## Overview

The audit logging system provides comprehensive, immutable tracking of all critical operations across stellAIverse contracts. This document specifies the structure, usage, and verification procedures for audit logs.

## AuditLog Struct

### Definition

```rust
pub struct AuditLog {
    /// Auto-incrementing unique identifier, guaranteed sequential
    pub id: u64,
    
    /// Block/Ledger timestamp (in seconds) at time of operation
    pub timestamp: u64,
    
    /// Address that triggered the operation
    pub operator: Address,
    
    /// Categorized operation type (see Operation Type Categories below)
    pub operation_type: OperationType,
    
    /// JSON-serialized snapshot of relevant state BEFORE the operation
    pub before_state: String,
    
    /// JSON-serialized snapshot of relevant state AFTER the operation
    pub after_state: String,
    
    /// Transaction hash for cross-referencing with blockchain
    pub tx_hash: String,
    
    /// Optional human-readable description of the operation
    pub description: Option<String>,
}
```

### Field Descriptions

#### id
- **Type**: `u64`
- **Immutable**: Yes
- **Uniqueness**: Guaranteed sequential, globally unique
- **Assigned**: Automatically at log creation
- **Purpose**: Enables pagination and log ordering

#### timestamp
- **Type**: `u64`
- **Unit**: Seconds (Unix epoch or Stellar ledger timestamp)
- **Immutable**: Yes
- **Source**: Captured from `env.ledger().timestamp()`
- **Purpose**: Enables temporal analysis and ordering

#### operator
- **Type**: Address (Stellar account or contract)
- **Immutable**: Yes
- **Purpose**: Tracks which entity initiated the operation
- **Format**: Stellar address (e.g., `GABC...`)

#### operation_type
- **Type**: `OperationType` enum
- **Immutable**: Yes
- **Purpose**: Categorizes operation for filtering and analysis
- **Categories**: Admin, Transaction, Security, Configuration, Error

#### before_state
- **Type**: JSON-serialized String
- **Immutable**: Yes
- **Purpose**: Records state snapshot before operation
- **Format**: JSON object with relevant fields
- **Example**: `{"owner":"GABC...","leased":false}`

#### after_state
- **Type**: JSON-serialized String
- **Immutable**: Yes
- **Purpose**: Records state snapshot after operation
- **Format**: JSON object with relevant fields
- **Example**: `{"owner":"GDEF...","leased":true}`

#### tx_hash
- **Type**: String
- **Immutable**: Yes
- **Purpose**: Cross-reference with blockchain transaction
- **Format**: Hex-encoded transaction hash (0x-prefixed)

#### description
- **Type**: `Option<String>`
- **Immutable**: Yes
- **Purpose**: Optional contextual information
- **Max Length**: Recommended 256 characters
- **Examples**: "Minted agent #1234", "Transfer failed - insufficient balance"

## Operation Type Categories

All operations must be assigned one of the following categories:

### Admin Operations

Operations that modify contract configuration or permissions.

| Operation Type | Value | Emitted When |
|---|---|---|
| `AdminMint` | 1 | Agent NFT is minted by admin or approved minter |
| `AdminTransfer` | 2 | Agent ownership is transferred by admin or owner |
| `AdminApprove` | 3 | Approval/authorization is granted |
| `AdminSettingsChange` | 4 | Contract settings are modified |
| `AdminAddMinter` | 5 | Minter address is added to approved list |

### Transaction Operations

Financial and marketplace transactions.

| Operation Type | Value | Emitted When |
|---|---|---|
| `SaleCreated` | 10 | Marketplace listing is created |
| `SaleCompleted` | 11 | Marketplace sale transaction completes |
| `LeaseStarted` | 12 | Agent lease begins |
| `LeaseEnded` | 13 | Agent lease terminates |
| `RoyaltyPaid` | 14 | Royalty payment is distributed |
| `AuctionCreated` | 15 | Auction is created |
| `AuctionBidPlaced` | 16 | Bid is placed in auction |
| `AuctionEnded` | 17 | Auction concludes and winner is determined |

### Security Operations

Authorization, authentication, and permission checks.

| Operation Type | Value | Emitted When |
|---|---|---|
| `AuthFailure` | 20 | Authorization requirement fails |
| `PermissionCheck` | 21 | Permission check is performed (pass or fail) |
| `UnauthorizedAttempt` | 22 | Unauthorized action is attempted |

### Configuration Operations

System and contract configuration changes.

| Operation Type | Value | Emitted When |
|---|---|---|
| `ConfigurationChange` | 30 | General configuration parameter changes |
| `ParameterUpdate` | 31 | Specific parameter is updated |

### Error Operations

Unexpected failures and error conditions.

| Operation Type | Value | Emitted When |
|---|---|---|
| `ErrorOccurred` | 40 | Unexpected error or exception occurs |
| `ValidationFailed` | 41 | Input validation fails |
| `OverflowDetected` | 42 | Arithmetic overflow is detected |

## Query Function Reference

### query_audit_logs()

Paginated query across the audit log storage.

```rust
pub fn query_audit_logs(
    env: &Env,
    start_id: u64,    // Inclusive starting ID (0 defaults to 1)
    end_id: u64,      // Inclusive ending ID (0 uses total count)
    max_results: u32, // Maximum logs to return (0 defaults to 100)
) -> AuditLogQueryResult
```

#### Return Type

```rust
pub struct AuditLogQueryResult {
    pub logs: Vec<AuditLog>,    // Returned log entries
    pub total_count: u64,       // Total logs in system
    pub start_id: u64,          // Actual start ID used
    pub end_id: u64,            // Actual end ID queried
    pub has_more: bool,         // Whether more results exist
}
```

#### Usage Examples

**Query first 50 logs:**
```
query_audit_logs(env, 1, 50, 50)
```

**Query logs 100-149:**
```
query_audit_logs(env, 100, 149, 50)
```

**Query all remaining logs from ID 500:**
```
query_audit_logs(env, 500, 0, 0)  // 0 defaults to maximum
```

#### Pagination Pattern

```
1. Get total_count from first query
2. Calculate pages: total_count / page_size
3. For each page:
   - start_id = (page_num - 1) * page_size + 1
   - end_id = page_num * page_size
   - Call query_audit_logs(start_id, end_id, page_size)
   - If has_more == false, stop
```

#### Error Handling

- **Out-of-range start_id**: Returns empty Vec with has_more=false
- **Out-of-range end_id**: Clamps to total_count
- **start_id > end_id**: Returns empty Vec
- **Empty audit log**: Returns empty Vec with total_count=0

## State Snapshot Format

### JSON Structure

State snapshots use JSON format for consistency and external auditability.

#### Agent State Example
```json
{
  "agent_id": 1234,
  "owner": "GABC...",
  "name": "Agent Alpha",
  "evolution_level": 5,
  "leased": false,
  "created_at": 1704067200
}
```

#### Listing/Marketplace State Example
```json
{
  "listing_id": 5678,
  "agent_id": 1234,
  "seller": "GABC...",
  "price": 1000000000,
  "listing_type": "Sale",
  "active": true,
  "created_at": 1704067200
}
```

#### Transaction State Example
```json
{
  "tx_id": 9999,
  "from": "GABC...",
  "to": "GDEF...",
  "amount": 500000000,
  "status": "completed",
  "timestamp": 1704067201
}
```

### Minimal State Representation

For lightweight state tracking, minimal JSON can be used:
```json
{"field": "value"}
```

## Export Format Specification

### AuditLogExportEntry

Export format converts all fields to strings for signing and external use.

```rust
pub struct AuditLogExportEntry {
    pub id: String,                      // "1"
    pub timestamp: String,               // "1704067200"
    pub operator: String,                // "GABC..."
    pub operation_type: String,          // "AdminMint"
    pub before_state: String,            // JSON as string
    pub after_state: String,             // JSON as string
    pub tx_hash: String,                 // "0x..."
    pub description: Option<String>,     // Optional description
}
```

### Export Function

```rust
pub fn export_audit_logs(
    env: &Env,
    start_id: u64,
    end_id: u64,
    max_results: u32,
) -> Vec<AuditLogExportEntry>
```

### CSV Export Format (Recommended for External Auditors)

```
id,timestamp,operator,operation_type,before_state,after_state,tx_hash,description
1,1704067200,GABC...,AdminMint,{},"{""created"":true}",0x...,Minted agent
2,1704067201,GABC...,SaleCreated,{},"{""listing"":1}",0x...,Created listing
```

### JSON Export Format

```json
{
  "export_metadata": {
    "exported_at": 1704067300,
    "exporter": "GABC...",
    "start_id": 1,
    "end_id": 100,
    "total_exported": 100,
    "signature": "0x..."
  },
  "audit_logs": [
    {
      "id": "1",
      "timestamp": "1704067200",
      "operator": "GABC...",
      "operation_type": "AdminMint",
      "before_state": "{}",
      "after_state": "{\"created\":true}",
      "tx_hash": "0x...",
      "description": "Minted agent"
    }
  ]
}
```

## Signed Export Verification

### Signing Process

1. **Collect logs** using `export_audit_logs()`
2. **Create payload** with all fields in consistent order
3. **Sign payload** using contract's private key
4. **Include signature** with exported data

### Payload Format for Signing

```
AUDIT_EXPORT_V1
ExportedAt={timestamp}
StartId={start_id}
EndId={end_id}
TotalCount={count}
Entries=[
  {id}|{timestamp}|{operator}|{operation_type}|{tx_hash}
  ...
]
```

### Verification Instructions

For external auditors:

1. **Obtain signature** and export data
2. **Reconstruct payload** using same format
3. **Verify signature** against contract's public key
4. **Check entry hashes** for consistency
5. **Cross-reference tx_hash** with blockchain

### Signature Verification Example

```rust
// Pseudocode for external verification
let public_key = contract_address.get_public_key();
let payload = reconstruct_payload(&export_data);
let is_valid = verify_signature(&payload, &signature, public_key);
```

## Storage Namespace

Audit logs are stored in a separate namespace to prevent interference with contract state:

- **Namespace**: `audit_log_*`
- **Counter key**: `audit_log_id_counter`
- **Entry key format**: `(audit_log_entry, {id})`
- **Storage layer**: Persistent storage
- **Durability**: Permanent (no expiration or archival)

## Retention Policy

### Permanent Retention

All audit logs are permanently retained without deletion or modification.

**Enforcement:**
- Logs are write-once, immutable after creation
- No deletion operations available
- Counter never decrements
- Persistent storage ensures durability across ledger reset

### Storage Optimization for Large Volumes

For systems with 1M+ entries, consider:

1. **Batching**: Group 1000+ entries per block
2. **Compression**: Compress old entries (>1 year) into archives
3. **Archival**: Export and store externally (IPFS, S3)
4. **Merkle Trees**: Batch entries into tree structures for verification
5. **Pruning Metadata**: Keep references to archived data

**Example Metadata:**
```json
{
  "archived_batches": [
    {
      "batch_id": 1,
      "start_id": 1,
      "end_id": 100000,
      "ipfs_cid": "QmXXX...",
      "merkle_root": "0x..."
    }
  ],
  "active_start_id": 100001
}
```

## Integration Examples

### Emitting an Admin Operation Log

```rust
// In AgentNFT::mint_agent()
let log_id = create_audit_log(
    &env,
    operator.clone(),
    OperationType::AdminMint,
    before_state,
    after_state,
    tx_hash,
    Some(String::from_slice(&env, "Minted agent NFT")),
);
```

### Emitting a Transaction Log

```rust
// In Marketplace::create_listing()
let log_id = create_audit_log(
    &env,
    seller.clone(),
    OperationType::SaleCreated,
    before_state,
    after_state,
    tx_hash,
    Some(description),
);
```

### Emitting a Security Log

```rust
// On authorization failure
let log_id = create_audit_log(
    &env,
    unauthorized_actor.clone(),
    OperationType::AuthFailure,
    empty_state,
    empty_state,
    tx_hash,
    Some(String::from_slice(&env, "Unauthorized access attempt")),
);
```

### Querying Audit Logs

```rust
// Get first 100 logs
let result = query_audit_logs(&env, 1, 100, 100);

// Get logs from specific range
let result = query_audit_logs(&env, 500, 599, 100);

// Export for audit
let export = export_audit_logs(&env, 1, 1000, 1000);
```

## Best Practices

1. **Always provide before_state and after_state** for complete audit trail
2. **Use consistent JSON formatting** for state snapshots
3. **Include meaningful descriptions** for complex operations
4. **Log both successes and failures** for security visibility
5. **Query regularly** to verify logging is functioning
6. **Archive exports periodically** to external storage
7. **Verify signatures** on exported logs for integrity
8. **Monitor log growth** for storage optimization needs
9. **Document custom OperationType usage** in contract-specific audit guides
10. **Test pagination** with various batch sizes for your use case

## Troubleshooting

### Issue: Logs not appearing
- Verify `create_audit_log()` is called in operation handlers
- Check that operation_type is properly assigned
- Ensure storage namespace is not corrupted

### Issue: Pagination returns fewer results than expected
- Verify end_id >= start_id
- Check if result.has_more indicates more pages available
- Try smaller max_results value

### Issue: Export signature validation fails
- Verify public key matches contract address
- Ensure payload reconstruction matches exact format
- Check for timezone/timestamp discrepancies

## Version History

- **v1.0**: Initial audit logging specification
- Fields: id, timestamp, operator, operation_type, before_state, after_state, tx_hash, description
- Categories: Admin (5 types), Transaction (8 types), Security (3 types), Configuration (2 types), Error (3 types)
