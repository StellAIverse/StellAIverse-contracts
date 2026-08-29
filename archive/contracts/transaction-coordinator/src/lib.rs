#![no_std]

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod test;
#[cfg(test)]
mod atomic_transaction_test;
mod workflows;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, String, Symbol, Vec,
};
use stellai_lib::{
    atomic::AtomicTransactionUtils, AtomicTransaction, TransactionEvent, TransactionJournalEntry,
    TransactionStatus, TransactionStep, MAX_TRANSACTION_STEPS, TRANSACTION_TIMEOUT_SECONDS,
};

pub use workflows::AtomicAgentSaleWorkflow;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    TransactionCounter,
    Transaction(u64),
    Journal(u64, u32),  // (transaction_id, step_id)
    PreparedSteps(u64), // Track prepared steps per transaction
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    Unauthorized = 2,
    TransactionNotFound = 3,
    InvalidTransactionState = 4,
    TransactionTimedOut = 5,
    StepPreparationFailed = 6,
    StepCommitFailed = 7,
    RollbackFailed = 8,
    InvalidDependency = 9,
    TooManySteps = 10,
    CircularDependency = 11,
}

#[contract]
pub struct TransactionCoordinator;

#[contractimpl]
impl TransactionCoordinator {
    /// Initialize the transaction coordinator
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::NotInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::TransactionCounter, &0u64);

        env.events().publish((symbol_short!("init"),), admin);
    }

    /// Create a new atomic transaction
    pub fn create_transaction(env: Env, initiator: Address, steps: Vec<TransactionStep>) -> u64 {
        initiator.require_auth();

        if steps.is_empty() || steps.len() > MAX_TRANSACTION_STEPS {
            panic_with_error!(&env, Error::TooManySteps);
        }

        // Generate transaction ID
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TransactionCounter)
            .unwrap_or(0);
        let transaction_id = counter + 1;

        let deadline = env.ledger().timestamp() + TRANSACTION_TIMEOUT_SECONDS;

        let transaction = AtomicTransaction {
            transaction_id,
            initiator: initiator.clone(),
            steps: steps.clone(),
            status: TransactionStatus::Initiated,
            created_at: env.ledger().timestamp(),
            deadline,
            prepared_steps: Vec::new(&env),
            executed_steps: Vec::new(&env),
            failure_reason: None,
        };

        // Validate transaction structure
        if let Err(_error) = AtomicTransactionUtils::validate_transaction(&transaction) {
            panic_with_error!(&env, Error::CircularDependency);
        }

        // Store transaction
        env.storage()
            .instance()
            .set(&DataKey::Transaction(transaction_id), &transaction);
        env.storage()
            .instance()
            .set(&DataKey::TransactionCounter, &transaction_id);

        // Initialize prepared steps tracking
        env.storage().instance().set(
            &DataKey::PreparedSteps(transaction_id),
            &Vec::<u32>::new(&env),
        );

        // Emit event
        Self::emit_transaction_event(&env, transaction_id, "initiated", None, None);

        // Create journal entry
        Self::create_journal_entry(&env, transaction_id, 0, "transaction_created", true, None);

        transaction_id
    }

    /// Execute atomic transaction using two-phase commit
    pub fn execute_transaction(env: Env, transaction_id: u64, executor: Address) -> bool {
        executor.require_auth();

        let mut transaction: AtomicTransaction = env
            .storage()
            .instance()
            .get(&DataKey::Transaction(transaction_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::TransactionNotFound));

        // Check authorization
        if transaction.initiator != executor {
            panic_with_error!(&env, Error::Unauthorized);
        }

        // Check timeout
        if AtomicTransactionUtils::is_transaction_timed_out(&env, &transaction) {
            transaction.status = TransactionStatus::TimedOut;
            env.storage()
                .instance()
                .set(&DataKey::Transaction(transaction_id), &transaction);
            Self::emit_transaction_event(&env, transaction_id, "timed_out", None, None);
            return false;
        }

        // Phase 1: Prepare all steps
        transaction.status = TransactionStatus::Preparing;
        env.storage()
            .instance()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        let execution_order =
            AtomicTransactionUtils::resolve_execution_order(&env, &transaction.steps);
        let mut prepared_steps = Vec::new(&env);

        for step_id in execution_order.iter() {
            let step = transaction
                .steps
                .iter()
                .find(|s| s.step_id == step_id)
                .unwrap();

            // Check dependencies
            if let Some(dep_id) = step.depends_on {
                let mut found = false;
                for i in 0..prepared_steps.len() {
                    if prepared_steps.get(i).unwrap() == dep_id {
                        found = true;
                        break;
                    }
                }
                if !found {
                    Self::rollback_transaction(&env, transaction_id, &prepared_steps);
                    return false;
                }
            }

            // Prepare step
            let prepare_success = Self::prepare_step(&env, transaction_id, &step);

            if prepare_success {
                prepared_steps.push_back(step_id);
                Self::emit_transaction_event(
                    &env,
                    transaction_id,
                    "step_prepared",
                    Some(step_id),
                    None,
                );
            } else {
                Self::create_journal_entry(
                    &env,
                    transaction_id,
                    step_id,
                    "prepare_failed",
                    false,
                    Some("Step preparation failed"),
                );
                Self::rollback_transaction(&env, transaction_id, &prepared_steps);
                return false;
            }
        }

        // All steps prepared successfully
        transaction.status = TransactionStatus::Prepared;
        transaction.prepared_steps = prepared_steps.clone();
        env.storage()
            .instance()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        // Phase 2: Commit all steps
        transaction.status = TransactionStatus::Committing;
        env.storage()
            .instance()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        let mut executed_steps = Vec::new(&env);

        for step_id in execution_order.iter() {
            let step = transaction
                .steps
                .iter()
                .find(|s| s.step_id == step_id)
                .unwrap();

            let commit_success = Self::commit_step(&env, transaction_id, &step);

            if commit_success {
                executed_steps.push_back(step_id);
                Self::emit_transaction_event(
                    &env,
                    transaction_id,
                    "step_committed",
                    Some(step_id),
                    None,
                );
            } else {
                Self::create_journal_entry(
                    &env,
                    transaction_id,
                    step_id,
                    "commit_failed",
                    false,
                    Some("Step commit failed"),
                );
                Self::rollback_transaction(&env, transaction_id, &executed_steps);
                return false;
            }
        }

        // Transaction completed successfully
        transaction.status = TransactionStatus::Committed;
        transaction.executed_steps = executed_steps;
        env.storage()
            .instance()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        Self::emit_transaction_event(&env, transaction_id, "completed", None, None);
        Self::create_journal_entry(&env, transaction_id, 0, "transaction_completed", true, None);

        true
    }

    /// Get transaction details
    pub fn get_transaction(env: Env, transaction_id: u64) -> Option<AtomicTransaction> {
        env.storage()
            .instance()
            .get(&DataKey::Transaction(transaction_id))
    }

    /// Get transaction status
    pub fn get_transaction_status(env: Env, transaction_id: u64) -> Option<TransactionStatus> {
        env.storage()
            .instance()
            .get::<DataKey, AtomicTransaction>(&DataKey::Transaction(transaction_id))
            .map(|tx| tx.status)
    }

    /// Prepare a single step by invoking the contract's AtomicTransactionSupport implementation
    fn prepare_step(env: &Env, transaction_id: u64, step: &TransactionStep) -> bool {
        use stellai_lib::atomic::AtomicTransactionSupport;
        use crate::workflows::MarketplaceAtomicSupport;
        
        // Check if this is a marketplace contract that implements AtomicTransactionSupport
        // In a full implementation, we would use a generic approach to handle any contract
        // that implements AtomicTransactionSupport
        let prepare_success = MarketplaceAtomicSupport::prepare_step(
                env,
                transaction_id,
                step.step_id,
                &step.function,
                &step.args,
                step,
            );

        Self::create_journal_entry(
            env, 
            transaction_id, 
            step.step_id, 
            "prepare", 
            prepare_success, 
            if prepare_success { None } else { Some("Step preparation failed") }
        );
        
        prepare_success
    }

    /// Commit a single step by invoking the contract's AtomicTransactionSupport implementation
    fn commit_step(env: &Env, transaction_id: u64, step: &TransactionStep) -> bool {
        use stellai_lib::atomic::AtomicTransactionSupport;
        use crate::workflows::MarketplaceAtomicSupport;
        
        // Invoke the contract's commit_step method
        let result = MarketplaceAtomicSupport::commit_step(
            env,
            transaction_id,
            step.step_id,
            &step.function,
            &step.args
        );

        let commit_success = result.try_into_val(env).unwrap_or(false);
        Self::create_journal_entry(
            env, 
            transaction_id, 
            step.step_id, 
            "commit", 
            commit_success, 
            if commit_success { None } else { Some("Step commit failed") }
        );
        
        commit_success
    }

    /// Rollback transaction by undoing executed steps in reverse order
    fn rollback_transaction(env: &Env, transaction_id: u64, executed_steps: &Vec<u32>) -> bool {
        let mut transaction: AtomicTransaction = env
            .storage()
            .instance()
            .get(&DataKey::Transaction(transaction_id))
            .unwrap();

        transaction.status = TransactionStatus::RollingBack;
        env.storage()
            .instance()
            .set(&DataKey::Transaction(transaction_id), &transaction);

        let mut rollback_success = true;

        // Rollback in reverse order
        for i in (0..executed_steps.len()).rev() {
            let step_id = executed_steps.get(i).unwrap();
            let step = transaction
                .steps
                .iter()
                .find(|s| s.step_id == step_id)
                .unwrap();

            if let (Some(rollback_contract), Some(rollback_function), Some(rollback_args)) =
                (&step.rollback_contract, &step.rollback_function, &step.rollback_args)
            {
                use stellai_lib::atomic::AtomicTransactionSupport;
                use crate::workflows::MarketplaceAtomicSupport;
                
                // Invoke the contract's rollback_step method
                let step_rollback_success = MarketplaceAtomicSupport::rollback_step(
                    env,
                    transaction_id,
                    step.step_id,
                    &rollback_function,
                    &rollback_args
                );

                if !step_rollback_success {
                    rollback_success = false;
                }

                Self::create_journal_entry(
                    env, 
                    transaction_id, 
                    step_id, 
                    "rollback", 
                    step_rollback_success, 
                    if step_rollback_success { None } else { Some("Step rollback failed") }
                );
            }
        }

        transaction.status = if rollback_success {
            TransactionStatus::RolledBack
        } else {
            TransactionStatus::Failed
        };

        env.storage()
            .instance()
            .set(&DataKey::Transaction(transaction_id), &transaction);
        Self::emit_transaction_event(env, transaction_id, "rolled_back", None, None);

        rollback_success
    }

    /// Create journal entry for audit trail
    fn create_journal_entry(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        action: &str,
        success: bool,
        error_message: Option<&str>,
    ) {
        let entry = TransactionJournalEntry {
            transaction_id,
            step_id,
            action: String::from_str(env, action),
            timestamp: env.ledger().timestamp(),
            success,
            error_message: error_message.map(|s| String::from_str(env, s)),
            state_snapshot: None,
        };

        env.storage()
            .instance()
            .set(&DataKey::Journal(transaction_id, step_id), &entry);
    }

    /// Emit transaction event
    fn emit_transaction_event(
        env: &Env,
        transaction_id: u64,
        event_type: &str,
        step_id: Option<u32>,
        details: Option<&str>,
    ) {
        let _event = TransactionEvent {
            transaction_id,
            event_type: String::from_str(env, event_type),
            step_id,
            timestamp: env.ledger().timestamp(),
            details: details.map(|s| String::from_str(env, s)),
        };

        env.events().publish(
            (Symbol::new(env, "tx_event"),),
            (transaction_id, event_type, step_id),
        );
    }

    /// Get transaction journal for audit.
    ///
    /// Iterates through stored journal entries for the given transaction.
    /// Journal entries are keyed by (transaction_id, step_id) where step_id 0
    /// is used for transaction-level events (created, completed, rolled_back).
    pub fn get_transaction_journal(env: Env, transaction_id: u64) -> Vec<TransactionJournalEntry> {
        let mut journal = Vec::new(&env);

        let transaction: Option<AtomicTransaction> = env
            .storage()
            .instance()
            .get(&DataKey::Transaction(transaction_id));

        if let Some(tx) = transaction {
            // Step 0 is the transaction-level journal entry (created/completed)
            if let Some(entry) = env
                .storage()
                .instance()
                .get::<_, TransactionJournalEntry>(&DataKey::Journal(transaction_id, 0))
            {
                journal.push_back(entry);
            }

            // Iterate through step journal entries
            for i in 0..tx.steps.len() {
                let step: TransactionStep = tx.steps.get(i).unwrap();
                if let Some(entry) =
                    env.storage()
                        .instance()
                        .get::<_, TransactionJournalEntry>(&DataKey::Journal(
                            transaction_id,
                            step.step_id,
                        ))
                {
                    journal.push_back(entry);
                }
            }
        }

        journal
    }

    /// Clean up expired transactions (admin only)
    pub fn cleanup_expired_transactions(env: Env, admin: Address, _max_cleanup: u32) -> u32 {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));

        if admin != stored_admin {
            panic_with_error!(&env, Error::Unauthorized);
        }

        // In a real implementation, you'd iterate through transactions and clean up expired ones
        // This is a placeholder that returns 0 cleaned up transactions
        0
    }
}