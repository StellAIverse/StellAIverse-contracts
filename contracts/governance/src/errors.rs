use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum GovernanceError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    ProposalNotFound = 3,
    ProposalAlreadyExecuted = 4,
    ProposalNotActive = 5,
    VotingPeriodNotEnded = 6,
    VotingPeriodEnded = 7,
    AlreadyVoted = 8,
    InsufficientVotingPower = 9,
    QuorumNotMet = 10,
    ThresholdNotMet = 11,
    TimelockNotExpired = 12,
    InvalidProposalState = 13,
    InvalidInput = 14,
    ProposalCancelled = 15,
    DelegationToSelf = 16,
    AlreadyDelegated = 17,
    CannotCancelExecuted = 18,
    InvalidVoteType = 19,
    InsufficientBalance = 20,
    TimelockAlreadyQueued = 21,
    ProposalNotQueued = 22,
    LockAlreadyExists = 23,
    LockNotExpired = 24,
    NoLockFound = 25,
    LockAmountZero = 26,
    InvalidLockDuration = 27,
}
