use crate::{AtomicTransaction, TransactionJournalEntry, TransactionStep};
use soroban_sdk::{Env, String, Symbol, Val, Vec};

/// Trait for contracts that support atomic transactions
pub trait AtomicTransactionSupport {
    /// Prepare phase: validate and lock resources without committing changes
    /// Returns true if step can be committed, false if it should be aborted
    fn prepare_step(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        function: &Symbol,
        args: &Vec<Val>,
        step: &TransactionStep, // Added step parameter to check dependencies
    ) -> bool;

    /// Commit phase: execute the prepared step
    /// Should only be called after successful prepare
    fn commit_step(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        function: &Symbol,
        args: &Vec<Val>,
    ) -> Val;

    /// Rollback phase: undo the effects of a committed step
    /// Called when transaction needs to be aborted
    fn rollback_step(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        rollback_function: &Symbol,
        rollback_args: &Vec<Val>,
    ) -> bool;

    /// Check if a step is prepared and ready for commit
    fn is_step_prepared(env: &Env, transaction_id: u64, step_id: u32) -> bool;

    /// Get step execution result for dependent steps
    fn get_step_result(env: &Env, transaction_id: u64, step_id: u32) -> Option<Val>;
}

/// Utility functions for atomic transaction management
pub struct AtomicTransactionUtils;

impl AtomicTransactionUtils {
    /// Validate transaction structure and dependencies
    pub fn validate_transaction(transaction: &AtomicTransaction) -> Result<(), &'static str> {
        if transaction.steps.is_empty() {
            return Err("Transaction must have at least one step");
        }

        if transaction.steps.len() > crate::MAX_TRANSACTION_STEPS {
            return Err("Too many transaction steps");
        }

        // Check for circular dependencies
        for step in &transaction.steps {
            if let Some(depends_on) = step.depends_on {
                if depends_on >= step.step_id {
                    return Err("Invalid dependency: step cannot depend on itself or later steps");
                }

                // Verify dependency exists
                if !transaction.steps.iter().any(|s| s.step_id == depends_on) {
                    return Err("Dependency step not found");
                }
            }
        }

        Ok(())
    }

    /// Resolve step execution order based on dependencies
    pub fn resolve_execution_order(env: &Env, steps: &Vec<TransactionStep>) -> Vec<u32> {
        let mut ordered_steps = Vec::new(env);
        let mut remaining_steps = Vec::new(env);

        // Copy steps to remaining_steps
        for step in steps.iter() {
            remaining_steps.push_back(step.clone());
        }

        while !remaining_steps.is_empty() {
            let mut progress = false;

            // Find steps with no unresolved dependencies
            let mut i = 0;
            while i < remaining_steps.len() {
                let step = remaining_steps.get(i).unwrap();
                let can_execute = match step.depends_on {
                    None => true,
                    Some(dep_id) => {
                        let mut found = false;
                        for j in 0..ordered_steps.len() {
                            if ordered_steps.get(j).unwrap() == dep_id {
                                found = true;
                                break;
                            }
                        }
                        found
                    }
                };

                if can_execute {
                    ordered_steps.push_back(step.step_id);
                    remaining_steps.remove(i);
                    progress = true;
                } else {
                    i += 1;
                }
            }

            if !progress {
                // Circular dependency detected
                break;
            }
        }

        ordered_steps
    }

    /// Check if transaction has timed out
    pub fn is_transaction_timed_out(env: &Env, transaction: &AtomicTransaction) -> bool {
        env.ledger().timestamp() > transaction.deadline
    }

    /// Create journal entry for transaction step
    pub fn create_journal_entry(
        env: &Env,
        transaction_id: u64,
        step_id: u32,
        action: &str,
        timestamp: u64,
        success: bool,
        error_message: Option<&str>,
    ) -> TransactionJournalEntry {
        TransactionJournalEntry {
            transaction_id,
            step_id,
            action: String::from_str(env, action),
            timestamp,
            success,
            error_message: error_message.map(|s| String::from_str(env, s)),
            state_snapshot: None,
        }
    }
}
