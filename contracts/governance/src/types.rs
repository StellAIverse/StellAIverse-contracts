use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

/// Proposal states
#[derive(Clone, Copy, PartialEq, Debug)]
#[contracttype]
pub enum ProposalState {
    Pending = 0,
    Active = 1,
    Canceled = 2,
    Defeated = 3,
    Succeeded = 4,
    Queued = 5,
    Expired = 6,
    Executed = 7,
}

/// Vote types
#[derive(Clone, Copy, PartialEq, Debug)]
#[contracttype]
pub enum VoteType {
    Against = 0,
    For = 1,
    Abstain = 2,
}

/// Proposal structure
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub description: String,
    pub targets: Vec<Address>,
    pub functions: Vec<String>,
    pub calldatas: Vec<Bytes>,
    pub vote_start: u64,
    pub vote_end: u64,
    pub eta: u64,
    pub for_votes: i128,
    pub against_votes: i128,
    pub abstain_votes: i128,
    pub actions: Vec<ProposalAction>,
    pub canceled: bool,
    pub executed: bool,
    pub created_at: u64,
}

/// Proposal vote tracker
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ProposalVote {
    pub has_voted: bool,
    pub vote_type: VoteType,
    pub weight: i128,
}

/// Delegation information
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Delegation {
    pub delegator: Address,
    pub delegatee: Address,
    pub amount: i128,
    pub timestamp: u64,
}

/// veToken lock information
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct VeTokenLock {
    pub account: Address,
    pub amount: i128,
    pub lock_end: u64,
    pub created_at: u64,
}

/// Typed governance action that proposals can execute
/// Variants use tuple syntax (Soroban contracttype requirement)
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum ProposalAction {
    // (token, to, amount)
    Transfer(Address, Address, i128),
}

/// Governance settings
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct GovernanceSettings {
    pub admin: Address,
    pub voting_token: Address,
    pub voting_delay: u64,
    pub voting_period: u64,
    pub timelock_delay: u64,
    pub quorum: i128,
    pub approval_threshold: i128,
    pub proposal_threshold: i128,
}
