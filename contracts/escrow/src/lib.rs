#![no_std]
// Contract entrypoints mirror on-chain parameter lists; keep clippy from
// complaining about multi-parameter configuration functions.
#![allow(clippy::too_many_arguments)]

//! Escrow module (issue #288): secure fund holding with configurable
//! M-of-N multi-signature approval, time-locked execution, grace periods,
//! transaction queueing/cancellation, and rate limiting. Supports both
//! native (internally-accounted) and token (SAC/ERC20-style) escrows, with
//! optional integration against the `access-control` contract for admin
//! authorization.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token::TokenClient, Address,
    Env, String, Vec,
};

use access_control::{AccessControlClient, Role};

const MIN_AMOUNT: i128 = 1;
const MAX_SIGNERS: u32 = 20;
const ZERO_DELAY: u64 = 0;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Config,
    Signer(Address),
    SignerList,
    Escrow(u64),
    EscrowBalance(u64),
    Transaction(u64),
    GlobalExecutions,
    RecipientExecutions(Address),
    NextEscrowId,
    NextTxId,
}

/// Global escrow configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct EscrowConfig {
    pub admin: Address,
    /// Optional access-control contract; when set, admin-gated calls accept
    /// an account holding the Admin role there as well as the stored admin.
    pub access_control: Option<Address>,
    pub required_approvals: u32,
    pub signer_count: u32,
    /// Minimum seconds between queueing and executing a transaction.
    pub timelock_delay: u64,
    /// Seconds after the time-lock elapses during which execution is still
    /// possible; afterwards the transaction can only be cancelled.
    pub grace_period: u64,
    /// Sliding window (seconds) used by the rate limiter.
    pub rate_limit_window: u64,
    /// Maximum executions per window, both globally and per recipient.
    pub rate_limit_max: u32,
    pub paused: bool,
}

/// A purpose-specific escrow instance holding one asset type.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct EscrowRecord {
    pub id: u64,
    pub token: Address,
    /// When true the escrow accounts value internally instead of moving a
    /// token contract's balances.
    pub is_native: bool,
    pub creator: Address,
    pub purpose: String,
    pub created_at: u64,
    pub active: bool,
}

