#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

#[contract]
pub struct SecurityEventsContract;

#[contractimpl]
impl SecurityEventsContract {
    /// Emit a structured security event for audit trails.
    /// Publishes actor, target, action, and timestamp so off-chain indexers
    /// can reconstruct a complete security log.
    pub fn emit_security_event(
        env: Env,
        actor: Address,
        target: Symbol,
        action: Symbol,
    ) {
        actor.require_auth();

        let timestamp: u64 = env.ledger().timestamp();

        env.events().publish(
            (symbol_short!("sec_evt"), action.clone()),
            (actor, target, action, timestamp),
        );
    }
}
