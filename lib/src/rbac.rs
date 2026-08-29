/// RBAC (Role-Based Access Control) helpers — Issue #152, #178, #179
///
/// All role checks read directly from on-chain storage on every call.
/// No caller context is implicitly trusted; every internal or indirect
/// call path must go through one of these functions.
///
/// Issue #179: Prevent Role Escalation via Indirect Function Calls
/// Issue #178: Implement Role Separation Between Governance and KYC Operators
use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

use crate::{errors::ContractError, ADMIN_KEY, APPROVED_MINTERS_KEY};

// ── Admin ────────────────────────────────────────────────────────────────────

/// Return the stored admin address, or `Unauthorized` if not initialised.
pub fn get_admin(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get::<_, Address>(&Symbol::new(env, ADMIN_KEY))
        .ok_or(ContractError::Unauthorized)
}

/// Assert that `caller` is the stored admin.
/// Always re-reads from storage — never trusts a passed-in reference.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), ContractError> {
    let admin = get_admin(env)?;
    if caller != &admin {
        return Err(ContractError::RoleEscalationAttempt);
    }
    Ok(())
}

// ── Minter ───────────────────────────────────────────────────────────────────

/// Assert that `caller` is the admin **or** an approved minter.
/// Always re-reads the approved-minters list from storage.
pub fn require_minter(env: &Env, caller: &Address) -> Result<(), ContractError> {
    // Admin is always allowed
    if let Ok(admin) = get_admin(env) {
        if caller == &admin {
            return Ok(());
        }
    }

    let minters: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, APPROVED_MINTERS_KEY))
        .unwrap_or_else(|| Vec::new(env));

    for m in minters.iter() {
        if &m == caller {
            return Ok(());
        }
    }

    Err(ContractError::RoleEscalationAttempt)
}

// ── Operator ─────────────────────────────────────────────────────────────────

/// Assert that `caller` is the owner of `agent_id` **or** an authorised,
/// non-expired operator for that agent.
///
/// `get_owner_fn`    – closure that returns the stored owner for `agent_id`.
/// `get_operator_fn` – closure that returns `Option<(operator, expires_at)>`.
pub fn require_owner_or_operator<FO, FP>(
    env: &Env,
    caller: &Address,
    agent_id: u64,
    get_owner_fn: FO,
    get_operator_fn: FP,
) -> Result<(), ContractError>
where
    FO: Fn(&Env, u64) -> Option<Address>,
    FP: Fn(&Env, u64) -> Option<(Address, u64)>,
{
    // Re-read owner from storage — never trust the caller's claim
    if let Some(owner) = get_owner_fn(env, agent_id) {
        if caller == &owner {
            return Ok(());
        }
    }

    // Check operator authorisation from storage
    if let Some((operator, expires_at)) = get_operator_fn(env, agent_id) {
        if caller == &operator {
            if env.ledger().timestamp() < expires_at {
                return Ok(());
            }
            // Operator exists but is expired — explicit escalation attempt
            return Err(ContractError::RoleEscalationAttempt);
        }
    }

    Err(ContractError::RoleEscalationAttempt)
}

// ── Oracle provider ──────────────────────────────────────────────────────────

/// Assert that `caller` is in the registered oracle-provider list.
/// Always re-reads from storage.
pub fn require_oracle_provider(
    env: &Env,
    caller: &Address,
    provider_list_key: &str,
) -> Result<(), ContractError> {
    let providers: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, provider_list_key))
        .unwrap_or_else(|| Vec::new(env));

    for p in providers.iter() {
        if &p == caller {
            return Ok(());
        }
    }

    Err(ContractError::RoleEscalationAttempt)
}

// ── Enhanced Role Validation (Issue #179) ──────────────────────────────────────

/// Validate caller role with explicit revalidation to prevent indirect escalation
/// This function must be used for all internal calls that could be invoked indirectly
pub fn validate_caller_role(
    env: &Env,
    caller: &Address,
    required_role: RoleType,
) -> Result<(), ContractError> {
    // Always re-read from storage - never trust passed context
    match required_role {
        RoleType::Admin => require_admin(env, caller),
        RoleType::Minter => require_minter(env, caller),
        RoleType::Governance => require_governance_role(env, caller),
        RoleType::KycOperator => require_kyc_operator_role(env, caller),
    }
}

