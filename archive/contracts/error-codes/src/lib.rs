#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, symbol_short, Env};

/// Deterministic, named error codes for the contract.
/// Each variant maps to a unique integer so callers get stable, predictable codes.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    Unauthorized = 1,
    AlreadyInitialized = 2,
    InvalidInput = 3,
    NotFound = 4,
}

#[contract]
pub struct ErrorCodesContract;

#[contractimpl]
impl ErrorCodesContract {
    /// Return a stored u64 value or a deterministic error code if missing.
    pub fn get_value(env: Env) -> Result<u64, ContractError> {
        env.storage()
            .instance()
            .get::<_, u64>(&symbol_short!("value"))
            .ok_or(ContractError::NotFound)
    }

    /// Store a value; returns InvalidInput if zero is supplied.
    pub fn set_value(env: Env, value: u64) -> Result<(), ContractError> {
        if value == 0 {
            return Err(ContractError::InvalidInput);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("value"), &value);
        Ok(())
    }
}
