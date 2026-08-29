#![no_std]

use soroban_sdk::{contracttype, Address, Bytes, String, Vec};

// ============================================================================
// MODULE IDENTIFIERS
// ============================================================================

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ModuleId {
    AgentNft = 0,
    AgentToken = 1,
    Marketplace = 2,
    Evolution = 3,
    ExecutionHub = 4,
    Oracle = 5,
    Faucet = 6,
    Governance = 7,
    Compliance = 8,
    Staking = 9,
    Lifecycle = 10,
    Threshold = 11,
    TransactionCoord = 12,
    VerifiableCreds = 13,
    Metrics = 14,
    Prediction = 15,
    Referral = 16,
    RiskEval = 17,
    BugBounty = 18,
    Affiliate = 19,
    CreditScore = 20,
    Waitlist = 21,
    MultisigWaitlist = 22,
    BridgeManager = 23,
    Amm = 24,
}

// ============================================================================
// STORAGE KEYS
// ============================================================================

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamespacedKey {
    pub module: ModuleId,
    pub category: String,
    pub identifier: String,
}

impl NamespacedKey {
    pub fn validate(&self) -> bool {
        !self.category.is_empty()
            && !self.identifier.is_empty()
            && self.category.len() <= MAX_STRING_LENGTH as u32
            && self.identifier.len() <= MAX_STRING_LENGTH as u32
    }
}

pub fn validate_namespaced_key(key: &NamespacedKey) -> bool {
    key.validate()
}

// ============================================================================
// AGENTS
// ============================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub struct Agent {
    pub id: u64,
    pub owner: Address,
    pub name: String,
    pub model_hash: String,
    pub capabilities: Vec<String>,
    pub evolution_level: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub nonce: u64,
    pub escrow_locked: bool,
    pub escrow_holder: Option<Address>,
}

// ============================================================================
// RATE LIMITING
// ============================================================================

#[contracttype]
#[derive(Clone, Copy, Debug)]
pub struct RateLimit {
    pub window_seconds: u64,
    pub max_operations: u32,
}

// ============================================================================
// MARKETPLACE
// ============================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub struct Listing {
    pub listing_id: u64,
    pub agent_id: u64,
    pub seller: Address,
    pub price: i128,
    pub listing_type: ListingType,
    pub active: bool,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ListingType {
    Sale = 0,
    Lease = 1,
    Auction = 2,
}

// ============================================================================
// EVOLUTION
// ============================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub struct EvolutionRequest {
    pub request_id: u64,
    pub agent_id: u64,
    pub owner: Address,
    pub stake_amount: i128,
    pub status: EvolutionStatus,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum EvolutionStatus {
    Pending = 0,
    InProgress = 1,
    Completed = 2,
    Failed = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EvolutionAttestation {
    pub request_id: u64,
    pub agent_id: u64,
    pub oracle_provider: Address,
    pub new_model_hash: String,
    pub attestation_data: Bytes,
    pub signature: Bytes,
    pub timestamp: u64,
    pub nonce: u64,
}

// ============================================================================
// ORACLE
// ============================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub struct OracleData {
    pub key: String,
    pub value: String,
    pub timestamp: u64,
    pub source: String,
}

// ============================================================================
// ROYALTIES
// ============================================================================

#[contracttype]
#[derive(Clone, Debug)]
pub struct RoyaltyInfo {
    pub recipient: Address,

    /// Basis points (0–10000)
    pub percentage: u32,
}

impl RoyaltyInfo {
    pub fn is_valid(&self) -> bool {
        self.percentage <= MAX_ROYALTY_PERCENTAGE
    }
}

// ============================================================================
// SECURITY CONSTANTS
// ============================================================================

pub const MAX_STRING_LENGTH: usize = 256;
pub const MAX_CAPABILITIES: usize = 32;

pub const MAX_ROYALTY_PERCENTAGE: u32 = 10_000;
pub const MIN_ROYALTY_PERCENTAGE: u32 = 0;

pub const PRICE_UPPER_BOUND: i128 = i128::MAX / 2;
pub const PRICE_LOWER_BOUND: i128 = 0;

pub const MAX_DURATION_DAYS: u64 = 36_500;
pub const MAX_AGE_SECONDS: u64 = 365 * 24 * 60 * 60;

pub const ATTESTATION_SIGNATURE_SIZE: usize = 64;
pub const MAX_ATTESTATION_DATA_SIZE: usize = 1024;

// ============================================================================
// TEST UTILITIES
// ============================================================================

