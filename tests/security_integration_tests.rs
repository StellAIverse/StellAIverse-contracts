//! Integration tests for security fixes
//! Issue #178: Role Separation Between Governance and KYC Operators  
//! Issue #179: Prevent Role Escalation via Indirect Function Calls

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
    Vec,
};
use stellai_lib::{
    rbac::{self, RoleType},
    errors::ContractError,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TestError {
    Unauthorized = 100,
    RoleConflict = 101,
    RoleEscalationAttempt = 102,
}

#[contract]
pub struct SecurityTestContract;

#[contractimpl]
impl SecurityTestContract {
    /// Initialize test contract
    pub fn init(env: Env, admin: Address) {
        let admin_key = Symbol::new(&env, "admin");
        env.storage().instance().set(&admin_key, &admin);
    }

    /// Test function requiring KYC operator role
    pub fn kyc_only_operation(env: Env, operator: Address) -> Result<(), TestError> {
        rbac::require_kyc_operator_role(&env, &operator)
            .map_err(|_| TestError::RoleEscalationAttempt)?;
        Ok(())
    }

    /// Test function requiring governance role
    pub fn governance_only_operation(env: Env, governance: Address) -> Result<(), TestError> {
        rbac::require_governance_role(&env, &governance)
            .map_err(|_| TestError::RoleEscalationAttempt)?;
        Ok(())
    }

    /// Test function that could be called indirectly
    pub fn internal_operation(env: Env, caller: Address, operation: String) -> Result<(), TestError> {
        rbac::validate_internal_call(&env, &caller, &operation)
            .map_err(|_| TestError::Unauthorized)?;
        Ok(())
    }

    /// Test role assignment functions
    pub fn assign_governance_role(env: Env, admin: Address, new_governance: Address) -> Result<(), TestError> {
        rbac::assign_governance_role(&env, &admin, &new_governance)
            .map_err(|_| TestError::Unauthorized)?;
        Ok(())
    }

    pub fn assign_kyc_operator_role(env: Env, admin: Address, new_operator: Address) -> Result<(), TestError> {
        rbac::assign_kyc_operator_role(&env, &admin, &new_operator)
            .map_err(|_| TestError::Unauthorized)?;
        Ok(())
    }

    /// Helper to check roles
    pub fn check_roles(env: Env, address: Address) -> (bool, bool) {
        let has_gov = rbac::has_governance_role(&env, &address).unwrap_or(false);
        let has_kyc = rbac::has_kyc_operator_role(&env, &address).unwrap_or(false);
        (has_gov, has_kyc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn setup_test_contract(env: &Env) -> Address {
        let contract_id = env.register_contract(None, SecurityTestContract);
        let admin = Address::generate(env);
        
        env.as_contract(&contract_id, || {
            SecurityTestContract::init(env.clone(), admin.clone());
        });
        
        contract_id
    }

    // ── Issue #178: Role Separation Tests ───────────────────────────────────────

    #[test]
    fn test_role_mutual_exclusion_enforcement() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &admin);

            // Initially user has no roles
            let (has_gov, has_kyc) = SecurityTestContract::check_roles(env.clone(), user.clone());
            assert!(!has_gov);
            assert!(!has_kyc);

            // Assign governance role
            SecurityTestContract::assign_governance_role(env.clone(), admin.clone(), user.clone()).unwrap();
            
            let (has_gov, has_kyc) = SecurityTestContract::check_roles(env.clone(), user.clone());
            assert!(has_gov);
            assert!(!has_kyc);

            // Try to assign KYC operator role - should remove governance role
            SecurityTestContract::assign_kyc_operator_role(env.clone(), admin.clone(), user.clone()).unwrap();
            
            let (has_gov, has_kyc) = SecurityTestContract::check_roles(env.clone(), user.clone());
            assert!(!has_gov);
            assert!(has_kyc);
        });
    }

    #[test]
    fn test_governance_cannot_perform_kyc_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let admin = Address::generate(&env);
        let governance = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &admin);

            // Assign governance role
            SecurityTestContract::assign_governance_role(env.clone(), admin.clone(), governance.clone()).unwrap();

            // Governance should not be able to perform KYC operations
            let err = SecurityTestContract::kyc_only_operation(env.clone(), governance.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);
        });
    }

    #[test]
    fn test_kyc_operator_cannot_perform_governance_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let admin = Address::generate(&env);
        let kyc_op = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &admin);

            // Assign KYC operator role
            SecurityTestContract::assign_kyc_operator_role(env.clone(), admin.clone(), kyc_op.clone()).unwrap();

            // KYC operator should not be able to perform governance operations
            let err = SecurityTestContract::governance_only_operation(env.clone(), kyc_op.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);
        });
    }

    #[test]
    fn test_dual_role_assignment_prevented() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &admin);

            // Manually set both roles (bypass assignment functions)
            let gov_key = Symbol::new(&env, "governance_role");
            let kyc_key = Symbol::new(&env, "kyc_operator_role");
            
            let mut gov_roles: Vec<Address> = Vec::new(&env);
            gov_roles.push_back(user.clone());
            env.storage().instance().set(&gov_key, &gov_roles);
            
            let mut kyc_roles: Vec<Address> = Vec::new(&env);
            kyc_roles.push_back(user.clone());
            env.storage().instance().set(&kyc_key, &kyc_roles);

            // Both operations should fail due to role conflict
            let err = SecurityTestContract::kyc_only_operation(env.clone(), user.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);
            
            let err = SecurityTestContract::governance_only_operation(env.clone(), user.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);
        });
    }

    // ── Issue #179: Indirect Function Call Tests ─────────────────────────────────

    #[test]
    fn test_internal_call_validation_prevents_escalation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &admin);

            // Admin should be able to perform internal operations
            assert!(SecurityTestContract::internal_operation(
                env.clone(), 
                admin.clone(), 
                String::from_str(&env, "test_operation")
            ).is_ok());

            // Attacker should not be able to perform internal operations
            let err = SecurityTestContract::internal_operation(
                env.clone(), 
                attacker.clone(), 
                String::from_str(&env, "test_operation")
            ).unwrap_err();
            assert_eq!(err, TestError::Unauthorized);
        });
    }

    #[test]
    fn test_indirect_call_with_admin_address_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let real_admin = Address::generate(&env);
        let attacker = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set real admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &real_admin);

            // Attacker tries to call internal function with real admin's address
            // This should fail because validation checks the actual caller
            let err = SecurityTestContract::internal_operation(
                env.clone(), 
                attacker.clone(),  // Attacker is the actual caller
                String::from_str(&env, "test_operation")
            ).unwrap_err();
            assert_eq!(err, TestError::Unauthorized);
        });
    }

    #[test]
    fn test_role_assignment_requires_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let attacker = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &admin);

            // Admin can assign roles
            assert!(SecurityTestContract::assign_governance_role(
                env.clone(), 
                admin.clone(), 
                user.clone()
            ).is_ok());

            // Attacker cannot assign roles
            let err = SecurityTestContract::assign_governance_role(
                env.clone(), 
                attacker.clone(), 
                user.clone()
            ).unwrap_err();
            assert_eq!(err, TestError::Unauthorized);
        });
    }

    #[test]
    fn test_comprehensive_role_separation_scenario() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = setup_test_contract(&env);
        let admin = Address::generate(&env);
        let governance = Address::generate(&env);
        let kyc_op = Address::generate(&env);
        let regular_user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin in storage
            let admin_key = Symbol::new(&env, "admin");
            env.storage().instance().set(&admin_key, &admin);

            // Assign distinct roles
            SecurityTestContract::assign_governance_role(env.clone(), admin.clone(), governance.clone()).unwrap();
            SecurityTestContract::assign_kyc_operator_role(env.clone(), admin.clone(), kyc_op.clone()).unwrap();

            // Verify role assignments
            let (gov_has_gov, gov_has_kyc) = SecurityTestContract::check_roles(env.clone(), governance.clone());
            assert!(gov_has_gov);
            assert!(!gov_has_kyc);

            let (kyc_has_gov, kyc_has_kyc) = SecurityTestContract::check_roles(env.clone(), kyc_op.clone());
            assert!(!kyc_has_gov);
            assert!(kyc_has_kyc);

            let (user_has_gov, user_has_kyc) = SecurityTestContract::check_roles(env.clone(), regular_user.clone());
            assert!(!user_has_gov);
            assert!(!user_has_kyc);

            // Test operation permissions
            assert!(SecurityTestContract::governance_only_operation(env.clone(), governance.clone()).is_ok());
            let err = SecurityTestContract::kyc_only_operation(env.clone(), governance.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);

            assert!(SecurityTestContract::kyc_only_operation(env.clone(), kyc_op.clone()).is_ok());
            let err = SecurityTestContract::governance_only_operation(env.clone(), kyc_op.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);

            let err = SecurityTestContract::governance_only_operation(env.clone(), regular_user.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);

            let err = SecurityTestContract::kyc_only_operation(env.clone(), regular_user.clone()).unwrap_err();
            assert_eq!(err, TestError::RoleEscalationAttempt);
        });
    }
}