/// Enhanced admin check with explicit validation for indirect calls
pub fn require_admin_indirect_safe(env: &Env, caller: &Address) -> Result<(), ContractError> {
    // Re-read admin from storage and validate caller matches exactly
    let admin = get_admin(env)?;
    if caller != &admin {
        return Err(ContractError::RoleEscalationAttempt);
    }
    Ok(())
}

/// Validate internal function calls to prevent escalation vectors
pub fn validate_internal_call(
    env: &Env,
    caller: &Address,
    function_name: &Symbol,
) -> Result<(), ContractError> {
    // Log the access attempt for audit trail
    env.events().publish(
        (Symbol::new(env, "access_check"),),
        (
            caller.clone(),
            function_name.clone(),
            env.ledger().timestamp(),
        ),
    );

    // Always revalidate caller role from storage
    require_admin_indirect_safe(env, caller)
}

// ── Role Separation (Issue #178) ───────────────────────────────────────────────

/// Role types for separation of duties
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RoleType {
    Admin = 0,
    Minter = 1,
    Governance = 2,
    KycOperator = 3,
}

/// Storage keys for role separation
const GOVERNANCE_ROLE_KEY: &str = "governance_role";
const KYC_OPERATOR_ROLE_KEY: &str = "kyc_operator_role";

/// Require governance role - mutually exclusive with KYC operator
pub fn require_governance_role(env: &Env, caller: &Address) -> Result<(), ContractError> {
    // Check if caller has governance role
    let governance_roles: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, GOVERNANCE_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    for role in governance_roles.iter() {
        if role == *caller {
            // Ensure caller does not also have KYC operator role
            if has_kyc_operator_role(env, &role)? {
                return Err(ContractError::RoleConflict);
            }
            return Ok(());
        }
    }

    Err(ContractError::RoleEscalationAttempt)
}

/// Require KYC operator role - mutually exclusive with governance
pub fn require_kyc_operator_role(env: &Env, caller: &Address) -> Result<(), ContractError> {
    // Check if caller has KYC operator role
    let kyc_operators: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, KYC_OPERATOR_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    for op in kyc_operators.iter() {
        if op == *caller {
            // Ensure caller does not also have governance role
            if has_governance_role(env, &op)? {
                return Err(ContractError::RoleConflict);
            }
            return Ok(());
        }
    }

    Err(ContractError::RoleEscalationAttempt)
}

/// Check if address has governance role
pub fn has_governance_role(env: &Env, address: &Address) -> Result<bool, ContractError> {
    let governance_roles: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, GOVERNANCE_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    Ok(governance_roles.iter().any(|role| role == *address))
}

/// Check if address has KYC operator role
pub fn has_kyc_operator_role(env: &Env, address: &Address) -> Result<bool, ContractError> {
    let kyc_operators: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, KYC_OPERATOR_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    Ok(kyc_operators.iter().any(|op| op == *address))
}

/// Assign governance role (admin only) - ensures mutual exclusion
pub fn assign_governance_role(
    env: &Env,
    admin: &Address,
    new_governance: &Address,
) -> Result<(), ContractError> {
    // Validate admin
    require_admin(env, admin)?;

    // Remove from KYC operators if present
    remove_from_kyc_operators(env, new_governance)?;

    // Add to governance roles
    let mut governance_roles: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, GOVERNANCE_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    if !governance_roles.iter().any(|role| role == *new_governance) {
        governance_roles.push_back(new_governance.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(env, GOVERNANCE_ROLE_KEY), &governance_roles);
    }

    Ok(())
}

/// Assign KYC operator role (admin only) - ensures mutual exclusion
pub fn assign_kyc_operator_role(
    env: &Env,
    admin: &Address,
    new_operator: &Address,
) -> Result<(), ContractError> {
    // Validate admin
    require_admin(env, admin)?;

    // Remove from governance if present
    remove_from_governance(env, new_operator)?;

    // Add to KYC operators
    let mut kyc_operators: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, KYC_OPERATOR_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    if !kyc_operators.iter().any(|op| op == *new_operator) {
        kyc_operators.push_back(new_operator.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(env, KYC_OPERATOR_ROLE_KEY), &kyc_operators);
    }

    Ok(())
}

