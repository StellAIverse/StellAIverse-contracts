use soroban_sdk::{contracttype, Env, String, Vec};

use shared::{
    BridgeConfig, BridgeDispute, BridgeFeeTier, BridgeTransaction, EmergencyConfig, LiquidityPool,
    LiquidityProviderDeposit, SupportedChain, ValidatorSetConfig, ValidatorSignature, WrappedAsset,
    DEFAULT_FEE_BPS, DEFAULT_TIMELOCK_DURATION, MAX_BRIDGE_AMOUNT, MIN_BRIDGE_AMOUNT,
};

// ============================================================================
// Storage Keys
// ============================================================================

#[derive(Clone)]
#[contracttype]
pub enum StorageKey {
    Config,
    Tx(u64),
    TxHash(String),
    ValidatorSig(u64, u32), // (tx_id, validator_index)
    ValidatorSet(SupportedChain),
    WrappedAsset(String, SupportedChain), // (asset_code, chain)
    FeeTier(SupportedChain, u32),         // (chain, tier_index)
    LiquidityPool(u64),
    LpDeposit(u64, u32), // (pool_id, deposit_index)
    Emergency,
    Dispute(u64),
    SupportedChains,
}

// ============================================================================
// Initialization
// ============================================================================

pub fn initialize_bridge(env: &Env, admin: &Address) -> BridgeConfig {
    let config = BridgeConfig {
        admin: admin.clone(),
        paused: false,
        timelock_duration: DEFAULT_TIMELOCK_DURATION,
        min_bridge_amount: MIN_BRIDGE_AMOUNT,
        max_bridge_amount: MAX_BRIDGE_AMOUNT,
        default_fee_bps: DEFAULT_FEE_BPS,
        tx_counter: 0,
        dispute_counter: 0,
        lp_counter: 0,
    };

    env.storage().instance().set(&StorageKey::Config, &config);

    // Initialize emergency config
    let emergency = EmergencyConfig {
        paused: false,
        paused_by: None,
        paused_at: None,
        unpause_after: None,
        reason: None,
    };
    env.storage()
        .instance()
        .set(&StorageKey::Emergency, &emergency);

    // Initialize supported chains list
    let chains: Vec<SupportedChain> = Vec::new(env);
    env.storage()
        .instance()
        .set(&StorageKey::SupportedChains, &chains);

    config
}

// ============================================================================
// Config Operations
// ============================================================================

pub fn get_config(env: &Env) -> BridgeConfig {
    env.storage()
        .instance()
        .get(&StorageKey::Config)
        .expect("Bridge not initialized")
}

pub fn set_config(env: &Env, config: &BridgeConfig) {
    env.storage().instance().set(&StorageKey::Config, config);
}

// ============================================================================
// Emergency Operations
// ============================================================================

pub fn get_emergency(env: &Env) -> EmergencyConfig {
    env.storage()
        .instance()
        .get(&StorageKey::Emergency)
        .unwrap_or(EmergencyConfig {
            paused: false,
            paused_by: None,
            paused_at: None,
            unpause_after: None,
            reason: None,
        })
}

pub fn set_emergency(env: &Env, emergency: &EmergencyConfig) {
    env.storage()
        .instance()
        .set(&StorageKey::Emergency, emergency);
}

pub fn require_not_paused(env: &Env) {
    let emergency = get_emergency(env);
    if emergency.paused {
        // Check if timelock has expired
        if let Some(unpause_after) = emergency.unpause_after {
            if env.ledger().timestamp() < unpause_after {
                panic!("Bridge is paused");
            }
            // Timelock expired, auto-unpause
            let mut new_emergency = emergency;
            new_emergency.paused = false;
            new_emergency.unpause_after = None;
            set_emergency(env, &new_emergency);
            return;
        }
        panic!("Bridge is paused");
    }
}

// ============================================================================
// Chain Support
// ============================================================================

pub fn is_chain_supported(env: &Env, chain: &SupportedChain) -> bool {
    let chains: Vec<SupportedChain> = env
        .storage()
        .instance()
        .get(&StorageKey::SupportedChains)
        .unwrap_or(Vec::new(env));

    for c in chains.iter() {
        if c == *chain {
            return true;
        }
    }
    false
}

pub fn add_supported_chain(env: &Env, chain: &SupportedChain) {
    let mut chains: Vec<SupportedChain> = env
        .storage()
        .instance()
        .get(&StorageKey::SupportedChains)
        .unwrap_or(Vec::new(env));

    if !is_chain_supported(env, chain) {
        chains.push_back(chain.clone());
        env.storage()
            .instance()
            .set(&StorageKey::SupportedChains, &chains);
    }
}

pub fn remove_supported_chain(env: &Env, chain: &SupportedChain) {
    let chains: Vec<SupportedChain> = env
        .storage()
        .instance()
        .get(&StorageKey::SupportedChains)
        .unwrap_or(Vec::new(env));

    let mut updated = Vec::new(env);
    for c in chains.iter() {
        if c != *chain {
            updated.push_back(c.clone());
        }
    }
    env.storage()
        .instance()
        .set(&StorageKey::SupportedChains, &updated);
}

// ============================================================================
// Validator Set Operations
// ============================================================================

pub fn get_validator_set(env: &Env, chain: &SupportedChain) -> Option<ValidatorSetConfig> {
    env.storage()
        .instance()
        .get(&StorageKey::ValidatorSet(*chain))
}

