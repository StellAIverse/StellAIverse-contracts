use soroban_sdk::{Address, Env, Symbol};

use crate::{errors::ContractError, ADMIN_KEY};

use soroban_sdk::{Address, String, Vec};

use crate::{errors::ContractError, MAX_CAPABILITIES, MAX_STRING_LENGTH};

pub fn validate_address(address: &Address) -> Result<(), ContractError> {
    let _ = address;
    Ok(())
}

pub fn validate_metadata(metadata: &String) -> Result<(), ContractError> {
    if metadata.is_empty() {
        return Err(ContractError::InvalidMetadata);
    }
    if metadata.len() > MAX_STRING_LENGTH {
        return Err(ContractError::MetadataTooLong);
    }
    Ok(())
}

pub fn validate_capabilities(capabilities: &Vec<String>) -> Result<(), ContractError> {
    if capabilities.len() > MAX_CAPABILITIES as u32 {
        return Err(ContractError::CapabilitiesExceeded);
    }

    for i in 0..capabilities.len() {
        let capability = capabilities.get(i).ok_or(ContractError::InvalidInput)?;
        if capability.is_empty() {
            return Err(ContractError::InvalidMetadata); // Or a specific InvalidCapability if it existed
        }
        if capability.len() > MAX_STRING_LENGTH {
            return Err(ContractError::MetadataTooLong);
        }
    }

    Ok(())
}

pub fn validate_nonzero_id(id: u64) -> Result<(), ContractError> {
    if id == 0 {
        return Err(ContractError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn metadata_validation_works() {
        let env = Env::default();
        let ok = String::from_str(&env, "ipfs://cid");
        assert!(validate_metadata(&ok).is_ok());

        let empty = String::from_str(&env, "");
        assert!(validate_metadata(&empty).is_err());
    }

    #[test]
    fn capabilities_validation_works() {
        let env = Env::default();
        let caps = Vec::from_array(&env, [String::from_str(&env, "exec")]);
        assert!(validate_capabilities(&caps).is_ok());

        let bad = Vec::from_array(&env, [String::from_str(&env, "")]);
        assert!(validate_capabilities(&bad).is_err());
    }
}


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
