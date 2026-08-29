#![no_std]

use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Bytes, Env, IntoVal, String, Symbol, Val, Vec,
};

use stellai_lib::{
    OptionalWorkflowCallback, WorkflowCallback, WorkflowInstance, WorkflowStatus, WorkflowStep,
    WorkflowStepStatus, WorkflowSummary,
};

// ── Storage keys ──────────────────────────────────────────────────────────────

const ADMIN_KEY: Symbol = symbol_short!("hub_adm");
const WORKFLOW_CTR_KEY: Symbol = symbol_short!("wf_ctr");
const EXEC_CTR_KEY: Symbol = symbol_short!("exec_ctr");

const WF_PREFIX: &str = "wf";
const WF_HIST_PREFIX: &str = "wf_hist";
const RULE_PREFIX: &str = "rule";

const NONCE_PREFIX: Symbol = symbol_short!("nonce");
const HIST_PREFIX: Symbol = symbol_short!("hist");
const RATE_PREFIX: Symbol = symbol_short!("ratelim");

// ── Config ────────────────────────────────────────────────────────────────────

const MAX_STEPS: u32 = 10;
const MAX_STRING_LENGTH: u32 = 256;
const MAX_DATA_SIZE: u32 = 65536;
const MAX_HISTORY_SIZE: u32 = 1000;
const MAX_HISTORY_QUERY_LIMIT: u32 = 500;
const DEFAULT_RATE_LIMIT_OPS: u32 = 100;
const DEFAULT_RATE_LIMIT_WINDOW: u64 = 60;
const DEFAULT_WORKFLOW_TIMEOUT: u64 = 300;
const MAX_HISTORY_PER_INITIATOR: u32 = 200;

// ── Local types ───────────────────────────────────────────────────────────────

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct ActionRecord {
    pub execution_id: u64,
    pub action: String,
    pub executor: Address,
    pub timestamp: u64,
    pub nonce: u64,
}

#[derive(Clone)]
#[soroban_sdk::contracttype]
pub struct RateLimitData {
    pub last_reset: u64,
    pub count: u32,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct ExecutionHub;

#[contractimpl]
impl ExecutionHub {
    // =========================================================================
    // Initialisation
    // =========================================================================

    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("Already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&WORKFLOW_CTR_KEY, &0u64);
        env.storage().instance().set(&EXEC_CTR_KEY, &0u64);
        env.events().publish((symbol_short!("hub_init"),), admin);
    }

    // =========================================================================
    // Rule management
    // =========================================================================

    pub fn register_rule(
        env: Env,
        agent_id: u64,
        owner: Address,
        rule_name: String,
        rule_data: Bytes,
    ) {
        owner.require_auth();
        Self::validate_agent_id(agent_id);
        Self::validate_string(&rule_name);
        Self::validate_data(&rule_data);

        let key = (
            String::from_str(&env, RULE_PREFIX),
            agent_id,
            rule_name.clone(),
        );
        env.storage().instance().set(&key, &rule_data);
        env.events().publish(
            (symbol_short!("rule_reg"),),
            (agent_id, rule_name, owner, env.ledger().timestamp()),
        );
    }

    pub fn revoke_rule(env: Env, agent_id: u64, owner: Address, rule_name: String) {
        owner.require_auth();
        Self::validate_agent_id(agent_id);
        let key = (
            String::from_str(&env, RULE_PREFIX),
            agent_id,
            rule_name.clone(),
        );
        env.storage().instance().remove(&key);
        env.events()
            .publish((symbol_short!("rule_rev"),), (agent_id, rule_name, owner));
    }

    pub fn get_rule(env: Env, agent_id: u64, rule_name: String) -> Option<Bytes> {
        Self::validate_agent_id(agent_id);
        let key = (String::from_str(&env, RULE_PREFIX), agent_id, rule_name);
        env.storage().instance().get(&key)
    }

    // =========================================================================
    // Legacy action execution
    // =========================================================================

