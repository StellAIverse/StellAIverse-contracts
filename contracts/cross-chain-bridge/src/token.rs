use soroban_sdk::{contractclient, Address, Env};

/// Interface for Soroban token contracts (Compatible with standard Soroban token)
#[contractclient(name = "TokenClient")]
pub trait Token {
    /// Transfer tokens from one address to another
    fn transfer(env: Env, from: Address, to: Address, amount: i128);

    /// Mint new tokens to an address
    fn mint(env: Env, to: Address, amount: i128);

    /// Burn tokens from an address
    fn burn(env: Env, from: Address, amount: i128);

    /// Get the balance of an address
    fn balance(env: Env, addr: Address) -> i128;

    /// Get the total supply of tokens
    fn total_supply(env: Env) -> i128;
}

/// Allow all approvals for the bridge contract to manipulate tokens
/// This is needed to allow the bridge to transfer/burn/mint tokens
pub fn allow_all(_env: &Env, _token_address: &Address, _owner: &Address) {
    // In a real implementation, this would set the necessary approvals
    // For Soroban, contracts need to be authorized to transfer tokens
}