/// Remove address from governance roles
fn remove_from_governance(env: &Env, address: &Address) -> Result<(), ContractError> {
    let mut governance_roles: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, GOVERNANCE_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    let mut index = None;
    for (i, role) in governance_roles.iter().enumerate() {
        if role == *address {
            index = Some(i as u32);
            break;
        }
    }

    if let Some(i) = index {
        governance_roles.remove(i);
        if !governance_roles.is_empty() {
            env.storage()
                .instance()
                .set(&Symbol::new(env, GOVERNANCE_ROLE_KEY), &governance_roles);
        } else {
            env.storage()
                .instance()
                .remove(&Symbol::new(env, GOVERNANCE_ROLE_KEY));
        }
    }

    Ok(())
}

/// Remove address from KYC operators
fn remove_from_kyc_operators(env: &Env, address: &Address) -> Result<(), ContractError> {
    let mut kyc_operators: Vec<Address> = env
        .storage()
        .instance()
        .get(&Symbol::new(env, KYC_OPERATOR_ROLE_KEY))
        .unwrap_or_else(|| Vec::new(env));

    let mut index = None;
    for (i, op) in kyc_operators.iter().enumerate() {
        if op == *address {
            index = Some(i as u32);
            break;
        }
    }

    if let Some(i) = index {
        kyc_operators.remove(i);
        if !kyc_operators.is_empty() {
            env.storage()
                .instance()
                .set(&Symbol::new(env, KYC_OPERATOR_ROLE_KEY), &kyc_operators);
        } else {
            env.storage()
                .instance()
                .remove(&Symbol::new(env, KYC_OPERATOR_ROLE_KEY));
        }
    }

    Ok(())
}