    pub fn execute_action(
        env: Env,
        agent_id: u64,
        executor: Address,
        action: String,
        parameters: Bytes,
        nonce: u64,
    ) -> u64 {
        executor.require_auth();
        Self::validate_agent_id(agent_id);
        Self::validate_string(&action);
        Self::validate_data(&parameters);

        let stored_nonce = Self::get_nonce_inner(&env, agent_id);
        if nonce <= stored_nonce {
            panic!("Invalid nonce: replay protection triggered");
        }

        Self::check_rate_limit(
            &env,
            agent_id,
            DEFAULT_RATE_LIMIT_OPS,
            DEFAULT_RATE_LIMIT_WINDOW,
        );

        let exec_id = Self::next_exec_id(&env);
        Self::set_nonce_inner(&env, agent_id, nonce);
        Self::append_action_record(&env, agent_id, exec_id, &action, &executor, nonce);

        env.events().publish(
            (symbol_short!("act_exec"),),
            (
                exec_id,
                agent_id,
                action,
                executor,
                env.ledger().timestamp(),
                nonce,
            ),
        );
        exec_id
    }

    pub fn get_history(env: Env, agent_id: u64, limit: u32) -> Vec<ActionRecord> {
        Self::validate_agent_id(agent_id);
        if limit > MAX_HISTORY_QUERY_LIMIT {
            panic!("Limit exceeds maximum allowed");
        }
        let key = (HIST_PREFIX, agent_id);
        let history: Vec<ActionRecord> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        let mut result = Vec::new(&env);
        let start = if history.len() > limit {
            history.len() - limit
        } else {
            0
        };
        for i in start..history.len() {
            if let Some(item) = history.get(i) {
                result.push_back(item);
            }
        }
        result
    }

    pub fn get_action_count(env: Env, agent_id: u64) -> u32 {
        Self::validate_agent_id(agent_id);
        let key = (HIST_PREFIX, agent_id);
        let history: Vec<ActionRecord> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        history.len()
    }

    // =========================================================================
    // Workflow creation
    // =========================================================================

    /// Create a new workflow.  Returns the workflow_id.
    ///
    /// `steps` must be non-empty, ordered, and have `status = Pending`.
    /// `callback_contract`: if provided, `wf_done(workflow_id, status)` will be called on it at terminal status.
    pub fn create_workflow(
        env: Env,
        initiator: Address,
        name: String,
        steps: Vec<WorkflowStep>,
        deadline_offset_seconds: Option<u64>,
        context_tag: Option<String>,
        callback_contract: Option<Address>,
    ) -> u64 {
        initiator.require_auth();

        if steps.is_empty() {
            panic!("Workflow must have at least one step");
        }
        if steps.len() > MAX_STEPS {
            panic!("Too many workflow steps");
        }
        Self::validate_string(&name);

        let now = env.ledger().timestamp();
        let offset = deadline_offset_seconds.unwrap_or(DEFAULT_WORKFLOW_TIMEOUT);
        let deadline = now.checked_add(offset).expect("Deadline overflow");

        let callback = match callback_contract {
            Some(contract) => OptionalWorkflowCallback::Some(WorkflowCallback {
                callback_contract: contract,
                fired: false,
            }),
            None => OptionalWorkflowCallback::None,
        };

        let workflow_id = Self::next_workflow_id(&env);
        let workflow = WorkflowInstance {
            workflow_id,
            initiator: initiator.clone(),
            name: name.clone(),
            steps,
            status: WorkflowStatus::Pending,
            current_step: 0,
            completed_steps: 0,
            created_at: now,
            updated_at: now,
            deadline,
            context_tag,
            callback,
            failure_reason: None,
            rolled_back_steps: 0,
        };

        Self::save_workflow(&env, &workflow);
        Self::append_history_summary(&env, &initiator, &workflow);

        env.events().publish(
            (symbol_short!("wf_crt"),),
            (workflow_id, initiator, name, now),
        );
        workflow_id
    }

