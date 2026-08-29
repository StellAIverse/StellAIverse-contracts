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
