#[cfg(test)]
mod tests {
    use crate::contract::CrossChainBridge;
    use crate::errors::BridgeError;
    use crate::types::*;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

    #[test]
    fn test_initialize_bridge() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let _ = env.register(CrossChainBridge, ());

        env.mock_all_auths();

        // Create configuration
        let sig_config = SignatureConfig {
            required_signatures: 2,
            total_validators: 3,
            quorum_percentage: 67,
        };

        let fee_config = FeeConfig {
            basis_points: 25, // 0.25%
            min_fee: 1000,
            fee_collector: Address::generate(&env),
        };

        let rate_config = RateLimitConfig {
            daily_limit: 1000000000,        // 1B
            monthly_limit: 30000000000,     // 30B
            per_transaction_max: 100000000, // 100M
            per_transaction_min: 1000,
        };

        // Initialize bridge
        let result = CrossChainBridge::initialize(
            env.clone(),
            admin,
            ChainID::Stellar,
            sig_config,
            fee_config,
            rate_config,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_cannot_initialize_twice() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let _ = env.register(CrossChainBridge, ());

        env.mock_all_auths();

        let sig_config = SignatureConfig {
            required_signatures: 2,
            total_validators: 3,
            quorum_percentage: 67,
        };

        let fee_config = FeeConfig {
            basis_points: 25,
            min_fee: 1000,
            fee_collector: Address::generate(&env),
        };

        let rate_config = RateLimitConfig {
            daily_limit: 1000000000,
            monthly_limit: 30000000000,
            per_transaction_max: 100000000,
            per_transaction_min: 1000,
        };

        // First initialization
        let _ = CrossChainBridge::initialize(
            env.clone(),
            admin.clone(),
            ChainID::Stellar,
            sig_config,
            fee_config.clone(),
            rate_config,
        );

        // Second initialization should fail
        let result = CrossChainBridge::initialize(
            env.clone(),
            admin,
            ChainID::Stellar,
            sig_config,
            fee_config,
            rate_config,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BridgeError::AlreadyInitialized);
    }

    #[test]
    fn test_add_validator() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let validator = Address::generate(&env);
        let _ = env.register(CrossChainBridge, ());

        env.mock_all_auths();

        // First initialize
        let sig_config = SignatureConfig {
            required_signatures: 1,
            total_validators: 0,
            quorum_percentage: 67,
        };

        let fee_config = FeeConfig {
            basis_points: 25,
            min_fee: 1000,
            fee_collector: Address::generate(&env),
        };

        let rate_config = RateLimitConfig {
            daily_limit: 1000000000,
            monthly_limit: 30000000000,
            per_transaction_max: 100000000,
            per_transaction_min: 1000,
        };

        let _ = CrossChainBridge::initialize(
            env.clone(),
            admin,
            ChainID::Stellar,
            sig_config,
            fee_config,
            rate_config,
        );

        // Add validator
        let pub_key = Bytes::from_array(&env, &[0u8; 32]); // Ed25519 public key
        let result = CrossChainBridge::add_validator(
            env.clone(),
            validator,
            pub_key,
            100, // power
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let _ = env.register(CrossChainBridge, ());

        env.mock_all_auths();

        // Initialize
        let sig_config = SignatureConfig {
            required_signatures: 1,
            total_validators: 0,
            quorum_percentage: 67,
        };

        let fee_config = FeeConfig {
            basis_points: 25,
            min_fee: 1000,
            fee_collector: Address::generate(&env),
        };

        let rate_config = RateLimitConfig {
            daily_limit: 1000000000,
            monthly_limit: 30000000000,
            per_transaction_max: 100000000,
            per_transaction_min: 1000,
        };

        let _ = CrossChainBridge::initialize(
            env.clone(),
            admin.clone(),
            ChainID::Stellar,
            sig_config,
            fee_config,
            rate_config,
        );

        // Pause
        let pause_result = CrossChainBridge::pause_bridge(env.clone());
        assert!(pause_result.is_ok());

        // Cannot pause twice
        let double_pause = CrossChainBridge::pause_bridge(env.clone());
        assert!(double_pause.is_err());
        assert_eq!(double_pause.unwrap_err(), BridgeError::AlreadyPaused);

        // Unpause
        let unpause_result = CrossChainBridge::unpause_bridge(env.clone());
        assert!(unpause_result.is_ok());

        // Cannot unpause twice
        let double_unpause = CrossChainBridge::unpause_bridge(env.clone());
        assert!(double_unpause.is_err());
        assert_eq!(double_unpause.unwrap_err(), BridgeError::AlreadyUnpaused);
    }

    #[test]
    fn test_bridge_paused_blocks_transfers() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let _ = env.register(CrossChainBridge, ());

        env.mock_all_auths();

        // Initialize
        let sig_config = SignatureConfig {
            required_signatures: 1,
            total_validators: 0,
            quorum_percentage: 67,
        };

        let fee_config = FeeConfig {
            basis_points: 25,
            min_fee: 1000,
            fee_collector: Address::generate(&env),
        };

        let rate_config = RateLimitConfig {
            daily_limit: 1000000000,
            monthly_limit: 30000000000,
            per_transaction_max: 100000000,
            per_transaction_min: 1000,
        };

        let _ = CrossChainBridge::initialize(
            env.clone(),
            admin,
            ChainID::Stellar,
            sig_config,
            fee_config,
            rate_config,
        );

        // Pause the bridge
        let _ = CrossChainBridge::pause_bridge(env.clone());

        // Try to initiate transfer while paused
        let recipient = Bytes::from_array(&env, &[0u8; 32]);
        let token = Address::generate(&env);
        let sender = Address::generate(&env);

        let result = CrossChainBridge::initiate_transfer(
            env,
            ChainID::Ethereum,
            recipient,
            token,
            1000000,
            1,
            sender,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BridgeError::BridgePaused);
    }
}
