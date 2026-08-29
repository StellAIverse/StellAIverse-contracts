use crate::errors::*;
use crate::storage_keys::*;
use crate::types::{IncentiveDistribution, OracleProvider, ProviderHealth, RewardType};
use soroban_sdk::{Address, Env};

pub struct IncentiveManager;

impl IncentiveManager {
    /// Distribute rewards to oracle providers for successful updates
    pub fn distribute_update_reward(
        env: &Env,
        provider: &Address,
        feed_id: &soroban_sdk::Symbol,
        base_reward: i128,
        distribution_counter: &mut u64,
    ) {
        // Calculate reward based on performance
        let reward = Self::calculate_reward(env, provider, base_reward);

        // Update provider's incentive balance
        let ib_key = get_incentive_balance_key(provider);
        let mut current_balance: i128 = env.storage().instance().get(&ib_key).unwrap_or(0);

        current_balance += reward;
        env.storage().instance().set(&ib_key, &current_balance);

        let distribution_id = *distribution_counter;
        *distribution_counter += 1;

        let distribution = IncentiveDistribution {
            distribution_id,
            provider: provider.clone(),
            amount: reward,
            feed_id: feed_id.clone(),
            timestamp: env.ledger().timestamp(),
            reward_type: RewardType::UpdateReward,
        };

        // Emit reward event
        env.events().publish(
            (
                soroban_sdk::Symbol::new(env, "reward_distributed"),
                provider.clone(),
            ),
            (
                distribution.amount,
                distribution.feed_id.clone(),
                RewardType::UpdateReward as u32,
            ),
        );
    }

    /// Calculate reward based on provider's reputation and health.
    ///
    /// Availability runs 0..=1000 with 1000 meaning always available, and
    /// reputation runs 0..=100 with the initial value of 50 acting as the
    /// neutral point. A healthy, mid-reputation provider therefore earns
    /// exactly the base reward; deviations scale it up or down from there.
    fn calculate_reward(env: &Env, provider: &Address, base_reward: i128) -> i128 {
        let mut reward = base_reward;

        // Get provider health metrics
        let ph_key = get_provider_health_key(provider);
        let stored_health: Option<ProviderHealth> = env.storage().instance().get(&ph_key);
        if let Some(health) = stored_health {
            reward = reward * (health.availability_score as i128) / 1000;

            // Penalty for consecutive failures
            if health.consecutive_failures > 0 {
                let penalty_bps = (health.consecutive_failures as i128).min(100) * 100; // 1% per failure
                reward = (reward * (10000 - penalty_bps)) / 10000;
            }
        }

        // Get provider data for reputation
        let p_key = get_provider_key(provider);
        let stored_provider: Option<OracleProvider> = env.storage().instance().get(&p_key);
        if let Some(provider_data) = stored_provider {
            reward = reward * (provider_data.reputation_score as i128) / 50;
        }

        reward.max(0)
    }

    /// Apply penalty to misbehaving providers
    pub fn apply_penalty(
        env: &Env,
        provider: &Address,
        feed_id: &soroban_sdk::Symbol,
        penalty_amount: i128,
        reason: &str,
        distribution_counter: &mut u64,
    ) {
        // Slash from provider's staked amount and incentives
        let ib_key = get_incentive_balance_key(provider);
        let mut current_balance: i128 = env.storage().instance().get(&ib_key).unwrap_or(0);

        if current_balance < penalty_amount {
            // Slash from stake if insufficient incentive balance
            let p_key = get_provider_key(provider);
            let stored_provider: Option<OracleProvider> = env.storage().instance().get(&p_key);
            if let Some(mut provider_data) = stored_provider {
                let remaining_penalty = penalty_amount - current_balance;
                if provider_data.staked_amount >= remaining_penalty {
                    provider_data.staked_amount -= remaining_penalty;
                    current_balance = 0;
                    env.storage().instance().set(&p_key, &provider_data);
                } else {
                    // Can't fully slash - take what's available
                    current_balance = 0;
                }
            }
        } else {
            current_balance -= penalty_amount;
        }

        env.storage().instance().set(&ib_key, &current_balance);

        *distribution_counter += 1;

        // Update provider health
        Self::update_health_on_penalty(env, provider);

        // Emit penalty event
        env.events().publish(
            (
                soroban_sdk::Symbol::new(env, "penalty_applied"),
                provider.clone(),
            ),
            (
                penalty_amount,
                feed_id.clone(),
                soroban_sdk::String::from_str(env, reason),
            ),
        );
    }

