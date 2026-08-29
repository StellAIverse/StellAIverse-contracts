#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_lending_protocol_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let _liquidator = Address::generate(&env);
    let token = Address::generate(&env);

    let contract_id = env.register(LendingProtocolContract, ());
    let client = LendingProtocolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &1000); // 10% reserve factor

    client.deposit_collateral(&user, &token, &10000);
    let data = client.get_user_account_data(&user);
    assert_eq!(data.total_collateral_usd, 10000);
    assert_eq!(data.total_debt_usd, 0);

    // Borrow 5000 USD (within 75% LTV limit of 7500)
    client.borrow(&user, &token, &5000);
    let data_after_borrow = client.get_user_account_data(&user);
    assert_eq!(data_after_borrow.total_debt_usd, 5000);
    assert!(data_after_borrow.health_factor >= 10000);

    // Repay partial debt
    client.repay(&user, &token, &2000);
    let data_after_repay = client.get_user_account_data(&user);
    assert_eq!(data_after_repay.total_debt_usd, 3000);

    // Emergency pause test
    client.set_emergency_pause(&true);
    client.repay(&user, &token, &1000); // Repay works while paused
    client.set_emergency_pause(&false);
}
