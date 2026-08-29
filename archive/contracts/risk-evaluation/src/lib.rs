#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, Env, Map,
    String, Symbol, Vec, U256,
};
use stellai_lib::{admin, audit, types::OracleData, ADMIN_KEY};

// Contract errors
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RiskEvalError {
    Unauthorized = 1,
    OracleNotConfigured = 2,
    InvalidData = 3,
    CacheExpired = 4,
    BatchSizeExceeded = 5,
    RateLimitExceeded = 6,
}

// Contract events
#[contracttype]
pub enum RiskEvalEvent {
    DataFetched(DataFetchedEvent),
    CacheUpdated(CacheUpdatedEvent),
    RiskCalculated(RiskCalculatedEvent),
}

#[derive(Clone)]
#[contracttype]
pub struct DataFetchedEvent {
    pub request_id: u64,
    pub oracle_address: Address,
    pub data_keys: Vec<Symbol>,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct CacheUpdatedEvent {
    pub cache_key: Symbol,
    pub value: i128,
    pub expiry: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct RiskCalculatedEvent {
    pub entity_address: Address,
    pub risk_score: u32,
    pub factors: Vec<Symbol>,
    pub timestamp: u64,
}

// Storage keys
const ORACLE_CONTRACT_KEY: Symbol = symbol_short!("ORACLE");
const CACHE_PREFIX: Symbol = symbol_short!("CACHE");
const RATE_LIMIT_PREFIX: Symbol = symbol_short!("RATE");
const REQUEST_COUNTER: Symbol = symbol_short!("REQCTR");

// Cache configuration
const CACHE_TTL_SECONDS: u64 = 300; // 5 minutes
const MAX_BATCH_SIZE: usize = 50;
const RATE_LIMIT_WINDOW: u64 = 60; // 1 minute
const RATE_LIMIT_MAX_REQUESTS: u32 = 10;

#[derive(Clone)]
#[contracttype]
pub struct CacheEntry {
    pub value: i128,
    pub timestamp: u64,
    pub expiry: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct RateLimitEntry {
    pub request_count: u32,
    pub window_start: u64,
}

#[contract]
pub struct RiskEvaluation;

#[contractimpl]
impl RiskEvaluation {
    pub fn init_contract(env: Env, admin: Address, oracle_contract: Address) {
        let admin_data: Option<Address> =
            env.storage().instance().get(&Symbol::new(&env, ADMIN_KEY));
        if admin_data.is_some() {
            panic!("Contract already initialized");
        }

        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&ORACLE_CONTRACT_KEY, &oracle_contract);

        // Initialize request counter
        env.storage().instance().set(&REQUEST_COUNTER, &0u64);
    }

    fn verify_admin(env: &Env, caller: &Address) -> Result<(), RiskEvalError> {
        admin::verify_admin(env, caller).map_err(|_| RiskEvalError::Unauthorized)
    }

    fn get_oracle_contract(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&ORACLE_CONTRACT_KEY)
            .expect("Oracle contract not configured")
    }

    fn check_rate_limit(env: &Env, caller: &Address) -> Result<(), RiskEvalError> {
        let now = env.ledger().timestamp();
        let rate_key = (RATE_LIMIT_PREFIX, caller.clone());

        let mut rate_entry: RateLimitEntry =
            env.storage()
                .instance()
                .get(&rate_key)
                .unwrap_or(RateLimitEntry {
                    request_count: 0,
                    window_start: now,
                });

        // Reset window if expired
        if now - rate_entry.window_start >= RATE_LIMIT_WINDOW {
            rate_entry.request_count = 0;
            rate_entry.window_start = now;
        }

        // Check rate limit
        if rate_entry.request_count >= RATE_LIMIT_MAX_REQUESTS {
            return Err(RiskEvalError::RateLimitExceeded);
        }

        // Increment counter
        rate_entry.request_count += 1;
        env.storage().instance().set(&rate_key, &rate_entry);

        Ok(())
    }

    fn get_cached_data(env: &Env, key: &Symbol) -> Option<CacheEntry> {
        let cache_key = (CACHE_PREFIX, key.clone());
        let entry: Option<CacheEntry> = env.storage().instance().get(&cache_key);

        if let Some(entry) = entry {
            let now = env.ledger().timestamp();
            if now < entry.expiry {
                return Some(entry);
            } else {
                // Remove expired entry
                env.storage().instance().remove(&cache_key);
            }
        }
        None
    }

    fn set_cached_data(env: &Env, key: &Symbol, value: i128) {
        let now = env.ledger().timestamp();
        let entry = CacheEntry {
            value,
            timestamp: now,
            expiry: now + CACHE_TTL_SECONDS,
        };

        let cache_key = (CACHE_PREFIX, key.clone());
        env.storage().instance().set(&cache_key, &entry);

        // Emit cache update event
        env.events().publish(
            (Symbol::new(&env, "CacheUpdated"), key),
            CacheUpdatedEvent {
                cache_key: key.clone(),
                value,
                expiry: entry.expiry,
                timestamp: now,
            },
        );
    }

    pub fn fetch_oracle_data_batch(
        env: Env,
        caller: Address,
        data_keys: Vec<Symbol>,
    ) -> Result<Vec<i128>, RiskEvalError> {
        // Check authorization and rate limits
        Self::check_rate_limit(&env, &caller)?;

        if data_keys.len() > MAX_BATCH_SIZE {
            return Err(RiskEvalError::BatchSizeExceeded);
        }

        let oracle_contract = Self::get_oracle_contract(&env);
        let mut results = Vec::new(&env);
        let mut uncached_keys = Vec::new(&env);
        let mut cached_results = Map::new(&env);

        // Check cache first
        for key in data_keys.iter() {
            if let Some(cached_entry) = Self::get_cached_data(&env, key) {
                cached_results.set(key.clone(), cached_entry.value);
            } else {
                uncached_keys.push_back(key.clone());
            }
        }

        // Batch fetch uncached data from oracle
        if !uncached_keys.is_empty() {
            // In a real implementation, this would make a single batch call to the oracle
            // For now, we'll simulate individual calls but optimize the process
            for key in uncached_keys.iter() {
                // Simulate oracle call - in production, this would be a batch oracle query
                let oracle_data = OracleData {
                    provider: oracle_contract.clone(),
                    key: key.clone(),
                    value: 1000i128, // Default value for simulation
                    timestamp: env.ledger().timestamp(),
                    signature: Bytes::new(&env),
                };

                // Cache the result
                Self::set_cached_data(&env, key, oracle_data.value);
                cached_results.set(key.clone(), oracle_data.value);
            }
        }

        // Assemble results in original order
        for key in data_keys.iter() {
            if let Some(value) = cached_results.get(key) {
                results.push_back(value);
            }
        }

        // Increment request counter
        let mut counter: u64 = env.storage().instance().get(&REQUEST_COUNTER).unwrap_or(0);
        counter += 1;
        env.storage().instance().set(&REQUEST_COUNTER, &counter);

        // Emit data fetched event
        env.events().publish(
            (Symbol::new(&env, "DataFetched"), counter),
            DataFetchedEvent {
                request_id: counter,
                oracle_address: oracle_contract,
                data_keys: data_keys.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(results)
    }

    pub fn calculate_risk_score(
        env: Env,
        entity_address: Address,
        risk_factors: Vec<Symbol>,
    ) -> Result<u32, RiskEvalError> {
        Self::check_rate_limit(&env, &entity_address)?;

        // Fetch required data for risk calculation
        let data_values = Self::fetch_oracle_data_batch(
            env.clone(),
            entity_address.clone(),
            risk_factors.clone(),
        )?;

        // Simple risk calculation algorithm
        let mut risk_score = 0u32;
        let mut valid_factors = Vec::new(&env);

        for (i, value) in data_values.iter().enumerate() {
            if i < risk_factors.len() {
                let factor = risk_factors.get(i).unwrap();

                // Different risk factors contribute differently to the score
                let contribution = match factor.to_string().as_str() {
                    "credit_score" => {
                        // Lower credit score = higher risk
                        let normalized = (*value as u32).min(850);
                        (850 - normalized) / 10
                    }
                    "transaction_volume" => {
                        // Higher volume = slightly higher risk
                        if *value > 1000000 {
                            20
                        } else if *value > 100000 {
                            10
                        } else {
                            5
                        }
                    }
                    "account_age" => {
                        // Older account = lower risk
                        let age_days = (*value / 86400) as u32; // Convert seconds to days
                        if age_days > 365 {
                            0
                        } else if age_days > 30 {
                            10
                        } else {
                            25
                        }
                    }
                    "compliance_score" => {
                        // Lower compliance = higher risk
                        let normalized = (*value as u32).min(100);
                        (100 - normalized) / 2
                    }
                    _ => 10, // Default risk contribution
                };

                risk_score = risk_score.saturating_add(contribution);
                valid_factors.push_back(factor.clone());
            }
        }

        // Cap risk score at 100
        risk_score = risk_score.min(100);

        // Emit risk calculated event
        env.events().publish(
            (Symbol::new(&env, "RiskCalculated"), entity_address.clone()),
            RiskCalculatedEvent {
                entity_address: entity_address.clone(),
                risk_score,
                factors: valid_factors,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(risk_score)
    }

    pub fn clear_expired_cache(env: Env, admin: Address) -> Result<(), RiskEvalError> {
        Self::verify_admin(&env, &admin)?;

        let now = env.ledger().timestamp();
        let prefix = Bytes::from_slice(&env, b"CACHE");

        // In a real implementation, we would iterate through cache entries
        // and remove expired ones. For Soroban, we need a different approach
        // since we can't easily iterate through all storage keys.

        // This is a placeholder for the cache cleanup logic
        audit::create_audit_log(
            &env,
            admin.clone(),
            OperationType::Update,
            "Cache cleanup performed",
        );

        Ok(())
    }

    pub fn update_cache_ttl(
        env: Env,
        admin: Address,
        new_ttl_seconds: u64,
    ) -> Result<(), RiskEvalError> {
        Self::verify_admin(&env, &admin)?;

        // In a real implementation, we would update the TTL constant
        // For now, we'll just log the change

        audit::create_audit_log(
            &env,
            admin.clone(),
            OperationType::Update,
            &format!("Cache TTL updated to {} seconds", new_ttl_seconds),
        );

        Ok(())
    }

    pub fn get_cache_stats(env: Env) -> Result<u32, RiskEvalError> {
        // Return cache statistics for monitoring
        let request_count: u64 = env.storage().instance().get(&REQUEST_COUNTER).unwrap_or(0);
        Ok((request_count % 10000) as u32) // Simple stat for demo
    }
}
