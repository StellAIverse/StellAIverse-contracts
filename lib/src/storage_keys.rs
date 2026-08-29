use soroban_sdk::{symbol_short, Symbol};

pub const EXEC_CTR_KEY: Symbol = symbol_short!("exec_ctr");
pub const REQUEST_COUNTER_KEY: &str = "request_counter";
pub const CLAIM_COOLDOWN_KEY: &str = "claim_cooldown";
pub const MAX_CLAIMS_PER_PERIOD_KEY: &str = "max_claims_per_period";
pub const TESTNET_FLAG_KEY: &str = "testnet_mode";
pub const DEFAULT_COOLDOWN_SECONDS: u64 = 86400;
pub const DEFAULT_MAX_CLAIMS: u32 = 1;
pub const LISTING_COUNTER_KEY: &str = "listing_counter";
pub const PROVIDER_LIST_KEY: &str = "providers";
pub const AGENT_COUNTER_KEY: &str = "agent_counter";
pub const AGENT_KEY_PREFIX: &str = "agent_";
pub const AGENT_LEASE_STATUS_PREFIX: &str = "agent_lease_";
pub const OWNERSHIP_HISTORY_KEY_PREFIX: &str = "own_hist_";
pub const APPROVED_MINTERS_KEY: &str = "approved_minters";
pub const IMPLEMENTATION_KEY: Symbol = symbol_short!("impl_key");
pub const UPGRADE_HISTORY_KEY: Symbol = symbol_short!("up_hist");
pub const IS_PAUSED_KEY: Symbol = symbol_short!("is_paused");

pub const APPROVAL_CONFIG_KEY: &str = "approval_config";
pub const APPROVAL_COUNTER_KEY: &str = "approval_counter";
pub const APPROVAL_KEY_PREFIX: &str = "approval_";
pub const APPROVAL_HISTORY_KEY_PREFIX: &str = "approval_history_";

// Oracle subscription keys
pub const SUB_PLAN_KEY_PREFIX: &str = "sub_plan_";
pub const SUBSCRIPTION_KEY_PREFIX: &str = "subscription_";
pub const SUB_USAGE_KEY_PREFIX: &str = "sub_usage_";
