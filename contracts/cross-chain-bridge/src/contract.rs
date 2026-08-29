use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, Map, String, Symbol, Vec};

use crate::errors::BridgeError;
use crate::storage_keys::*;
use crate::token::TokenClient;
use crate::types::*;

#[contract]
pub struct CrossChainBridge;

#[contractimpl]
impl CrossChainBridge {
    /// Initialize the bridge contract
    pub fn initialize(
        env: Env,
        admin: Address,
        chain_id: ChainID,
        signature_config: SignatureConfig,
        fee_config: FeeConfig,
        rate_limit_config: RateLimitConfig,
    ) -> Result<(), BridgeError> {
        // Check if already initialized
        if env
            .storage()
            .instance()
            .has(&Symbol::new(&env, INITIALIZED_KEY))
        {
            return Err(BridgeError::AlreadyInitialized);
        }

        // Validate configurations
        if signature_config.quorum_percentage < 51 || signature_config.quorum_percentage > 100 {
            return Err(BridgeError::InvalidArgument);
        }
        if fee_config.basis_points > 1000 {
            // Max 10% fee
            return Err(BridgeError::InvalidFeeConfiguration);
        }

        // Store initial state
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, CHAIN_ID_KEY), &chain_id);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, SIGNATURE_CONFIG_KEY), &signature_config);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, FEE_CONFIG_KEY), &fee_config);
        env.storage().instance().set(
            &Symbol::new(&env, RATE_LIMIT_CONFIG_KEY),
            &rate_limit_config,
        );

        // Initialize rate limit state
        let current_time = env.ledger().timestamp();
        let rate_state = RateLimitState {
            daily_used: 0,
            monthly_used: 0,
            last_daily_reset: current_time,
            last_monthly_reset: current_time,
            per_user_daily: Map::new(&env),
        };
        env.storage()
            .instance()
            .set(&Symbol::new(&env, RATE_LIMIT_STATE_KEY), &rate_state);

        // Initialize counters
        env.storage()
            .instance()
            .set(&Symbol::new(&env, TRANSFER_COUNTER_KEY), &0u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, VALIDATOR_COUNT_KEY), &0u32);
        env.storage().instance().set(
            &Symbol::new(&env, VALIDATOR_LIST_KEY),
            &Vec::<Address>::new(&env),
        );
        env.storage()
            .instance()
            .set(&Symbol::new(&env, TOKEN_COUNT_KEY), &0u32);

        // Initialize total fees
        env.storage()
            .instance()
            .set(&Symbol::new(&env, TOTAL_FEES_KEY), &0i128);

        // Set initialized flag
        env.storage()
            .instance()
            .set(&Symbol::new(&env, INITIALIZED_KEY), &true);
        // Start unpaused
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSED_KEY), &false);

        Ok(())
    }

    /// Add a new validator to the bridge
    pub fn add_validator(
        env: Env,
        validator_address: Address,
        public_key: Bytes,
        power: u32,
    ) -> Result<(), BridgeError> {
        // Authorization check
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .ok_or(BridgeError::Unauthorized)?;
        admin.require_auth();

        // Check if not paused
        Self::ensure_not_paused(&env)?;

        // Check if validator already exists
        let val_key = validator_key(&env, &validator_address);
        if env.storage().instance().has(&val_key) {
            return Err(BridgeError::ValidatorAlreadyExists);
        }

        // Create validator
        let validator = Validator {
            address: validator_address.clone(),
            public_key,
            is_active: true,
            power,
            joined_at: env.ledger().timestamp(),
        };

        // Store validator
        env.storage().instance().set(&val_key, &validator);

        let mut validators: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, VALIDATOR_LIST_KEY))
            .unwrap_or_else(|| Vec::new(&env));
        validators.push_back(validator_address.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, VALIDATOR_LIST_KEY), &validators);

        // Update validator count
        let mut count: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, VALIDATOR_COUNT_KEY))
            .unwrap_or(0);
        count += 1;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, VALIDATOR_COUNT_KEY), &count);

        // Update signature config
        let mut sig_config: SignatureConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, SIGNATURE_CONFIG_KEY))
            .ok_or(BridgeError::InvalidArgument)?;
        sig_config.total_validators = count;
        let required = ((count as u64 * sig_config.quorum_percentage as u64) / 100) as u32;
        sig_config.required_signatures = core::cmp::max(required, 1);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, SIGNATURE_CONFIG_KEY), &sig_config);

        Ok(())
    }

    /// Deactivate a validator without changing historical signatures.
    pub fn remove_validator(env: Env, validator_address: Address) -> Result<(), BridgeError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .ok_or(BridgeError::Unauthorized)?;
        admin.require_auth();
        Self::ensure_not_paused(&env)?;

        let key = validator_key(&env, &validator_address);
        let mut validator: Validator = env
            .storage()
            .instance()
            .get(&key)
            .ok_or(BridgeError::ValidatorNotFound)?;
        if !validator.is_active {
            return Err(BridgeError::ValidatorAlreadyRemoved);
        }
        validator.is_active = false;
        env.storage().instance().set(&key, &validator);

        let mut sig_config: SignatureConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, SIGNATURE_CONFIG_KEY))
            .ok_or(BridgeError::InvalidArgument)?;
        sig_config.total_validators = sig_config.total_validators.saturating_sub(1);
        let required =
            (sig_config.total_validators as u64 * sig_config.quorum_percentage as u64 / 100) as u32;
        sig_config.required_signatures = core::cmp::max(required, 1);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, SIGNATURE_CONFIG_KEY), &sig_config);
        env.events()
            .publish((Symbol::new(&env, "validator_removed"),), validator_address);
        Ok(())
    }

    /// Add a supported token to the bridge
    pub fn add_supported_token(
        env: Env,
        token_address: Address,
        symbol: String,
        decimals: u32,
        is_mintable: bool,
        is_locked: bool,
        bridge_addresses: Map<ChainID, Bytes>,
    ) -> Result<(), BridgeError> {
        // Authorization check
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .ok_or(BridgeError::Unauthorized)?;
        admin.require_auth();

        Self::ensure_not_paused(&env)?;

        // Check if token already exists
        let token_key = token_key(&env, &token_address);
        if env.storage().instance().has(&token_key) {
            return Err(BridgeError::InvalidArgument);
        }

        let token = SupportedToken {
            token_address: token_address.clone(),
            symbol,
            decimals,
            is_mintable,
            is_locked,
            bridge_address_on_other_chains: bridge_addresses,
        };

        // Store token
        env.storage().instance().set(&token_key, &token);

        // Update token count
        let mut count: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, TOKEN_COUNT_KEY))
            .unwrap_or(0);
        count += 1;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, TOKEN_COUNT_KEY), &count);

        Ok(())
    }

    /// Initiate a cross-chain token transfer (lock or burn tokens on source chain)
    pub fn initiate_transfer(
        env: Env,
        destination_chain: ChainID,
        recipient: Bytes,
        token_address: Address,
        amount: i128,
        nonce: u64,
        sender: Address,
    ) -> Result<u64, BridgeError> {
        Self::ensure_not_paused(&env)?;

        // Validate sender is authenticated
        sender.require_auth();

        if nonce == 0 {
            return Err(BridgeError::InvalidNonce);
        }

        // Check if nonce has been used
        let nonce_key = nonce_key(&env, &sender, nonce);
        if env.storage().instance().has(&nonce_key) {
            return Err(BridgeError::NonceAlreadyUsed);
        }

        // Validate token is supported
        let token: SupportedToken = env
            .storage()
            .instance()
            .get(&token_key(&env, &token_address))
            .ok_or(BridgeError::TokenNotSupported)?;

        // Get current chain ID
        let source_chain: ChainID = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, CHAIN_ID_KEY))
            .ok_or(BridgeError::InvalidArgument)?;

        if source_chain == destination_chain {
            return Err(BridgeError::InvalidChainPair);
        }

        // Validate amount
        if amount <= 0 {
            return Err(BridgeError::InvalidAmount);
        }

        // Check rate limits
        Self::check_rate_limits(&env, &sender, amount)?;

        // Calculate fee
        let fee_config: FeeConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, FEE_CONFIG_KEY))
            .ok_or(BridgeError::InvalidFeeConfiguration)?;
        let fee = core::cmp::max(
            (amount * fee_config.basis_points as i128) / 10000,
            fee_config.min_fee,
        );

        let total_amount = amount + fee;

        // Lock or burn tokens based on token configuration
        if token.is_locked {
            // Lock mechanism: transfer tokens from sender to bridge contract
            let token_client = TokenClient::new(&env, &token_address);
            token_client.transfer(&sender, &env.current_contract_address(), &total_amount);
        } else if token.is_mintable {
            // Burn mechanism: burn tokens from sender
            let token_client = TokenClient::new(&env, &token_address);
            token_client.burn(&sender, &total_amount);
        } else {
            return Err(BridgeError::InvalidArgument);
        }

        // Mark nonce as used
        env.storage().instance().set(&nonce_key, &true);

        // Update fee tracking
        let mut total_fees: i128 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, TOTAL_FEES_KEY))
            .unwrap_or(0);
        total_fees += fee;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, TOTAL_FEES_KEY), &total_fees);

        // Create transfer record
        let mut transfer_counter: u64 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, TRANSFER_COUNTER_KEY))
            .unwrap_or(0);
        transfer_counter += 1;

        let transfer = BridgeTransfer {
            transfer_id: transfer_counter,
            source_chain,
            destination_chain,
            sender,
            recipient,
            token_address: token_address.clone(),
            amount,
            fee,
            nonce,
            timestamp: env.ledger().timestamp(),
            status: TransactionStatus::Locked,
            direction: if token.is_locked {
                TransferDirection::LockAndMint
            } else {
                TransferDirection::BurnAndUnlock
            },
            signatures: Vec::new(&env),
        };

        // Store transfer
        env.storage()
            .instance()
            .set(&transfer_key(&env, transfer_counter), &transfer);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, TRANSFER_COUNTER_KEY), &transfer_counter);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "transfer_initiated"), transfer_counter),
            (source_chain, destination_chain, token_address, amount, fee),
        );

        Ok(transfer_counter)
    }

    /// Complete a transfer by minting/unlocking tokens on destination chain
    pub fn complete_transfer(
        env: Env,
        transfer_id: u64,
        signatures: Vec<Bytes>,
    ) -> Result<(), BridgeError> {
        Self::ensure_not_paused(&env)?;

        // Get transfer
        let transfer_key = transfer_key(&env, transfer_id);
        let mut transfer: BridgeTransfer = env
            .storage()
            .instance()
            .get(&transfer_key)
            .ok_or(BridgeError::TransferNotFound)?;

        // Validate transfer can be completed
        if transfer.status != TransactionStatus::Locked
            && transfer.status != TransactionStatus::Pending
        {
            return Err(BridgeError::InvalidTransferStatus);
        }

        if transfer.status != TransactionStatus::Locked {
            return Err(BridgeError::TransferAlreadyProcessed);
        }

        // Verify signatures
        Self::verify_transfer_signatures(&env, &transfer, &signatures)?;

        // Get current chain ID (must be destination chain)
        let current_chain: ChainID = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, CHAIN_ID_KEY))
            .ok_or(BridgeError::InvalidArgument)?;

        if current_chain != transfer.destination_chain {
            return Err(BridgeError::InvalidChainPair);
        }

        // Get token
        let token: SupportedToken = env
            .storage()
            .instance()
            .get(&token_key(&env, &transfer.token_address))
            .ok_or(BridgeError::TokenNotSupported)?;

        // Decode recipient address
        let recipient = Address::from_string_bytes(&transfer.recipient);

        // Mint or unlock tokens
        if token.is_mintable {
            // Mint tokens to recipient
            let token_client = TokenClient::new(&env, &transfer.token_address);
            token_client.mint(&recipient, &transfer.amount);
            transfer.status = TransactionStatus::Minted;
        } else if token.is_locked {
            // Unlock tokens from bridge to recipient
            let token_client = TokenClient::new(&env, &transfer.token_address);
            token_client.transfer(
                &env.current_contract_address(),
                &recipient,
                &transfer.amount,
            );
            transfer.status = TransactionStatus::Unlocked;
        } else {
            return Err(BridgeError::InvalidArgument);
        }

        // Update transfer with signatures and new status
        transfer.signatures = signatures;
        env.storage().instance().set(&transfer_key, &transfer);

        // Emit completion event
        env.events().publish(
            (Symbol::new(&env, "transfer_completed"), transfer_id),
            (recipient, transfer.amount, transfer.status),
        );

        Ok(())
    }

    /// Emergency pause the bridge
    pub fn pause_bridge(env: Env) -> Result<(), BridgeError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .ok_or(BridgeError::Unauthorized)?;
        admin.require_auth();

        let mut paused: bool = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, PAUSED_KEY))
            .unwrap_or(false);

        if paused {
            return Err(BridgeError::AlreadyPaused);
        }

        paused = true;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSED_KEY), &paused);

        env.events()
            .publish((Symbol::new(&env, "bridge_paused"),), ());

        Ok(())
    }

    /// Unpause the bridge
    pub fn unpause_bridge(env: Env) -> Result<(), BridgeError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .ok_or(BridgeError::Unauthorized)?;
        admin.require_auth();

        let mut paused: bool = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, PAUSED_KEY))
            .unwrap_or(false);

        if !paused {
            return Err(BridgeError::AlreadyUnpaused);
        }

        paused = false;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSED_KEY), &paused);

        env.events()
            .publish((Symbol::new(&env, "bridge_unpaused"),), ());

        Ok(())
    }

    /// Get transfer status
    pub fn get_transfer_status(
        env: Env,
        transfer_id: u64,
    ) -> Result<TransactionStatus, BridgeError> {
        let transfer: BridgeTransfer = env
            .storage()
            .instance()
            .get(&transfer_key(&env, transfer_id))
            .ok_or(BridgeError::TransferNotFound)?;

        Ok(transfer.status)
    }

    /// Get transfer details
    pub fn get_transfer(env: Env, transfer_id: u64) -> Result<BridgeTransfer, BridgeError> {
        let transfer: BridgeTransfer = env
            .storage()
            .instance()
            .get(&transfer_key(&env, transfer_id))
            .ok_or(BridgeError::TransferNotFound)?;

        Ok(transfer)
    }

    // Internal helper functions

    /// Ensure bridge is not paused
    fn ensure_not_paused(env: &Env) -> Result<(), BridgeError> {
        let paused: bool = env
            .storage()
            .instance()
            .get(&Symbol::new(env, PAUSED_KEY))
            .unwrap_or(false);

        if paused {
            return Err(BridgeError::BridgePaused);
        }

        Ok(())
    }

    /// Check and update rate limits
    fn check_rate_limits(env: &Env, user: &Address, amount: i128) -> Result<(), BridgeError> {
        let rate_config: RateLimitConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(env, RATE_LIMIT_CONFIG_KEY))
            .ok_or(BridgeError::InvalidArgument)?;

        let mut rate_state: RateLimitState = env
            .storage()
            .instance()
            .get(&Symbol::new(env, RATE_LIMIT_STATE_KEY))
            .ok_or(BridgeError::InvalidArgument)?;

        let current_time = env.ledger().timestamp();

        // Reset daily counter if 24 hours have passed
        if current_time - rate_state.last_daily_reset > 86400 {
            rate_state.daily_used = 0;
            rate_state.last_daily_reset = current_time;
            rate_state.per_user_daily = Map::new(env);
        }

        // Reset monthly counter if 30 days have passed
        if current_time - rate_state.last_monthly_reset > 2592000 {
            rate_state.monthly_used = 0;
            rate_state.last_monthly_reset = current_time;
        }

        // Check per-transaction limits
        if amount > rate_config.per_transaction_max {
            return Err(BridgeError::PerTransactionLimitExceeded);
        }
        if amount < rate_config.per_transaction_min {
            return Err(BridgeError::TransactionBelowMinimum);
        }

        // Check daily limits
        if rate_state.daily_used + amount > rate_config.daily_limit {
            return Err(BridgeError::DailyLimitExceeded);
        }

        // Check monthly limits
        if rate_state.monthly_used + amount > rate_config.monthly_limit {
            return Err(BridgeError::MonthlyLimitExceeded);
        }

        // Check user-specific daily limit
        let user_used = rate_state.per_user_daily.get(user.clone()).unwrap_or(0);
        rate_state
            .per_user_daily
            .set(user.clone(), user_used + amount);

        // Update state
        rate_state.daily_used += amount;
        rate_state.monthly_used += amount;
        env.storage()
            .instance()
            .set(&Symbol::new(env, RATE_LIMIT_STATE_KEY), &rate_state);

        Ok(())
    }

    /// Verify validator signatures for a transfer
    fn verify_transfer_signatures(
        env: &Env,
        transfer: &BridgeTransfer,
        signatures: &Vec<Bytes>,
    ) -> Result<(), BridgeError> {
        let sig_config: SignatureConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(env, SIGNATURE_CONFIG_KEY))
            .ok_or(BridgeError::InvalidArgument)?;

        // Check minimum signatures
        if signatures.len() < sig_config.required_signatures {
            return Err(BridgeError::InsufficientSignatures);
        }

        // Create message hash from transfer data
        let mut message = Bytes::new(env);
        message.append(&Bytes::from_slice(env, &transfer.transfer_id.to_be_bytes()));
        message.append(&Bytes::from_slice(
            env,
            &(transfer.source_chain as u32).to_be_bytes(),
        ));
        message.append(&Bytes::from_slice(
            env,
            &(transfer.destination_chain as u32).to_be_bytes(),
        ));
        message.append(&Bytes::from_slice(env, &transfer.amount.to_be_bytes()));

        let mut seen_validators: Vec<Address> = Vec::new(env);
        let mut valid_signatures = 0;

        // Verify each signature
        for sig_bytes in signatures.iter() {
            // Convert to Ed25519 signature
            let signature: BytesN<64> = sig_bytes
                .try_into()
                .map_err(|_| BridgeError::InvalidSignature)?;

            // Find validator that signed
            let mut found = false;
            for validator_addr in Self::get_all_validators(env) {
                if seen_validators.contains(&validator_addr) {
                    continue;
                }

                let val_key = validator_key(env, &validator_addr);
                let validator: Validator = env
                    .storage()
                    .instance()
                    .get(&val_key)
                    .ok_or(BridgeError::ValidatorNotFound)?;

                if !validator.is_active {
                    continue;
                }

                let pub_key: BytesN<32> = validator
                    .public_key
                    .try_into()
                    .map_err(|_| BridgeError::InvalidSignature)?;

                // Verify signature
                env.crypto().ed25519_verify(&pub_key, &message, &signature);
                seen_validators.push_back(validator_addr.clone());
                valid_signatures += 1;
                found = true;
                break;
            }

            if !found {
                return Err(BridgeError::InvalidSignature);
            }
        }

        if valid_signatures < sig_config.required_signatures {
            return Err(BridgeError::InsufficientSignatures);
        }

        Ok(())
    }

    /// Helper to get all active validators (simplified)
    fn get_all_validators(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Symbol::new(env, VALIDATOR_LIST_KEY))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Collect fees to fee collector
    pub fn collect_fees(env: Env, token_address: Address) -> Result<(), BridgeError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .ok_or(BridgeError::Unauthorized)?;
        admin.require_auth();

        let fee_config: FeeConfig = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, FEE_CONFIG_KEY))
            .ok_or(BridgeError::InvalidFeeConfiguration)?;

        let total_fees: i128 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, TOTAL_FEES_KEY))
            .unwrap_or(0);

        if total_fees > 0 {
            let token_client = TokenClient::new(&env, &token_address);
            token_client.transfer(
                &env.current_contract_address(),
                &fee_config.fee_collector,
                &total_fees,
            );

            // Reset fee counter
            env.storage()
                .instance()
                .set(&Symbol::new(&env, TOTAL_FEES_KEY), &0i128);
        }

        Ok(())
    }
}
