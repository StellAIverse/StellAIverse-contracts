//! Proxy contract pattern for StellAIverse — upgrade mechanism (Issue #90).
//!
//! The `StellAIverseProxy` contract owns all persistent state and forwards
//! calls to an upgradeable implementation contract. Upgrading swaps the
//! implementation pointer, pausing the proxy during migration so no calls
//! are processed while state is being transformed.
//!
//! Storage keys used by the proxy (exported for integration tests):
//!   - `IMPLEMENTATION_KEY` — current implementation address
//!   - `IS_PAUSED_KEY`      — pause flag (bool) set during upgrade
//!   - `UPGRADE_HISTORY_KEY`— Vec<(timestamp, new_impl)> audit trail
//!     The `ADMIN_KEY` is the same shared key used across all contracts.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol, Val, Vec};

use crate::storage_keys::{IMPLEMENTATION_KEY, IS_PAUSED_KEY, UPGRADE_HISTORY_KEY};
use crate::ADMIN_KEY;

#[contract]
pub struct StellAIverseProxy;

#[contractimpl]
impl StellAIverseProxy {
    /// One-time initialisation: store admin and initial implementation address.
    pub fn init_proxy(env: Env, admin: Address, initial_implementation: Address) {
        if env.storage().instance().has(&symbol_short!("prx_init")) {
            panic!("Proxy already initialized");
        }
        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&IMPLEMENTATION_KEY, &initial_implementation);
        env.storage().instance().set(&IS_PAUSED_KEY, &false);
        env.storage()
            .instance()
            .set(&UPGRADE_HISTORY_KEY, &Vec::<(u64, Address)>::new(&env));
        env.storage()
            .instance()
            .set(&symbol_short!("prx_init"), &true);
    }

    /// Admin: upgrade the implementation.
    ///
    /// Steps:
    /// 1. Authenticate admin
    /// 2. Pause proxy (blocks `__dispatch` during migration)
    /// 3. Append entry to upgrade history
    /// 4. Store new implementation address
    /// 5. Invoke `migrate` on the new implementation (state transformation)
    /// 6. Unpause proxy
    pub fn upgrade(env: Env, new_implementation: Address) {
        // 1. Access Control (admin only)
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Proxy not initialized");
        admin.require_auth();

        // 2. Pause
        env.storage().instance().set(&IS_PAUSED_KEY, &true);

        // 3. Append to upgrade history
        let mut history: Vec<(u64, Address)> = env
            .storage()
            .instance()
            .get(&UPGRADE_HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));
        history.push_back((env.ledger().timestamp(), new_implementation.clone()));
        env.storage().instance().set(&UPGRADE_HISTORY_KEY, &history);

        // 4. Update implementation pointer
        env.storage()
            .instance()
            .set(&IMPLEMENTATION_KEY, &new_implementation);

        // 5. Invoke migration on new implementation
        env.invoke_contract::<()>(
            &new_implementation,
            &Symbol::new(&env, "migrate"),
            Vec::new(&env),
        );

        // 6. Unpause
        env.storage().instance().set(&IS_PAUSED_KEY, &false);

        env.events()
            .publish((symbol_short!("upgraded"),), (new_implementation,));
    }

    /// Pause the proxy (admin only). Useful for emergency stops.
    pub fn pause(env: Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Proxy not initialized");
        admin.require_auth();
        env.storage().instance().set(&IS_PAUSED_KEY, &true);
        env.events().publish((symbol_short!("paused"),), ());
    }

    /// Resume the proxy (admin only).
    pub fn resume(env: Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("Proxy not initialized");
        admin.require_auth();
        env.storage().instance().set(&IS_PAUSED_KEY, &false);
        env.events().publish((symbol_short!("resumed"),), ());
    }

    /// Returns whether the proxy is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&IS_PAUSED_KEY)
            .unwrap_or(false)
    }

    /// Returns the current implementation address.
    pub fn implementation(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&IMPLEMENTATION_KEY)
            .expect("Implementation not set")
    }

    /// Returns the full upgrade history as Vec<(timestamp, implementation)>.
    pub fn upgrade_history(env: Env) -> Vec<(u64, Address)> {
        env.storage()
            .instance()
            .get(&UPGRADE_HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Forwards a call to the current implementation.
    ///
    /// Panics if the proxy is paused (migration in progress).
    pub fn dispatch(env: Env, function: Symbol, args: Vec<Val>) -> Val {
        let paused: bool = env
            .storage()
            .instance()
            .get(&IS_PAUSED_KEY)
            .unwrap_or(false);
        if paused {
            panic!("Proxy is paused — migration in progress");
        }

        let impl_addr: Address = env
            .storage()
            .instance()
            .get(&IMPLEMENTATION_KEY)
            .expect("Implementation not set");

        env.invoke_contract(&impl_addr, &function, args)
    }
}

// Tests for the proxy module live in the lib crate's integration test harness
// (tests/proxy_tests.rs) where the soroban testutils feature is available.