    /// Update provider health metrics when penalty is applied
    fn update_health_on_penalty(env: &Env, provider: &Address) {
        let ph_key = get_provider_health_key(provider);
        let mut health: ProviderHealth =
            env.storage()
                .instance()
                .get(&ph_key)
                .unwrap_or_else(|| ProviderHealth {
                    provider: provider.clone(),
                    consecutive_failures: 0,
                    last_successful_update: 0,
                    price_deviation_count: 0,
                    availability_score: 1000,
                });

        health.consecutive_failures += 1;
        health.price_deviation_count += 1;
        health.availability_score = health.availability_score.saturating_sub(50); // 5% penalty
        env.storage().instance().set(&ph_key, &health);

        // Also update provider's reputation
        let p_key = get_provider_key(provider);
        let stored_provider: Option<OracleProvider> = env.storage().instance().get(&p_key);
        if let Some(mut provider_data) = stored_provider {
            provider_data.reputation_score = provider_data.reputation_score.saturating_sub(5);
            env.storage().instance().set(&p_key, &provider_data);
        }
    }

    /// Allow providers to withdraw their earned incentives
    pub fn withdraw_incentives(env: &Env, provider: Address) -> i128 {
        provider.require_auth();

        let ib_key = get_incentive_balance_key(&provider);
        let current_balance: i128 = env.storage().instance().get(&ib_key).unwrap_or(0);

        if current_balance <= 0 {
            insufficient_balance(env);
        }

        // Reset balance to 0
        env.storage().instance().set(&ib_key, &0i128);

        // Transfer the tokens (implementation would depend on token contract)
        // This is where you'd interact with the payment token to transfer to provider

        env.events().publish(
            (
                soroban_sdk::Symbol::new(env, "incentives_withdrawn"),
                provider.clone(),
            ),
            (current_balance, env.ledger().timestamp()),
        );

        current_balance
    }

    /// Get current withdrawable balance for a provider
    pub fn get_balance(env: &Env, provider: &Address) -> i128 {
        let ib_key = get_incentive_balance_key(provider);
        env.storage().instance().get(&ib_key).unwrap_or(0)
    }

    /// Reset consecutive failures on successful update
    pub fn record_successful_update(env: &Env, provider: &Address) {
        let ph_key = get_provider_health_key(provider);
        let mut health: ProviderHealth =
            env.storage()
                .instance()
                .get(&ph_key)
                .unwrap_or_else(|| ProviderHealth {
                    provider: provider.clone(),
                    consecutive_failures: 0,
                    last_successful_update: env.ledger().timestamp(),
                    price_deviation_count: 0,
                    availability_score: 1000,
                });

        health.consecutive_failures = 0;
        health.last_successful_update = env.ledger().timestamp();
        health.availability_score = health.availability_score.saturating_add(1).min(1000);
        env.storage().instance().set(&ph_key, &health);

        // Update provider's success metrics
        let p_key = get_provider_key(provider);
        let stored_provider: Option<OracleProvider> = env.storage().instance().get(&p_key);
        if let Some(mut provider_data) = stored_provider {
            provider_data.total_updates += 1;
            provider_data.successful_updates += 1;
            provider_data.reputation_score =
                provider_data.reputation_score.saturating_add(1).min(100);
            env.storage().instance().set(&p_key, &provider_data);
        }
    }

    /// Record failed update
    pub fn record_failed_update(env: &Env, provider: &Address) {
        let ph_key = get_provider_health_key(provider);
        let mut health: ProviderHealth =
            env.storage()
                .instance()
                .get(&ph_key)
                .unwrap_or_else(|| ProviderHealth {
                    provider: provider.clone(),
                    consecutive_failures: 1,
                    last_successful_update: 0,
                    price_deviation_count: 0,
                    availability_score: 950,
                });

        health.consecutive_failures += 1;
        env.storage().instance().set(&ph_key, &health);

        // Update provider metrics
        let p_key = get_provider_key(provider);
        let stored_provider: Option<OracleProvider> = env.storage().instance().get(&p_key);
        if let Some(mut provider_data) = stored_provider {
            provider_data.total_updates += 1;
            env.storage().instance().set(&p_key, &provider_data);
        }
    }
}
