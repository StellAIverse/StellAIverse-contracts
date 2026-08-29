#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, Address, Env, Symbol,
    Vec,
};

const BPS_DENOMINATOR: i128 = 10_000;
/// Absolute ceiling on the fee rate, enforced even for emergency adjustments.
const HARD_CAP_FEE_RATE_BPS: u32 = 2_000;
/// Maximum change per `set_fee_rate` call; larger jumps require `emergency_set_fee_rate`.
const MAX_FEE_RATE_STEP_BPS: u32 = 500;
const MAX_CATEGORIES: u32 = 20;
const MAX_BATCH_SIZE: u32 = 50;
const MAX_HISTORY_LIMIT: u32 = 100;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    Paused,
    FeeRateBps,
    MaxFeeRateBps,
    TotalCollected,
    TotalAllocated,
    TotalSharesBps,
    CategoryIds,
    Category(Symbol),
    Exempt(Address),
    DistributionCounter,
    Distribution(u64),
    ReentrancyLock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RecipientCategory {
    pub category_id: Symbol,
    pub recipient: Address,
    pub share_bps: u32,
    pub total_allocated: i128,
    pub total_claimed: i128,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CategoryAllocation {
    pub category_id: Symbol,
    pub amount: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct DistributionRecord {
    pub distribution_id: u64,
    pub total_amount: i128,
    pub category_count: u32,
    pub timestamp: u64,
    pub allocations: Vec<CategoryAllocation>,
}

#[contract]
pub struct FeeDistribution;

#[contractimpl]
impl FeeDistribution {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        fee_rate_bps: u32,
        max_fee_rate_bps: u32,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();
        Self::validate_max_rate(max_fee_rate_bps);
        if fee_rate_bps > max_fee_rate_bps {
            panic!("Fee rate exceeds max fee rate");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::FeeRateBps, &fee_rate_bps);
        env.storage()
            .instance()
            .set(&DataKey::MaxFeeRateBps, &max_fee_rate_bps);
        env.storage()
            .instance()
            .set(&DataKey::TotalCollected, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocated, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalSharesBps, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::CategoryIds, &Vec::<Symbol>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::DistributionCounter, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);

        env.events()
            .publish((symbol_short!("fee_init"),), (admin, token, fee_rate_bps));
    }

    // ----- Admin controls -----

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if Self::paused(&env) {
            panic!("Already paused");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((symbol_short!("fee_pause"),), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if !Self::paused(&env) {
            panic!("Not paused");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish((symbol_short!("fee_resm"),), admin);
    }

    pub fn set_fee_rate(env: Env, admin: Address, new_rate_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let max_rate = Self::max_fee_rate(&env);
        if new_rate_bps > max_rate {
            panic!("Fee rate exceeds max fee rate");
        }
        let current = Self::fee_rate(&env);
        let delta = new_rate_bps.abs_diff(current);
        if delta > MAX_FEE_RATE_STEP_BPS {
            panic!("Rate change exceeds max step; use emergency_set_fee_rate");
        }

        env.storage()
            .instance()
            .set(&DataKey::FeeRateBps, &new_rate_bps);
        env.events()
            .publish((symbol_short!("fee_rate"),), (admin, new_rate_bps));
    }

    pub fn emergency_set_fee_rate(env: Env, admin: Address, new_rate_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let max_rate = Self::max_fee_rate(&env);
        if new_rate_bps > max_rate {
            panic!("Fee rate exceeds max fee rate");
        }

        env.storage()
            .instance()
            .set(&DataKey::FeeRateBps, &new_rate_bps);
        env.events()
            .publish((symbol_short!("fee_emg"),), (admin, new_rate_bps));
    }

    pub fn set_max_fee_rate(env: Env, admin: Address, new_max_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        Self::validate_max_rate(new_max_bps);

        env.storage()
            .instance()
            .set(&DataKey::MaxFeeRateBps, &new_max_bps);
        if Self::fee_rate(&env) > new_max_bps {
            env.storage()
                .instance()
                .set(&DataKey::FeeRateBps, &new_max_bps);
        }
        env.events()
            .publish((symbol_short!("fee_maxr"),), (admin, new_max_bps));
    }

    pub fn add_category(
        env: Env,
        admin: Address,
        category_id: Symbol,
        recipient: Address,
        share_bps: u32,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if env
            .storage()
            .instance()
            .has(&DataKey::Category(category_id.clone()))
        {
            panic!("Category already exists");
        }
        Self::validate_share(share_bps);

        let mut category_ids = Self::category_ids(&env);
        if category_ids.len() >= MAX_CATEGORIES {
            panic!("Maximum categories reached");
        }

        let total_shares = Self::total_shares_bps(&env);
        let new_total = total_shares
            .checked_add(share_bps)
            .expect("Share total overflow");
        if new_total > BPS_DENOMINATOR as u32 {
            panic!("Total shares exceed 100%");
        }

        let category = RecipientCategory {
            category_id: category_id.clone(),
            recipient: recipient.clone(),
            share_bps,
            total_allocated: 0,
            total_claimed: 0,
            active: true,
        };
        env.storage()
            .instance()
            .set(&DataKey::Category(category_id.clone()), &category);

        category_ids.push_back(category_id.clone());
        env.storage()
            .instance()
            .set(&DataKey::CategoryIds, &category_ids);
        env.storage()
            .instance()
            .set(&DataKey::TotalSharesBps, &new_total);

        env.events().publish(
            (symbol_short!("fee_cat"),),
            (category_id, recipient, share_bps),
        );
    }

    pub fn update_category(
        env: Env,
        admin: Address,
        category_id: Symbol,
        new_recipient: Option<Address>,
        new_share_bps: Option<u32>,
    ) -> RecipientCategory {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut category = Self::load_category(&env, &category_id);
        if !category.active {
            panic!("Category is not active");
        }

        if let Some(share_bps) = new_share_bps {
            Self::validate_share(share_bps);
            let total_shares = Self::total_shares_bps(&env);
            let remaining = total_shares
                .checked_sub(category.share_bps)
                .expect("Share total underflow");
            let new_total = remaining
                .checked_add(share_bps)
                .expect("Share total overflow");
            if new_total > BPS_DENOMINATOR as u32 {
                panic!("Total shares exceed 100%");
            }
            env.storage()
                .instance()
                .set(&DataKey::TotalSharesBps, &new_total);
            category.share_bps = share_bps;
        }

        if let Some(recipient) = new_recipient {
            category.recipient = recipient;
        }

        env.storage()
            .instance()
            .set(&DataKey::Category(category_id.clone()), &category);
        env.events()
            .publish((symbol_short!("fee_catu"),), (category_id, admin));
        category
    }

    pub fn remove_category(env: Env, admin: Address, category_id: Symbol) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut category = Self::load_category(&env, &category_id);
        if !category.active {
            panic!("Category already inactive");
        }

        let total_shares = Self::total_shares_bps(&env);
        let new_total = total_shares
            .checked_sub(category.share_bps)
            .expect("Share total underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSharesBps, &new_total);

        category.active = false;
        category.share_bps = 0;
        env.storage()
            .instance()
            .set(&DataKey::Category(category_id.clone()), &category);

        env.events()
            .publish((symbol_short!("fee_catr"),), (category_id, admin));
    }

    pub fn reactivate_category(env: Env, admin: Address, category_id: Symbol, share_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut category = Self::load_category(&env, &category_id);
        if category.active {
            panic!("Category already active");
        }
        Self::validate_share(share_bps);

        let total_shares = Self::total_shares_bps(&env);
        let new_total = total_shares
            .checked_add(share_bps)
            .expect("Share total overflow");
        if new_total > BPS_DENOMINATOR as u32 {
            panic!("Total shares exceed 100%");
        }
        env.storage()
            .instance()
            .set(&DataKey::TotalSharesBps, &new_total);

        category.active = true;
        category.share_bps = share_bps;
        env.storage()
            .instance()
            .set(&DataKey::Category(category_id.clone()), &category);

        env.events()
            .publish((symbol_short!("fee_catv"),), (category_id, admin));
    }

    pub fn set_exempt(env: Env, admin: Address, account: Address, exempt: bool) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        env.storage()
            .instance()
            .set(&DataKey::Exempt(account.clone()), &exempt);
        env.events()
            .publish((symbol_short!("fee_exm"),), (account, exempt));
    }

    // ----- Fee collection -----

    /// Computes the fee owed on `amount` at the current fee rate.
    ///
    /// Example: with a fee rate of 250 bps (2.5%), `calculate_fee(10_000)` returns `250`.
    pub fn calculate_fee(env: Env, amount: i128) -> i128 {
        if amount <= 0 {
            panic!("Amount must be positive");
        }
        let rate = Self::fee_rate(&env);
        amount
            .checked_mul(rate as i128)
            .expect("Fee multiplication overflow")
            / BPS_DENOMINATOR
    }

    pub fn collect_fee(env: Env, payer: Address, gross_amount: i128) -> i128 {
        payer.require_auth();
        if Self::paused(&env) {
            panic!("Fee collection is paused");
        }
        if gross_amount <= 0 {
            panic!("Amount must be positive");
        }

        if Self::is_exempt(env.clone(), payer.clone()) {
            env.events()
                .publish((symbol_short!("fee_col"),), (payer, gross_amount, 0i128));
            return 0;
        }

        let fee = Self::calculate_fee(env.clone(), gross_amount);
        if fee <= 0 {
            return 0;
        }

        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        Self::transfer_token(&env, &token, &payer, &contract_address, fee);

        let total_collected = Self::total_collected(&env)
            .checked_add(fee)
            .expect("Total collected overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalCollected, &total_collected);

        env.events()
            .publish((symbol_short!("fee_col"),), (payer, gross_amount, fee));
        fee
    }

    // ----- Distribution -----

    pub fn distribute(env: Env) -> DistributionRecord {
        let total_shares = Self::total_shares_bps(&env);
        if total_shares != BPS_DENOMINATOR as u32 {
            panic!("Recipient shares are not fully configured");
        }

        let total_collected = Self::total_collected(&env);
        let total_allocated = Self::total_allocated(&env);
        let pending = total_collected
            .checked_sub(total_allocated)
            .expect("Pending underflow");
        if pending <= 0 {
            panic!("No pending fees to distribute");
        }

        let category_ids = Self::category_ids(&env);
        let mut allocations = Vec::new(&env);
        let mut allocated_sum: i128 = 0;
        let mut active_count: u32 = 0;
        for idx in 0..category_ids.len() {
            let category_id = category_ids.get_unchecked(idx);
            let category = Self::load_category(&env, &category_id);
            if category.active && category.share_bps > 0 {
                active_count += 1;
            }
        }
        if active_count == 0 {
            panic!("No active recipient categories");
        }

        let mut processed: u32 = 0;
        for idx in 0..category_ids.len() {
            let category_id = category_ids.get_unchecked(idx);
            let mut category = Self::load_category(&env, &category_id);
            if !category.active || category.share_bps == 0 {
                continue;
            }
            processed += 1;

            let mut amount = pending
                .checked_mul(category.share_bps as i128)
                .expect("Allocation multiplication overflow")
                / BPS_DENOMINATOR;

            if processed == active_count {
                // Assign rounding remainder to the last active category so the
                // sum of allocations always equals `pending` exactly.
                let remainder = pending
                    .checked_sub(allocated_sum)
                    .expect("Remainder underflow")
                    .checked_sub(amount)
                    .expect("Remainder underflow");
                amount = amount.checked_add(remainder).expect("Remainder overflow");
            }

            category.total_allocated = category
                .total_allocated
                .checked_add(amount)
                .expect("Category allocation overflow");
            env.storage()
                .instance()
                .set(&DataKey::Category(category_id.clone()), &category);

            allocated_sum = allocated_sum
                .checked_add(amount)
                .expect("Allocated sum overflow");
            allocations.push_back(CategoryAllocation {
                category_id,
                amount,
            });
        }

        let new_total_allocated = total_allocated
            .checked_add(pending)
            .expect("Total allocated overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalAllocated, &new_total_allocated);

        let distribution_id = Self::next_distribution_id(&env);
        let record = DistributionRecord {
            distribution_id,
            total_amount: pending,
            category_count: active_count,
            timestamp: env.ledger().timestamp(),
            allocations,
        };
        env.storage()
            .instance()
            .set(&DataKey::Distribution(distribution_id), &record);

        env.events().publish(
            (symbol_short!("fee_dist"),),
            (distribution_id, pending, active_count),
        );

        record
    }

    pub fn claim(env: Env, category_id: Symbol, recipient: Address) -> i128 {
        recipient.require_auth();
        let paid = Self::claim_one(&env, &category_id, &recipient, true);
        if paid <= 0 {
            panic!("Nothing to claim");
        }
        paid
    }

    pub fn claim_batch(env: Env, category_ids: Vec<Symbol>, recipient: Address) -> i128 {
        recipient.require_auth();
        if category_ids.is_empty() {
            panic!("No categories provided");
        }
        if category_ids.len() > MAX_BATCH_SIZE {
            panic!("Batch exceeds maximum size");
        }

        let mut total_paid: i128 = 0;
        for idx in 0..category_ids.len() {
            let category_id = category_ids.get_unchecked(idx);
            let paid = Self::claim_one(&env, &category_id, &recipient, false);
            total_paid = total_paid.checked_add(paid).expect("Claim total overflow");
        }
        if total_paid <= 0 {
            panic!("Nothing to claim");
        }

        env.events().publish(
            (symbol_short!("fee_bclm"),),
            (recipient, total_paid, category_ids.len()),
        );
        total_paid
    }

    // ----- Views -----

    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    pub fn get_token(env: Env) -> Address {
        Self::token(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        Self::paused(&env)
    }

    pub fn get_fee_rate(env: Env) -> u32 {
        Self::fee_rate(&env)
    }

    pub fn get_max_fee_rate(env: Env) -> u32 {
        Self::max_fee_rate(&env)
    }

    pub fn is_exempt(env: Env, account: Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Exempt(account))
            .unwrap_or(false)
    }

    pub fn get_category(env: Env, category_id: Symbol) -> RecipientCategory {
        Self::load_category(&env, &category_id)
    }

    pub fn get_category_ids(env: Env) -> Vec<Symbol> {
        Self::category_ids(&env)
    }

    pub fn get_claimable(env: Env, category_id: Symbol) -> i128 {
        let category = Self::load_category(&env, &category_id);
        category
            .total_allocated
            .checked_sub(category.total_claimed)
            .expect("Claimable underflow")
    }

    pub fn get_total_shares_bps(env: Env) -> u32 {
        Self::total_shares_bps(&env)
    }

    pub fn get_total_collected(env: Env) -> i128 {
        Self::total_collected(&env)
    }

    pub fn get_total_allocated(env: Env) -> i128 {
        Self::total_allocated(&env)
    }

    pub fn get_total_claimed(env: Env) -> i128 {
        let category_ids = Self::category_ids(&env);
        let mut total: i128 = 0;
        for idx in 0..category_ids.len() {
            let category_id = category_ids.get_unchecked(idx);
            let category = Self::load_category(&env, &category_id);
            total = total
                .checked_add(category.total_claimed)
                .expect("Total claimed overflow");
        }
        total
    }

    pub fn get_pending_distribution(env: Env) -> i128 {
        Self::total_collected(&env)
            .checked_sub(Self::total_allocated(&env))
            .expect("Pending underflow")
    }

    pub fn get_distribution_counter(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DistributionCounter)
            .unwrap_or(0u64)
    }

    pub fn get_distribution(env: Env, distribution_id: u64) -> DistributionRecord {
        env.storage()
            .instance()
            .get(&DataKey::Distribution(distribution_id))
            .expect("Distribution not found")
    }

    /// Returns up to `limit` distribution records, most recent first, starting
    /// `offset` records back from the latest distribution.
    pub fn get_distribution_history(env: Env, offset: u32, limit: u32) -> Vec<DistributionRecord> {
        let bounded_limit = if limit > MAX_HISTORY_LIMIT {
            MAX_HISTORY_LIMIT
        } else {
            limit
        };
        let counter = Self::get_distribution_counter(env.clone());
        let mut records = Vec::new(&env);
        if bounded_limit == 0 || counter <= offset as u64 {
            return records;
        }

        let start = counter - offset as u64;
        let mut fetched: u32 = 0;
        let mut id = start;
        while id >= 1 && fetched < bounded_limit {
            records.push_back(Self::get_distribution(env.clone(), id));
            fetched += 1;
            id -= 1;
        }
        records
    }

    // ----- Internal helpers -----

    fn claim_one(
        env: &Env,
        category_id: &Symbol,
        recipient: &Address,
        require_positive: bool,
    ) -> i128 {
        let mut category = Self::load_category(env, category_id);
        if category.recipient != *recipient {
            if require_positive {
                panic!("Only recipient can claim");
            }
            return 0;
        }

        let claimable = category
            .total_allocated
            .checked_sub(category.total_claimed)
            .expect("Claimable underflow");
        if claimable <= 0 {
            if require_positive {
                panic!("Nothing to claim");
            }
            return 0;
        }

        category.total_claimed = category
            .total_claimed
            .checked_add(claimable)
            .expect("Claimed overflow");
        env.storage()
            .instance()
            .set(&DataKey::Category(category_id.clone()), &category);

        let token = Self::token(env);
        let contract_address = env.current_contract_address();
        Self::transfer_token(env, &token, &contract_address, recipient, claimable);

        env.events().publish(
            (symbol_short!("fee_clm"),),
            (category_id.clone(), recipient.clone(), claimable),
        );

        claimable
    }

    fn next_distribution_id(env: &Env) -> u64 {
        let current = env
            .storage()
            .instance()
            .get(&DataKey::DistributionCounter)
            .unwrap_or(0u64);
        let next = current.checked_add(1).expect("Distribution ID overflow");
        env.storage()
            .instance()
            .set(&DataKey::DistributionCounter, &next);
        next
    }

    fn load_category(env: &Env, category_id: &Symbol) -> RecipientCategory {
        env.storage()
            .instance()
            .get(&DataKey::Category(category_id.clone()))
            .expect("Category not found")
    }

    fn category_ids(env: &Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&DataKey::CategoryIds)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn total_shares_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSharesBps)
            .unwrap_or(0u32)
    }

    fn total_collected(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalCollected)
            .unwrap_or(0i128)
    }

    fn total_allocated(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAllocated)
            .unwrap_or(0i128)
    }

    fn fee_rate(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeeRateBps)
            .expect("Contract not initialized")
    }

    fn max_fee_rate(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxFeeRateBps)
            .expect("Contract not initialized")
    }

    fn paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn token(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Token)
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

    fn validate_share(share_bps: u32) {
        if share_bps == 0 || share_bps > BPS_DENOMINATOR as u32 {
            panic!("Share must be between 1 and 10000 bps");
        }
    }

    fn validate_max_rate(max_fee_rate_bps: u32) {
        if max_fee_rate_bps == 0 || max_fee_rate_bps > HARD_CAP_FEE_RATE_BPS {
            panic!("Max fee rate exceeds hard cap");
        }
    }

    fn transfer_token(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
        Self::enter_non_reentrant(env);
        if amount <= 0 {
            panic!("Transfer amount must be positive");
        }
        let token_client = TokenClient::new(env, token);
        token_client.transfer(from, to, &amount);
        Self::exit_non_reentrant(env);
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
