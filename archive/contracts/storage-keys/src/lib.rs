#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

/// Namespaced storage keys prevent accidental collisions between modules.
/// Each variant occupies its own slot in the contract's key space.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Singleton admin address.
    AdminKey,
    /// Per-user balance, keyed by the user's Address.
    UserBalance(Address),
    /// Per-module configuration, keyed by a Symbol module name.
    ModuleConfig(Symbol),
}

#[contract]
pub struct StorageKeysContract;

#[contractimpl]
impl StorageKeysContract {
    pub fn set_admin(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::AdminKey, &admin);
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::AdminKey)
    }

    pub fn set_balance(env: Env, user: Address, amount: u64) {
        user.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::UserBalance(user), &amount);
    }

    pub fn get_balance(env: Env, user: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::UserBalance(user))
            .unwrap_or(0)
    }

    pub fn set_module_config(env: Env, admin: Address, module: Symbol, config: u64) {
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::ModuleConfig(module), &config);
    }

    pub fn get_module_config(env: Env, module: Symbol) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::ModuleConfig(module))
            .unwrap_or(0)
    }
}
