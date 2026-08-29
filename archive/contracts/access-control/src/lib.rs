#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

#[contract]
pub struct AccessControlContract;

#[contractimpl]
impl AccessControlContract {
    /// Public entry point: only the admin may call execute.
    /// Delegates to internal_helper which is not exposed externally.
    pub fn execute(env: Env, admin: Address, payload: Symbol) -> Symbol {
        admin.require_auth();
        Self::internal_helper(&env, payload)
    }

    /// Private helper — not part of the public contract interface.
    fn internal_helper(_env: &Env, payload: Symbol) -> Symbol {
        // Internal processing logic lives here, unreachable without execute().
        payload
    }

    /// Store the designated admin address during deployment.
    pub fn set_admin(env: Env, admin: Address) {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&symbol_short!("admin"), &admin);
    }
}
