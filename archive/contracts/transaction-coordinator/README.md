# Atomic Transaction Framework

The Atomic Transaction Framework provides two-phase commit protocol for coordinating operations across multiple Soroban contracts, ensuring either all operations succeed or all fail together.

## Overview

Currently, operations involving multiple contracts (e.g., marketplace sale + NFT transfer + royalty distribution) happen sequentially and aren't atomic. If one fails mid-way, funds or NFTs can be left in inconsistent states. This framework eliminates race conditions and prevents partial failures.

## Architecture

### Core Components

1. **TransactionCoordinator Contract**: Orchestrates atomic transactions using two-phase commit
2. **AtomicTransaction Trait**: Interface for contracts to support atomic operations
3. **Transaction Workflows**: Pre-built workflows for common operations
4. **Transaction Journal**: Audit trail for recovery and replay

### Transaction States

```
Initiated → Preparing → Prepared → Committing → Committed
                    ↓
                RollingBack → RolledBack
                    ↓
                  Failed
                    ↓
                TimedOut
```

### Two-Phase Commit Protocol

**Phase 1: Prepare**
- Validate all steps can be executed
- Lock necessary resources
- Return prepare/abort decision

**Phase 2: Commit/Rollback**
- If all steps prepared successfully: commit all steps
- If any step failed to prepare: rollback all prepared steps

## Key Features

✅ **Atomic Operations**: All-or-nothing execution across multiple contracts  
✅ **Dependency Resolution**: Steps can depend on outputs from previous steps  
✅ **Timeout Handling**: 5-minute deadline prevents stuck transactions  
✅ **Rollback Mechanism**: Automatic cleanup on failures  
✅ **Transaction Journal**: Complete audit trail for recovery  
✅ **Deadlock Prevention**: Dependency validation prevents circular dependencies  
✅ **Replay Capability**: Journal enables transaction recovery  

## Usage

### 1. Basic Transaction Creation

```rust
use stellai_lib::{TransactionStep, AtomicTransaction};

// Create transaction steps
let steps = Vec::from_array(&env, [
    TransactionStep {
        step_id: 1,
        contract: marketplace_contract,
        function: Symbol::new(&env, "validate_listing"),
        args: vec![&env, listing_id.into(), agent_id.into(), price.into()],
        depends_on: None,
        rollback_contract: Some(marketplace_contract),
        rollback_function: Some(Symbol::new(&env, "unlock_listing")),
        rollback_args: Some(vec![&env, listing_id.into()]),
        executed: false,
        result: None,
    },
    // ... more steps
]);

// Create and execute transaction
let tx_id = coordinator.create_transaction(&initiator, &steps);
let success = coordinator.execute_transaction(&tx_id, &initiator);
```

### 2. Using Pre-built Workflows

```rust
use crate::workflows::AtomicAgentSaleWorkflow;

// Create atomic agent sale transaction
let steps = AtomicAgentSaleWorkflow::create_sale_transaction(
    &env,
    buyer,
    seller,
    agent_id,
    listing_id,
    price,
    marketplace_contract,
    agent_nft_contract,
    payment_token_contract,
    Some(royalty_recipient),
    Some(royalty_fee_bps),
);

let tx_id = coordinator.create_transaction(&buyer, &steps);
let success = coordinator.execute_transaction(&tx_id, &buyer);
```

### 3. Implementing Atomic Support in Contracts

```rust
use stellai_lib::atomic::AtomicTransactionSupport;

impl AtomicTransactionSupport for MyContract {
    fn prepare_step(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        function: &Symbol,
        args: &Vec<Val>,
    ) -> bool {
        match function.to_string().as_str() {
            "my_function" => {
                // Validate and lock resources
                // Return true if step can be committed
                true
            }
            _ => false,
        }
    }

    fn commit_step(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        function: &Symbol,
        args: &Vec<Val>,
    ) -> Val {
        match function.to_string().as_str() {
            "my_function" => {
                // Execute the prepared step
                // Return result value
                Val::from_bool(true)
            }
            _ => Val::from_bool(false),
        }
    }

    fn rollback_step(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        rollback_function: &Symbol,
        rollback_args: &Vec<Val>,
    ) -> bool {
        // Undo the effects of the committed step
        true
    }
}
```

## Pre-built Workflows

### 1. Atomic Agent Sale

Coordinates: listing validation → payment transfer → NFT transfer → royalty distribution → sale completion