/// A queued transfer awaiting approvals, time-lock and rate-limit checks.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct EscrowTransaction {
    pub tx_id: u64,
    pub escrow_id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub submitter: Address,
    pub approvers: Vec<Address>,
    pub queued_at: u64,
    pub executed: bool,
    pub cancelled: bool,
    pub executed_at: u64,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidParam = 4,
    NotFound = 5,
    AlreadySigner = 6,
    NotASigner = 7,
    Paused = 8,
    InsufficientBalance = 9,
    AlreadyExecuted = 10,
    AlreadyCancelled = 11,
    NotApproved = 12,
    TimeLocked = 13,
    GraceExpired = 14,
    RateLimited = 15,
    InactiveEscrow = 16,
    AlreadyApproved = 17,
    ThresholdTooHigh = 18,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Initializes the module. `m`-of-`n` multisig, time-lock delay, grace
    /// period and rate limit are all configured here and adjustable later
    /// by the admin.
    pub fn initialize(
        env: Env,
        admin: Address,
        access_control: Option<Address>,
        signers: Vec<Address>,
        required_approvals: u32,
        timelock_delay: u64,
        grace_period: u64,
        rate_limit_window: u64,
        rate_limit_max: u32,
    ) -> Result<(), EscrowError> {
        if env.storage().instance().has(&DataKey::Config) {
            return Err(EscrowError::AlreadyInitialized);
        }
        admin.require_auth();
        Self::validate_signer_set(signers.clone(), required_approvals)?;
        if grace_period == ZERO_DELAY {
            return Err(EscrowError::InvalidParam);
        }
        if rate_limit_max == 0 || rate_limit_window == ZERO_DELAY {
            return Err(EscrowError::InvalidParam);
        }

        for idx in 0..signers.len() {
            env.storage()
                .instance()
                .set(&DataKey::Signer(signers.get_unchecked(idx)), &true);
        }
        env.storage().instance().set(&DataKey::SignerList, &signers);
        env.storage()
            .instance()
            .set(&DataKey::GlobalExecutions, &Vec::<u64>::new(&env));
        env.storage().instance().set(&DataKey::NextEscrowId, &1u64);
        env.storage().instance().set(&DataKey::NextTxId, &1u64);

        let config = EscrowConfig {
            admin: admin.clone(),
            access_control,
            required_approvals,
            signer_count: signers.len(),
            timelock_delay,
            grace_period,
            rate_limit_window,
            rate_limit_max,
            paused: false,
        };
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (symbol_short!("esc_init"),),
            (admin, required_approvals, config.signer_count),
        );
        Ok(())
    }

    // ------------------------------------------------------------------
    // Configuration (admin / AccessControl gated)
    // ------------------------------------------------------------------

    /// Adds a signer; the threshold is lowered automatically if it would
    /// otherwise exceed the signer count.
    pub fn add_signer(env: Env, caller: Address, signer: Address) -> Result<(), EscrowError> {
        let mut config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        if Self::is_signer(&env, &signer) {
            return Err(EscrowError::AlreadySigner);
        }
        if config.signer_count >= MAX_SIGNERS {
            return Err(EscrowError::InvalidParam);
        }
        env.storage()
            .instance()
            .set(&DataKey::Signer(signer.clone()), &true);
        let mut list: Vec<Address> = env.storage().instance().get(&DataKey::SignerList).unwrap();
        list.push_back(signer.clone());
        env.storage().instance().set(&DataKey::SignerList, &list);
        config.signer_count += 1;
        if config.required_approvals > config.signer_count {
            config.required_approvals = config.signer_count;
        }
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((symbol_short!("esc_sign"),), ("added", signer));
        Ok(())
    }

    /// Removes a signer; refused while it would drop the signer count below
    /// the approval threshold.
    pub fn remove_signer(env: Env, caller: Address, signer: Address) -> Result<(), EscrowError> {
        let mut config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        if !Self::is_signer(&env, &signer) {
            return Err(EscrowError::NotASigner);
        }
        if config.signer_count <= config.required_approvals {
            return Err(EscrowError::ThresholdTooHigh);
        }
        env.storage()
            .instance()
            .remove(&DataKey::Signer(signer.clone()));
        let mut list: Vec<Address> = env.storage().instance().get(&DataKey::SignerList).unwrap();
        let idx = Self::index_of(&list, &signer);
        list.remove(idx);
        env.storage().instance().set(&DataKey::SignerList, &list);
        config.signer_count -= 1;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((symbol_short!("esc_sign"),), ("removed", signer));
        Ok(())
    }

    pub fn set_threshold(env: Env, caller: Address, m: u32) -> Result<(), EscrowError> {
        let mut config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        if m == 0 || m > config.signer_count {
            return Err(EscrowError::InvalidParam);
        }
        config.required_approvals = m;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish((symbol_short!("esc_thr"),), (m,));
        Ok(())
    }

    pub fn set_timelock(
        env: Env,
        caller: Address,
        delay: u64,
        grace_period: u64,
    ) -> Result<(), EscrowError> {
        let mut config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        if grace_period == ZERO_DELAY || delay > u64::MAX - grace_period {
            return Err(EscrowError::InvalidParam);
        }
        config.timelock_delay = delay;
        config.grace_period = grace_period;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((symbol_short!("esc_tlck"),), (delay, grace_period));
        Ok(())
    }

    pub fn set_rate_limit(
        env: Env,
        caller: Address,
        window: u64,
        max: u32,
    ) -> Result<(), EscrowError> {
        let mut config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        if window == ZERO_DELAY || max == 0 {
            return Err(EscrowError::InvalidParam);
        }
        config.rate_limit_window = window;
        config.rate_limit_max = max;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((symbol_short!("esc_rate"),), (window, max));
        Ok(())
    }

    pub fn pause(env: Env, caller: Address) -> Result<(), EscrowError> {
        let mut config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        config.paused = true;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish((symbol_short!("esc_pause"),), (true,));
        Ok(())
    }

    pub fn unpause(env: Env, caller: Address) -> Result<(), EscrowError> {
        let mut config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        config.paused = false;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((symbol_short!("esc_pause"),), (false,));
        Ok(())
    }

    // ------------------------------------------------------------------
    // Escrow instances
    // ------------------------------------------------------------------

    /// Creates a new escrow instance for a given token. Pass
    /// `is_native = true` to use internal accounting instead of a token
    /// contract (the `token` argument is ignored then).
    pub fn create_escrow(
        env: Env,
        creator: Address,
        token: Address,
        is_native: bool,
        purpose: String,
    ) -> Result<u64, EscrowError> {
        let config = Self::config(&env)?;
        if config.paused {
            return Err(EscrowError::Paused);
        }
        creator.require_auth();
        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextEscrowId)
            .unwrap_or(1);
        let record = EscrowRecord {
            id,
            token,
            is_native,
            creator: creator.clone(),
            purpose,
            created_at: env.ledger().timestamp(),
            active: true,
        };
        env.storage().instance().set(&DataKey::Escrow(id), &record);
        env.storage()
            .instance()
            .set(&DataKey::EscrowBalance(id), &0i128);
        env.storage()
            .instance()
            .set(&DataKey::NextEscrowId, &(id + 1));
        env.events()
            .publish((symbol_short!("esc_new"),), (creator, id, record.is_native));
        Ok(id)
    }

    /// Deposits funds into an escrow instance. For token escrows the
    /// depositor moves tokens into this contract; for native escrows the
    /// deposit is accounted internally.
    pub fn deposit(
        env: Env,
        escrow_id: u64,
        from: Address,
        amount: i128,
    ) -> Result<(), EscrowError> {
        from.require_auth();
        if amount < MIN_AMOUNT {
            return Err(EscrowError::InvalidParam);
        }
        let record = Self::escrow_record(&env, escrow_id)?;
        if !record.active {
            return Err(EscrowError::InactiveEscrow);
        }
        // Credit the internal ledger for both native and token escrows so
        // `escrow_balance` and the withdrawal-path balance checks stay
        // consistent regardless of asset type; only token escrows also move
        // real tokens into the contract.
        let balance = Self::escrow_balance_raw(&env, escrow_id);
        env.storage()
            .instance()
            .set(&DataKey::EscrowBalance(escrow_id), &(balance + amount));
        if !record.is_native {
            let wallet = env.current_contract_address();
            TokenClient::new(&env, &record.token).transfer(&from, &wallet, &amount);
        }
        env.events()
            .publish((symbol_short!("esc_dep"),), (escrow_id, from, amount));
        Ok(())
    }

    /// Returns the recorded balance of an escrow instance.
    pub fn escrow_balance(env: Env, escrow_id: u64) -> i128 {
        Self::escrow_balance_raw(&env, escrow_id)
    }

    // ------------------------------------------------------------------
    // Transaction queue
    // ------------------------------------------------------------------

    /// Queues a withdrawal of `amount` from `escrow_id` to `recipient`.
    /// Submitters must be signers or admins; the submitter counts as the
    /// first approver.
    pub fn submit_transaction(
        env: Env,
        submitter: Address,
        escrow_id: u64,
        recipient: Address,
        amount: i128,
    ) -> Result<u64, EscrowError> {
        let config = Self::config(&env)?;
        if config.paused {
            return Err(EscrowError::Paused);
        }
        Self::require_signer_or_admin(&env, &config, &submitter)?;
        if amount < MIN_AMOUNT {
            return Err(EscrowError::InvalidParam);
        }
        Self::escrow_record(&env, escrow_id)?;
        if Self::escrow_balance_raw(&env, escrow_id) < amount {
            return Err(EscrowError::InsufficientBalance);
        }
        let tx_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextTxId)
            .unwrap_or(1);
        let now = env.ledger().timestamp();
        let mut approvers = Vec::new(&env);
        approvers.push_back(submitter.clone());
        let tx = EscrowTransaction {
            tx_id,
            escrow_id,
            recipient,
            amount,
            submitter: submitter.clone(),
            approvers,
            queued_at: now,
            executed: false,
            cancelled: false,
            executed_at: 0,
        };
        env.storage()
            .instance()
            .set(&DataKey::Transaction(tx_id), &tx);
        env.storage()
            .instance()
            .set(&DataKey::NextTxId, &(tx_id + 1));
        env.events()
            .publish((symbol_short!("esc_queue"),), (tx_id, escrow_id, tx.amount));
        Ok(tx_id)
    }

    /// Adds the caller's approval to a queued transaction.
    pub fn approve_transaction(env: Env, signer: Address, tx_id: u64) -> Result<(), EscrowError> {
        let config = Self::config(&env)?;
        Self::require_signer(&env, &config, &signer)?;
        let mut tx = Self::transaction(&env, tx_id)?;
        if tx.executed {
            return Err(EscrowError::AlreadyExecuted);
        }
        if tx.cancelled {
            return Err(EscrowError::AlreadyCancelled);
        }
        if Self::contains(&tx.approvers, &signer) {
            return Err(EscrowError::AlreadyApproved);
        }
        tx.approvers.push_back(signer.clone());
        env.storage()
            .instance()
            .set(&DataKey::Transaction(tx_id), &tx);
        env.events().publish(
            (symbol_short!("esc_appr"),),
            (tx_id, signer, tx.approvers.len()),
        );
        Ok(())
    }

    /// Withdraws a previously given approval.
    pub fn revoke_approval(env: Env, signer: Address, tx_id: u64) -> Result<(), EscrowError> {
        let config = Self::config(&env)?;
        Self::require_signer(&env, &config, &signer)?;
        let mut tx = Self::transaction(&env, tx_id)?;
        if tx.executed || tx.cancelled {
            return Err(EscrowError::NotFound);
        }
        match Self::try_index_of(&tx.approvers, &signer) {
            Some(i) => {
                tx.approvers.remove(i);
            }
            None => return Err(EscrowError::NotApproved),
        }
        env.storage()
            .instance()
            .set(&DataKey::Transaction(tx_id), &tx);
        env.events().publish(
            (symbol_short!("esc_revk"),),
            (tx_id, signer, tx.approvers.len()),
        );
        Ok(())
    }

    /// Executes a queued transaction once all guards pass:
    /// M-of-N approvals, time-lock elapsed, still within the grace period
    /// and not rate-limited (globally or per recipient). Permissionless so
    /// any keeper can drive execution.
    pub fn execute_transaction(env: Env, tx_id: u64) -> Result<(), EscrowError> {
        let config = Self::config(&env)?;
        if config.paused {
            return Err(EscrowError::Paused);
        }
        let mut tx = Self::transaction(&env, tx_id)?;
        if tx.executed {
            return Err(EscrowError::AlreadyExecuted);
        }
        if tx.cancelled {
            return Err(EscrowError::AlreadyCancelled);
        }
        if tx.approvers.len() < config.required_approvals {
            return Err(EscrowError::NotApproved);
        }
        let now = env.ledger().timestamp();
        let unlock_at = tx
            .queued_at
            .checked_add(config.timelock_delay)
            .ok_or(EscrowError::InvalidParam)?;
        if now < unlock_at {
            return Err(EscrowError::TimeLocked);
        }
        let grace_until = unlock_at
            .checked_add(config.grace_period)
            .ok_or(EscrowError::InvalidParam)?;
        if now > grace_until {
            return Err(EscrowError::GraceExpired);
        }
        Self::check_rate_limits(&env, &config, &tx.recipient)?;

        let record = Self::escrow_record(&env, tx.escrow_id)?;
        let balance = Self::escrow_balance_raw(&env, tx.escrow_id);
        if balance < tx.amount {
            return Err(EscrowError::InsufficientBalance);
        }
        // Debit the internal ledger for both native and token escrows
        // (mirrors the credit in `deposit`); only token escrows also move
        // real tokens out of the contract.
        env.storage().instance().set(
            &DataKey::EscrowBalance(tx.escrow_id),
            &(balance - tx.amount),
        );
        if !record.is_native {
            TokenClient::new(&env, &record.token).transfer(
                &env.current_contract_address(),
                &tx.recipient,
                &tx.amount,
            );
        }

        tx.executed = true;
        tx.executed_at = now;
        env.storage()
            .instance()
            .set(&DataKey::Transaction(tx_id), &tx);

        let cutoff = now.saturating_sub(config.rate_limit_window);

        let mut global: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::GlobalExecutions)
            .unwrap();
        global.push_back(now);
        Self::prune_window(&mut global, cutoff);
        env.storage()
            .instance()
            .set(&DataKey::GlobalExecutions, &global);

        let recipient = tx.recipient.clone();
        let mut per_recipient: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::RecipientExecutions(recipient.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        per_recipient.push_back(now);
        Self::prune_window(&mut per_recipient, cutoff);
        env.storage()
            .instance()
            .set(&DataKey::RecipientExecutions(recipient), &per_recipient);

        env.events().publish(
            (symbol_short!("esc_exec"),),
            (tx_id, tx.escrow_id, tx.recipient, tx.amount, now),
        );
        Ok(())
    }

    /// Cancels a queued transaction. Allowed for the admin or the submitter
    /// any time before execution, and for anyone once the grace period has
    /// lapsed (garbage collection of expired transactions).
    pub fn cancel_transaction(env: Env, caller: Address, tx_id: u64) -> Result<(), EscrowError> {
        caller.require_auth();
        let config = Self::config(&env)?;
        let mut tx = Self::transaction(&env, tx_id)?;
        if tx.executed {
            return Err(EscrowError::AlreadyExecuted);
        }
        if tx.cancelled {
            return Err(EscrowError::AlreadyCancelled);
        }
        let is_admin = Self::is_admin(&env, &config, &caller);
        let is_submitter = caller == tx.submitter;
        let unlock_at = tx.queued_at.saturating_add(config.timelock_delay);
        let grace_lapsed = env.ledger().timestamp() > unlock_at.saturating_add(config.grace_period);
        if !is_admin && !is_submitter && !grace_lapsed {
            return Err(EscrowError::Unauthorized);
        }
        tx.cancelled = true;
        env.storage()
            .instance()
            .set(&DataKey::Transaction(tx_id), &tx);
        env.events()
            .publish((symbol_short!("esc_cxl"),), (tx_id, caller));
        Ok(())
    }

    /// Deactivates an escrow instance; further deposits are rejected while
    /// already queued transactions remain executable until they expire.
    pub fn close_escrow(env: Env, caller: Address, escrow_id: u64) -> Result<(), EscrowError> {
        let config = Self::config(&env)?;
        Self::require_admin(&env, &config, &caller)?;
        let mut record = Self::escrow_record(&env, escrow_id)?;
        record.active = false;
        env.storage()
            .instance()
            .set(&DataKey::Escrow(escrow_id), &record);
        env.events()
            .publish((symbol_short!("esc_close"),), (escrow_id,));
        Ok(())
    }

    // ------------------------------------------------------------------
    // Views
    // ------------------------------------------------------------------

    pub fn get_config(env: Env) -> Option<EscrowConfig> {
        env.storage().instance().get(&DataKey::Config)
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> Option<EscrowRecord> {
        env.storage().instance().get(&DataKey::Escrow(escrow_id))
    }

    pub fn get_transaction(env: Env, tx_id: u64) -> Option<EscrowTransaction> {
        env.storage().instance().get(&DataKey::Transaction(tx_id))
    }

    pub fn get_signers(env: Env) -> Vec<Address> {
        env.storage().instance().get(&DataKey::SignerList).unwrap()
    }

    pub fn is_signer_public(env: Env, who: Address) -> bool {
        Self::is_signer(&env, &who)
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    fn config(env: &Env) -> Result<EscrowConfig, EscrowError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(EscrowError::NotInitialized)
    }

    fn escrow_record(env: &Env, id: u64) -> Result<EscrowRecord, EscrowError> {
        env.storage()
            .instance()
            .get(&DataKey::Escrow(id))
            .ok_or(EscrowError::NotFound)
    }

    fn transaction(env: &Env, id: u64) -> Result<EscrowTransaction, EscrowError> {
        env.storage()
            .instance()
            .get(&DataKey::Transaction(id))
            .ok_or(EscrowError::NotFound)
    }

    fn escrow_balance_raw(env: &Env, id: u64) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::EscrowBalance(id))
            .unwrap_or(0)
    }

    fn is_signer(env: &Env, who: &Address) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Signer(who.clone()))
            .unwrap_or(false)
    }

    /// True when `who` is the stored admin or, if an AccessControl contract
    /// is linked, holds its Admin role there.
    fn is_admin(env: &Env, config: &EscrowConfig, who: &Address) -> bool {
        if *who == config.admin {
            return true;
        }
        if let Some(ac) = &config.access_control {
            let client = AccessControlClient::new(env, ac);
            return client.has_role(&Role::Admin, who);
        }
        false
    }

    fn require_admin(env: &Env, config: &EscrowConfig, who: &Address) -> Result<(), EscrowError> {
        who.require_auth();
        if !Self::is_admin(env, config, who) {
            return Err(EscrowError::Unauthorized);
        }
        Ok(())
    }

    fn require_signer(env: &Env, _config: &EscrowConfig, who: &Address) -> Result<(), EscrowError> {
        who.require_auth();
        if !Self::is_signer(env, who) {
            return Err(EscrowError::NotASigner);
        }
        Ok(())
    }

    fn require_signer_or_admin(
        env: &Env,
        config: &EscrowConfig,
        who: &Address,
    ) -> Result<(), EscrowError> {
        who.require_auth();
        if !Self::is_signer(env, who) && !Self::is_admin(env, config, who) {
            return Err(EscrowError::Unauthorized);
        }
        Ok(())
    }

    /// Rejects execution when either the global or the per-recipient
    /// execution count within the sliding window has reached the cap.
    fn check_rate_limits(
        env: &Env,
        config: &EscrowConfig,
        recipient: &Address,
    ) -> Result<(), EscrowError> {
        let now = env.ledger().timestamp();
        let cutoff = now.saturating_sub(config.rate_limit_window);
        let global: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::GlobalExecutions)
            .unwrap();
        if Self::count_in_window(&global, cutoff) >= config.rate_limit_max {
            return Err(EscrowError::RateLimited);
        }
        let per_recipient: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::RecipientExecutions(recipient.clone()))
            .unwrap_or_else(|| Vec::new(env));
        if Self::count_in_window(&per_recipient, cutoff) >= config.rate_limit_max {
            return Err(EscrowError::RateLimited);
        }
        Ok(())
    }

    fn count_in_window(timestamps: &Vec<u64>, cutoff: u64) -> u32 {
        let mut count = 0u32;
        for i in 0..timestamps.len() {
            if timestamps.get_unchecked(i) >= cutoff {
                count += 1;
            }
        }
        count
    }

    fn prune_window(timestamps: &mut Vec<u64>, cutoff: u64) {
        let mut kept = Vec::new(timestamps.env());
        for i in 0..timestamps.len() {
            let v = timestamps.get_unchecked(i);
            if v >= cutoff {
                kept.push_back(v);
            }
        }
        *timestamps = kept;
    }

    fn contains(vec: &Vec<Address>, item: &Address) -> bool {
        Self::try_index_of(vec, item).is_some()
    }

    fn try_index_of(vec: &Vec<Address>, item: &Address) -> Option<u32> {
        (0..vec.len()).find(|&i| vec.get_unchecked(i) == *item)
    }

    fn index_of(vec: &Vec<Address>, item: &Address) -> u32 {
        Self::try_index_of(vec, item).unwrap_or_else(|| panic!("item not found"))
    }

    fn validate_signer_set(
        signers: Vec<Address>,
        required_approvals: u32,
    ) -> Result<(), EscrowError> {
        if required_approvals == 0
            || required_approvals > signers.len()
            || signers.len() > MAX_SIGNERS
        {
            return Err(EscrowError::InvalidParam);
        }
        for i in 0..signers.len() {
            for j in (i + 1)..signers.len() {
                if signers.get_unchecked(i) == signers.get_unchecked(j) {
                    return Err(EscrowError::InvalidParam);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
