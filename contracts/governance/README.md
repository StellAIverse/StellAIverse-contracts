# StellAIverse Governance Voting Contract

A robust on-chain governance system that enables token holders to propose, vote on, and execute protocol changes with secure timelocking and delegation mechanisms.

## Features

### Core Governance Functionality
- **Proposal Creation**: Users with sufficient tokens can create proposals with executable actions
- **Flexible Voting**: Support for For/Against/Abstain votes with weight based on token holdings
- **Vote Delegation**: Delegate voting power to another address without transferring tokens
- **Proposal Cancellation**: Proposers or admin can cancel active proposals
- **Timelock Execution**: Successful proposals are queued for a delay before execution for security
- **State Machine**: Comprehensive proposal lifecycle tracking

### Configuration Parameters
- **Voting Delay**: Blocks after proposal creation before voting starts
- **Voting Period**: Duration voting remains active
- **Timelock Delay**: Waiting period after passing before execution
- **Quorum**: Minimum percentage of total supply that must participate
- **Approval Threshold**: Percentage of votes needed to pass a proposal
- **Proposal Threshold**: Minimum tokens required to create a proposal

## Contract Structure

### Key Components
- `contract.rs`: Main governance contract implementation
- `types.rs`: Data structures for proposals, votes, and state
- `errors.rs`: Custom error types for the contract
- `storage_keys.rs`: Storage key generation helpers
- `utils.rs`: Utility functions for safe math and calculations

### Proposal States
1. **Pending**: Waiting for voting period to start
2. **Active**: Voting is currently open
3. **Canceled**: Proposal was canceled before completion
4. **Defeated**: Failed to meet quorum or threshold requirements
5. **Succeeded**: Passed but not yet queued for execution
6. **Queued**: In timelock waiting for execution
7. **Expired**: Timelock expired without execution
8. **Executed**: Successfully executed on-chain

## Events

- `contract_initialized`: Emitted when governance contract is deployed
- `proposal_created`: New proposal submitted
- `vote_cast`: Token voter cast their vote
- `proposal_canceled`: Proposal was canceled
- `proposal_queued`: Successful proposal queued for timelock
- `proposal_executed`: Proposal executed on-chain
- `delegation_created`: Voting power delegated to another address
- `params_updated`: Governance parameters modified by admin

## Security Features

- **Access Control**: Admin-only for parameter updates
- **Vote Weighting**: Voting power accurately reflects token holdings
- **Double Voting Protection**: Cannot vote more than once per proposal
- **Timelock Security**: All executions delayed for review period
- **Input Validation**: Comprehensive validation of all parameters
- **Role Separation**: Clear separation of duties

## Usage

### Initialize
```rust
governance.initialize(
    env,
    admin_address,
    token_address,
    voting_delay,      // e.g., 1 block
    voting_period,     // e.g., 10080 blocks (~1 week)
    timelock_delay,    // e.g., 2 days in seconds
    quorum,           // e.g., 2000 (20% of supply)
    approval_threshold, // e.g., 5100 (51% majority)
    proposal_threshold  // Minimum tokens to propose
);
```

### Create Proposal
```rust
governance.propose(
    env,
    proposer,
    description,
    targets,        // Contract addresses to call
    values,         // ETH values (if any)
    functions,      // Function signatures to call
    calldatas       // Encoded function parameters
);
```

### Cast Vote
```rust
governance.cast_vote(env, voter, proposal_id, VoteType::For);
```

### Queue and Execute
After voting succeeds, queue for timelock:
```rust
governance.queue(env, caller, proposal_id);
```

After timelock expires, execute:
```rust
governance.execute(env, caller, proposal_id);
```

### Delegate Voting Power
```rust
governance.delegate(env, delegator, delegatee, amount);
```