    // =========================================================================
    // Workflow step execution
    // =========================================================================

    /// Advance the workflow by executing its next pending step.
    /// Returns the step's final status.
    pub fn execute_workflow_step(env: Env, workflow_id: u64) -> WorkflowStepStatus {
        let mut wf = Self::load_workflow(&env, workflow_id);

        if wf.status != WorkflowStatus::Pending && wf.status != WorkflowStatus::Running {
            panic!("Workflow is not in an executable state");
        }

        let now = env.ledger().timestamp();

        if now > wf.deadline {
            wf.status = WorkflowStatus::Failed;
            wf.failure_reason = Some(String::from_str(&env, "Deadline exceeded"));
            wf.updated_at = now;
            Self::save_workflow(&env, &wf);
            Self::maybe_fire_callback(&env, &mut wf);
            Self::save_workflow(&env, &wf);
            env.events()
                .publish((symbol_short!("wf_tmout"),), (workflow_id, now));
            return WorkflowStepStatus::Failed;
        }

        wf.status = WorkflowStatus::Running;
        let step_index = wf.current_step;
        if step_index >= wf.steps.len() {
            panic!("All steps already executed");
        }

        let mut step = wf.steps.get(step_index).expect("Step not found");
        step.status = WorkflowStepStatus::Executing;
        step.updated_at = now;
        wf.steps.set(step_index, step.clone());
        Self::save_workflow(&env, &wf);

        let call_ok = Self::try_invoke_step(&env, &step);

        if call_ok {
            step.status = WorkflowStepStatus::Completed;
            step.updated_at = now;
            step.error = None;
            wf.steps.set(step_index, step.clone());
            wf.completed_steps = wf
                .completed_steps
                .checked_add(1)
                .expect("completed_steps overflow");
            wf.current_step = wf
                .current_step
                .checked_add(1)
                .expect("current_step overflow");
            wf.updated_at = now;

            env.events().publish(
                (symbol_short!("wf_stp_ok"),),
                (workflow_id, step_index, step.name.clone(), now),
            );

            if wf.current_step >= wf.steps.len() {
                wf.status = WorkflowStatus::Completed;
                Self::update_history_summary(&env, &wf);
                Self::save_workflow(&env, &wf);
                Self::maybe_fire_callback(&env, &mut wf);
                Self::save_workflow(&env, &wf);
                env.events().publish(
                    (symbol_short!("wf_done"),),
                    (workflow_id, wf.completed_steps, now),
                );
            } else {
                Self::save_workflow(&env, &wf);
            }

            WorkflowStepStatus::Completed
        } else if step.retry_count < step.max_retries {
            step.retry_count = step
                .retry_count
                .checked_add(1)
                .expect("retry_count overflow");
            step.status = WorkflowStepStatus::Pending;
            step.error = Some(String::from_str(&env, "Transient failure; will retry"));
            step.updated_at = now;
            wf.steps.set(step_index, step);
            wf.updated_at = now;
            Self::save_workflow(&env, &wf);
            env.events()
                .publish((symbol_short!("wf_retry"),), (workflow_id, step_index, now));
            WorkflowStepStatus::Pending
        } else if step.required {
            step.status = WorkflowStepStatus::Failed;
            step.error = Some(String::from_str(&env, "Step failed after all retries"));
            step.updated_at = now;
            wf.steps.set(step_index, step);
            wf.failure_reason = Some(String::from_str(&env, "Required step failed"));
            wf.updated_at = now;
            Self::save_workflow(&env, &wf);

            Self::rollback_completed_steps(&env, &mut wf);
            Self::update_history_summary(&env, &wf);
            Self::save_workflow(&env, &wf);
            Self::maybe_fire_callback(&env, &mut wf);
            Self::save_workflow(&env, &wf);

            env.events()
                .publish((symbol_short!("wf_fail"),), (workflow_id, step_index, now));
            WorkflowStepStatus::Failed
        } else {
            // Optional step — skip it
            step.status = WorkflowStepStatus::Skipped;
            step.error = Some(String::from_str(&env, "Optional step failed; skipped"));
            step.updated_at = now;
            wf.steps.set(step_index, step);
            wf.current_step = wf
                .current_step
                .checked_add(1)
                .expect("current_step overflow");
            wf.updated_at = now;
            Self::save_workflow(&env, &wf);
            env.events()
                .publish((symbol_short!("wf_skip"),), (workflow_id, step_index, now));
            WorkflowStepStatus::Skipped
        }
    }