/// Supported destination chains
#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum SupportedChain {
    Ethereum = 0,
    Solana = 1,
    Polygon = 2,
    Bsc = 3,
}

/// Status of a bridge transaction
#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum BridgeStatus {
    Pending = 0,
    Validated = 1,
    Completed = 2,
    Failed = 3,
    Disputed = 4,
    Cancelled = 5,
}

/// Direction of a bridge transfer
#[derive(Clone, Copy, PartialEq, Eq)]
#[contracttype]
#[repr(u32)]
pub enum BridgeDirection {
    Outbound = 0, // Stellar -> Other
    Inbound = 1,  // Other -> Stellar
}

/// A cross-chain bridge transaction record
#[derive(Clone)]
#[contracttype]
pub struct BridgeTransaction {
    pub tx_id: u64,
    pub direction: BridgeDirection,
    pub source_chain: SupportedChain,
    pub dest_chain: SupportedChain,
    pub sender: Address,
    pub recipient: Address,
    pub asset_code: String,
    pub amount: i128,
    pub fee: i128,
    pub status: BridgeStatus,
    pub source_tx_hash: String,
    pub dest_tx_hash: Option<String>,
    pub nonce: u64,
    pub timestamp: u64,
    pub validator_approvals: u32,
    pub merkle_root: Option<String>,
}

/// Validator signature for bridge approval
#[derive(Clone)]
#[contracttype]
pub struct ValidatorSignature {
    pub validator: Address,
    pub tx_id: u64,
    pub approved: bool,
    pub timestamp: u64,
    pub signature_data: String,
}

/// Validator set configuration per chain
#[derive(Clone)]
#[contracttype]
pub struct ValidatorSetConfig {
    pub chain: SupportedChain,
    pub validators: Vec<Address>,
    pub required_approvals: u32,
    pub active: bool,
}

/// Wrapped asset info on a destination chain
#[derive(Clone)]
#[contracttype]
pub struct WrappedAsset {
    pub asset_code: String,
    pub asset_issuer: Address,
    pub wrapped_address: String,
    pub chain: SupportedChain,
    pub total_supply: i128,
    pub total_locked: i128,
    pub active: bool,
}

/// Bridge fee tier configuration
#[derive(Clone)]
#[contracttype]
pub struct BridgeFeeTier {
    pub chain: SupportedChain,
    pub min_amount: i128,
    pub max_amount: i128,
    pub fee_bps: u32, // basis points
    pub active: bool,
}

/// Liquidity pool for bridge swap operations
#[derive(Clone)]
#[contracttype]
pub struct LiquidityPool {
    pub pool_id: u64,
    pub asset_code: String,
    pub chain: SupportedChain,
    pub total_liquidity: i128,
    pub total_volume: i128,
    pub lp_token_balance: i128,
    pub active: bool,
}

/// Liquidity provider deposit record
#[derive(Clone)]
#[contracttype]
pub struct LiquidityProviderDeposit {
    pub provider: Address,
    pub pool_id: u64,
    pub amount: i128,
    pub lp_tokens: i128,
    pub timestamp: u64,
}

/// Emergency pause configuration
#[derive(Clone)]
#[contracttype]
pub struct EmergencyConfig {
    pub paused: bool,
    pub paused_by: Option<Address>,
    pub paused_at: Option<u64>,
    pub unpause_after: Option<u64>,
    pub reason: Option<String>,
}

/// Bridge dispute record
#[derive(Clone)]
#[contracttype]
pub struct BridgeDispute {
    pub dispute_id: u64,
    pub tx_id: u64,
    pub disputer: Address,
    pub reason: String,
    pub resolved: bool,
    pub resolution: Option<String>,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}

/// Bridge contract configuration
#[derive(Clone)]
#[contracttype]
pub struct BridgeConfig {
    pub admin: Address,
    pub paused: bool,
    pub timelock_duration: u64,
    pub min_bridge_amount: i128,
    pub max_bridge_amount: i128,
    pub default_fee_bps: u32,
    pub tx_counter: u64,
    pub dispute_counter: u64,
    pub lp_counter: u64,
}

// Bridge storage keys
pub const BRIDGE_CONFIG_KEY: &str = "bridge_config";
pub const BRIDGE_TX_KEY: &str = "bridge_tx_";
pub const BRIDGE_TX_HASH_KEY: &str = "bridge_tx_hash_";
pub const VALIDATOR_SIG_KEY: &str = "validator_sig_";
pub const VALIDATOR_SET_KEY: &str = "validator_set_";
pub const WRAPPED_ASSET_KEY: &str = "wrapped_asset_";
pub const FEE_TIER_KEY: &str = "fee_tier_";
pub const LIQUIDITY_POOL_KEY: &str = "lp_pool_";
pub const LP_DEPOSIT_KEY: &str = "lp_deposit_";
pub const EMERGENCY_KEY: &str = "emergency_config";
pub const DISPUTE_KEY: &str = "dispute_";
pub const CHAIN_SUPPORTED_KEY: &str = "chain_supported_";