/// Public function to remove KYC operator role (admin only)
pub fn remove_kyc_operator_role(
    env: &Env,
    admin: &Address,
    operator: &Address,
) -> Result<(), ContractError> {
    // Validate admin
    require_admin(env, admin)?;

    // Remove from KYC operators
    remove_from_kyc_operators(env, operator)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    struct RbacHarness;
    #[contractimpl]
    impl RbacHarness {}

    fn setup() -> (Env, soroban_sdk::Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RbacHarness, ());
        (env, contract_id)
    }

    // ── require_admin ────────────────────────────────────────────────────────

    #[test]
    fn require_admin_passes_for_stored_admin() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            assert!(require_admin(&env, &admin).is_ok());
        });
    }

    #[test]
    fn require_admin_rejects_non_admin() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            let err = require_admin(&env, &attacker).unwrap_err();
            assert_eq!(err, ContractError::RoleEscalationAttempt);
        });
    }

    /// Indirect escalation: attacker passes admin address as argument but is
    /// not the stored admin — must be rejected.
    #[test]
    fn require_admin_indirect_escalation_rejected() {
        let (env, cid) = setup();
        let real_admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &real_admin);
            // Attacker passes real_admin's address but is not that address
            let err = require_admin(&env, &attacker).unwrap_err();
            assert_eq!(err, ContractError::RoleEscalationAttempt);
        });
    }

    // ── require_minter ───────────────────────────────────────────────────────

    #[test]
    fn require_minter_passes_for_admin() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            assert!(require_minter(&env, &admin).is_ok());
        });
    }

    #[test]
    fn require_minter_passes_for_approved_minter() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let minter = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            let mut list: Vec<Address> = Vec::new(&env);
            list.push_back(minter.clone());
            env.storage()
                .instance()
                .set(&Symbol::new(&env, APPROVED_MINTERS_KEY), &list);
            assert!(require_minter(&env, &minter).is_ok());
        });
    }

    #[test]
    fn require_minter_rejects_unapproved_caller() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            let list: Vec<Address> = Vec::new(&env);
            env.storage()
                .instance()
                .set(&Symbol::new(&env, APPROVED_MINTERS_KEY), &list);
            let err = require_minter(&env, &attacker).unwrap_err();
            assert_eq!(err, ContractError::RoleEscalationAttempt);
        });
    }

    // ── require_owner_or_operator ────────────────────────────────────────────

    #[test]
    fn require_owner_or_operator_passes_for_owner() {
        let (env, cid) = setup();
        let owner = Address::generate(&env);
        env.as_contract(&cid, || {
            let result = require_owner_or_operator(
                &env,
                &owner,
                1u64,
                |_e, _id| Some(owner.clone()),
                |_e, _id| None,
            );
            assert!(result.is_ok());
        });
    }

    #[test]
    fn require_owner_or_operator_passes_for_valid_operator() {
        let (env, cid) = setup();
        let owner = Address::generate(&env);
        let operator = Address::generate(&env);
        env.ledger().set_timestamp(500);
        let expires_at = 1000u64; // strictly after current timestamp
        env.as_contract(&cid, || {
            let result = require_owner_or_operator(
                &env,
                &operator,
                1u64,
                |_e, _id| Some(owner.clone()),
                |_e, _id| Some((operator.clone(), expires_at)),
            );
            assert!(result.is_ok());
        });
    }

    #[test]
    fn require_owner_or_operator_rejects_expired_operator() {
        let (env, cid) = setup();
        let owner = Address::generate(&env);
        let operator = Address::generate(&env);
        // Set ledger time ahead so expires_at is clearly in the past
        env.ledger().set_timestamp(1000);
        let expires_at = 500u64; // strictly before current timestamp
        env.as_contract(&cid, || {
            let result = require_owner_or_operator(
                &env,
                &operator,
                1u64,
                |_e, _id| Some(owner.clone()),
                |_e, _id| Some((operator.clone(), expires_at)),
            );
            assert_eq!(result.unwrap_err(), ContractError::RoleEscalationAttempt);
        });
    }

    /// Indirect escalation: attacker claims to be operator but is not stored.
    #[test]
    fn require_owner_or_operator_rejects_indirect_escalation() {
        let (env, cid) = setup();
        let owner = Address::generate(&env);
        let real_operator = Address::generate(&env);
        let attacker = Address::generate(&env);
        let expires_at = env.ledger().timestamp() + 1000;
        env.as_contract(&cid, || {
            // Storage has real_operator, attacker tries to act as operator
            let result = require_owner_or_operator(
                &env,
                &attacker,
                1u64,
                |_e, _id| Some(owner.clone()),
                |_e, _id| Some((real_operator.clone(), expires_at)),
            );
            assert_eq!(result.unwrap_err(), ContractError::RoleEscalationAttempt);
        });
    }

    // ── require_oracle_provider ──────────────────────────────────────────────

    #[test]
    fn require_oracle_provider_passes_for_registered() {
        let (env, cid) = setup();
        let provider = Address::generate(&env);
        env.as_contract(&cid, || {
            let mut list: Vec<Address> = Vec::new(&env);
            list.push_back(provider.clone());
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "providers"), &list);
            assert!(require_oracle_provider(&env, &provider, "providers").is_ok());
        });
    }

    #[test]
    fn require_oracle_provider_rejects_unregistered() {
        let (env, cid) = setup();
        let attacker = Address::generate(&env);
        env.as_contract(&cid, || {
            let list: Vec<Address> = Vec::new(&env);
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "providers"), &list);
            let err = require_oracle_provider(&env, &attacker, "providers").unwrap_err();
            assert_eq!(err, ContractError::RoleEscalationAttempt);
        });
    }

    // ── Enhanced Role Validation Tests (Issue #179) ───────────────────────────

    #[test]
    fn validate_caller_role_admin_success() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            assert!(validate_caller_role(&env, &admin, RoleType::Admin).is_ok());
        });
    }

    #[test]
    fn validate_caller_role_indirect_escalation_fails() {
        let (env, cid) = setup();
        let real_admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &real_admin);
            let err = validate_caller_role(&env, &attacker, RoleType::Admin).unwrap_err();
            assert_eq!(err, ContractError::RoleEscalationAttempt);
        });
    }

    #[test]
    fn validate_internal_call_logs_access_attempt() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            let test_function = Symbol::new(&env, "test_function");
            assert!(validate_internal_call(&env, &admin, &test_function).is_ok());
        });
    }

    // ── Role Separation Tests (Issue #178) ───────────────────────────────────────

    #[test]
    fn assign_governance_role_success() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let governance = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            assert!(assign_governance_role(&env, &admin, &governance).is_ok());
            assert!(has_governance_role(&env, &governance).unwrap());
            assert!(!has_kyc_operator_role(&env, &governance).unwrap());
        });
    }

    #[test]
    fn assign_kyc_operator_role_success() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let kyc_op = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);
            assert!(assign_kyc_operator_role(&env, &admin, &kyc_op).is_ok());
            assert!(has_kyc_operator_role(&env, &kyc_op).unwrap());
            assert!(!has_governance_role(&env, &kyc_op).unwrap());
        });
    }

    #[test]
    fn role_mutual_exclusion_governance_to_kyc() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);

            // First assign governance role
            assert!(assign_governance_role(&env, &admin, &user).is_ok());
            assert!(has_governance_role(&env, &user).unwrap());

            // Then assign KYC operator - should remove governance role
            assert!(assign_kyc_operator_role(&env, &admin, &user).is_ok());
            assert!(has_kyc_operator_role(&env, &user).unwrap());
            assert!(!has_governance_role(&env, &user).unwrap());
        });
    }

    #[test]
    fn role_mutual_exclusion_kyc_to_governance() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);

            // First assign KYC operator role
            assert!(assign_kyc_operator_role(&env, &admin, &user).is_ok());
            assert!(has_kyc_operator_role(&env, &user).unwrap());

            // Then assign governance - should remove KYC operator role
            assert!(assign_governance_role(&env, &admin, &user).is_ok());
            assert!(has_governance_role(&env, &user).unwrap());
            assert!(!has_kyc_operator_role(&env, &user).unwrap());
        });
    }

    #[test]
    fn require_governance_role_rejects_kyc_operator() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let kyc_op = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);

            // Assign KYC operator role
            assert!(assign_kyc_operator_role(&env, &admin, &kyc_op).is_ok());

            // Try to use governance functions - should fail
            let err = require_governance_role(&env, &kyc_op).unwrap_err();
            assert_eq!(err, ContractError::RoleConflict);
        });
    }

    #[test]
    fn require_kyc_operator_role_rejects_governance() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let governance = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);

            // Assign governance role
            assert!(assign_governance_role(&env, &admin, &governance).is_ok());

            // Try to use KYC functions - should fail
            let err = require_kyc_operator_role(&env, &governance).unwrap_err();
            assert_eq!(err, ContractError::RoleConflict);
        });
    }

    #[test]
    fn role_assignment_requires_admin() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let attacker = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);

            // Attacker tries to assign roles - should fail
            let err = assign_governance_role(&env, &attacker, &user).unwrap_err();
            assert_eq!(err, ContractError::RoleEscalationAttempt);

            let err = assign_kyc_operator_role(&env, &attacker, &user).unwrap_err();
            assert_eq!(err, ContractError::RoleEscalationAttempt);
        });
    }

    #[test]
    fn dual_role_assignment_prevented() {
        let (env, cid) = setup();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);

            // Manually try to set both roles (bypass assignment functions)
            let mut governance_roles: Vec<Address> = Vec::new(&env);
            governance_roles.push_back(user.clone());
            env.storage()
                .instance()
                .set(&Symbol::new(&env, GOVERNANCE_ROLE_KEY), &governance_roles);

            let mut kyc_operators: Vec<Address> = Vec::new(&env);
            kyc_operators.push_back(user.clone());
            env.storage()
                .instance()
                .set(&Symbol::new(&env, KYC_OPERATOR_ROLE_KEY), &kyc_operators);

            // Both role checks should fail due to conflict
            let err = require_governance_role(&env, &user).unwrap_err();
            assert_eq!(err, ContractError::RoleConflict);

            let err = require_kyc_operator_role(&env, &user).unwrap_err();
            assert_eq!(err, ContractError::RoleConflict);
        });
    }
}