    // =========================================================================
    // Manual cancellation
    // =========================================================================

    pub fn cancel_workflow(env: Env, workflow_id: u64, caller: Address) {
        caller.require_auth();
        let mut wf = Self::load_workflow(&env, workflow_id);

        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("Hub not initialized");

        if caller != wf.initiator && caller != admin {
            panic!("Unauthorized: only initiator or admin can cancel");
        }

        match wf.status {
            WorkflowStatus::Completed
            | WorkflowStatus::RolledBack
            | WorkflowStatus::Failed
            | WorkflowStatus::Cancelled => panic!("Workflow already terminal"),
            _ => {}
        }

        let now = env.ledger().timestamp();
        wf.failure_reason = Some(String::from_str(&env, "Cancelled by caller"));
        wf.updated_at = now;

        Self::rollback_completed_steps(&env, &mut wf);
        wf.status = WorkflowStatus::Cancelled;
        Self::update_history_summary(&env, &wf);
        Self::save_workflow(&env, &wf);
        Self::maybe_fire_callback(&env, &mut wf);
        Self::save_workflow(&env, &wf);

        env.events()
            .publish((symbol_short!("wf_cncl"),), (workflow_id, caller, now));
    }

    // =========================================================================
    // Queries
    // =========================================================================

    pub fn get_workflow(env: Env, workflow_id: u64) -> WorkflowInstance {
        Self::load_workflow(&env, workflow_id)
    }

    pub fn get_workflow_status(env: Env, workflow_id: u64) -> WorkflowStatus {
        Self::load_workflow(&env, workflow_id).status
    }

    pub fn get_workflow_history(env: Env, initiator: Address) -> Vec<WorkflowSummary> {
        let key = (String::from_str(&env, WF_HIST_PREFIX), initiator);
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("Hub not initialized")
    }

    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::assert_admin(&env, &current_admin);
        env.storage().instance().set(&ADMIN_KEY, &new_admin);
        env.events()
            .publish((symbol_short!("adm_xfer"),), (current_admin, new_admin));
    }

    pub fn get_execution_counter(env: Env) -> u64 {
        env.storage().instance().get(&EXEC_CTR_KEY).unwrap_or(0)
    }

    pub fn get_workflow_counter(env: Env) -> u64 {
        env.storage().instance().get(&WORKFLOW_CTR_KEY).unwrap_or(0)
    }

    // =========================================================================
    // Private — IDs
    // =========================================================================

    fn next_exec_id(env: &Env) -> u64 {
        let current: u64 = env.storage().instance().get(&EXEC_CTR_KEY).unwrap_or(0);
        let next = current.checked_add(1).expect("Execution ID overflow");
        env.storage().instance().set(&EXEC_CTR_KEY, &next);
        next
    }

    fn next_workflow_id(env: &Env) -> u64 {
        let current: u64 = env.storage().instance().get(&WORKFLOW_CTR_KEY).unwrap_or(0);
        let next = current.checked_add(1).expect("Workflow ID overflow");
        env.storage().instance().set(&WORKFLOW_CTR_KEY, &next);
        next
    }

    // =========================================================================
    // Private — storage
    // =========================================================================

    fn workflow_key(env: &Env, workflow_id: u64) -> (String, u64) {
        (String::from_str(env, WF_PREFIX), workflow_id)
    }

