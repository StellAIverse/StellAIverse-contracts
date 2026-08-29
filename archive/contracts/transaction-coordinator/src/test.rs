#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};
use stellai_lib::{TransactionStatus, TransactionStep};

fn create_test_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    (env, admin, user)
}

fn create_test_contract(env: &Env) -> Address {
    env.register(TransactionCoordinator, ())
}

fn create_mock_contract(env: &Env) -> Address {
    Address::generate(env)
}

#[test]
fn test_initialization() {
    let (env, admin, _) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Verify admin is set and counter is initialized by calling a function that would fail if not initialized
    let cleaned_up = client.cleanup_expired_transactions(&admin, &10);
    assert_eq!(cleaned_up, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialization() {
    let (env, admin, _) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.initialize(&admin); // Should panic
}

#[test]
fn test_create_transaction() {
    let (env, admin, user) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);
    let mock_contract = create_mock_contract(&env);

    client.initialize(&admin);

    let steps = Vec::from_array(
        &env,
        [TransactionStep {
            step_id: 1,
            contract: mock_contract.clone(),
            function: Symbol::new(&env, "test_function"),
            args: Vec::new(&env),
            depends_on: None,
            rollback_contract: Some(mock_contract.clone()),
            rollback_function: Some(Symbol::new(&env, "rollback_function")),
            rollback_args: Some(Vec::new(&env)),
            executed: false,
            result: None,
        }],
    );

    let tx_id = client.create_transaction(&user, &steps);
    assert_eq!(tx_id, 1);

    let transaction = client.get_transaction(&tx_id).unwrap();
    assert_eq!(transaction.transaction_id, tx_id);
    assert_eq!(transaction.initiator, user);
    assert_eq!(transaction.status, TransactionStatus::Initiated);
    assert_eq!(transaction.steps.len(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_create_transaction_empty_steps() {
    let (env, admin, user) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);

    client.initialize(&admin);

    let empty_steps = Vec::new(&env);
    client.create_transaction(&user, &empty_steps);
}

#[test]
fn test_transaction_with_dependencies() {
    let (env, admin, user) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);
    let mock_contract = create_mock_contract(&env);

    client.initialize(&admin);

    let steps = Vec::from_array(
        &env,
        [
            TransactionStep {
                step_id: 1,
                contract: mock_contract.clone(),
                function: Symbol::new(&env, "step_1"),
                args: Vec::new(&env),
                depends_on: None,
                rollback_contract: Some(mock_contract.clone()),
                rollback_function: Some(Symbol::new(&env, "rollback_1")),
                rollback_args: Some(Vec::new(&env)),
                executed: false,
                result: None,
            },
            TransactionStep {
                step_id: 2,
                contract: mock_contract.clone(),
                function: Symbol::new(&env, "step_2"),
                args: Vec::new(&env),
                depends_on: Some(1), // Depends on step 1
                rollback_contract: Some(mock_contract.clone()),
                rollback_function: Some(Symbol::new(&env, "rollback_2")),
                rollback_args: Some(Vec::new(&env)),
                executed: false,
                result: None,
            },
        ],
    );

    let tx_id = client.create_transaction(&user, &steps);
    let transaction = client.get_transaction(&tx_id).unwrap();

    assert_eq!(transaction.steps.len(), 2);
    assert_eq!(transaction.steps.get(1).unwrap().depends_on, Some(1));
}

#[test]
fn test_transaction_status_queries() {
    let (env, admin, user) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);
    let mock_contract = create_mock_contract(&env);

    client.initialize(&admin);

    let steps = Vec::from_array(
        &env,
        [TransactionStep {
            step_id: 1,
            contract: mock_contract.clone(),
            function: Symbol::new(&env, "test_function"),
            args: Vec::new(&env),
            depends_on: None,
            rollback_contract: None,
            rollback_function: None,
            rollback_args: None,
            executed: false,
            result: None,
        }],
    );

    let tx_id = client.create_transaction(&user, &steps);

    let status = client.get_transaction_status(&tx_id).unwrap();
    assert_eq!(status, TransactionStatus::Initiated);
}

#[test]
fn test_nonexistent_transaction() {
    let (env, admin, _) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);

    client.initialize(&admin);

    let transaction = client.get_transaction(&999);
    assert!(transaction.is_none());

    let status = client.get_transaction_status(&999);
    assert!(status.is_none());
}

