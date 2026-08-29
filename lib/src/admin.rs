use soroban_sdk::{Address, Env, Symbol};

use crate::{errors::ContractError, ADMIN_KEY};

pub fn get_admin(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get(&Symbol::new(env, ADMIN_KEY))
        .ok_or(ContractError::Unauthorized)
}

pub fn verify_admin(env: &Env, caller: &Address) -> Result<(), ContractError> {
    let admin = get_admin(env)?;
    if &admin != caller {
        return Err(ContractError::Unauthorized);
    }
    Ok(())
}

pub fn transfer_admin(
    env: &Env,
    current_admin: &Address,
    new_admin: &Address,
) -> Result<(), ContractError> {
    current_admin.require_auth();
    verify_admin(env, current_admin)?;
    env.storage()
        .instance()
        .set(&Symbol::new(env, ADMIN_KEY), new_admin);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contractimpl};

    #[contract]
    struct AdminHarness;

    #[contractimpl]
    impl AdminHarness {}

    #[test]
    fn verify_admin_success_and_transfer() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let next = Address::generate(&env);
        let contract_id = env.register(AdminHarness, ());

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, ADMIN_KEY), &admin);

            assert!(verify_admin(&env, &admin).is_ok());
            assert!(transfer_admin(&env, &admin, &next).is_ok());
            assert_eq!(get_admin(&env).unwrap(), next);
        });
    }
}