    fn load_workflow(env: &Env, workflow_id: u64) -> WorkflowInstance {
        let key = Self::workflow_key(env, workflow_id);
        env.storage()
            .instance()
            .get(&key)
            .expect("Workflow not found")
    }

    fn save_workflow(env: &Env, wf: &WorkflowInstance) {
        let key = Self::workflow_key(env, wf.workflow_id);
        env.storage().instance().set(&key, wf);
    }

    fn append_history_summary(env: &Env, initiator: &Address, wf: &WorkflowInstance) {
        let key = (String::from_str(env, WF_HIST_PREFIX), initiator.clone());
        let mut history: Vec<WorkflowSummary> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        if history.len() >= MAX_HISTORY_PER_INITIATOR {
            history.remove(0);
        }
        history.push_back(WorkflowSummary {
            workflow_id: wf.workflow_id,
            name: wf.name.clone(),
            status: wf.status,
            created_at: wf.created_at,
            completed_at: None,
        });
        env.storage().instance().set(&key, &history);
    }

    fn update_history_summary(env: &Env, wf: &WorkflowInstance) {
        let key = (String::from_str(env, WF_HIST_PREFIX), wf.initiator.clone());
        let mut history: Vec<WorkflowSummary> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        let now = env.ledger().timestamp();
        for i in 0..history.len() {
            if let Some(mut s) = history.get(i) {
                if s.workflow_id == wf.workflow_id {
                    s.status = wf.status;
                    s.completed_at = Some(now);
                    history.set(i, s);
                    break;
                }
            }
        }
        env.storage().instance().set(&key, &history);
    }

    // =========================================================================
    // Private — step execution
    // =========================================================================

    /// Invoke the step's target contract.  Returns true on success.
    ///
    /// Because Soroban panics abort the entire transaction, every workflow
    /// step must be idempotent and its target function must not panic on
    /// inputs it controls.  The `required` / `max_retries` fields let
    /// callers classify transient vs permanent failures.
    fn try_invoke_step(env: &Env, step: &WorkflowStep) -> bool {
        let mut args = Vec::<Val>::new(env);
        args.push_back(step.step_index.into_val(env));
        args.push_back(step.encoded_args.clone().into_val(env));
        env.invoke_contract::<()>(&step.target_contract, &symbol_short!("exec_step"), args);
        true
    }

    /// Roll back completed steps in reverse order by calling `rollback` on
    /// their target contracts with the original `encoded_args`.
    fn rollback_completed_steps(env: &Env, wf: &mut WorkflowInstance) {
        let now = env.ledger().timestamp();
        let rollback_sym = symbol_short!("rollback");
        let step_count = wf.steps.len();

        let mut i = step_count;
        while i > 0 {
            i -= 1;
            let mut step = wf.steps.get(i).expect("Step index out of range");
            if step.status != WorkflowStepStatus::Completed {
                continue;
            }

            let mut rb_args = Vec::<Val>::new(env);
            rb_args.push_back(step.encoded_args.clone().into_val(env));
            env.invoke_contract::<()>(&step.target_contract, &rollback_sym, rb_args);

            step.status = WorkflowStepStatus::RolledBack;
            step.updated_at = now;
            wf.steps.set(i, step);
            wf.rolled_back_steps = wf
                .rolled_back_steps
                .checked_add(1)
                .expect("rolled_back_steps overflow");
            wf.updated_at = now;

            env.events()
                .publish((symbol_short!("wf_rb"),), (wf.workflow_id, i, now));
        }

        // Determine terminal status
        let any_step_failed = {
            let mut failed = false;
            for idx in 0..wf.steps.len() {
                if let Some(s) = wf.steps.get(idx) {
                    if s.status == WorkflowStepStatus::Failed {
                        failed = true;
                        break;
                    }
                }
            }
            failed
        };
        wf.status = if any_step_failed {
            WorkflowStatus::Failed
        } else {
            WorkflowStatus::RolledBack
        };
        wf.updated_at = now;
    }

