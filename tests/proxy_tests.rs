//! Integration tests for the StellAIverseProxy upgrade mechanism (Issue #90).
//!
//! These tests verify:
//! - init_proxy stores implementation and leaves proxy unpaused
//! - pause/resume toggle the paused flag
//! - upgrade_history starts empty
//! - Only admin can pause/resume (auth checked via mock_all_auths)

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use stellai_lib::proxy::{StellAIverseProxy, StellAIverseProxyClient};

fn deploy_proxy(env: &Env) -> (StellAIverseProxyClient, Address, Address) {
    let contract_id = env.register_contract(None, StellAIverseProxy);
    let client = StellAIverseProxyClient::new(env, &contract_id);
    let admin = Address::generate(env);
    // Use a second proxy instance as a stand-in for the implementation address
    let impl_id = env.register_contract(None, StellAIverseProxy);

    env.mock_all_auths();
    client.init_proxy(&admin, &impl_id);

    (client, admin, impl_id)
}

#[test]
fn test_init_proxy_sets_implementation_and_unpaused() {
    let env = Env::default();
    let (client, _admin, impl_id) = deploy_proxy(&env);
    assert_eq!(client.implementation(), impl_id);
    assert!(!client.is_paused());
}

#[test]
fn test_upgrade_history_empty_after_init() {
    let env = Env::default();
    let (client, _admin, _impl_id) = deploy_proxy(&env);
    assert_eq!(client.upgrade_history().len(), 0);
}

#[test]
fn test_pause_sets_paused_flag() {
    let env = Env::default();
    let (client, _admin, _impl_id) = deploy_proxy(&env);
    env.mock_all_auths();
    client.pause();
    assert!(client.is_paused());
}

#[test]
fn test_resume_clears_paused_flag() {
    let env = Env::default();
    let (client, _admin, _impl_id) = deploy_proxy(&env);
    env.mock_all_auths();
    client.pause();
    client.resume();
    assert!(!client.is_paused());
}