#[test]
fn test_cleanup_expired_transactions() {
    let (env, admin, _) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);

    client.initialize(&admin);

    let cleaned_up = client.cleanup_expired_transactions(&admin, &10);
    assert_eq!(cleaned_up, 0); // No transactions to clean up
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_cleanup_unauthorized() {
    let (env, admin, user) = create_test_env();
    let contract_id = create_test_contract(&env);
    let client = TransactionCoordinatorClient::new(&env, &contract_id);

    client.initialize(&admin);

    client.cleanup_expired_transactions(&user, &10); // Should panic
}

// Integration tests for atomic transaction utils
#[test]
fn test_transaction_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);
    let mock_contract = Address::generate(&env);

    // Valid transaction
    let valid_steps = Vec::from_array(
        &env,
        [TransactionStep {
            step_id: 1,
            contract: mock_contract.clone(),
            function: Symbol::new(&env, "test"),
            args: Vec::new(&env),
            depends_on: None,
            rollback_contract: None,
            rollback_function: None,
            rollback_args: None,
            executed: false,
            result: None,
        }],
    );

    let valid_tx = AtomicTransaction {
        transaction_id: 1,
        initiator: user.clone(),
        steps: valid_steps,
        status: TransactionStatus::Initiated,
        created_at: 0,
        deadline: 300,
        prepared_steps: Vec::new(&env),
        executed_steps: Vec::new(&env),
        failure_reason: None,
    };

    assert!(AtomicTransactionUtils::validate_transaction(&valid_tx).is_ok());

    // Empty transaction
    let empty_tx = AtomicTransaction {
        transaction_id: 2,
        initiator: user.clone(),
        steps: Vec::new(&env),
        status: TransactionStatus::Initiated,
        created_at: 0,
        deadline: 300,
        prepared_steps: Vec::new(&env),
        executed_steps: Vec::new(&env),
        failure_reason: None,
    };

    assert!(AtomicTransactionUtils::validate_transaction(&empty_tx).is_err());
}

#[test]
fn test_execution_order_resolution() {
    let env = Env::default();
    env.mock_all_auths();
    let mock_contract = Address::generate(&env);

    let steps = Vec::from_array(
        &env,
        [
            TransactionStep {
                step_id: 3,
                contract: mock_contract.clone(),
                function: Symbol::new(&env, "step_3"),
                args: Vec::new(&env),
                depends_on: Some(1),
                rollback_contract: None,
                rollback_function: None,
                rollback_args: None,
                executed: false,
                result: None,
            },
            TransactionStep {
                step_id: 1,
                contract: mock_contract.clone(),
                function: Symbol::new(&env, "step_1"),
                args: Vec::new(&env),
                depends_on: None,
                rollback_contract: None,
                rollback_function: None,
                rollback_args: None,
                executed: false,
                result: None,
            },
            TransactionStep {
                step_id: 2,
                contract: mock_contract.clone(),
                function: Symbol::new(&env, "step_2"),
                args: Vec::new(&env),
                depends_on: Some(1),
                rollback_contract: None,
                rollback_function: None,
                rollback_args: None,
                executed: false,
                result: None,
            },
        ],
    );

    let execution_order = AtomicTransactionUtils::resolve_execution_order(&env, &steps);

    // Should execute in dependency order: 1, then 2 and 3 (which both depend on 1)
    assert_eq!(execution_order.get(0).unwrap(), 1);
    assert!(execution_order.contains(&2));
    assert!(execution_order.contains(&3));
}

#[test]
fn test_timeout_detection() {
    let env = Env::default();
    env.mock_all_auths();
    let user = Address::generate(&env);

    // Set current time to 100
    env.ledger().with_mut(|li| li.timestamp = 100);

    let transaction = AtomicTransaction {
        transaction_id: 1,
        initiator: user.clone(),
        steps: Vec::new(&env),
        status: TransactionStatus::Initiated,
        created_at: 0,
        deadline: 50, // Deadline in the past
        prepared_steps: Vec::new(&env),
        executed_steps: Vec::new(&env),
        failure_reason: None,
    };

    assert!(AtomicTransactionUtils::is_transaction_timed_out(
        &env,
        &transaction
    ));

    let not_expired_tx = AtomicTransaction {
        transaction_id: 2,
        initiator: user,
        steps: Vec::new(&env),
        status: TransactionStatus::Initiated,
        created_at: 0,
        deadline: 200, // Deadline in the future
        prepared_steps: Vec::new(&env),
        executed_steps: Vec::new(&env),
        failure_reason: None,
    };

    assert!(!AtomicTransactionUtils::is_transaction_timed_out(
        &env,
        &not_expired_tx
    ));
}