    /// Fire the registered callback if present and not yet fired.
    fn maybe_fire_callback(env: &Env, wf: &mut WorkflowInstance) {
        let cb = match &wf.callback {
            OptionalWorkflowCallback::Some(c) if !c.fired => c.callback_contract.clone(),
            _ => return,
        };

        let status_u32: u32 = match wf.status {
            WorkflowStatus::Completed => 2,
            WorkflowStatus::RolledBack => 3,
            WorkflowStatus::Failed => 4,
            WorkflowStatus::Cancelled => 5,
            _ => 0,
        };

        let mut args = Vec::<Val>::new(env);
        args.push_back(wf.workflow_id.into_val(env));
        args.push_back(status_u32.into_val(env));

        env.invoke_contract::<()>(&cb, &symbol_short!("wf_done"), args);

        if let OptionalWorkflowCallback::Some(ref mut c) = wf.callback {
            c.fired = true;
        }

        env.events().publish(
            (symbol_short!("wf_cb"),),
            (wf.workflow_id, status_u32, env.ledger().timestamp()),
        );
    }

    // =========================================================================
    // Private — v1 compatibility helpers
    // =========================================================================

    fn get_nonce_inner(env: &Env, agent_id: u64) -> u64 {
        let key = (NONCE_PREFIX, agent_id);
        env.storage().instance().get(&key).unwrap_or(0)
    }

    fn set_nonce_inner(env: &Env, agent_id: u64, nonce: u64) {
        let key = (NONCE_PREFIX, agent_id);
        env.storage().instance().set(&key, &nonce);
    }

    fn append_action_record(
        env: &Env,
        agent_id: u64,
        exec_id: u64,
        action: &String,
        executor: &Address,
        nonce: u64,
    ) {
        let key = (HIST_PREFIX, agent_id);
        let mut history: Vec<ActionRecord> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        if history.len() >= MAX_HISTORY_SIZE {
            panic!("Action history limit exceeded");
        }
        history.push_back(ActionRecord {
            execution_id: exec_id,
            action: action.clone(),
            executor: executor.clone(),
            timestamp: env.ledger().timestamp(),
            nonce,
        });
        env.storage().instance().set(&key, &history);
    }

    fn check_rate_limit(env: &Env, agent_id: u64, max_ops: u32, window: u64) {
        let now = env.ledger().timestamp();
        let key = (RATE_PREFIX, agent_id);
        let data: Option<RateLimitData> = env.storage().instance().get(&key);
        let (last_reset, count) = match data {
            Some(d) => (d.last_reset, d.count),
            None => (now, 0),
        };
        let elapsed = now.saturating_sub(last_reset);
        let (new_reset, new_count) = if elapsed > window {
            (now, 1)
        } else if count < max_ops {
            (last_reset, count + 1)
        } else {
            panic!("Rate limit exceeded");
        };
        env.storage().instance().set(
            &key,
            &RateLimitData {
                last_reset: new_reset,
                count: new_count,
            },
        );
    }

    // =========================================================================
    // Validation
    // =========================================================================

    fn validate_agent_id(agent_id: u64) {
        if agent_id == 0 {
            panic!("Invalid agent ID: must be non-zero");
        }
    }

    fn validate_string(s: &String) {
        if s.len() > MAX_STRING_LENGTH {
            panic!("String exceeds maximum length");
        }
    }

