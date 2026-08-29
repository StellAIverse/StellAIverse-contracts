use soroban_sdk::{contracttype, Address};

/// Storage key enum for all contract state
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    // Configuration
    Admin,
    VotingToken,
    ProposalCount,
    VotingDelay,
    VotingPeriod,
    TimelockDelay,
    Quorum,
    ApprovalThreshold,
    ProposalThreshold,
    TotalSupply,

    // Proposal data
    Proposal(u64),

    // Vote tracking: (proposal_id, voter)
    Vote(u64, Address),

    // Delegation: delegator -> Delegation record
    Delegation(Address),
    // Delegated power: delegatee -> total delegated i128
    DelegatedPower(Address),

    // veToken lock: account
    VeLock(Address),
}