// Bridge constants
pub const MAX_VALIDATORS_PER_CHAIN: u32 = 50;
pub const MIN_VALIDATORS_PER_CHAIN: u32 = 3;
pub const MAX_FEE_BPS: u32 = 500; // 5% max
pub const MIN_FEE_BPS: u32 = 0;
pub const DEFAULT_TIMELOCK_DURATION: u64 = 86400; // 24 hours
pub const MAX_BRIDGE_AMOUNT: i128 = 50_000_000_000_000_000; // 50M with 6 decimals
pub const MIN_BRIDGE_AMOUNT: i128 = 1_000_000; // 1 unit min (6 decimals)
pub const MAX_DISPUTE_REASON_LENGTH: usize = 512;
pub const DEFAULT_FEE_BPS: u32 = 10; // 0.1% default
pub const BRIDGE_TX_COUNTER_KEY: &str = "bridge_tx_counter";
pub const BRIDGE_DISPUTE_COUNTER_KEY: &str = "bridge_dispute_counter";
pub const BRIDGE_LP_COUNTER_KEY: &str = "bridge_lp_counter";

#[cfg(any(test, feature = "testutils"))]
pub mod testutils {
    use super::*;
    use soroban_sdk::Env;

    pub fn create_oracle_data(env: &Env, key: &str, value: &str, source: &str) -> OracleData {
        OracleData {
            key: String::from_str(env, key),
            value: String::from_str(env, value),
            timestamp: env.ledger().timestamp(),
            source: String::from_str(env, source),
        }
    }

    pub fn create_evolution_attestation(
        env: &Env,
        request_id: u64,
        agent_id: u64,
        oracle_provider: Address,
        new_model_hash: &str,
        nonce: u64,
    ) -> EvolutionAttestation {
        EvolutionAttestation {
            request_id,
            agent_id,
            oracle_provider,
            new_model_hash: String::from_str(env, new_model_hash),
            attestation_data: Bytes::from_slice(env, b"mock_attestation_data"),
            signature: Bytes::from_slice(env, &[0u8; 64]),
            timestamp: env.ledger().timestamp(),
            nonce,
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, String};

    fn key(env: &Env, module: ModuleId, category: &str, identifier: &str) -> NamespacedKey {
        NamespacedKey {
            module,
            category: String::from_str(env, category),
            identifier: String::from_str(env, identifier),
        }
    }

    #[test]
    fn valid_key_passes_validation() {
        let env = Env::default();

        let k = key(&env, ModuleId::Marketplace, "listing", "123");

        assert!(k.validate());
    }

    #[test]
    fn empty_category_fails() {
        let env = Env::default();

        let k = key(&env, ModuleId::Marketplace, "", "123");

        assert!(!k.validate());
    }

    #[test]
    fn empty_identifier_fails() {
        let env = Env::default();

        let k = key(&env, ModuleId::Marketplace, "listing", "");

        assert!(!k.validate());
    }

    #[test]
    fn modules_are_unique() {
        assert_ne!(ModuleId::Marketplace, ModuleId::Evolution);

        assert_ne!(ModuleId::Marketplace, ModuleId::AgentNft);
    }

    #[test]
    fn dynamic_keys_remain_unique() {
        let env = Env::default();

        let modules = [
            ModuleId::AgentNft,
            ModuleId::Marketplace,
            ModuleId::Evolution,
            ModuleId::Governance,
            ModuleId::Compliance,
        ];

        let categories = ["user", "agent", "listing", "request", "proposal"];

        let identifiers = ["1", "2", "100", "999", "dynamic_key"];

        let mut keys: Vec<(ModuleId, String, String)> = Vec::new(&env);

        for module in modules {
            for category in categories {
                for identifier in identifiers {
                    let k = key(&env, module, category, identifier);

                    assert!(k.validate());

                    for existing in keys.iter() {
                        assert!(
                            !(existing.0 == k.module
                                && existing.1 == k.category
                                && existing.2 == k.identifier)
                        );
                    }

                    keys.push_back((k.module, k.category, k.identifier));
                }
            }
        }

        assert_eq!(
            keys.len(),
            (modules.len() * categories.len() * identifiers.len()) as u32
        );
    }
}
