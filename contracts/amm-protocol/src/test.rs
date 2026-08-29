#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_amm_protocol_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    let contract_id = env.register(AmmProtocolContract, ());
    let client = AmmProtocolContractClient::new(&env, &contract_id);

    client.initialize(&admin, &token_a, &token_b, &30); // 0.3% fee

    let lp_minted = client.deposit_liquidity(&user, &10000, &10000);
    assert_eq!(lp_minted, 20000);
    assert_eq!(client.get_lp_balance(&user), 20000);

    let (res_a, res_b) = client.get_reserves();
    assert_eq!(res_a, 10000);
    assert_eq!(res_b, 10000);

    // Swap 1000 token_a for token_b
    let amount_out = client.swap_exact_tokens_for_tokens(&user, &1000, &800, &token_a);
    assert!(amount_out > 0);

    let (res_a_after, res_b_after) = client.get_reserves();
    assert_eq!(res_a_after, 11000);
    assert_eq!(res_b_after, 10000 - amount_out);

    // Withdraw half liquidity
    let (out_a, out_b) = client.withdraw_liquidity(&user, &10000);
    assert!(out_a > 0 && out_b > 0);
}
