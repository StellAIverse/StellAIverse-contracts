#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

#[test]
fn test_derivatives_swaps_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let party_a = Address::generate(&env);
    let party_b = Address::generate(&env);

    let contract_id = env.register(DerivativesSwapsContract, ());
    let client = DerivativesSwapsContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    client.deposit_margin(&party_a, &5000);
    assert_eq!(client.get_user_margin(&party_a), 5000);

    let swap_id = client.create_interest_rate_swap(
        &party_a,
        &party_b,
        &100000,
        &500, // 5% fixed
        &365,
    );
    assert_eq!(swap_id, 1);

    let agreement = client.get_swap(&1);
    assert_eq!(agreement.notional, 100000);
    assert_eq!(agreement.fixed_rate_bps, 500);
    assert_eq!(agreement.is_active, true);

    // Settle with floating rate at 4% (400 bps) -> fixed payment (5000) - floating (4000) = net +1000
    let net = client.settle_swap(&1, &400);
    assert_eq!(net, 1000);

    // Terminate swap
    let exit_val = client.terminate_swap(&1);
    assert_eq!(exit_val, 95000);
    assert_eq!(client.get_swap(&1).is_active, false);

    // Currency swap test
    let usd = Symbol::new(&env, "USD");
    let eur = Symbol::new(&env, "EUR");
    let curr_swap_id = client.create_currency_swap(
        &party_a,
        &party_b,
        &50000,
        &usd,
        &45000,
        &eur,
        &180,
    );
    assert_eq!(curr_swap_id, 2);
}
