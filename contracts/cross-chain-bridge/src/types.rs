use soroban_sdk::{contracttype, Address, Bytes, Map, String, Vec};

/// Chain identifier for supported blockchains
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
#[repr(u32)]
pub enum ChainID {
    Stellar = 1,
    Ethereum = 2,
    BSC = 3,
    Polygon = 4,
    Arbitrum = 5,
    Optimism = 6,
}

/// Transaction status for cross-chain transfers
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
#[repr(u32)]
pub enum TransactionStatus {
    Pending = 0,
    Locked = 1,
    Minted = 2,
    Burned = 3,
    Unlocked = 4,
    Failed = 5,
    Reverted = 6,
}

/// Direction of the cross-chain transfer
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
#[repr(u32)]
pub enum TransferDirection {
    LockAndMint = 0,   // Lock on source, mint on destination
    BurnAndUnlock = 1, // Burn on destination, unlock on source
}

/// Cross-chain transfer request
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeTransfer {
    pub transfer_id: u64,
    pub source_chain: ChainID,
    pub destination_chain: ChainID,
    pub sender: Address,
    pub recipient: Bytes, // Bytes to support non-Stellar addresses
    pub token_address: Address,
    pub amount: i128,
    pub fee: i128,
    pub nonce: u64,
    pub timestamp: u64,
    pub status: TransactionStatus,
    pub direction: TransferDirection,
    pub signatures: Vec<Bytes>, // Collect validator signatures
}

/// Bridge validator
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validator {
    pub address: Address,
    pub public_key: Bytes, // Ed25519 public key for signature verification
    pub is_active: bool,
    pub power: u32, // Voting power
    pub joined_at: u64,
}

/// Rate limit configuration for the bridge
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct RateLimitConfig {
    pub daily_limit: i128,         // Total daily volume limit
    pub monthly_limit: i128,       // Total monthly volume limit
    pub per_transaction_max: i128, // Maximum per transfer
    pub per_transaction_min: i128, // Minimum per transfer
}

/// Rate limit state tracking
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimitState {
    pub daily_used: i128,
    pub monthly_used: i128,
    pub last_daily_reset: u64,
    pub last_monthly_reset: u64,
    pub per_user_daily: Map<Address, i128>,
}

/// Supported token configuration
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportedToken {
    pub token_address: Address,
    pub symbol: String,
    pub decimals: u32,
    pub is_mintable: bool, // Whether this token can be minted/burned
    pub is_locked: bool,   // Whether this token uses lock/unlock mechanism
    pub bridge_address_on_other_chains: Map<ChainID, Bytes>, // Bridge addresses on other chains
}

/// Fee configuration
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeConfig {
    pub basis_points: u32, // Fee in basis points (1 = 0.01%)
    pub min_fee: i128,     // Minimum fee
    pub fee_collector: Address,
}

/// Signature verification parameters
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct SignatureConfig {
    pub required_signatures: u32, // Number of signatures needed
    pub total_validators: u32,    // Total active validators
    pub quorum_percentage: u32,   // Minimum percentage needed (e.g., 67 for 2/3)
}
