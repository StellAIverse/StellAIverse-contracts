//! Integration tests for the transaction coordinator contract.
#![cfg(test)]

extern crate std;

use crate::{AtomicAgentSaleWorkflow, TransactionCoordinator, TransactionCoordinatorClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    string::String,
    vec, Address, Env, IntoVal, Symbol, Val,
};
use stellai_lib::{
    atomic::AtomicTransactionSupport,
    testutils::{create_test_marketplace, create_test_token},
    TransactionStatus, TransactionStep,
};

#[test]
fn test_atomic_transaction_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::random(&env);
    let initiator = Address::random(&env);

    let marketplace_contract_id = create_test_marketplace(&env);
    let token_contract_id = create_test_token(&env, &admin);

    let coordinator_id = env.register_contract(None, TransactionCoordinator);
    let coordinator_client = TransactionCoordinatorClient::new(&env, &coordinator_id);
    coordinator_client.initialize(&admin);

    // Define transaction steps
    let mut steps = vec![&env];
    steps.push_back(TransactionStep {
        step_id: 1,
        contract: marketplace_contract_id.clone(),
        function: Symbol::new(&env, "prepare_sale"),
        args: vec![&env],
        depends_on: None,
        rollback_contract: Some(marketplace_contract_id.clone()),
        rollback_function: Some(Symbol::new(&env, "cancel_sale_preparation")),
        rollback_args: Some(vec![&env]),
        executed: false,
        result: None,
    });

    let transaction_id = coordinator_client.create_transaction(&initiator, &steps);
    assert_eq!(transaction_id, 1);

    let transaction = coordinator_client.get_transaction(&transaction_id).unwrap();
    assert_eq!(transaction.status, TransactionStatus::Initiated);

    // Execute the transaction
    let success = coordinator_client.execute_transaction(&transaction_id, &initiator);
    assert!(success, "Transaction should complete successfully");
    
    let transaction = coordinator_client.get_transaction(&transaction_id).unwrap();
    assert_eq!(transaction.status, TransactionStatus::Committed);

    // Verify events
    let events = env.events().all();
    assert!(events.iter().any(|e| format!("{:?}", e.1).contains("tx_event") && format!("{:?}", e.1).contains("completed")));
    
    // Get journal entries for audit
    let journal = coordinator_client.get_transaction_journal(&transaction_id);
    assert!(!journal.is_empty(), "Journal should have entries");
    assert!(journal.iter().any(|entry| entry.action == String::from_str(&env, "transaction_created")));
    assert!(journal.iter().any(|entry| entry.action == String::from_str(&env, "transaction_completed")));
}

#[test]
fn test_atomic_transaction_failure_rollback() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::random(&env);
    let initiator = Address::random(&env);

    let marketplace_contract_id = create_test_marketplace(&env);
    let token_contract_id = create_test_token(&env, &admin);

    let coordinator_id = env.register_contract(None, TransactionCoordinator);
    let coordinator_client = TransactionCoordinatorClient::new(&env, &coordinator_id);
    coordinator_client.initialize(&admin);

    // Create a transaction with an invalid step that will fail
    let mut steps = vec![&env];
    steps.push_back(TransactionStep {
        step_id: 1,
        contract: marketplace_contract_id.clone(),
        function: Symbol::new(&env, "invalid_function"), // This will cause prepare to fail
        args: vec![&env],
        depends_on: None,
        rollback_contract: Some(marketplace_contract_id.clone()),
        rollback_function: Some(Symbol::new(&env, "rollback_invalid")),
        rollback_args: Some(vec![&env]),
        executed: false,
        result: None,
    });

    let transaction_id = coordinator_client.create_transaction(&initiator, &steps);
    assert_eq!(transaction_id, 1);

    // Execute transaction - should fail and trigger rollback
    let success = coordinator_client.execute_transaction(&transaction_id, &initiator);
    assert!(!success, "Transaction should fail");
    
    let transaction = coordinator_client.get_transaction(&transaction_id).unwrap();
    assert_eq!(transaction.status, TransactionStatus::RolledBack);
    
    // Verify journal has rollback entries
    let journal = coordinator_client.get_transaction_journal(&transaction_id);
    assert!(journal.iter().any(|entry| entry.action == String::from_str(&env, "rolled_back")));
}

#[test]
fn test_transaction_with_dependencies() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::random(&env);
    let initiator = Address::random(&env);

    let marketplace_contract_id = create_test_marketplace(&env);
    let token_contract_id = create_test_token(&env, &admin);
    let nft_contract_id = create_test_marketplace(&env); // Use as mock NFT contract for testing

    let coordinator_id = env.register_contract(None, TransactionCoordinator);
    let coordinator_client = TransactionCoordinatorClient::new(&env, &coordinator_id);
    coordinator_client.initialize(&admin);

    // Create transaction with proper dependency chain
    let steps = AtomicAgentSaleWorkflow::create_sale_transaction(
        &env,
        initiator.clone(),
        Address::random(&env), // seller
        Address::random(&env), // buyer
        1, // agent_id
        1, // listing_id
        1000, // price
        marketplace_contract_id,
        nft_contract_id,
        token_contract_id,
        None, // no royalties
        None,
    );

    let transaction_id = coordinator_client.create_transaction(&initiator, &steps);
    let transaction = coordinator_client.get_transaction(&transaction_id).unwrap();
    
    // Verify dependencies are properly set
    assert_eq!(transaction.steps.len(), 4); // 4 steps when no royalties
    assert_eq!(transaction.steps.get(1).unwrap().depends_on, Some(1)); // step 2 depends on 1
    assert_eq!(transaction.steps.get(2).unwrap().depends_on, Some(2)); // step3 depends on 2
    assert_eq!(transaction.steps.get(3).unwrap().depends_on, Some(3)); // step4 depends on 3
}