```rust
let steps = AtomicAgentSaleWorkflow::create_sale_transaction(
    &env, buyer, seller, agent_id, listing_id, price,
    marketplace_contract, agent_nft_contract, payment_token_contract,
    Some(royalty_recipient), Some(royalty_fee_bps)
);
```

### 2. Atomic Agent Lease

Coordinates: lease validation → payment + deposit → access grant → lease record creation

```rust
let steps = AtomicAgentSaleWorkflow::create_lease_transaction(
    &env, lessee, lessor, agent_id, listing_id, lease_price, duration_seconds, deposit_amount,
    marketplace_contract, agent_nft_contract, payment_token_contract
);
```

### 3. Atomic Evolution Upgrade

Coordinates: validation → stake locking → model update → evolution recording

```rust
let steps = AtomicAgentSaleWorkflow::create_evolution_transaction(
    &env, owner, agent_id, stake_amount, new_model_hash,
    evolution_contract, agent_nft_contract, stake_token_contract
);
```

## Configuration

### Transaction Limits

```rust
pub const TRANSACTION_TIMEOUT_SECONDS: u64 = 300; // 5 minutes
pub const MAX_TRANSACTION_STEPS: u32 = 10;        // Prevent DoS
pub const MAX_ROLLBACK_ATTEMPTS: u32 = 3;         // Retry limit
```

### Storage Keys

```rust
pub const TRANSACTION_COUNTER_KEY: &str = "tx_counter";
pub const TRANSACTION_KEY_PREFIX: &str = "tx_";
pub const TRANSACTION_JOURNAL_KEY_PREFIX: &str = "tx_journal_";
```

## Error Handling

The framework provides comprehensive error handling:

- **NotInitialized**: Contract not properly initialized
- **Unauthorized**: Caller not authorized for operation
- **TransactionNotFound**: Invalid transaction ID
- **InvalidTransactionState**: Transaction in wrong state for operation
- **TransactionTimedOut**: Transaction exceeded 5-minute deadline
- **StepPreparationFailed**: Step failed during prepare phase
- **StepCommitFailed**: Step failed during commit phase
- **RollbackFailed**: Rollback operation failed
- **InvalidDependency**: Circular or invalid step dependency
- **TooManySteps**: Exceeds maximum step limit
- **CircularDependency**: Steps have circular dependencies

## Testing

### Unit Tests

```bash
cargo test --package transaction-coordinator
```

### Integration Tests

```bash
cargo test --package transaction-coordinator integration_test
```

### Test Coverage

- ✅ Transaction state machine
- ✅ Two-phase commit protocol
- ✅ Dependency resolution
- ✅ Timeout handling
- ✅ Rollback mechanisms
- ✅ Concurrent transactions
- ✅ Error scenarios
- ✅ Workflow integration

## Security Considerations

1. **Authorization**: Only transaction initiator can execute their transactions
2. **Timeout Enforcement**: Prevents resource locking indefinitely
3. **Dependency Validation**: Prevents circular dependencies and deadlocks
4. **Atomic State Management**: Ensures consistent state across all contracts
5. **Audit Trail**: Complete transaction journal for forensics
6. **Resource Locking**: Prevents concurrent access to locked resources
7. **Rollback Safety**: Guaranteed cleanup on failures

## Performance

- **Prepare Phase**: O(n) where n = number of steps
- **Commit Phase**: O(n) where n = number of steps
- **Rollback Phase**: O(n) where n = number of executed steps
- **Dependency Resolution**: O(n²) worst case for complex dependencies
- **Storage**: O(n) per transaction for journal entries

## Deployment

1. Deploy TransactionCoordinator contract
2. Initialize with admin address
3. Update existing contracts to implement AtomicTransactionSupport
4. Deploy updated contracts
5. Configure transaction workflows

## Examples

See `integration_test.rs` for comprehensive examples of:
- Creating atomic transactions
- Using pre-built workflows
- Handling failures and rollbacks
- Testing concurrent transactions
- Dependency resolution

## Roadmap

- [ ] Cross-chain atomic transactions
- [ ] Advanced dependency patterns
- [ ] Transaction batching optimization
- [ ] Automated recovery mechanisms
- [ ] Performance monitoring and metrics
- [ ] Transaction fee optimization
- [ ] Advanced rollback strategies

## Contributing

1. Implement AtomicTransactionSupport in your contract
2. Add rollback functions for all state-changing operations
3. Write comprehensive tests for prepare/commit/rollback phases
4. Update documentation with new workflow patterns
5. Submit PR with integration tests

## License

MIT License - see LICENSE file for details.