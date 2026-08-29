#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum Role {
    Admin = 0,
    Minter = 1,
    Burner = 2,
    Pauser = 3,
    Governance = 4,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Initialized,
    Member(Role, Address),
    RoleAdmin(Role),
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AccessControlError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    InvalidRole = 3,
}

#[contract]
pub struct AccessControl;

#[contractimpl]
impl AccessControl {
    pub fn initialize(env: Env, admin: Address) -> Result<(), AccessControlError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(AccessControlError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Initialized, &true);
        for role in Self::all_roles() {
            env.storage()
                .instance()
                .set(&DataKey::RoleAdmin(role), &Role::Admin);
        }
        Self::set_member(&env, &Role::Admin, &admin, true);
        Self::publish_role_event(
            &env,
            Symbol::new(&env, "RoleGranted"),
            &Role::Admin,
            &admin,
            &admin,
        );
        Ok(())
    }

    pub fn has_role(env: Env, role: Role, account: Address) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::Member(role, account))
    }

    pub fn get_role_admin(env: Env, role: Role) -> Result<Role, AccessControlError> {
        env.storage()
            .instance()
            .get(&DataKey::RoleAdmin(role))
            .ok_or(AccessControlError::InvalidRole)
    }

    pub fn grant_role(
        env: Env,
        role: Role,
        account: Address,
        sender: Address,
    ) -> Result<(), AccessControlError> {
        sender.require_auth();
        Self::require_role_admin(&env, &role, &sender)?;
        if !Self::has_role(env.clone(), role, account.clone()) {
            Self::set_member(&env, &role, &account, true);
            Self::publish_role_event(
                &env,
                Symbol::new(&env, "RoleGranted"),
                &role,
                &account,
                &sender,
            );
        }
        Ok(())
    }

    pub fn revoke_role(
        env: Env,
        role: Role,
        account: Address,
        sender: Address,
    ) -> Result<(), AccessControlError> {
        sender.require_auth();
        Self::require_role_admin(&env, &role, &sender)?;
        if Self::has_role(env.clone(), role, account.clone()) {
            Self::set_member(&env, &role, &account, false);
            Self::publish_role_event(
                &env,
                Symbol::new(&env, "RoleRevoked"),
                &role,
                &account,
                &sender,
            );
        }
        Ok(())
    }

    pub fn set_role_admin(
        env: Env,
        role: Role,
        admin_role: Role,
        sender: Address,
    ) -> Result<(), AccessControlError> {
        sender.require_auth();
        Self::require_role_admin(&env, &role, &sender)?;
        env.storage()
            .instance()
            .set(&DataKey::RoleAdmin(role), &admin_role);
        env.events().publish(
            (Symbol::new(&env, "RoleAdminChanged"),),
            (role, admin_role, sender),
        );
        Ok(())
    }

    fn all_roles() -> [Role; 5] {
        [
            Role::Admin,
            Role::Minter,
            Role::Burner,
            Role::Pauser,
            Role::Governance,
        ]
    }

    fn require_role_admin(
        env: &Env,
        role: &Role,
        sender: &Address,
    ) -> Result<(), AccessControlError> {
        let admin_role: Role = env
            .storage()
            .instance()
            .get(&DataKey::RoleAdmin(*role))
            .ok_or(AccessControlError::InvalidRole)?;
        if Self::has_role(env.clone(), admin_role, sender.clone()) {
            Ok(())
        } else {
            Err(AccessControlError::Unauthorized)
        }
    }

    fn set_member(env: &Env, role: &Role, account: &Address, value: bool) {
        let key = DataKey::Member(*role, account.clone());
        if value {
            env.storage().instance().set(&key, &true);
        } else {
            env.storage().instance().remove(&key);
        }
    }

    fn publish_role_event(
        env: &Env,
        event: Symbol,
        role: &Role,
        account: &Address,
        sender: &Address,
    ) {
        env.events()
            .publish((event,), (*role, account.clone(), sender.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn initializes_admin_and_role_hierarchy() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.mock_all_auths();
        AccessControl::initialize(env.clone(), admin.clone()).unwrap();

        assert!(AccessControl::has_role(
            env.clone(),
            Role::Admin,
            admin.clone()
        ));
        assert_eq!(
            AccessControl::get_role_admin(env, Role::Minter).unwrap(),
            Role::Admin
        );
    }

    #[test]
    fn grants_and_revokes_roles() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let minter = Address::generate(&env);
        env.mock_all_auths();
        AccessControl::initialize(env.clone(), admin.clone()).unwrap();

        AccessControl::grant_role(env.clone(), Role::Minter, minter.clone(), admin.clone())
            .unwrap();
        assert!(AccessControl::has_role(
            env.clone(),
            Role::Minter,
            minter.clone()
        ));
        AccessControl::revoke_role(env.clone(), Role::Minter, minter.clone(), admin).unwrap();
        assert!(!AccessControl::has_role(env, Role::Minter, minter));
    }

    #[test]
    fn denies_unauthorized_role_changes() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let unauthorized = Address::generate(&env);
        let minter = Address::generate(&env);
        env.mock_all_auths();
        AccessControl::initialize(env.clone(), admin).unwrap();

        assert_eq!(
            AccessControl::grant_role(env, Role::Minter, minter, unauthorized).unwrap_err(),
            AccessControlError::Unauthorized
        );
    }
}
