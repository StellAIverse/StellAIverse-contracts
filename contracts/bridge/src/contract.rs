use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Vec};

use shared::{
    BridgeConfig, BridgeDirection, BridgeDispute, BridgeFeeTier, BridgeStatus, BridgeTransaction,
    EmergencyConfig, LiquidityPool, LiquidityProviderDeposit, SupportedChain, ValidatorSetConfig,
    ValidatorSignature, WrappedAsset, MAX_DISPUTE_REASON_LENGTH, MAX_FEE_BPS,
    MAX_VALIDATORS_PER_CHAIN, MIN_FEE_BPS, MIN_VALIDATORS_PER_CHAIN,
};

use crate::storage;

#[contract]
pub struct BridgeGateway;

#[contractimpl]
impl BridgeGateway {
    // =========================================================================
    // Initialization
    // =========================================================================

    /// Initialize the bridge contract with an admin
    pub fn initialize(env: Env, admin: Address) -> BridgeConfig {
        // Check if already initialized
        if storage::get_config(&env).tx_counter > 0 {
            panic!("Bridge already initialized");
        }
        admin.require_auth();
        storage::initialize_bridge(&env, &admin)
    }

    /// Get the bridge configuration
    pub fn get_config(env: Env) -> BridgeConfig {
        storage::get_config(&env)
    }

    /// Update bridge configuration (admin only)
    pub fn update_config(
        env: Env,
        admin: Address,
        timelock_duration: Option<u64>,
        min_bridge_amount: Option<i128>,
        max_bridge_amount: Option<i128>,
        default_fee_bps: Option<u32>,
    ) -> BridgeConfig {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        if let Some(td) = timelock_duration {
            config.timelock_duration = td;
        }
        if let Some(mba) = min_bridge_amount {
            config.min_bridge_amount = mba;
        }
        if let Some(mxa) = max_bridge_amount {
            config.max_bridge_amount = mxa;
        }
        if let Some(fee) = default_fee_bps {
            assert!(fee <= MAX_FEE_BPS, "Fee exceeds maximum");
            config.default_fee_bps = fee;
        }

        storage::set_config(&env, &config);
        config
    }

