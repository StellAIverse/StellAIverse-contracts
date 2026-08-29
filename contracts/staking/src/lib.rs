#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, Address, Env, Symbol,
    Vec,
};

// ═══════════════════════════════════════════════════════════════
//  CONSTANTS
// ═══════════════════════════════════════════════════════════════

const BPS_DENOMINATOR: i128 = 10_000;
const PRECISION_FACTOR: i128 = 1_000_000_000_000_000_000; // 1e18
const MAX_TIERS: u32 = 10;
const MAX_BATCH_SIZE: u32 = 50;
const MAX_PENALTY_BPS: u32 = 10_000;

// ═══════════════════════════════════════════════════════════════
//  DATA TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    RewardToken,
    Paused,
    RewardRatePerSecond,
    LastRewardTime,
    RewardPerTokenStored, // Global accumulator scaled by 1e18
    TotalWeightedStake,   // Sum of (amount * multiplier_bps / BPS_DENOMINATOR)
    TotalStaked,
    TotalRewardsDistributed,
    StakeCounter,
    StakePosition(u64),
    UserStakeIds(Address),
    TierCounter,
    Tier(u32),
    TierIds,
    ReentrancyLock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StakePosition {
    pub stake_id: u64,
    pub user: Address,
    pub amount: i128,
    pub tier_id: u32,
    pub stake_time: u64,
    pub lock_end_time: u64,
    pub reward_per_token_paid: i128, // Snapshot of global accumulator
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StakingTier {
    pub tier_id: u32,
    pub name: Symbol,
    pub min_stake_amount: i128,
    pub lock_duration_seconds: u64,
    pub reward_multiplier_bps: u32, // e.g., 10000 = 1x, 15000 = 1.5x
    pub penalty_bps: u32,           // Early withdrawal penalty in bps
    pub active: bool,
    pub created_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StakingInfo {
    pub admin: Address,
    pub reward_token: Address,
    pub total_staked: i128,
    pub total_rewards_distributed: i128,
    pub reward_rate_per_second: i128,
    pub last_reward_time: u64,
    pub paused: bool,
    pub tier_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StakeResult {
    pub stake_id: u64,
    pub amount: i128,
    pub tier_id: u32,
    pub lock_end_time: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct UnstakeResult {
    pub stake_id: u64,
    pub principal_returned: i128,
    pub rewards_claimed: i128,
    pub penalty_amount: i128,
    pub total_returned: i128,
}

// ═══════════════════════════════════════════════════════════════
//  CONTRACT
// ═══════════════════════════════════════════════════════════════

#[contract]
pub struct Staking;

#[contractimpl]
impl Staking {
    // ── INITIALIZATION ─────────────────────────────────────────

    pub fn initialize(
        env: Env,
        admin: Address,
        reward_token: Address,
        reward_rate_per_second: i128,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();
        if reward_rate_per_second < 0 {
            panic!("Reward rate cannot be negative");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::RewardToken, &reward_token);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::RewardRatePerSecond, &reward_rate_per_second);
        env.storage()
            .instance()
            .set(&DataKey::LastRewardTime, &env.ledger().timestamp());
        env.storage()
            .instance()
            .set(&DataKey::RewardPerTokenStored, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalWeightedStake, &0i128);
        env.storage().instance().set(&DataKey::TotalStaked, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalRewardsDistributed, &0i128);
        env.storage().instance().set(&DataKey::StakeCounter, &0u64);
        env.storage().instance().set(&DataKey::TierCounter, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::TierIds, &Vec::<u32>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);

        env.events().publish(
            (symbol_short!("stk_init"),),
            (admin, reward_token, reward_rate_per_second),
        );
    }

    // ── ADMIN CONTROLS ─────────────────────────────────────────

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if Self::paused(&env) {
            panic!("Already paused");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((symbol_short!("stk_pause"),), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if !Self::paused(&env) {
            panic!("Not paused");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish((symbol_short!("stk_unpse"),), admin);
    }

    pub fn set_reward_rate(env: Env, admin: Address, new_rate: i128) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if new_rate < 0 {
            panic!("Reward rate cannot be negative");
        }

        Self::update_pool(&env);

        env.storage()
            .instance()
            .set(&DataKey::RewardRatePerSecond, &new_rate);
        env.events()
            .publish((symbol_short!("stk_rate"),), (admin, new_rate));
    }

    // ── TIER MANAGEMENT ────────────────────────────────────────

    pub fn add_tier(
        env: Env,
        admin: Address,
        name: Symbol,
        min_stake_amount: i128,
        lock_duration_seconds: u64,
        reward_multiplier_bps: u32,
        penalty_bps: u32,
    ) -> u32 {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if min_stake_amount <= 0 {
            panic!("Minimum stake amount must be positive");
        }
        if lock_duration_seconds == 0 {
            panic!("Lock duration must be positive");
        }
        if reward_multiplier_bps == 0 {
            panic!("Reward multiplier must be positive");
        }
        if penalty_bps > MAX_PENALTY_BPS {
            panic!("Penalty exceeds 100%");
        }

        let tier_ids = Self::tier_ids(&env);
        if tier_ids.len() >= MAX_TIERS {
            panic!("Maximum tiers reached");
        }

        let tier_id = Self::next_tier_id(&env);
        let tier = StakingTier {
            tier_id,
            name: name.clone(),
            min_stake_amount,
            lock_duration_seconds,
            reward_multiplier_bps,
            penalty_bps,
            active: true,
            created_at: env.ledger().timestamp(),
        };

        env.storage().instance().set(&DataKey::Tier(tier_id), &tier);

        let mut ids = tier_ids;
        ids.push_back(tier_id);
        env.storage().instance().set(&DataKey::TierIds, &ids);

        env.events().publish(
            (symbol_short!("stk_tier"),),
            (
                tier_id,
                name,
                min_stake_amount,
                lock_duration_seconds,
                reward_multiplier_bps,
                penalty_bps,
            ),
        );

        tier_id
    }

    pub fn update_tier(
        env: Env,
        admin: Address,
        tier_id: u32,
        new_name: Option<Symbol>,
        new_min_stake_amount: Option<i128>,
        new_lock_duration_seconds: Option<u64>,
        new_reward_multiplier_bps: Option<u32>,
        new_penalty_bps: Option<u32>,
    ) -> StakingTier {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut tier = Self::load_tier(&env, tier_id);
        if !tier.active {
            panic!("Tier is not active");
        }

        if let Some(name) = new_name {
            tier.name = name;
        }
        if let Some(amount) = new_min_stake_amount {
            if amount <= 0 {
                panic!("Minimum stake amount must be positive");
            }
            tier.min_stake_amount = amount;
        }
        if let Some(duration) = new_lock_duration_seconds {
            if duration == 0 {
                panic!("Lock duration must be positive");
            }
            tier.lock_duration_seconds = duration;
        }
        if let Some(multiplier) = new_reward_multiplier_bps {
            if multiplier == 0 {
                panic!("Reward multiplier must be positive");
            }
            tier.reward_multiplier_bps = multiplier;
        }
        if let Some(penalty) = new_penalty_bps {
            if penalty > MAX_PENALTY_BPS {
                panic!("Penalty exceeds 100%");
            }
            tier.penalty_bps = penalty;
        }

        env.storage().instance().set(&DataKey::Tier(tier_id), &tier);

        env.events()
            .publish((symbol_short!("stk_tupd"),), (tier_id, admin));

        tier
    }

    pub fn deactivate_tier(env: Env, admin: Address, tier_id: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut tier = Self::load_tier(&env, tier_id);
        if !tier.active {
            panic!("Tier already inactive");
        }

        tier.active = false;
        env.storage().instance().set(&DataKey::Tier(tier_id), &tier);

        env.events()
            .publish((symbol_short!("stk_tdis"),), (tier_id, admin));
    }

    // ── STAKING ────────────────────────────────────────────────

    pub fn stake(env: Env, user: Address, amount: i128, tier_id: u32) -> StakeResult {
        user.require_auth();
        if Self::paused(&env) {
            panic!("Staking is paused");
        }
        if amount <= 0 {
            panic!("Stake amount must be positive");
        }

        let tier = Self::load_tier(&env, tier_id);
        if !tier.active {
            panic!("Tier is not active");
        }
        if amount < tier.min_stake_amount {
            panic!("Amount below tier minimum");
        }

        // Update reward accumulator before modifying state
        Self::update_pool(&env);

        let now = env.ledger().timestamp();
        let lock_end_time = now
            .checked_add(tier.lock_duration_seconds)
            .expect("Lock end time overflow");

        let current_rpt = Self::reward_per_token_stored(&env);

        let stake_id = Self::next_stake_id(&env);
        let position = StakePosition {
            stake_id,
            user: user.clone(),
            amount,
            tier_id,
            stake_time: now,
            lock_end_time,
            reward_per_token_paid: current_rpt,
            active: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::StakePosition(stake_id), &position);

        Self::add_user_stake(&env, &user, stake_id);

        // Update total staked
        let new_total = Self::total_staked(&env)
            .checked_add(amount)
            .expect("Total staked overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalStaked, &new_total);

        // Update total weighted stake
        let user_weight = Self::calculate_weight(amount, tier.reward_multiplier_bps);
        let new_weighted = Self::total_weighted_stake(&env)
            .checked_add(user_weight)
            .expect("Total weighted overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalWeightedStake, &new_weighted);

        // Transfer tokens from user to contract
        let token = Self::reward_token(&env);
        let contract_address = env.current_contract_address();
        Self::transfer_token(&env, &token, &user, &contract_address, amount);

        env.events().publish(
            (symbol_short!("stk_new"),),
            (stake_id, user, amount, tier_id, lock_end_time),
        );

        StakeResult {
            stake_id,
            amount,
            tier_id,
            lock_end_time,
        }
    }

    pub fn unstake(env: Env, user: Address, stake_id: u64) -> UnstakeResult {
        user.require_auth();

        let mut position = Self::load_stake(&env, stake_id);
        if position.user != user {
            panic!("Only staker can unstake");
        }
        if !position.active {
            panic!("Stake is not active");
        }

        let tier = Self::load_tier(&env, position.tier_id);
        let now = env.ledger().timestamp();

        // Calculate pending rewards before updating pool
        let pending_rewards =
            Self::calculate_pending_rewards_static(&env, &position, tier.reward_multiplier_bps);

        // Update pool (before removing weight)
        Self::update_pool(&env);

        // Check if early withdrawal
        let (penalty_amount, principal_returned) = if now < position.lock_end_time {
            let penalty = Self::calculate_penalty(position.amount, tier.penalty_bps);
            let net_principal = position
                .amount
                .checked_sub(penalty)
                .expect("Penalty exceeds principal");
            (penalty, net_principal)
        } else {
            (0, position.amount)
        };

        let total_returned = principal_returned
            .checked_add(pending_rewards)
            .expect("Total returned overflow");

        // Deactivate position
        position.active = false;
        env.storage()
            .instance()
            .set(&DataKey::StakePosition(stake_id), &position);

        // Update total staked
        let new_total = Self::total_staked(&env)
            .checked_sub(position.amount)
            .expect("Total staked underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalStaked, &new_total);

        // Update total weighted stake (remove this position's weight)
        let user_weight = Self::calculate_weight(position.amount, tier.reward_multiplier_bps);
        let new_weighted = Self::total_weighted_stake(&env)
            .checked_sub(user_weight)
            .expect("Total weighted underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalWeightedStake, &new_weighted);

        // Update total rewards distributed
        if pending_rewards > 0 {
            let new_total_rewards = Self::total_rewards_distributed(&env)
                .checked_add(pending_rewards)
                .expect("Total rewards overflow");
            env.storage()
                .instance()
                .set(&DataKey::TotalRewardsDistributed, &new_total_rewards);
        }

        // Transfer tokens back to user
        let token = Self::reward_token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);

        if principal_returned > 0 {
            Self::transfer_token_unchecked(
                &env,
                &token,
                &contract_address,
                &user,
                principal_returned,
            );
        }
        if pending_rewards > 0 {
            Self::transfer_token_unchecked(&env, &token, &contract_address, &user, pending_rewards);
        }

        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("stk_unst"),),
            (
                stake_id,
                user,
                principal_returned,
                pending_rewards,
                penalty_amount,
            ),
        );

        UnstakeResult {
            stake_id,
            principal_returned,
            rewards_claimed: pending_rewards,
            penalty_amount,
            total_returned,
        }
    }

    pub fn claim_rewards(env: Env, user: Address, stake_id: u64) -> i128 {
        user.require_auth();

        let mut position = Self::load_stake(&env, stake_id);
        if position.user != user {
            panic!("Only staker can claim");
        }
        if !position.active {
            panic!("Stake is not active");
        }

        let tier = Self::load_tier(&env, position.tier_id);

        // Calculate pending rewards before updating pool
        let pending_rewards =
            Self::calculate_pending_rewards_static(&env, &position, tier.reward_multiplier_bps);

        if pending_rewards <= 0 {
            panic!("No rewards to claim");
        }

        // Update pool
        Self::update_pool(&env);

        // Update position's reward_per_token_paid to current global value
        position.reward_per_token_paid = Self::reward_per_token_stored(&env);
        env.storage()
            .instance()
            .set(&DataKey::StakePosition(stake_id), &position);

        // Update total rewards distributed
        let new_total_rewards = Self::total_rewards_distributed(&env)
            .checked_add(pending_rewards)
            .expect("Total rewards overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalRewardsDistributed, &new_total_rewards);

        // Transfer rewards
        let token = Self::reward_token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &user, pending_rewards);
        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("stk_claim"),),
            (stake_id, user, pending_rewards),
        );

        pending_rewards
    }

    pub fn claim_rewards_batch(env: Env, user: Address, stake_ids: Vec<u64>) -> i128 {
        user.require_auth();
        if stake_ids.is_empty() {
            panic!("No stakes provided");
        }
        if stake_ids.len() > MAX_BATCH_SIZE {
            panic!("Batch exceeds maximum size");
        }

        let mut total_claimed: i128 = 0;

        // First pass: calculate all pending rewards before updating pool
        let mut pending_list = Vec::<i128>::new(&env);
        for idx in 0..stake_ids.len() {
            let stake_id = stake_ids.get_unchecked(idx);
            let position = Self::load_stake(&env, stake_id);
            if position.user != user {
                panic!("Only staker can claim");
            }
            if !position.active {
                panic!("Stake is not active");
            }

            let tier = Self::load_tier(&env, position.tier_id);
            let pending =
                Self::calculate_pending_rewards_static(&env, &position, tier.reward_multiplier_bps);
            pending_list.push_back(pending);
        }

        // Update pool once
        Self::update_pool(&env);
        let current_rpt = Self::reward_per_token_stored(&env);

        // Second pass: update positions and accumulate claimed
        for idx in 0..stake_ids.len() {
            let stake_id = stake_ids.get_unchecked(idx);
            let pending = pending_list.get_unchecked(idx);

            if pending > 0 {
                let mut position = Self::load_stake(&env, stake_id);
                position.reward_per_token_paid = current_rpt;
                env.storage()
                    .instance()
                    .set(&DataKey::StakePosition(stake_id), &position);

                total_claimed = total_claimed
                    .checked_add(pending)
                    .expect("Total claimed overflow");
            }
        }

        if total_claimed <= 0 {
            panic!("No rewards to claim");
        }

        // Update total rewards distributed
        let new_total_rewards = Self::total_rewards_distributed(&env)
            .checked_add(total_claimed)
            .expect("Total rewards overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalRewardsDistributed, &new_total_rewards);

        // Transfer rewards
        let token = Self::reward_token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &user, total_claimed);
        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("stk_bclm"),),
            (user, total_claimed, stake_ids.len()),
        );

        total_claimed
    }

    // ── EMERGENCY WITHDRAWAL ───────────────────────────────────

    pub fn emergency_withdraw(
        env: Env,
        admin: Address,
        user: Address,
        stake_id: u64,
    ) -> UnstakeResult {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut position = Self::load_stake(&env, stake_id);
        if position.user != user {
            panic!("Stake does not belong to user");
        }
        if !position.active {
            panic!("Stake is not active");
        }

        let tier = Self::load_tier(&env, position.tier_id);

        // Emergency withdrawal returns principal only, forfeiting rewards
        let principal_returned = position.amount;

        // Deactivate position
        position.active = false;
        env.storage()
            .instance()
            .set(&DataKey::StakePosition(stake_id), &position);

        // Update total staked
        let new_total = Self::total_staked(&env)
            .checked_sub(position.amount)
            .expect("Total staked underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalStaked, &new_total);

        // Update total weighted stake
        let user_weight = Self::calculate_weight(position.amount, tier.reward_multiplier_bps);
        let new_weighted = Self::total_weighted_stake(&env)
            .checked_sub(user_weight)
            .expect("Total weighted underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalWeightedStake, &new_weighted);

        // Transfer principal back
        let token = Self::reward_token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &user, principal_returned);
        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("stk_emw"),),
            (stake_id, user, principal_returned),
        );

        UnstakeResult {
            stake_id,
            principal_returned,
            rewards_claimed: 0,
            penalty_amount: 0,
            total_returned: principal_returned,
        }
    }

    pub fn emergency_withdraw_all(env: Env, admin: Address, user: Address) -> i128 {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let stake_ids = Self::user_stake_ids(&env, &user);
        let mut total_returned: i128 = 0;

        for idx in 0..stake_ids.len() {
            let stake_id = stake_ids.get_unchecked(idx);
            let mut position = Self::load_stake(&env, stake_id);
            if position.user != user || !position.active {
                continue;
            }

            let tier = Self::load_tier(&env, position.tier_id);
            let principal = position.amount;
            position.active = false;
            env.storage()
                .instance()
                .set(&DataKey::StakePosition(stake_id), &position);

            let new_total = Self::total_staked(&env)
                .checked_sub(principal)
                .expect("Total staked underflow");
            env.storage()
                .instance()
                .set(&DataKey::TotalStaked, &new_total);

            let user_weight = Self::calculate_weight(principal, tier.reward_multiplier_bps);
            let new_weighted = Self::total_weighted_stake(&env)
                .checked_sub(user_weight)
                .expect("Total weighted underflow");
            env.storage()
                .instance()
                .set(&DataKey::TotalWeightedStake, &new_weighted);

            total_returned = total_returned
                .checked_add(principal)
                .expect("Total returned overflow");
        }

        if total_returned > 0 {
            let token = Self::reward_token(&env);
            let contract_address = env.current_contract_address();
            Self::enter_non_reentrant(&env);
            Self::transfer_token_unchecked(&env, &token, &contract_address, &user, total_returned);
            Self::exit_non_reentrant(&env);
        }

        env.events()
            .publish((symbol_short!("stk_emwa"),), (user, total_returned));

        total_returned
    }

    // ── FUND REWARDS ───────────────────────────────────────────

    /// Fund the staking contract with reward tokens
    pub fn fund_rewards(env: Env, funder: Address, amount: i128) {
        funder.require_auth();
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let token = Self::reward_token(&env);
        let contract_address = env.current_contract_address();
        Self::transfer_token(&env, &token, &funder, &contract_address, amount);

        env.events()
            .publish((symbol_short!("stk_fund"),), (funder, amount));
    }

    // ── VIEW FUNCTIONS ─────────────────────────────────────────

    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    pub fn get_reward_token(env: Env) -> Address {
        Self::reward_token(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        Self::paused(&env)
    }

    pub fn get_reward_rate(env: Env) -> i128 {
        Self::reward_rate_per_second(&env)
    }

    pub fn get_total_staked(env: Env) -> i128 {
        Self::total_staked(&env)
    }

    pub fn get_total_rewards_distributed(env: Env) -> i128 {
        Self::total_rewards_distributed(&env)
    }

    pub fn get_last_reward_time(env: Env) -> u64 {
        Self::last_reward_time(&env)
    }

    pub fn get_stake(env: Env, stake_id: u64) -> StakePosition {
        Self::load_stake(&env, stake_id)
    }

    pub fn get_user_stakes(env: Env, user: Address) -> Vec<StakePosition> {
        let stake_ids = Self::user_stake_ids(&env, &user);
        let mut stakes = Vec::new(&env);
        for idx in 0..stake_ids.len() {
            let stake_id = stake_ids.get_unchecked(idx);
            stakes.push_back(Self::load_stake(&env, stake_id));
        }
        stakes
    }

    pub fn get_pending_rewards(env: Env, stake_id: u64) -> i128 {
        let position = Self::load_stake(&env, stake_id);
        if !position.active {
            return 0;
        }
        let tier = Self::load_tier(&env, position.tier_id);
        Self::calculate_pending_rewards_static(&env, &position, tier.reward_multiplier_bps)
    }

    pub fn get_tier(env: Env, tier_id: u32) -> StakingTier {
        Self::load_tier(&env, tier_id)
    }

    pub fn get_tier_ids(env: Env) -> Vec<u32> {
        Self::tier_ids(&env)
    }

    pub fn get_stake_counter(env: Env) -> u64 {
        Self::stake_counter(&env)
    }

    pub fn get_staking_info(env: Env) -> StakingInfo {
        StakingInfo {
            admin: Self::admin(&env),
            reward_token: Self::reward_token(&env),
            total_staked: Self::total_staked(&env),
            total_rewards_distributed: Self::total_rewards_distributed(&env),
            reward_rate_per_second: Self::reward_rate_per_second(&env),
            last_reward_time: Self::last_reward_time(&env),
            paused: Self::paused(&env),
            tier_count: Self::tier_ids(&env).len(),
        }
    }

    pub fn calculate_apy(env: Env, tier_id: u32) -> i128 {
        let tier = Self::load_tier(&env, tier_id);
        let total_staked = Self::total_staked(&env);
        let reward_rate = Self::reward_rate_per_second(&env);

        if total_staked <= 0 || reward_rate <= 0 {
            return 0;
        }

        let seconds_per_year: i128 = 31_557_600; // 365.25 days
        let annual_rewards = reward_rate
            .checked_mul(seconds_per_year)
            .expect("Annual rewards overflow");

        let adjusted_rewards = annual_rewards
            .checked_mul(tier.reward_multiplier_bps as i128)
            .expect("Adjusted rewards overflow");

        adjusted_rewards
            .checked_mul(PRECISION_FACTOR)
            .expect("APY overflow")
            / total_staked
            / BPS_DENOMINATOR
    }

    // ── INTERNAL HELPERS ───────────────────────────────────────

    /// Calculate the weighted stake amount for a position
    fn calculate_weight(amount: i128, multiplier_bps: u32) -> i128 {
        // weight = amount * multiplier_bps / BPS_DENOMINATOR
        // We keep this in base units (not scaled by PRECISION_FACTOR)
        // The reward_per_token accumulator is scaled by PRECISION_FACTOR
        amount
            .checked_mul(multiplier_bps as i128)
            .expect("Weight overflow")
            / BPS_DENOMINATOR
    }

    /// Update the global reward_per_token accumulator
    fn update_pool(env: &Env) {
        let last_reward_time = Self::last_reward_time(env);
        let now = env.ledger().timestamp();
        if now <= last_reward_time {
            return;
        }

        let total_weighted = Self::total_weighted_stake(env);
        let current_rpt = Self::reward_per_token_stored(env);

        if total_weighted <= 0 {
            // No stakers, just advance time
            env.storage().instance().set(&DataKey::LastRewardTime, &now);
            return;
        }

        let reward_rate = Self::reward_rate_per_second(env);
        let elapsed = now
            .checked_sub(last_reward_time)
            .expect("Time elapsed underflow");

        // reward_per_token_increment = reward_rate * elapsed * PRECISION_FACTOR / total_weighted
        let increment = reward_rate
            .checked_mul(elapsed as i128)
            .expect("Reward increment overflow")
            .checked_mul(PRECISION_FACTOR)
            .expect("Reward increment precision overflow")
            / total_weighted;

        let new_rpt = current_rpt
            .checked_add(increment)
            .expect("Reward per token overflow");

        env.storage()
            .instance()
            .set(&DataKey::RewardPerTokenStored, &new_rpt);
        env.storage().instance().set(&DataKey::LastRewardTime, &now);
    }

    /// Calculate pending rewards for a position (read-only, does not modify state)
    fn calculate_pending_rewards_static(
        env: &Env,
        position: &StakePosition,
        multiplier_bps: u32,
    ) -> i128 {
        if !position.active {
            return 0;
        }

        let total_weighted = Self::total_weighted_stake(env);
        if total_weighted <= 0 {
            return 0;
        }

        // Calculate what reward_per_token would be if pool were updated now
        let last_reward_time = Self::last_reward_time(env);
        let now = env.ledger().timestamp();
        let current_rpt = Self::reward_per_token_stored(env);

        let future_rpt = if now > last_reward_time {
            let reward_rate = Self::reward_rate_per_second(env);
            let elapsed = now - last_reward_time;
            let increment = reward_rate
                .checked_mul(elapsed as i128)
                .expect("Reward increment overflow")
                .checked_mul(PRECISION_FACTOR)
                .expect("Reward increment precision overflow")
                / total_weighted;
            current_rpt
                .checked_add(increment)
                .expect("Reward per token overflow")
        } else {
            current_rpt
        };

        // pending = (future_rpt - user_rpt_paid) * user_weight / PRECISION_FACTOR
        let user_weight = Self::calculate_weight(position.amount, multiplier_bps);

        let rpt_diff = future_rpt
            .checked_sub(position.reward_per_token_paid)
            .expect("RPT diff underflow");

        rpt_diff
            .checked_mul(user_weight)
            .expect("Pending rewards overflow")
            / PRECISION_FACTOR
    }

    fn calculate_penalty(amount: i128, penalty_bps: u32) -> i128 {
        if penalty_bps == 0 {
            return 0;
        }
        amount
            .checked_mul(penalty_bps as i128)
            .expect("Penalty multiplication overflow")
            / BPS_DENOMINATOR
    }

    fn next_stake_id(env: &Env) -> u64 {
        let current = Self::stake_counter(env);
        let next = current.checked_add(1).expect("Stake ID overflow");
        env.storage().instance().set(&DataKey::StakeCounter, &next);
        next
    }

    fn next_tier_id(env: &Env) -> u32 {
        let current = Self::tier_counter(env);
        let next = current.checked_add(1).expect("Tier ID overflow");
        env.storage().instance().set(&DataKey::TierCounter, &next);
        next
    }

    fn load_stake(env: &Env, stake_id: u64) -> StakePosition {
        if stake_id == 0 {
            panic!("Invalid stake ID");
        }
        env.storage()
            .instance()
            .get(&DataKey::StakePosition(stake_id))
            .expect("Stake not found")
    }

    fn load_tier(env: &Env, tier_id: u32) -> StakingTier {
        if tier_id == 0 {
            panic!("Invalid tier ID");
        }
        env.storage()
            .instance()
            .get(&DataKey::Tier(tier_id))
            .expect("Tier not found")
    }

    fn user_stake_ids(env: &Env, user: &Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::UserStakeIds(user.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    fn add_user_stake(env: &Env, user: &Address, stake_id: u64) {
        let mut stake_ids = Self::user_stake_ids(env, user);
        stake_ids.push_back(stake_id);
        env.storage()
            .instance()
            .set(&DataKey::UserStakeIds(user.clone()), &stake_ids);
    }

    fn tier_ids(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&DataKey::TierIds)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn stake_counter(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::StakeCounter)
            .unwrap_or(0u64)
    }

    fn tier_counter(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::TierCounter)
            .unwrap_or(0u32)
    }

    fn total_staked(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalStaked)
            .unwrap_or(0i128)
    }

    fn total_weighted_stake(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalWeightedStake)
            .unwrap_or(0i128)
    }

    fn total_rewards_distributed(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalRewardsDistributed)
            .unwrap_or(0i128)
    }

    fn last_reward_time(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LastRewardTime)
            .unwrap_or(0u64)
    }

    fn reward_per_token_stored(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::RewardPerTokenStored)
            .unwrap_or(0i128)
    }

    fn reward_rate_per_second(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::RewardRatePerSecond)
            .unwrap_or(0i128)
    }

    fn paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn reward_token(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::RewardToken)
            .expect("Contract not initialized")
    }

    fn admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized")
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin = Self::admin(env);
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    fn transfer_token(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
        Self::enter_non_reentrant(env);
        Self::transfer_token_unchecked(env, token, from, to, amount);
        Self::exit_non_reentrant(env);
    }

    fn transfer_token_unchecked(
        env: &Env,
        token: &Address,
        from: &Address,
        to: &Address,
        amount: i128,
    ) {
        if amount <= 0 {
            panic!("Transfer amount must be positive");
        }
        let token_client = TokenClient::new(env, token);
        token_client.transfer(from, to, &amount);
    }

    fn enter_non_reentrant(env: &Env) {
        let locked = env
            .storage()
            .instance()
            .get(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        if locked {
            panic!("Reentrant call blocked");
        }
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    }

    fn exit_non_reentrant(env: &Env) {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);
    }
}

#[cfg(test)]
mod test;