    fn validate_data(data: &Bytes) {
        if data.len() > MAX_DATA_SIZE {
            panic!("Data exceeds maximum size");
        }
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("Hub not initialized");
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Env,
    };

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ExecutionHub, ());
        let admin = Address::generate(&env);
        let client = ExecutionHubClient::new(&env, &contract_id);
        client.initialize(&admin);
        (env, contract_id, admin)
    }

    fn make_step(
        env: &Env,
        idx: u32,
        target: &Address,
        fn_name: &str,
        required: bool,
    ) -> WorkflowStep {
        WorkflowStep {
            step_index: idx,
            name: String::from_str(env, fn_name),
            target_contract: target.clone(),
            function_name: String::from_str(env, fn_name),
            encoded_args: Bytes::new(env),
            required,
            max_retries: 0,
            retry_count: 0,
            status: WorkflowStepStatus::Pending,
            result: None,
            error: None,
            updated_at: 0,
        }
    }

    #[test]
    fn test_initialization() {
        let (env, contract_id, admin) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_execution_counter(), 0);
        assert_eq!(client.get_workflow_counter(), 0);
    }

    #[test]
    #[should_panic(expected = "Already initialized")]
    fn test_double_init() {
        let (env, contract_id, admin) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        client.initialize(&admin);
    }

    #[test]
    fn test_register_and_get_rule() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let rule_name = String::from_str(&env, "buy_rule");
        let rule_data = Bytes::from_array(&env, &[1, 2, 3, 4]);
        client.register_rule(&1u64, &owner, &rule_name, &rule_data);
        let retrieved = client.get_rule(&1u64, &rule_name);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), rule_data);
    }

    #[test]
    fn test_revoke_rule() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let rule_name = String::from_str(&env, "tmp_rule");
        let rule_data = Bytes::from_array(&env, &[9, 8]);
        client.register_rule(&2u64, &owner, &rule_name, &rule_data);
        client.revoke_rule(&2u64, &owner, &rule_name);
        assert!(client.get_rule(&2u64, &rule_name).is_none());
    }

    #[test]
    fn test_execute_action_increments_counter() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let executor = Address::generate(&env);
        let action = String::from_str(&env, "mint");
        let params = Bytes::from_array(&env, &[0]);
        let id1 = client.execute_action(&1u64, &executor, &action, &params, &1u64);
        let id2 = client.execute_action(&1u64, &executor, &action, &params, &2u64);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    #[should_panic(expected = "Invalid nonce")]
    fn test_replay_protection() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let executor = Address::generate(&env);
        let action = String::from_str(&env, "act");
        let params = Bytes::from_array(&env, &[1]);
        client.execute_action(&1u64, &executor, &action, &params, &5u64);
        client.execute_action(&1u64, &executor, &action, &params, &5u64);
    }

    #[test]
    fn test_get_history() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let executor = Address::generate(&env);
        let action = String::from_str(&env, "do_thing");
        let params = Bytes::from_array(&env, &[7]);
        client.execute_action(&3u64, &executor, &action, &params, &1u64);
        client.execute_action(&3u64, &executor, &action, &params, &2u64);
        assert_eq!(client.get_history(&3u64, &10u32).len(), 2);
        assert_eq!(client.get_action_count(&3u64), 2);
    }

    #[test]
    fn test_admin_transfer() {
        let (env, contract_id, admin) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    fn test_rate_limiting_low_volume() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let executor = Address::generate(&env);
        let action = String::from_str(&env, "ping");
        let params = Bytes::from_array(&env, &[0]);
        for i in 1u64..=10 {
            assert!(client.execute_action(&1u64, &executor, &action, &params, &i) > 0);
        }
    }

    // ── Workflow tests ────────────────────────────────────────────────────────

    #[test]
    fn test_create_workflow_assigns_id() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let initiator = Address::generate(&env);
        let target = Address::generate(&env);
        let mut steps = Vec::new(&env);
        steps.push_back(make_step(&env, 0, &target, "do_work", true));

        let wf_id = client.create_workflow(
            &initiator,
            &String::from_str(&env, "test_wf"),
            &steps,
            &None,
            &None,
            &None,
        );
        assert_eq!(wf_id, 1u64);
        assert_eq!(client.get_workflow_counter(), 1u64);

        let wf = client.get_workflow(&wf_id);
        assert_eq!(wf.status, WorkflowStatus::Pending);
        assert_eq!(wf.completed_steps, 0u32);
    }

    #[test]
    fn test_workflow_history_tracked_per_initiator() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let initiator = Address::generate(&env);
        let target = Address::generate(&env);
        let mut steps = Vec::new(&env);
        steps.push_back(make_step(&env, 0, &target, "noop", true));

        client.create_workflow(
            &initiator,
            &String::from_str(&env, "wf_a"),
            &steps,
            &None,
            &None,
            &None,
        );
        client.create_workflow(
            &initiator,
            &String::from_str(&env, "wf_b"),
            &steps,
            &None,
            &None,
            &None,
        );

        let history = client.get_workflow_history(&initiator);
        assert_eq!(history.len(), 2u32);
    }

    #[test]
    fn test_workflow_deadline_stored() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        env.ledger().set_timestamp(1_000_000);

        let initiator = Address::generate(&env);
        let target = Address::generate(&env);
        let mut steps = Vec::new(&env);
        steps.push_back(make_step(&env, 0, &target, "x", true));

        let wf_id = client.create_workflow(
            &initiator,
            &String::from_str(&env, "dl_wf"),
            &steps,
            &Some(600u64),
            &None,
            &None,
        );
        assert_eq!(client.get_workflow(&wf_id).deadline, 1_000_600u64);
    }

    #[test]
    fn test_context_tag_stored() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let initiator = Address::generate(&env);
        let target = Address::generate(&env);
        let mut steps = Vec::new(&env);
        steps.push_back(make_step(&env, 0, &target, "x", true));
        let tag = Some(String::from_str(&env, "listing:42"));

        let wf_id = client.create_workflow(
            &initiator,
            &String::from_str(&env, "tag_wf"),
            &steps,
            &None,
            &tag,
            &None,
        );
        let wf = client.get_workflow(&wf_id);
        assert_eq!(wf.context_tag, Some(String::from_str(&env, "listing:42")));
    }

    #[test]
    #[should_panic(expected = "Workflow must have at least one step")]
    fn test_empty_workflow_rejected() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let initiator = Address::generate(&env);
        client.create_workflow(
            &initiator,
            &String::from_str(&env, "empty"),
            &Vec::new(&env),
            &None,
            &None,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "Too many workflow steps")]
    fn test_too_many_steps_rejected() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let initiator = Address::generate(&env);
        let target = Address::generate(&env);
        let mut steps = Vec::new(&env);
        for i in 0..=10u32 {
            steps.push_back(make_step(&env, i, &target, "x", true));
        }
        client.create_workflow(
            &initiator,
            &String::from_str(&env, "too_many"),
            &steps,
            &None,
            &None,
            &None,
        );
    }

    #[test]
    fn test_workflow_status_query() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let initiator = Address::generate(&env);
        let target = Address::generate(&env);
        let mut steps = Vec::new(&env);
        steps.push_back(make_step(&env, 0, &target, "x", true));

        let wf_id = client.create_workflow(
            &initiator,
            &String::from_str(&env, "stat_wf"),
            &steps,
            &None,
            &None,
            &None,
        );
        assert_eq!(client.get_workflow_status(&wf_id), WorkflowStatus::Pending);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: only initiator or admin can cancel")]
    fn test_cancel_unauthorized() {
        let (env, contract_id, _) = setup();
        let client = ExecutionHubClient::new(&env, &contract_id);
        let initiator = Address::generate(&env);
        let stranger = Address::generate(&env);
        let target = Address::generate(&env);
        let mut steps = Vec::new(&env);
        steps.push_back(make_step(&env, 0, &target, "x", true));
        let wf_id = client.create_workflow(
            &initiator,
            &String::from_str(&env, "c_wf"),
            &steps,
            &None,
            &None,
            &None,
        );
        client.cancel_workflow(&wf_id, &stranger);
    }
}
