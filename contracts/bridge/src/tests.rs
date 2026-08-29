#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Env},
        Address, String, Vec,
    };

    use super::super::contract::BridgeGateway;
    use shared::{BridgeDirection, BridgeStatus, SupportedChain, MAX_FEE_BPS};

    // ========================================================================
    // Test Helpers
    // ========================================================================

    fn create_test_env() -> Env {
        Env::default()
    }

    fn create_admin(env: &Env) -> Address {
        Address::generate(env)
    }

    fn create_validators(env: &Env, count: u32) -> Vec<Address> {
        let mut validators = Vec::new(env);
        for _ in 0..count {
            validators.push_back(Address::generate(env));
        }
        validators
    }

    fn setup_bridge(env: &Env) -> Address {
        let admin = create_admin(env);
        env.mock_all_auths();
        BridgeGateway::initialize(env.clone(), admin.clone());
        admin
    }

    fn setup_bridge_with_validators(
        env: &Env,
        chain: SupportedChain,
        num_validators: u32,
        threshold: u32,
    ) -> (Address, Vec<Address>) {
        let admin = setup_bridge(env);
        let validators = create_validators(env, num_validators);

        env.mock_all_auths();
        BridgeGateway::add_chain(env.clone(), admin.clone(), chain);
        BridgeGateway::register_validator_set(
            env.clone(),
            admin.clone(),
            chain,
            validators.clone(),
            threshold,
        );

        (admin, validators)
    }

    fn setup_wrapped_asset(env: &Env, admin: &Address, chain: SupportedChain) {
        let asset_code = String::from_str(env, "USDC");
        let asset_issuer = Address::generate(env);
        env.mock_all_auths();
        BridgeGateway::register_wrapped_asset(
            env.clone(),
            admin.clone(),
            asset_code,
            asset_issuer,
            chain,
        );
    }

    // ========================================================================
    // Initialization Tests
    // ========================================================================

    #[test]
    fn test_initialize_bridge() {
        let env = create_test_env();
        let admin = create_admin(&env);
        env.mock_all_auths();

        let config = BridgeGateway::initialize(env.clone(), admin.clone());
        assert_eq!(config.admin, admin);
        assert!(!config.paused);
        assert_eq!(config.tx_counter, 0);
    }

    #[test]
    #[should_panic(expected = "Bridge already initialized")]
    fn test_initialize_bridge_twice() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        // Try to initialize again
        BridgeGateway::initialize(env.clone(), admin);
    }

    #[test]
    fn test_update_config() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        let config = BridgeGateway::update_config(
            env.clone(),
            admin.clone(),
            Some(172800),      // timelock
            Some(5_000_000),   // min
            Some(100_000_000), // max
            Some(30),          // fee
        );

        assert_eq!(config.timelock_duration, 172800);
        assert_eq!(config.min_bridge_amount, 5_000_000);
        assert_eq!(config.max_bridge_amount, 100_000_000);
        assert_eq!(config.default_fee_bps, 30);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: not admin")]
    fn test_update_config_unauthorized() {
        let env = create_test_env();
        let _admin = setup_bridge(&env);
        let non_admin = create_admin(&env);
        env.mock_all_auths();

        BridgeGateway::update_config(env.clone(), non_admin, None, None, None, None);
    }

    #[test]
    fn test_transfer_admin() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        let new_admin = create_admin(&env);
        env.mock_all_auths();

        BridgeGateway::transfer_admin(env.clone(), admin.clone(), new_admin.clone());

        let config = BridgeGateway::get_config(env.clone());
        assert_eq!(config.admin, new_admin);
    }

    // ========================================================================
    // Chain Support Tests
    // ========================================================================

    #[test]
    fn test_add_chain() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin, SupportedChain::Ethereum);
        assert!(BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Ethereum
        ));
    }

    #[test]
    fn test_remove_chain() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        BridgeGateway::remove_chain(env.clone(), admin, SupportedChain::Ethereum);

        assert!(!BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Ethereum
        ));
    }

    #[test]
    fn test_add_multiple_chains() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Solana);
        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Polygon);

        assert!(BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Ethereum
        ));
        assert!(BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Solana
        ));
        assert!(BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Polygon
        ));
    }

    // ========================================================================
    // Validator Set Tests
    // ========================================================================

    #[test]
    fn test_register_validator_set() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        let validators = create_validators(&env, 5);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        BridgeGateway::register_validator_set(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            validators,
            3,
        );

        let vs = BridgeGateway::get_validator_set(env.clone(), SupportedChain::Ethereum);
        assert!(vs.is_some());
        let vs = vs.unwrap();
        assert_eq!(vs.required_approvals, 3);
        assert!(vs.active);
    }

    #[test]
    #[should_panic(expected = "Need at least")]
    fn test_register_validator_set_too_few() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        let validators = create_validators(&env, 2);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        BridgeGateway::register_validator_set(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            validators,
            2,
        );
    }

    #[test]
    #[should_panic(expected = "Invalid threshold")]
    fn test_register_invalid_threshold() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        let validators = create_validators(&env, 5);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        BridgeGateway::register_validator_set(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            validators,
            6, // More than validators count
        );
    }

    #[test]
    fn test_add_validator() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);

        let new_validator = Address::generate(&env);
        env.mock_all_auths();

        BridgeGateway::add_validator(env.clone(), admin, SupportedChain::Ethereum, new_validator);

        let vs = BridgeGateway::get_validator_set(env.clone(), SupportedChain::Ethereum).unwrap();
        assert_eq!(vs.validators.len(), 4);
    }

    #[test]
    fn test_remove_validator() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);

        env.mock_all_auths();
        BridgeGateway::remove_validator(
            env.clone(),
            admin.clone(),
            SupportedChain::Ethereum,
            validators.get(0).unwrap(),
        );

        let vs = BridgeGateway::get_validator_set(env.clone(), SupportedChain::Ethereum).unwrap();
        assert_eq!(vs.validators.len(), 4);
    }

    #[test]
    #[should_panic(expected = "Cannot go below minimum")]
    fn test_remove_validator_below_minimum() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);

        env.mock_all_auths();
        // Remove 1
        BridgeGateway::remove_validator(
            env.clone(),
            admin.clone(),
            SupportedChain::Ethereum,
            validators.get(0).unwrap(),
        );
        // Remove another - should fail since we'd go below minimum
        BridgeGateway::remove_validator(
            env.clone(),
            admin.clone(),
            SupportedChain::Ethereum,
            validators.get(1).unwrap(),
        );
    }

    #[test]
    fn test_update_validator_threshold() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        env.mock_all_auths();

        BridgeGateway::update_validator_threshold(env.clone(), admin, SupportedChain::Ethereum, 4);

        let vs = BridgeGateway::get_validator_set(env.clone(), SupportedChain::Ethereum).unwrap();
        assert_eq!(vs.required_approvals, 4);
    }

    // ========================================================================
    // Wrapped Asset Tests
    // ========================================================================

    #[test]
    fn test_register_wrapped_asset() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        let asset_code = String::from_str(&env, "USDC");
        let issuer = Address::generate(&env);

        BridgeGateway::register_wrapped_asset(
            env.clone(),
            admin,
            asset_code.clone(),
            issuer,
            SupportedChain::Ethereum,
        );

        let asset =
            BridgeGateway::get_wrapped_asset(env.clone(), asset_code, SupportedChain::Ethereum);
        assert!(asset.is_some());
        let asset = asset.unwrap();
        assert!(asset.active);
        assert_eq!(asset.total_supply, 0);
    }

    #[test]
    fn test_deactivate_wrapped_asset() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        let asset_code = String::from_str(&env, "USDC");
        let issuer = Address::generate(&env);

        BridgeGateway::register_wrapped_asset(
            env.clone(),
            admin.clone(),
            asset_code.clone(),
            issuer,
            SupportedChain::Ethereum,
        );

        BridgeGateway::deactivate_wrapped_asset(
            env.clone(),
            admin,
            asset_code.clone(),
            SupportedChain::Ethereum,
        );

        let asset =
            BridgeGateway::get_wrapped_asset(env.clone(), asset_code, SupportedChain::Ethereum)
                .unwrap();
        assert!(!asset.active);
    }

    // ========================================================================
    // Fee Tier Tests
    // ========================================================================

    #[test]
    fn test_set_fee_tier() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::set_fee_tier(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            0,
            1_000_000,
            100_000_000,
            30, // 0.3%
        );

        let fee = BridgeGateway::calculate_fee(env.clone(), SupportedChain::Ethereum, 10_000_000);
        assert_eq!(fee, 30_000); // 0.3% of 10M
    }

    #[test]
    fn test_fee_tier_by_amount_range() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        // Small amount tier: 10 bps
        BridgeGateway::set_fee_tier(
            env.clone(),
            admin.clone(),
            SupportedChain::Ethereum,
            0,
            1_000_000,
            10_000_000,
            10,
        );

        // Large amount tier: 5 bps
        BridgeGateway::set_fee_tier(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            1,
            10_000_001,
            100_000_000,
            5,
        );

        let small_fee =
            BridgeGateway::calculate_fee(env.clone(), SupportedChain::Ethereum, 5_000_000);
        assert_eq!(small_fee, 5_000); // 0.1%

        let large_fee =
            BridgeGateway::calculate_fee(env.clone(), SupportedChain::Ethereum, 50_000_000);
        assert_eq!(large_fee, 25_000); // 0.05%
    }

    #[test]
    #[should_panic(expected = "Fee exceeds maximum")]
    fn test_set_fee_tier_exceeds_max() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::set_fee_tier(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            0,
            1_000_000,
            100_000_000,
            600, // 6% - exceeds MAX_FEE_BPS
        );
    }

    // ========================================================================
    // Lock Assets (Outbound Bridge) Tests
    // ========================================================================

    #[test]
    fn test_lock_assets() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF1234567890");
        let source_tx = String::from_str(&env, "stellar_tx_001");
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender.clone(),
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000, // 10 USDC (6 decimals)
            dest_address,
            source_tx,
        );

        assert!(tx_id > 0);

        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Pending);
        assert_eq!(tx.direction, BridgeDirection::Outbound);
        assert_eq!(tx.sender, sender);
    }

    #[test]
    #[should_panic(expected = "Amount below minimum")]
    fn test_lock_assets_below_minimum() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_002");
        env.mock_all_auths();

        BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            100, // Too small
            dest_address,
            source_tx,
        );
    }

    #[test]
    #[should_panic(expected = "Double spend detected")]
    fn test_lock_assets_double_spend() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_dup");
        env.mock_all_auths();

        // First lock
        BridgeGateway::lock_assets(
            env.clone(),
            sender.clone(),
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address.clone(),
            source_tx.clone(),
        );

        // Second lock with same source tx - should fail
        BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );
    }

    #[test]
    #[should_panic(expected = "Chain not supported")]
    fn test_lock_assets_unsupported_chain() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_chain");
        env.mock_all_auths();

        BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Solana, // Not supported
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );
    }

    // ========================================================================
    // Validator Approval Tests
    // ========================================================================

    #[test]
    fn test_approve_transaction() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_val");
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        // First validator approves
        let sig = String::from_str(&env, "sig1");
        let approvals = BridgeGateway::approve_transaction(
            env.clone(),
            validators.get(0).unwrap(),
            tx_id,
            SupportedChain::Ethereum,
            sig,
        );
        assert_eq!(approvals, 1);

        // Transaction should still be pending (need 3)
        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Pending);

        // Second validator approves
        let sig = String::from_str(&env, "sig2");
        BridgeGateway::approve_transaction(
            env.clone(),
            validators.get(1).unwrap(),
            tx_id,
            SupportedChain::Ethereum,
            sig,
        );

        // Third validator approves - should trigger validation
        let sig = String::from_str(&env, "sig3");
        let approvals = BridgeGateway::approve_transaction(
            env.clone(),
            validators.get(2).unwrap(),
            tx_id,
            SupportedChain::Ethereum,
            sig,
        );
        assert_eq!(approvals, 3);

        // Now transaction should be validated
        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Validated);
    }

    #[test]
    #[should_panic(expected = "Validator not in set")]
    fn test_approve_transaction_invalid_validator() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_inv_val");
        let fake_validator = Address::generate(&env);
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        let sig = String::from_str(&env, "sig_fake");
        BridgeGateway::approve_transaction(
            env.clone(),
            fake_validator, // Not in validator set
            tx_id,
            SupportedChain::Ethereum,
            sig,
        );
    }

    // ========================================================================
    // Complete Transaction Tests
    // ========================================================================

    #[test]
    fn test_complete_transaction() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_comp");
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        // Get 2 approvals (threshold met)
        for i in 0..2 {
            let sig = String::from_str(&env, &format!("sig{}", i));
            BridgeGateway::approve_transaction(
                env.clone(),
                validators.get(i).unwrap(),
                tx_id,
                SupportedChain::Ethereum,
                sig,
            );
        }

        // Complete
        let dest_tx_hash = String::from_str(&env, "eth_tx_0xabc123");
        BridgeGateway::complete_transaction(env.clone(), admin.clone(), tx_id, dest_tx_hash);

        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Completed);
        assert!(tx.dest_tx_hash.is_some());

        // Check wrapped asset supply increased
        let asset = BridgeGateway::get_wrapped_asset(
            env.clone(),
            String::from_str(&env, "USDC"),
            SupportedChain::Ethereum,
        )
        .unwrap();
        assert_eq!(asset.total_supply, 10_000_000);
    }

    #[test]
    #[should_panic(expected = "Transaction must be validated first")]
    fn test_complete_transaction_not_validated() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_noval");
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        // Try to complete without enough approvals
        let dest_tx_hash = String::from_str(&env, "eth_tx_0x");
        BridgeGateway::complete_transaction(env.clone(), admin, tx_id, dest_tx_hash);
    }

    // ========================================================================
    // Mint Assets (Inbound) Tests
    // ========================================================================

    #[test]
    fn test_mint_assets() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let source_tx = String::from_str(&env, "eth_lock_tx");
        let merkle = String::from_str(&env, "merkle_root_123");
        env.mock_all_auths();

        let tx_id = BridgeGateway::mint_assets(
            env.clone(),
            sender.clone(),
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            source_tx,
            merkle,
        );

        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Pending);
        assert_eq!(tx.direction, BridgeDirection::Inbound);
        assert_eq!(tx.sender, sender);
        assert!(tx.merkle_root.is_some());
    }

    // ========================================================================
    // Release Assets Tests
    // ========================================================================

    #[test]
    fn test_release_assets() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let source_tx = String::from_str(&env, "eth_tx_rel");
        let merkle = String::from_str(&env, "merkle_rel");
        env.mock_all_auths();

        let tx_id = BridgeGateway::mint_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            source_tx,
            merkle,
        );

        // Approve (2 needed)
        for i in 0..2 {
            let sig = String::from_str(&env, &format!("sig_rel{}", i));
            BridgeGateway::approve_transaction(
                env.clone(),
                validators.get(i).unwrap(),
                tx_id,
                SupportedChain::Ethereum,
                sig,
            );
        }

        // Release
        BridgeGateway::release_assets(env.clone(), admin.clone(), tx_id);

        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Completed);
    }

    #[test]
    #[should_panic(expected = "Only inbound transactions")]
    fn test_release_assets_outbound() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_out_rel");
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        // Approve
        for i in 0..2 {
            let sig = String::from_str(&env, &format!("sig_or{}", i));
            BridgeGateway::approve_transaction(
                env.clone(),
                validators.get(i).unwrap(),
                tx_id,
                SupportedChain::Ethereum,
                sig,
            );
        }

        // Try to release outbound - should fail
        BridgeGateway::release_assets(env.clone(), admin, tx_id);
    }

    // ========================================================================
    // Liquidity Pool Tests
    // ========================================================================

    #[test]
    fn test_create_liquidity_pool() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        let pool_id = BridgeGateway::create_liquidity_pool(
            env.clone(),
            admin,
            String::from_str(&env, "USDC"),
            SupportedChain::Ethereum,
        );

        assert!(pool_id > 0);

        let pool = BridgeGateway::get_liquidity_pool(env.clone(), pool_id).unwrap();
        assert!(pool.active);
        assert_eq!(pool.total_liquidity, 0);
    }

    #[test]
    fn test_add_and_remove_liquidity() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        let provider = Address::generate(&env);
        env.mock_all_auths();

        let pool_id = BridgeGateway::create_liquidity_pool(
            env.clone(),
            admin,
            String::from_str(&env, "USDC"),
            SupportedChain::Ethereum,
        );

        // Add liquidity
        let lp_tokens = BridgeGateway::add_liquidity(
            env.clone(),
            provider.clone(),
            pool_id,
            100_000_000, // 100 USDC
        );

        assert_eq!(lp_tokens, 100_000_000); // 1:1 for first deposit

        let pool = BridgeGateway::get_liquidity_pool(env.clone(), pool_id).unwrap();
        assert_eq!(pool.total_liquidity, 100_000_000);
        assert_eq!(pool.lp_token_balance, 100_000_000);

        // Remove half
        let withdrawn = BridgeGateway::remove_liquidity(
            env.clone(),
            provider,
            pool_id,
            50_000_000, // Half LP tokens
        );

        assert_eq!(withdrawn, 50_000_000);

        let pool = BridgeGateway::get_liquidity_pool(env.clone(), pool_id).unwrap();
        assert_eq!(pool.total_liquidity, 50_000_000);
        assert_eq!(pool.lp_token_balance, 50_000_000);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn test_add_liquidity_zero() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        let provider = Address::generate(&env);
        env.mock_all_auths();

        let pool_id = BridgeGateway::create_liquidity_pool(
            env.clone(),
            admin,
            String::from_str(&env, "USDC"),
            SupportedChain::Ethereum,
        );

        BridgeGateway::add_liquidity(env.clone(), provider, pool_id, 0);
    }

    #[test]
    #[should_panic(expected = "Insufficient LP tokens")]
    fn test_remove_liquidity_too_much() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        let provider = Address::generate(&env);
        env.mock_all_auths();

        let pool_id = BridgeGateway::create_liquidity_pool(
            env.clone(),
            admin,
            String::from_str(&env, "USDC"),
            SupportedChain::Ethereum,
        );

        BridgeGateway::add_liquidity(env.clone(), provider.clone(), pool_id, 10_000_000);

        // Try to remove more than deposited
        BridgeGateway::remove_liquidity(env.clone(), provider, pool_id, 20_000_000);
    }

    // ========================================================================
    // Emergency Controls Tests
    // ========================================================================

    #[test]
    fn test_pause_bridge() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::pause_bridge(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Security incident"),
            None,
        );

        let status = BridgeGateway::get_emergency_status(env.clone());
        assert!(status.paused);
        assert_eq!(
            status.reason.unwrap(),
            String::from_str(&env, "Security incident")
        );
    }

    #[test]
    fn test_unpause_bridge() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::pause_bridge(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Maintenance"),
            None,
        );

        BridgeGateway::unpause_bridge(env.clone(), admin);

        let status = BridgeGateway::get_emergency_status(env.clone());
        assert!(!status.paused);
    }

    #[test]
    #[should_panic(expected = "Bridge is paused")]
    fn test_lock_assets_while_paused() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_paused");
        env.mock_all_auths();

        // Pause
        BridgeGateway::pause_bridge(
            env.clone(),
            admin,
            String::from_str(&env, "Emergency"),
            None,
        );

        // Try to lock - should fail
        BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );
    }

    #[test]
    fn test_pause_with_timelock() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        // Pause with timelock
        BridgeGateway::pause_bridge(
            env.clone(),
            admin.clone(),
            String::from_str(&env, "Scheduled maintenance"),
            Some(1000), // Unpause after ledger time 1000
        );

        let status = BridgeGateway::get_emergency_status(env.clone());
        assert!(status.paused);
        assert!(status.unpause_after.is_some());
    }

    // ========================================================================
    // Dispute Resolution Tests
    // ========================================================================

    #[test]
    fn test_file_dispute() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_disp");
        let disputer = Address::generate(&env);
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        let dispute_id = BridgeGateway::file_dispute(
            env.clone(),
            disputer,
            tx_id,
            String::from_str(&env, "Transaction not confirmed on source chain"),
        );

        assert!(dispute_id > 0);

        let dispute = BridgeGateway::get_dispute(env.clone(), dispute_id).unwrap();
        assert!(!dispute.resolved);

        // Check transaction status changed to disputed
        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Disputed);
    }

    #[test]
    fn test_resolve_dispute() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_res_disp");
        let disputer = Address::generate(&env);
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        let dispute_id = BridgeGateway::file_dispute(
            env.clone(),
            disputer,
            tx_id,
            String::from_str(&env, "Issue with source tx"),
        );

        BridgeGateway::resolve_dispute(
            env.clone(),
            admin,
            dispute_id,
            String::from_str(&env, "resolved"),
        );

        let dispute = BridgeGateway::get_dispute(env.clone(), dispute_id).unwrap();
        assert!(dispute.resolved);
        assert!(dispute.resolution.is_some());
    }

    #[test]
    #[should_panic(expected = "Dispute already resolved")]
    fn test_resolve_dispute_twice() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_res2");
        let disputer = Address::generate(&env);
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        let dispute_id = BridgeGateway::file_dispute(
            env.clone(),
            disputer,
            tx_id,
            String::from_str(&env, "Issue"),
        );

        BridgeGateway::resolve_dispute(
            env.clone(),
            admin.clone(),
            dispute_id,
            String::from_str(&env, "resolved"),
        );

        // Try to resolve again
        BridgeGateway::resolve_dispute(
            env.clone(),
            admin,
            dispute_id,
            String::from_str(&env, "second resolution"),
        );
    }

    // ========================================================================
    // Cancel Transaction Tests
    // ========================================================================

    #[test]
    fn test_cancel_transaction() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_cancel");
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender.clone(),
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        BridgeGateway::cancel_transaction(env.clone(), sender, tx_id);

        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        assert_eq!(tx.status, BridgeStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "Can only cancel pending transactions")]
    fn test_cancel_completed_transaction() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_nocancel");
        env.mock_all_auths();

        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender.clone(),
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx,
        );

        // Approve and complete
        for i in 0..2 {
            let sig = String::from_str(&env, &format!("sig_nc{}", i));
            BridgeGateway::approve_transaction(
                env.clone(),
                validators.get(i).unwrap(),
                tx_id,
                SupportedChain::Ethereum,
                sig,
            );
        }

        let dest_tx = String::from_str(&env, "eth_tx_nc");
        BridgeGateway::complete_transaction(env.clone(), admin.clone(), tx_id, dest_tx);

        // Try to cancel - should fail
        BridgeGateway::cancel_transaction(env.clone(), sender, tx_id);
    }

    // ========================================================================
    // Double-Spend Prevention Tests
    // ========================================================================

    #[test]
    fn test_double_spend_detection() {
        let env = create_test_env();
        let (admin, _) = setup_bridge_with_validators(&env, SupportedChain::Ethereum, 5, 3);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        let source_tx = String::from_str(&env, "tx_dup_check");
        env.mock_all_auths();

        assert!(!BridgeGateway::check_duplicate(
            env.clone(),
            source_tx.clone()
        ));

        BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address,
            source_tx.clone(),
        );

        assert!(BridgeGateway::check_duplicate(env.clone(), source_tx));
    }

    // ========================================================================
    // Transaction Count Tests
    // ========================================================================

    #[test]
    fn test_get_tx_count() {
        let env = create_test_env();
        let _admin = setup_bridge(&env);
        env.mock_all_auths();

        assert_eq!(BridgeGateway::get_tx_count(env.clone()), 0);
    }

    // ========================================================================
    // Multi-Chain Support Tests
    // ========================================================================

    #[test]
    fn test_multi_chain_bridge() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        // Add all chains
        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Ethereum);
        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Solana);
        BridgeGateway::add_chain(env.clone(), admin.clone(), SupportedChain::Polygon);

        assert!(BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Ethereum
        ));
        assert!(BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Solana
        ));
        assert!(BridgeGateway::is_chain_supported(
            env.clone(),
            SupportedChain::Polygon
        ));

        // Register validators per chain
        let eth_validators = create_validators(&env, 5);
        let sol_validators = create_validators(&env, 4);

        BridgeGateway::register_validator_set(
            env.clone(),
            admin.clone(),
            SupportedChain::Ethereum,
            eth_validators,
            3,
        );
        BridgeGateway::register_validator_set(
            env.clone(),
            admin.clone(),
            SupportedChain::Solana,
            sol_validators,
            3,
        );

        // Register wrapped assets per chain
        let usdc_issuer = Address::generate(&env);
        let eth_asset_code = String::from_str(&env, "USDC");
        BridgeGateway::register_wrapped_asset(
            env.clone(),
            admin.clone(),
            eth_asset_code,
            usdc_issuer.clone(),
            SupportedChain::Ethereum,
        );

        let sol_asset_code = String::from_str(&env, "USDC");
        BridgeGateway::register_wrapped_asset(
            env.clone(),
            admin,
            sol_asset_code,
            usdc_issuer,
            SupportedChain::Solana,
        );
    }

    // ========================================================================
    // Fee Calculation Edge Cases
    // ========================================================================

    #[test]
    fn test_fee_zero_bps() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::set_fee_tier(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            0,
            1_000_000,
            100_000_000,
            0, // 0% fee
        );

        let fee = BridgeGateway::calculate_fee(env.clone(), SupportedChain::Ethereum, 10_000_000);
        assert_eq!(fee, 0);
    }

    #[test]
    fn test_fee_max_bps() {
        let env = create_test_env();
        let admin = setup_bridge(&env);
        env.mock_all_auths();

        BridgeGateway::set_fee_tier(
            env.clone(),
            admin,
            SupportedChain::Ethereum,
            0,
            1_000_000,
            100_000_000,
            MAX_FEE_BPS, // 5% fee
        );

        let fee = BridgeGateway::calculate_fee(env.clone(), SupportedChain::Ethereum, 10_000_000);
        assert_eq!(fee, 500_000); // 5% of 10M
    }

    // ========================================================================
    // Wrapped Asset Supply Tracking Tests
    // ========================================================================

    #[test]
    fn test_wrapped_asset_supply_tracking() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        env.mock_all_auths();

        // Lock 1
        let tx1 = BridgeGateway::lock_assets(
            env.clone(),
            sender.clone(),
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            10_000_000,
            dest_address.clone(),
            String::from_str(&env, "tx_s1"),
        );

        // Lock 2
        let tx2 = BridgeGateway::lock_assets(
            env.clone(),
            sender.clone(),
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            20_000_000,
            dest_address.clone(),
            String::from_str(&env, "tx_s2"),
        );

        // Approve and complete tx1
        for i in 0..2 {
            let sig = String::from_str(&env, &format!("sig_s1_{}", i));
            BridgeGateway::approve_transaction(
                env.clone(),
                validators.get(i).unwrap(),
                tx1,
                SupportedChain::Ethereum,
                sig,
            );
        }
        BridgeGateway::complete_transaction(
            env.clone(),
            admin.clone(),
            tx1,
            String::from_str(&env, "eth_comp1"),
        );

        // Approve and complete tx2
        for i in 0..2 {
            let sig = String::from_str(&env, &format!("sig_s2_{}", i));
            BridgeGateway::approve_transaction(
                env.clone(),
                validators.get(i).unwrap(),
                tx2,
                SupportedChain::Ethereum,
                sig,
            );
        }
        BridgeGateway::complete_transaction(
            env.clone(),
            admin.clone(),
            tx2,
            String::from_str(&env, "eth_comp2"),
        );

        let asset = BridgeGateway::get_wrapped_asset(
            env.clone(),
            String::from_str(&env, "USDC"),
            SupportedChain::Ethereum,
        )
        .unwrap();

        assert_eq!(asset.total_supply, 30_000_000); // 10 + 20
    }

    // ========================================================================
    // 1:1 Peg Verification Tests
    // ========================================================================

    #[test]
    fn test_one_to_one_peg_maintained() {
        let env = create_test_env();
        let (admin, validators) =
            setup_bridge_with_validators(&env, SupportedChain::Ethereum, 3, 2);
        setup_wrapped_asset(&env, &admin, SupportedChain::Ethereum);

        let sender = Address::generate(&env);
        let dest_address = String::from_str(&env, "0xABCDEF");
        env.mock_all_auths();

        let amount = 50_000_000; // 50 USDC

        // Lock
        let tx_id = BridgeGateway::lock_assets(
            env.clone(),
            sender,
            SupportedChain::Ethereum,
            String::from_str(&env, "USDC"),
            amount,
            dest_address,
            String::from_str(&env, "tx_peg"),
        );

        // Approve
        for i in 0..2 {
            let sig = String::from_str(&env, &format!("sig_peg{}", i));
            BridgeGateway::approve_transaction(
                env.clone(),
                validators.get(i).unwrap(),
                tx_id,
                SupportedChain::Ethereum,
                sig,
            );
        }

        // Complete
        BridgeGateway::complete_transaction(
            env.clone(),
            admin.clone(),
            tx_id,
            String::from_str(&env, "eth_peg"),
        );

        // Verify 1:1 peg
        let tx = BridgeGateway::get_transaction(env.clone(), tx_id).unwrap();
        let asset = BridgeGateway::get_wrapped_asset(
            env.clone(),
            String::from_str(&env, "USDC"),
            SupportedChain::Ethereum,
        )
        .unwrap();

        assert_eq!(tx.amount, asset.total_supply); // 1:1
    }
}