pub fn set_validator_set(env: &Env, config: &ValidatorSetConfig) {
    env.storage()
        .instance()
        .set(&StorageKey::ValidatorSet(config.chain), config);
}

// ============================================================================
// Transaction Operations
// ============================================================================

pub fn next_tx_id(env: &Env) -> u64 {
    let mut config = get_config(env);
    config.tx_counter += 1;
    set_config(env, &config);
    config.tx_counter
}

pub fn store_transaction(env: &Env, tx: &BridgeTransaction) {
    env.storage().instance().set(&StorageKey::Tx(tx.tx_id), tx);

    // Store hash -> tx_id mapping for duplicate detection
    env.storage()
        .instance()
        .set(&StorageKey::TxHash(tx.source_tx_hash.clone()), &tx.tx_id);
}

pub fn get_transaction(env: &Env, tx_id: u64) -> Option<BridgeTransaction> {
    env.storage().instance().get(&StorageKey::Tx(tx_id))
}

pub fn set_transaction(env: &Env, tx: &BridgeTransaction) {
    env.storage().instance().set(&StorageKey::Tx(tx.tx_id), tx);
}

pub fn is_duplicate_tx(env: &Env, source_tx_hash: &String) -> bool {
    env.storage()
        .instance()
        .get::<_, u64>(&StorageKey::TxHash(source_tx_hash.clone()))
        .is_some()
}

// ============================================================================
// Validator Signature Operations
// ============================================================================

pub fn get_validator_signature(
    env: &Env,
    tx_id: u64,
    validator_index: u32,
) -> Option<ValidatorSignature> {
    env.storage()
        .instance()
        .get(&StorageKey::ValidatorSig(tx_id, validator_index))
}

// ============================================================================
// Wrapped Asset Operations
// ============================================================================

pub fn get_wrapped_asset(
    env: &Env,
    asset_code: &String,
    chain: &SupportedChain,
) -> Option<WrappedAsset> {
    env.storage()
        .instance()
        .get(&StorageKey::WrappedAsset(asset_code.clone(), *chain))
}

pub fn set_wrapped_asset(env: &Env, asset: &WrappedAsset) {
    env.storage().instance().set(
        &StorageKey::WrappedAsset(asset.asset_code.clone(), asset.chain),
        asset,
    );
}

// ============================================================================
// Fee Tier Operations
// ============================================================================

pub fn get_fee_tier(env: &Env, chain: &SupportedChain, tier_index: u32) -> Option<BridgeFeeTier> {
    env.storage()
        .instance()
        .get(&StorageKey::FeeTier(*chain, tier_index))
}

pub fn set_fee_tier_at(env: &Env, chain: &SupportedChain, index: u32, tier: &BridgeFeeTier) {
    env.storage()
        .instance()
        .set(&StorageKey::FeeTier(*chain, index), tier);
}

pub fn calculate_fee(env: &Env, chain: &SupportedChain, amount: i128) -> i128 {
    let mut fee_bps = get_config(env).default_fee_bps;

    // Find matching fee tier
    let mut index: u32 = 0;
    while let Some(tier) = get_fee_tier(env, chain, index) {
        if tier.active && amount >= tier.min_amount && amount <= tier.max_amount {
            fee_bps = tier.fee_bps;
            break;
        }
        index += 1;
    }

    amount * fee_bps as i128 / 10_000
}

// ============================================================================
// Liquidity Pool Operations
// ============================================================================

pub fn get_liquidity_pool(env: &Env, pool_id: u64) -> Option<LiquidityPool> {
    env.storage()
        .instance()
        .get(&StorageKey::LiquidityPool(pool_id))
}

pub fn set_liquidity_pool(env: &Env, pool: &LiquidityPool) {
    env.storage()
        .instance()
        .set(&StorageKey::LiquidityPool(pool.pool_id), pool);
}

pub fn next_lp_id(env: &Env) -> u64 {
    let mut config = get_config(env);
    config.lp_counter += 1;
    set_config(env, &config);
    config.lp_counter
}

pub fn get_lp_deposit(
    env: &Env,
    pool_id: u64,
    deposit_index: u32,
) -> Option<LiquidityProviderDeposit> {
    env.storage()
        .instance()
        .get(&StorageKey::LpDeposit(pool_id, deposit_index))
}

pub fn set_lp_deposit(env: &Env, deposit: &LiquidityProviderDeposit) {
    let mut index: u32 = 0;
    while get_lp_deposit(env, deposit.pool_id, index).is_some() {
        index += 1;
    }
    env.storage()
        .instance()
        .set(&StorageKey::LpDeposit(deposit.pool_id, index), deposit);
}

// ============================================================================
// Dispute Operations
// ============================================================================

pub fn get_dispute(env: &Env, dispute_id: u64) -> Option<BridgeDispute> {
    env.storage()
        .instance()
        .get(&StorageKey::Dispute(dispute_id))
}

pub fn set_dispute(env: &Env, dispute: &BridgeDispute) {
    env.storage()
        .instance()
        .set(&StorageKey::Dispute(dispute.dispute_id), dispute);
}

pub fn next_dispute_id(env: &Env) -> u64 {
    let mut config = get_config(env);
    config.dispute_counter += 1;
    set_config(env, &config);
    config.dispute_counter
}