    /// Transfer admin role (admin only, requires timelock)
    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }
        config.admin = new_admin;
        storage::set_config(&env, &config);
    }

    // =========================================================================
    // Chain Support
    // =========================================================================

    /// Add a supported chain (admin only)
    pub fn add_chain(env: Env, admin: Address, chain: SupportedChain) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }
        storage::add_supported_chain(&env, &chain);
    }

    /// Remove a supported chain (admin only)
    pub fn remove_chain(env: Env, admin: Address, chain: SupportedChain) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }
        storage::remove_supported_chain(&env, &chain);
    }

    /// Check if a chain is supported
    pub fn is_chain_supported(env: Env, chain: SupportedChain) -> bool {
        storage::is_chain_supported(&env, &chain)
    }

    // =========================================================================
    // Validator Management
    // =========================================================================

    /// Register a validator set for a chain (admin only)
    pub fn register_validator_set(
        env: Env,
        admin: Address,
        chain: SupportedChain,
        validators: Vec<Address>,
        required_approvals: u32,
    ) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let validator_count = validators.len();
        assert!(
            validator_count >= MIN_VALIDATORS_PER_CHAIN,
            "Need at least {} validators",
            MIN_VALIDATORS_PER_CHAIN
        );
        assert!(
            validator_count <= MAX_VALIDATORS_PER_CHAIN,
            "Too many validators (max {})",
            MAX_VALIDATORS_PER_CHAIN
        );
        assert!(
            required_approvals > 0 && required_approvals <= validator_count,
            "Invalid threshold: must be 1..=count"
        );

        let vs_config = ValidatorSetConfig {
            chain,
            validators,
            required_approvals,
            active: true,
        };

        storage::set_validator_set(&env, &vs_config);
    }

    /// Update validator set threshold (admin only, subject to timelock)
    pub fn update_validator_threshold(
        env: Env,
        admin: Address,
        chain: SupportedChain,
        new_threshold: u32,
    ) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let mut vs = storage::get_validator_set(&env, &chain).expect("Validator set not found");

        assert!(
            new_threshold > 0 && new_threshold <= vs.validators.len(),
            "Invalid threshold"
        );

        vs.required_approvals = new_threshold;
        storage::set_validator_set(&env, &vs);
    }

    /// Add a validator to a chain's set (admin only)
    pub fn add_validator(env: Env, admin: Address, chain: SupportedChain, validator: Address) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let mut vs = storage::get_validator_set(&env, &chain).expect("Validator set not found");

        // Check not duplicate
        for v in vs.validators.iter() {
            if v == validator {
                panic!("Validator already in set");
            }
        }

        assert!(
            vs.validators.len() < MAX_VALIDATORS_PER_CHAIN,
            "Validator set is full"
        );

        vs.validators.push_back(validator);
        storage::set_validator_set(&env, &vs);
    }

    /// Remove a validator from a chain's set (admin only)
    pub fn remove_validator(env: Env, admin: Address, chain: SupportedChain, validator: Address) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let mut vs = storage::get_validator_set(&env, &chain).expect("Validator set not found");

        let mut new_validators = Vec::new(&env);
        let mut found = false;

        for v in vs.validators.iter() {
            if v != validator {
                new_validators.push_back(v);
            } else {
                found = true;
            }
        }

        if !found {
            panic!("Validator not in set");
        }

        assert!(
            new_validators.len() >= MIN_VALIDATORS_PER_CHAIN,
            "Cannot go below minimum validators"
        );

        // Adjust threshold if needed
        if vs.required_approvals > new_validators.len() {
            vs.required_approvals = new_validators.len();
        }

        vs.validators = new_validators;
        storage::set_validator_set(&env, &vs);
    }

    /// Get validator set for a chain
    pub fn get_validator_set(env: Env, chain: SupportedChain) -> Option<ValidatorSetConfig> {
        storage::get_validator_set(&env, &chain)
    }

    // =========================================================================
    // Wrapped Asset Management
    // =========================================================================

    /// Register a wrapped asset (admin only)
    pub fn register_wrapped_asset(
        env: Env,
        admin: Address,
        asset_code: String,
        asset_issuer: Address,
        chain: SupportedChain,
    ) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        if !storage::is_chain_supported(&env, &chain) {
            panic!("Chain not supported");
        }

        let wrapped = WrappedAsset {
            asset_code: asset_code.clone(),
            asset_issuer,
            wrapped_address: String::from_str(&env, ""),
            chain,
            total_supply: 0,
            total_locked: 0,
            active: true,
        };

        storage::set_wrapped_asset(&env, &wrapped);
    }

    /// Get wrapped asset info
    pub fn get_wrapped_asset(
        env: Env,
        asset_code: String,
        chain: SupportedChain,
    ) -> Option<WrappedAsset> {
        storage::get_wrapped_asset(&env, &asset_code, &chain)
    }

    /// Deactivate a wrapped asset (admin only)
    pub fn deactivate_wrapped_asset(
        env: Env,
        admin: Address,
        asset_code: String,
        chain: SupportedChain,
    ) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let mut asset =
            storage::get_wrapped_asset(&env, &asset_code, &chain).expect("Wrapped asset not found");
        asset.active = false;
        storage::set_wrapped_asset(&env, &asset);
    }

    // =========================================================================
    // Fee Tier Management
    // =========================================================================

    /// Set a fee tier for a chain (admin only)
    pub fn set_fee_tier(
        env: Env,
        admin: Address,
        chain: SupportedChain,
        tier_index: u32,
        min_amount: i128,
        max_amount: i128,
        fee_bps: u32,
    ) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        assert!(fee_bps <= MAX_FEE_BPS, "Fee exceeds maximum");
        assert!(min_amount >= 0, "Min amount must be non-negative");
        assert!(max_amount > min_amount, "Max must be greater than min");

        let tier = BridgeFeeTier {
            chain,
            min_amount,
            max_amount,
            fee_bps,
            active: true,
        };

        storage::set_fee_tier_at(&env, &chain, tier_index, &tier);
    }

    /// Get fee for a given chain and amount
    pub fn calculate_fee(env: Env, chain: SupportedChain, amount: i128) -> i128 {
        storage::calculate_fee(&env, &chain, amount)
    }

    // =========================================================================
    // Bridge Operations: Lock (Outbound)
    // =========================================================================

    /// Lock assets on Stellar for outbound bridging
    pub fn lock_assets(
        env: Env,
        sender: Address,
        dest_chain: SupportedChain,
        asset_code: String,
        amount: i128,
        dest_address: String,
        source_tx_hash: String,
    ) -> u64 {
        sender.require_auth();
        storage::require_not_paused(&env);

        let config = storage::get_config(&env);

        // Validate
        assert!(
            storage::is_chain_supported(&env, &dest_chain),
            "Chain not supported"
        );
        assert!(amount >= config.min_bridge_amount, "Amount below minimum");
        assert!(amount <= config.max_bridge_amount, "Amount exceeds maximum");
        assert!(!source_tx_hash.is_empty(), "Source tx hash required");
        assert!(!dest_address.is_empty(), "Destination address required");

        // Check for double spend
        if storage::is_duplicate_tx(&env, &source_tx_hash) {
            panic!("Double spend detected: duplicate source tx hash");
        }

        // Check wrapped asset exists
        let wrapped = storage::get_wrapped_asset(&env, &asset_code, &dest_chain)
            .expect("Wrapped asset not found for this chain");
        if !wrapped.active {
            panic!("Wrapped asset is not active");
        }

        // Calculate fee
        let fee = storage::calculate_fee(&env, &dest_chain, amount);
        let bridge_amount = amount - fee;

        // Generate transaction ID
        let tx_id = storage::next_tx_id(&env);
        let timestamp = env.ledger().timestamp();

        let tx = BridgeTransaction {
            tx_id,
            direction: BridgeDirection::Outbound,
            source_chain: SupportedChain::Ethereum, // Stellar as source (placeholder for chain ID)
            dest_chain,
            sender: sender.clone(),
            recipient: sender.clone(),
            asset_code: asset_code.clone(),
            amount: bridge_amount,
            fee,
            status: BridgeStatus::Pending,
            source_tx_hash: source_tx_hash.clone(),
            dest_tx_hash: None,
            nonce: tx_id,
            timestamp,
            validator_approvals: 0,
            merkle_root: None,
        };

        storage::store_transaction(&env, &tx);

        // Update locked assets count
        let mut wrapped = wrapped;
        wrapped.total_locked += bridge_amount;
        storage::set_wrapped_asset(&env, &wrapped);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "assets_locked"),),
            (
                tx_id,
                sender,
                dest_chain,
                asset_code,
                amount,
                fee,
                source_tx_hash,
            ),
        );

        tx_id
    }

    // =========================================================================
    // Bridge Operations: Validate (Validator Approval)
    // =========================================================================

    /// Validator approves a bridge transaction
    pub fn approve_transaction(
        env: Env,
        validator: Address,
        tx_id: u64,
        chain: SupportedChain,
        signature_data: String,
    ) -> u32 {
        validator.require_auth();
        storage::require_not_paused(&env);

        let mut tx = storage::get_transaction(&env, tx_id).expect("Transaction not found");

        assert!(
            tx.status == BridgeStatus::Pending,
            "Transaction not in pending state"
        );
        assert!(
            tx.dest_chain == chain || tx.source_chain == chain,
            "Chain mismatch"
        );

        // Verify validator is in the set
        let vs =
            storage::get_validator_set(&env, &chain).expect("Validator set not found for chain");
        assert!(vs.active, "Validator set is not active");

        let mut is_validator = false;
        for v in vs.validators.iter() {
            if v == validator {
                is_validator = true;
                break;
            }
        }
        assert!(is_validator, "Validator not in set");

        // Check validator hasn't already approved
        if tx.validator_approvals > 0 {
            // Check for duplicate approval
            let existing = storage::get_validator_signature(&env, tx_id, tx.validator_approvals);
            if let Some(sig) = existing {
                if sig.validator == validator {
                    panic!("Validator already approved");
                }
            }
        }

        // Record signature
        let sig = ValidatorSignature {
            validator: validator.clone(),
            tx_id,
            approved: true,
            timestamp: env.ledger().timestamp(),
            signature_data,
        };

        // Store signature with index matching approval count
        env.storage().instance().set(
            &(Symbol::new(&env, "vsig"), tx_id, tx.validator_approvals),
            &sig,
        );

        tx.validator_approvals += 1;

        // Check if we have enough approvals
        if tx.validator_approvals >= vs.required_approvals {
            tx.status = BridgeStatus::Validated;
        }

        storage::set_transaction(&env, &tx);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "tx_approved"),),
            (
                tx_id,
                validator,
                tx.validator_approvals,
                vs.required_approvals,
            ),
        );

        tx.validator_approvals
    }

    // =========================================================================
    // Bridge Operations: Complete (Destination Chain)
    // =========================================================================

    /// Complete a validated bridge transaction (mint wrapped assets)
    pub fn complete_transaction(env: Env, admin: Address, tx_id: u64, dest_tx_hash: String) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let mut tx = storage::get_transaction(&env, tx_id).expect("Transaction not found");

        assert!(
            tx.status == BridgeStatus::Validated,
            "Transaction must be validated first"
        );

        // Mint wrapped tokens
        let mut wrapped = storage::get_wrapped_asset(&env, &tx.asset_code, &tx.dest_chain)
            .expect("Wrapped asset not found");
        wrapped.total_supply += tx.amount;
        storage::set_wrapped_asset(&env, &wrapped);

        tx.status = BridgeStatus::Completed;
        tx.dest_tx_hash = Some(dest_tx_hash.clone());
        storage::set_transaction(&env, &tx);

        env.events().publish(
            (Symbol::new(&env, "tx_completed"),),
            (tx_id, tx.amount, tx.asset_code, tx.dest_chain, dest_tx_hash),
        );
    }

    // =========================================================================
    // Bridge Operations: Mint (Inbound)
    // =========================================================================

    /// Mint assets on Stellar from inbound bridge
    pub fn mint_assets(
        env: Env,
        sender: Address,
        source_chain: SupportedChain,
        asset_code: String,
        amount: i128,
        source_tx_hash: String,
        merkle_root: String,
    ) -> u64 {
        sender.require_auth();
        storage::require_not_paused(&env);

        let config = storage::get_config(&env);

        assert!(
            storage::is_chain_supported(&env, &source_chain),
            "Chain not supported"
        );
        assert!(amount >= config.min_bridge_amount, "Amount below minimum");

        if storage::is_duplicate_tx(&env, &source_tx_hash) {
            panic!("Double spend detected");
        }

        let wrapped = storage::get_wrapped_asset(&env, &asset_code, &source_chain)
            .expect("Wrapped asset not found");
        if !wrapped.active {
            panic!("Wrapped asset not active");
        }

        let tx_id = storage::next_tx_id(&env);
        let timestamp = env.ledger().timestamp();

        let tx = BridgeTransaction {
            tx_id,
            direction: BridgeDirection::Inbound,
            source_chain,
            dest_chain: SupportedChain::Ethereum, // Stellar as dest (placeholder)
            sender: sender.clone(),
            recipient: sender.clone(),
            asset_code: asset_code.clone(),
            amount,
            fee: 0,
            status: BridgeStatus::Pending,
            source_tx_hash,
            dest_tx_hash: None,
            nonce: tx_id,
            timestamp,
            validator_approvals: 0,
            merkle_root: Some(merkle_root),
        };

        storage::store_transaction(&env, &tx);

        env.events().publish(
            (Symbol::new(&env, "mint_requested"),),
            (tx_id, sender, source_chain, asset_code, amount),
        );

        tx_id
    }

    // =========================================================================
    // Bridge Operations: Release (Outbound claim)
    // =========================================================================

    /// Release locked assets when wrapped tokens are burned on dest chain
    pub fn release_assets(env: Env, admin: Address, tx_id: u64) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let mut tx = storage::get_transaction(&env, tx_id).expect("Transaction not found");

        assert!(
            tx.status == BridgeStatus::Validated,
            "Transaction must be validated"
        );
        assert!(
            tx.direction == BridgeDirection::Inbound,
            "Only inbound transactions release assets"
        );

        // Update wrapped asset supply
        let mut wrapped = storage::get_wrapped_asset(&env, &tx.asset_code, &tx.source_chain)
            .expect("Wrapped asset not found");
        wrapped.total_supply = wrapped.total_supply.saturating_sub(tx.amount);
        wrapped.total_locked = wrapped.total_locked.saturating_sub(tx.amount);
        storage::set_wrapped_asset(&env, &wrapped);

        tx.status = BridgeStatus::Completed;
        storage::set_transaction(&env, &tx);

        env.events().publish(
            (Symbol::new(&env, "assets_released"),),
            (tx_id, tx.amount, tx.asset_code, tx.sender),
        );
    }

    // =========================================================================
    // Liquidity Pools
    // =========================================================================

    /// Create a liquidity pool (admin only)
    pub fn create_liquidity_pool(
        env: Env,
        admin: Address,
        asset_code: String,
        chain: SupportedChain,
    ) -> u64 {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let pool_id = storage::next_lp_id(&env);

        let pool = LiquidityPool {
            pool_id,
            asset_code,
            chain,
            total_liquidity: 0,
            total_volume: 0,
            lp_token_balance: 0,
            active: true,
        };

        storage::set_liquidity_pool(&env, &pool);

        env.events()
            .publish((Symbol::new(&env, "pool_created"),), (pool_id,));

        pool_id
    }

    /// Add liquidity to a pool
    pub fn add_liquidity(env: Env, provider: Address, pool_id: u64, amount: i128) -> i128 {
        provider.require_auth();
        assert!(amount > 0, "Amount must be positive");

        let mut pool = storage::get_liquidity_pool(&env, pool_id).expect("Pool not found");
        assert!(pool.active, "Pool not active");

        // Calculate LP tokens: proportional to existing supply
        let lp_tokens = if pool.total_liquidity == 0 {
            amount // 1:1 for first deposit
        } else {
            (amount * pool.lp_token_balance) / pool.total_liquidity
        };

        pool.total_liquidity += amount;
        pool.lp_token_balance += lp_tokens;
        storage::set_liquidity_pool(&env, &pool);

        // Record deposit
        let deposit = LiquidityProviderDeposit {
            provider: provider.clone(),
            pool_id,
            amount,
            lp_tokens,
            timestamp: env.ledger().timestamp(),
        };
        storage::set_lp_deposit(&env, &deposit);

        env.events().publish(
            (Symbol::new(&env, "liquidity_added"),),
            (provider, pool_id, amount, lp_tokens),
        );

        lp_tokens
    }

    /// Remove liquidity from a pool
    pub fn remove_liquidity(env: Env, provider: Address, pool_id: u64, lp_amount: i128) -> i128 {
        provider.require_auth();
        assert!(lp_amount > 0, "LP amount must be positive");

        let mut pool = storage::get_liquidity_pool(&env, pool_id).expect("Pool not found");
        assert!(pool.active, "Pool not active");
        assert!(lp_amount <= pool.lp_token_balance, "Insufficient LP tokens");

        let withdraw_amount = (lp_amount * pool.total_liquidity) / pool.lp_token_balance;

        pool.total_liquidity -= withdraw_amount;
        pool.lp_token_balance -= lp_amount;
        storage::set_liquidity_pool(&env, &pool);

        env.events().publish(
            (Symbol::new(&env, "liquidity_removed"),),
            (provider, pool_id, withdraw_amount, lp_amount),
        );

        withdraw_amount
    }

    /// Get liquidity pool info
    pub fn get_liquidity_pool(env: Env, pool_id: u64) -> Option<LiquidityPool> {
        storage::get_liquidity_pool(&env, pool_id)
    }

    // =========================================================================
    // Emergency Controls
    // =========================================================================

    /// Pause the bridge (admin only)
    pub fn pause_bridge(env: Env, admin: Address, reason: String, unpause_after: Option<u64>) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let emergency = EmergencyConfig {
            paused: true,
            paused_by: Some(admin.clone()),
            paused_at: Some(env.ledger().timestamp()),
            unpause_after,
            reason: Some(reason.clone()),
        };

        storage::set_emergency(&env, &emergency);

        env.events().publish(
            (Symbol::new(&env, "bridge_paused"),),
            (admin, reason, unpause_after.unwrap_or(0)),
        );
    }

    /// Unpause the bridge (admin only)
    pub fn unpause_bridge(env: Env, admin: Address) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let emergency = EmergencyConfig {
            paused: false,
            paused_by: None,
            paused_at: None,
            unpause_after: None,
            reason: None,
        };

        storage::set_emergency(&env, &emergency);

        env.events()
            .publish((Symbol::new(&env, "bridge_unpaused"),), (admin,));
    }

    /// Get emergency status
    pub fn get_emergency_status(env: Env) -> EmergencyConfig {
        storage::get_emergency(&env)
    }

    // =========================================================================
    // Dispute Resolution
    // =========================================================================

    /// File a dispute for a bridge transaction
    pub fn file_dispute(env: Env, disputer: Address, tx_id: u64, reason: String) -> u64 {
        disputer.require_auth();

        assert!(!reason.is_empty(), "Reason required");
        assert!(
            reason.len() <= MAX_DISPUTE_REASON_LENGTH as u32,
            "Reason too long"
        );

        let tx = storage::get_transaction(&env, tx_id).expect("Transaction not found");

        assert!(
            tx.status == BridgeStatus::Pending || tx.status == BridgeStatus::Validated,
            "Transaction cannot be disputed in current state"
        );

        let dispute_id = storage::next_dispute_id(&env);
        let dispute = BridgeDispute {
            dispute_id,
            tx_id,
            disputer: disputer.clone(),
            reason: reason.clone(),
            resolved: false,
            resolution: None,
            created_at: env.ledger().timestamp(),
            resolved_at: None,
        };

        storage::set_dispute(&env, &dispute);

        // Mark transaction as disputed
        let mut tx = tx;
        tx.status = BridgeStatus::Disputed;
        storage::set_transaction(&env, &tx);

        env.events().publish(
            (Symbol::new(&env, "dispute_filed"),),
            (dispute_id, tx_id, disputer, reason),
        );

        dispute_id
    }

    /// Resolve a dispute (admin only)
    pub fn resolve_dispute(env: Env, admin: Address, dispute_id: u64, resolution: String) {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            panic!("Unauthorized: not admin");
        }

        let mut dispute = storage::get_dispute(&env, dispute_id).expect("Dispute not found");

        assert!(!dispute.resolved, "Dispute already resolved");

        dispute.resolved = true;
        dispute.resolution = Some(resolution.clone());
        dispute.resolved_at = Some(env.ledger().timestamp());
        storage::set_dispute(&env, &dispute);

        // If resolution is "cancel", cancel the transaction
        if resolution == String::from_str(&env, "cancel") {
            if let Some(mut tx) = storage::get_transaction(&env, dispute.tx_id) {
                tx.status = BridgeStatus::Cancelled;
                storage::set_transaction(&env, &tx);
            }
        }

        env.events().publish(
            (Symbol::new(&env, "dispute_resolved"),),
            (dispute_id, resolution),
        );
    }

    /// Get dispute info
    pub fn get_dispute(env: Env, dispute_id: u64) -> Option<BridgeDispute> {
        storage::get_dispute(&env, dispute_id)
    }

    // =========================================================================
    // Transaction Queries
    // =========================================================================

    /// Get a bridge transaction by ID
    pub fn get_transaction(env: Env, tx_id: u64) -> Option<BridgeTransaction> {
        storage::get_transaction(&env, tx_id)
    }

    /// Check if a source tx hash has already been used (double-spend check)
    pub fn check_duplicate(env: Env, source_tx_hash: String) -> bool {
        storage::is_duplicate_tx(&env, &source_tx_hash)
    }

    /// Get the count of completed transactions
    pub fn get_tx_count(env: Env) -> u64 {
        storage::get_config(&env).tx_counter
    }

    /// Cancel a pending transaction (sender or admin)
    pub fn cancel_transaction(env: Env, caller: Address, tx_id: u64) {
        caller.require_auth();

        let mut tx = storage::get_transaction(&env, tx_id).expect("Transaction not found");

        let config = storage::get_config(&env);

        // Only sender or admin can cancel
        if caller != tx.sender && caller != config.admin {
            panic!("Unauthorized: not sender or admin");
        }

        assert!(
            tx.status == BridgeStatus::Pending,
            "Can only cancel pending transactions"
        );

        tx.status = BridgeStatus::Cancelled;
        storage::set_transaction(&env, &tx);

        // Return locked amount
        if tx.direction == BridgeDirection::Outbound {
            let mut wrapped = storage::get_wrapped_asset(&env, &tx.asset_code, &tx.dest_chain)
                .expect("Wrapped asset not found");
            wrapped.total_locked = wrapped.total_locked.saturating_sub(tx.amount);
            storage::set_wrapped_asset(&env, &wrapped);
        }

        env.events()
            .publish((Symbol::new(&env, "tx_cancelled"),), (tx_id, caller));
    }
}
