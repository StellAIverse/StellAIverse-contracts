#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, token::TokenClient, Address, Bytes, Env, String, Symbol, Vec,
};

use crate::errors::GovernanceError;
use crate::storage_keys::DataKey;
use crate::types::*;
use crate::utils::*;

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    // ── Initialization ───────────────────────────────────────────────────

    /// Initialize the governance contract with core parameters
    pub fn initialize(
        env: Env,
        admin: Address,
        voting_token: Address,
        voting_delay: u64,
        voting_period: u64,
        timelock_delay: u64,
        quorum: i128,
        approval_threshold: i128,
        proposal_threshold: i128,
        total_supply: i128,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("{}", GovernanceError::AlreadyInitialized as u32);
        }

        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::VotingToken, &voting_token);
        env.storage()
            .instance()
            .set(&DataKey::VotingDelay, &voting_delay);
        env.storage()
            .instance()
            .set(&DataKey::VotingPeriod, &voting_period);
        env.storage()
            .instance()
            .set(&DataKey::TimelockDelay, &timelock_delay);
        env.storage().instance().set(&DataKey::Quorum, &quorum);
        env.storage()
            .instance()
            .set(&DataKey::ApprovalThreshold, &approval_threshold);
        env.storage()
            .instance()
            .set(&DataKey::ProposalThreshold, &proposal_threshold);
        env.storage().instance().set(&DataKey::ProposalCount, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &total_supply);

        env.events().publish(
            (Symbol::new(&env, "contract_initialized"),),
            (admin, voting_token, env.ledger().timestamp()),
        );
    }

    // ── Proposal Lifecycle ───────────────────────────────────────────────

    /// Create a new proposal with typed actions
    pub fn propose(
        env: Env,
        proposer: Address,
        description: String,
        targets: Vec<Address>,
        functions: Vec<String>,
        calldatas: Vec<Bytes>,
        actions: Vec<ProposalAction>,
    ) -> u64 {
        proposer.require_auth();

        let voting_power = Self::get_voting_power(&env, &proposer);
        let proposal_threshold: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalThreshold)
            .unwrap();

        if voting_power < proposal_threshold {
            panic!("{}", GovernanceError::InsufficientVotingPower as u32);
        }

        if !actions.is_empty()
            && (targets.len() != functions.len() || targets.len() != calldatas.len())
        {
            panic!("{}", GovernanceError::InvalidInput as u32);
        }

        let voting_delay: u64 = env.storage().instance().get(&DataKey::VotingDelay).unwrap();
        let voting_period: u64 = env
            .storage()
            .instance()
            .get(&DataKey::VotingPeriod)
            .unwrap();

        let current_block = env.ledger().sequence() as u64;
        let vote_start = current_block + voting_delay;
        let vote_end = vote_start + voting_period;

        let mut proposal_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);
        let proposal_id = proposal_count;
        proposal_count += 1;
        env.storage()
            .instance()
            .set(&DataKey::ProposalCount, &proposal_count);

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            description,
            targets,
            functions,
            calldatas,
            vote_start,
            vote_end,
            eta: 0,
            for_votes: 0,
            against_votes: 0,
            abstain_votes: 0,
            actions,
            canceled: false,
            executed: false,
            created_at: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "proposal_created"), proposal_id),
            (proposer, vote_start, vote_end),
        );

        proposal_id
    }

    /// Cast a vote on an active proposal
    pub fn cast_vote(env: Env, voter: Address, proposal_id: u64, vote_type: VoteType) {
        voter.require_auth();

        let mut proposal = Self::get_proposal_from_storage(&env, proposal_id);
        let current_state = Self::state(&env, &proposal);
        if current_state != ProposalState::Active {
            panic!("{}", GovernanceError::ProposalNotActive as u32);
        }

        let vote_key = DataKey::Vote(proposal_id, voter.clone());
        if env.storage().instance().has(&vote_key) {
            panic!("{}", GovernanceError::AlreadyVoted as u32);
        }

        let voting_power = Self::get_voting_power(&env, &voter);

        match vote_type {
            VoteType::For => {
                proposal.for_votes = safe_add(proposal.for_votes, voting_power);
            }
            VoteType::Against => {
                proposal.against_votes = safe_add(proposal.against_votes, voting_power);
            }
            VoteType::Abstain => {
                proposal.abstain_votes = safe_add(proposal.abstain_votes, voting_power);
            }
        }

        let vote = ProposalVote {
            has_voted: true,
            vote_type,
            weight: voting_power,
        };
        env.storage().instance().set(&vote_key, &vote);
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "vote_cast"), proposal_id),
            (voter, vote_type as u32, voting_power),
        );
    }

    /// Queue a successful proposal for timelock execution
    pub fn queue(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();

        let mut proposal = Self::get_proposal_from_storage(&env, proposal_id);
        let current_state = Self::state(&env, &proposal);

        if current_state != ProposalState::Succeeded {
            panic!("{}", GovernanceError::InvalidProposalState as u32);
        }

        let timelock_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TimelockDelay)
            .unwrap();

        proposal.eta = env.ledger().timestamp() + timelock_delay;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "proposal_queued"), proposal_id),
            (proposal.eta, env.ledger().timestamp()),
        );
    }

    /// Execute a queued proposal after timelock expires
    pub fn execute(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();

        let mut proposal = Self::get_proposal_from_storage(&env, proposal_id);
        let current_state = Self::state(&env, &proposal);

        if current_state != ProposalState::Queued {
            panic!("{}", GovernanceError::ProposalNotQueued as u32);
        }

        if env.ledger().timestamp() < proposal.eta {
            panic!("{}", GovernanceError::TimelockNotExpired as u32);
        }

        // Execute proposal actions
        for i in 0..proposal.actions.len() {
            let action = proposal.actions.get(i).unwrap();
            match action {
                ProposalAction::Transfer(token, to, amount) => {
                    let token_client = TokenClient::new(&env, &token);
                    let gov_addr = env.current_contract_address();
                    token_client.transfer(&gov_addr, &to, &amount);
                }
            }
        }

        proposal.executed = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "proposal_executed"), proposal_id),
            (caller, env.ledger().timestamp()),
        );
    }

    /// Cancel a proposal (only proposer or admin)
    pub fn cancel(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();

        let mut proposal = Self::get_proposal_from_storage(&env, proposal_id);

        if proposal.executed {
            panic!("{}", GovernanceError::CannotCancelExecuted as u32);
        }

        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != proposal.proposer && caller != admin {
            panic!("{}", GovernanceError::Unauthorized as u32);
        }

        proposal.canceled = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (Symbol::new(&env, "proposal_canceled"), proposal_id),
            (caller, env.ledger().timestamp()),
        );
    }

    // ── Delegation ───────────────────────────────────────────────────────

    /// Delegate voting power to another address
    pub fn delegate(env: Env, delegator: Address, delegatee: Address, amount: i128) {
        delegator.require_auth();

        if delegator == delegatee {
            panic!("{}", GovernanceError::DelegationToSelf as u32);
        }

        let delegation_key = DataKey::Delegation(delegator.clone());
        if env.storage().instance().has(&delegation_key) {
            panic!("{}", GovernanceError::AlreadyDelegated as u32);
        }

        let balance = Self::get_token_balance(&env, &delegator);
        if balance < amount {
            panic!("{}", GovernanceError::InsufficientBalance as u32);
        }

        let delegation = Delegation {
            delegator: delegator.clone(),
            delegatee: delegatee.clone(),
            amount,
            timestamp: env.ledger().timestamp(),
        };
        env.storage().instance().set(&delegation_key, &delegation);

        // Track total delegated power for the delegatee
        let delegated_key = DataKey::DelegatedPower(delegatee.clone());
        let current_delegated: i128 = env.storage().instance().get(&delegated_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&delegated_key, &safe_add(current_delegated, amount));

        env.events().publish(
            (Symbol::new(&env, "delegation_created"),),
            (delegator, delegatee, amount, env.ledger().timestamp()),
        );
    }

    /// Revoke a delegation
    pub fn revoke_delegation(env: Env, delegator: Address) {
        delegator.require_auth();

        let delegation_key = DataKey::Delegation(delegator.clone());
        let delegation: Delegation = env
            .storage()
            .instance()
            .get(&delegation_key)
            .unwrap_or_else(|| panic!("{}", GovernanceError::ProposalNotFound as u32));

        env.storage().instance().remove(&delegation_key);

        // Decrease delegated power for the delegatee
        let delegated_key = DataKey::DelegatedPower(delegation.delegatee);
        let current_delegated: i128 = env.storage().instance().get(&delegated_key).unwrap_or(0);
        let new_delegated = current_delegated - delegation.amount;
        env.storage().instance().set(&delegated_key, &new_delegated);

        env.events().publish(
            (Symbol::new(&env, "delegation_revoked"),),
            (delegator, env.ledger().timestamp()),
        );
    }

    // ── veToken ──────────────────────────────────────────────────────────

    /// Lock tokens for veToken voting power boost
    pub fn lock_tokens(env: Env, account: Address, amount: i128, lock_duration: u64) {
        account.require_auth();

        if amount <= 0 {
            panic!("{}", GovernanceError::LockAmountZero as u32);
        }

        if !(MIN_LOCK_DURATION..=MAX_LOCK_DURATION).contains(&lock_duration) {
            panic!("{}", GovernanceError::InvalidLockDuration as u32);
        }

        let lock_key = DataKey::VeLock(account.clone());
        if env.storage().instance().has(&lock_key) {
            panic!("{}", GovernanceError::LockAlreadyExists as u32);
        }

        let balance = Self::get_token_balance(&env, &account);
        if balance < amount {
            panic!("{}", GovernanceError::InsufficientBalance as u32);
        }

        let now = env.ledger().timestamp();
        let lock = VeTokenLock {
            account: account.clone(),
            amount,
            lock_end: now + lock_duration,
            created_at: now,
        };
        env.storage().instance().set(&lock_key, &lock);

        env.events().publish(
            (Symbol::new(&env, "tokens_locked"),),
            (account, amount, lock_duration, now),
        );
    }

    /// Withdraw locked tokens after lock period expires
    pub fn withdraw_tokens(env: Env, account: Address) {
        account.require_auth();

        let lock_key = DataKey::VeLock(account.clone());
        let lock: VeTokenLock = env
            .storage()
            .instance()
            .get(&lock_key)
            .unwrap_or_else(|| panic!("{}", GovernanceError::NoLockFound as u32));

        if env.ledger().timestamp() < lock.lock_end {
            panic!("{}", GovernanceError::LockNotExpired as u32);
        }

        env.storage().instance().remove(&lock_key);

        env.events().publish(
            (Symbol::new(&env, "tokens_withdrawn"),),
            (account, lock.amount, env.ledger().timestamp()),
        );
    }

    // ── Admin ────────────────────────────────────────────────────────────

    /// Update governance parameters (admin only)
    pub fn set_voting_params(
        env: Env,
        admin: Address,
        voting_delay: Option<u64>,
        voting_period: Option<u64>,
        timelock_delay: Option<u64>,
        quorum: Option<i128>,
        approval_threshold: Option<i128>,
    ) {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();

        if admin != stored_admin {
            panic!("{}", GovernanceError::Unauthorized as u32);
        }

        if let Some(delay) = voting_delay {
            env.storage().instance().set(&DataKey::VotingDelay, &delay);
        }
        if let Some(period) = voting_period {
            env.storage()
                .instance()
                .set(&DataKey::VotingPeriod, &period);
        }
        if let Some(delay) = timelock_delay {
            env.storage()
                .instance()
                .set(&DataKey::TimelockDelay, &delay);
        }
        if let Some(q) = quorum {
            env.storage().instance().set(&DataKey::Quorum, &q);
        }
        if let Some(t) = approval_threshold {
            env.storage()
                .instance()
                .set(&DataKey::ApprovalThreshold, &t);
        }

        env.events().publish(
            (Symbol::new(&env, "params_updated"),),
            env.ledger().timestamp(),
        );
    }

    // ── Query Functions ──────────────────────────────────────────────────

    /// Get proposal by ID
    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        Self::get_proposal_from_storage(&env, proposal_id)
    }

    /// Get proposal state
    pub fn get_proposal_state(env: Env, proposal_id: u64) -> ProposalState {
        let proposal = Self::get_proposal_from_storage(&env, proposal_id);
        Self::state(&env, &proposal)
    }

    /// Get proposal vote results and state
    pub fn get_proposal_results(env: Env, proposal_id: u64) -> (i128, i128, i128, ProposalState) {
        let proposal = Self::get_proposal_from_storage(&env, proposal_id);
        let state = Self::state(&env, &proposal);
        (
            proposal.for_votes,
            proposal.against_votes,
            proposal.abstain_votes,
            state,
        )
    }

    /// Get all proposal IDs
    pub fn get_proposal_ids(env: Env) -> Vec<u64> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);

        let mut ids = Vec::new(&env);
        for i in 0..count {
            ids.push_back(i);
        }
        ids
    }

    /// Get vote information for a voter on a specific proposal
    pub fn get_vote_info(env: Env, proposal_id: u64, voter: Address) -> Option<ProposalVote> {
        let vote_key = DataKey::Vote(proposal_id, voter);
        env.storage().instance().get(&vote_key)
    }

    /// Get delegation information
    pub fn get_delegation_info(env: Env, delegator: Address) -> Option<Delegation> {
        let delegation_key = DataKey::Delegation(delegator);
        env.storage().instance().get(&delegation_key)
    }

    /// Get veToken lock information
    pub fn get_ve_lock(env: Env, account: Address) -> Option<VeTokenLock> {
        let lock_key = DataKey::VeLock(account);
        env.storage().instance().get(&lock_key)
    }

    /// Get voting power for an account
    pub fn get_voting_power_view(env: Env, account: Address) -> i128 {
        Self::get_voting_power(&env, &account)
    }

    /// Get governance settings
    pub fn get_settings(env: Env) -> GovernanceSettings {
        GovernanceSettings {
            admin: env.storage().instance().get(&DataKey::Admin).unwrap(),
            voting_token: env.storage().instance().get(&DataKey::VotingToken).unwrap(),
            voting_delay: env.storage().instance().get(&DataKey::VotingDelay).unwrap(),
            voting_period: env
                .storage()
                .instance()
                .get(&DataKey::VotingPeriod)
                .unwrap(),
            timelock_delay: env
                .storage()
                .instance()
                .get(&DataKey::TimelockDelay)
                .unwrap(),
            quorum: env.storage().instance().get(&DataKey::Quorum).unwrap(),
            approval_threshold: env
                .storage()
                .instance()
                .get(&DataKey::ApprovalThreshold)
                .unwrap(),
            proposal_threshold: env
                .storage()
                .instance()
                .get(&DataKey::ProposalThreshold)
                .unwrap(),
        }
    }

    // ── Internal Functions ───────────────────────────────────────────────

    /// Determine the current state of a proposal
    fn state(env: &Env, proposal: &Proposal) -> ProposalState {
        if proposal.canceled {
            return ProposalState::Canceled;
        }

        let current_block = env.ledger().sequence() as u64;

        if current_block < proposal.vote_start {
            return ProposalState::Pending;
        } else if current_block <= proposal.vote_end {
            return ProposalState::Active;
        }

        // Voting period ended -- check pass/fail
        if !Self::quorum_reached(env, proposal) || !Self::threshold_reached(env, proposal) {
            return ProposalState::Defeated;
        }

        if proposal.executed {
            return ProposalState::Executed;
        }

        if proposal.eta == 0 {
            return ProposalState::Succeeded;
        }

        // Grace period: 14 days after eta during which execution is possible
        let grace_period: u64 = 14 * 24 * 60 * 60;
        if env.ledger().timestamp() > proposal.eta + grace_period && !proposal.executed {
            return ProposalState::Expired;
        }

        ProposalState::Queued
    }

    /// Check if quorum is reached
    fn quorum_reached(env: &Env, proposal: &Proposal) -> bool {
        let total_votes = proposal.for_votes + proposal.against_votes + proposal.abstain_votes;
        let total_supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        let quorum: i128 = env.storage().instance().get(&DataKey::Quorum).unwrap();

        if total_supply == 0 {
            return false;
        }

        // quorum is in basis points (10000 = 100%)
        (total_votes * 10000) >= (total_supply * quorum)
    }

    /// Check if approval threshold is reached
    fn threshold_reached(env: &Env, proposal: &Proposal) -> bool {
        let total_votes_cast = proposal.for_votes + proposal.against_votes;
        if total_votes_cast == 0 {
            return false;
        }

        let threshold: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ApprovalThreshold)
            .unwrap();

        // threshold is in basis points
        (proposal.for_votes * 10000) >= (total_votes_cast * threshold)
    }

    /// Get total voting power (token balance + veToken boost + delegated power)
    fn get_voting_power(env: &Env, account: &Address) -> i128 {
        let token_balance = Self::get_token_balance(env, account);
        let ve_balance = Self::calculate_ve_balance(env, account);
        let delegated_power = Self::get_delegated_power(env, account);
        safe_add(safe_add(token_balance, ve_balance), delegated_power)
    }

    /// Get total power delegated to this account
    fn get_delegated_power(env: &Env, account: &Address) -> i128 {
        let key = DataKey::DelegatedPower(account.clone());
        env.storage().instance().get(&key).unwrap_or(0)
    }

    /// Calculate time-weighted veToken balance (linear decay)
    fn calculate_ve_balance(env: &Env, account: &Address) -> i128 {
        let lock_key = DataKey::VeLock(account.clone());
        let lock: VeTokenLock = match env.storage().instance().get(&lock_key) {
            Some(l) => l,
            None => return 0,
        };

        let now = env.ledger().timestamp();
        if now >= lock.lock_end {
            return 0; // lock expired
        }

        let remaining = lock.lock_end - now;
        let total_duration = lock.lock_end - lock.created_at;

        if total_duration == 0 {
            return 0;
        }

        // Linear decay: full amount at start, 0 at expiry
        (lock.amount * remaining as i128) / (total_duration as i128)
    }

    /// Get token balance of an address via the voting token contract
    fn get_token_balance(env: &Env, account: &Address) -> i128 {
        let voting_token: Address = env.storage().instance().get(&DataKey::VotingToken).unwrap();
        let token_client = TokenClient::new(env, &voting_token);
        token_client.balance(account)
    }

    /// Internal helper to get proposal from storage
    fn get_proposal_from_storage(env: &Env, proposal_id: u64) -> Proposal {
        env.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("{}", GovernanceError::ProposalNotFound as u32))
    }
}
