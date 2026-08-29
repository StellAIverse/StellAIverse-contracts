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
const DEFAULT_PERFORMANCE_FEE_BPS: u32 = 2_000;
const MAX_STRATEGIES: u32 = 20;
const DEFAULT_MIN_QUEUED_WITHDRAWAL: i128 = 1_000;

// ═══════════════════════════════════════════════════════════════
//  DATA TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    Paused,
    PerformanceFeeBps,
    TotalAssets,
    TotalSupply,
    TotalDeposits,
    TotalWithdrawals,
    TotalFeesCollected,
    HighWaterMark,
    UserDeposit(Address),
    StrategyCounter,
    Strategy(Symbol),
    StrategyIds,
    StrategyAllocBps(Symbol),
    WithdrawalQueueCounter,
    WithdrawalQueueItem(u64),
    WithdrawalQueueIds,
    MinQueuedWithdrawal,
    ReentrancyLock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct VaultUserDeposit {
    pub shares: i128,
    pub total_deposited: i128,
    pub total_withdrawn: i128,
    pub last_deposit_at: u64,
    pub last_withdrawal_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct StrategyConfig {
    pub strategy_id: Symbol,
    pub strategy_address: Address,
    pub allocated_assets: i128,
    pub current_balance: i128,
    pub total_gains: i128,
    pub total_losses: i128,
    pub is_active: bool,
    pub created_at: u64,
    pub last_harvest_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct WithdrawalQueueItem {
    pub request_id: u64,
    pub user: Address,
    pub shares: i128,
    pub estimated_assets: i128,
    pub requested_at: u64,
    pub processed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct HarvestResult {
    pub strategy_id: Symbol,
    pub gains: i128,
    pub performance_fee: i128,
    pub new_total_assets: i128,
    pub new_high_water_mark: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct VaultInfo {
    pub admin: Address,
    pub token: Address,
    pub total_assets: i128,
    pub total_supply: i128,
    pub total_deposits: i128,
    pub total_withdrawals: i128,
    pub total_fees_collected: i128,
    pub share_price: i128,
    pub performance_fee_bps: u32,
    pub high_water_mark: i128,
    pub paused: bool,
}

// ═══════════════════════════════════════════════════════════════
//  CONTRACT
// ═══════════════════════════════════════════════════════════════

#[contract]
pub struct Vault;

#[contractimpl]
impl Vault {
    // ── INITIALIZATION ─────────────────────────────────────────

    pub fn initialize(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::PerformanceFeeBps, &DEFAULT_PERFORMANCE_FEE_BPS);
        env.storage().instance().set(&DataKey::TotalAssets, &0i128);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposits, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalWithdrawals, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::TotalFeesCollected, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::HighWaterMark, &0i128);
        env.storage()
            .instance()
            .set(&DataKey::StrategyCounter, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::StrategyIds, &Vec::<Symbol>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalQueueCounter, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalQueueIds, &Vec::<u64>::new(&env));
        env.storage().instance().set(
            &DataKey::MinQueuedWithdrawal,
            &DEFAULT_MIN_QUEUED_WITHDRAWAL,
        );
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);

        env.events()
            .publish((symbol_short!("v_init"),), (admin, token));
    }

    // ── DEPOSIT / WITHDRAWAL ───────────────────────────────────

    pub fn deposit(env: Env, user: Address, amount: i128) -> i128 {
        user.require_auth();
        if Self::is_paused_fn(&env) {
            panic!("Vault is paused");
        }
        if amount <= 0 {
            panic!("Deposit amount must be positive");
        }

        let total_assets = Self::total_assets(&env);
        let total_supply = Self::total_supply(&env);
        let shares = Self::calculate_shares_for_deposit(amount, total_assets, total_supply);

        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        Self::transfer_token(&env, &token, &user, &contract_address, amount);

        let new_total_assets = total_assets
            .checked_add(amount)
            .expect("Total assets overflow");
        let new_total_supply = total_supply
            .checked_add(shares)
            .expect("Total supply overflow");
        let new_total_deposits = Self::total_deposits(&env)
            .checked_add(amount)
            .expect("Total deposits overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalAssets, &new_total_assets);
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total_supply);
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposits, &new_total_deposits);

        // Update high water mark so deposits don't trigger false performance fees
        let hwm = Self::high_water_mark(&env);
        if new_total_assets > hwm {
            env.storage()
                .instance()
                .set(&DataKey::HighWaterMark, &new_total_assets);
        }

        let mut user_deposit = Self::load_user_deposit(&env, &user);
        user_deposit.shares = user_deposit
            .shares
            .checked_add(shares)
            .expect("User shares overflow");
        user_deposit.total_deposited = user_deposit
            .total_deposited
            .checked_add(amount)
            .expect("User deposited overflow");
        user_deposit.last_deposit_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::UserDeposit(user.clone()), &user_deposit);

        env.events().publish(
            (symbol_short!("v_dep"),),
            (user, amount, shares, new_total_assets, new_total_supply),
        );
        shares
    }

    pub fn withdraw(env: Env, user: Address, shares: i128) -> i128 {
        user.require_auth();
        if shares <= 0 {
            panic!("Shares must be positive");
        }

        let user_deposit = Self::load_user_deposit(&env, &user);
        if user_deposit.shares < shares {
            panic!("Insufficient shares");
        }

        let total_assets = Self::total_assets(&env);
        let total_supply = Self::total_supply(&env);
        if total_supply <= 0 {
            panic!("No assets in vault");
        }

        let gross_assets =
            Self::calculate_assets_for_withdrawal(shares, total_assets, total_supply);

        let hwm = Self::high_water_mark(&env);
        let performance_fee = if total_assets > hwm {
            let gains = total_assets.checked_sub(hwm).expect("Gains underflow");
            gains
                .checked_mul(Self::performance_fee_bps(&env) as i128)
                .expect("Fee calc overflow")
                / BPS_DENOMINATOR
        } else {
            0
        };

        let net_assets = gross_assets
            .checked_sub(performance_fee)
            .expect("Net assets underflow");

        let new_total_supply = total_supply
            .checked_sub(shares)
            .expect("Total supply underflow");
        let new_total_assets = total_assets
            .checked_sub(gross_assets)
            .expect("Total assets underflow");
        let new_total_withdrawals = Self::total_withdrawals(&env)
            .checked_add(net_assets)
            .expect("Total withdrawals overflow");

        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total_supply);
        env.storage()
            .instance()
            .set(&DataKey::TotalAssets, &new_total_assets);
        env.storage()
            .instance()
            .set(&DataKey::TotalWithdrawals, &new_total_withdrawals);

        if performance_fee > 0 {
            Self::collect_fee(&env, performance_fee);
        }

        let mut user_dep = user_deposit;
        user_dep.shares = user_dep
            .shares
            .checked_sub(shares)
            .expect("User shares underflow");
        user_dep.total_withdrawn = user_dep
            .total_withdrawn
            .checked_add(net_assets)
            .expect("User withdrawn overflow");
        user_dep.last_withdrawal_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::UserDeposit(user.clone()), &user_dep);

        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &user, net_assets);
        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("v_wth"),),
            (
                user,
                shares,
                gross_assets,
                performance_fee,
                net_assets,
                new_total_assets,
                new_total_supply,
            ),
        );
        net_assets
    }

    pub fn emergency_withdraw(env: Env, user: Address) -> i128 {
        user.require_auth();
        let user_deposit = Self::load_user_deposit(&env, &user);
        if user_deposit.shares <= 0 {
            panic!("No shares to withdraw");
        }

        let shares = user_deposit.shares;
        let total_assets = Self::total_assets(&env);
        let total_supply = Self::total_supply(&env);
        if total_supply <= 0 {
            panic!("No assets in vault");
        }

        let assets = Self::calculate_assets_for_withdrawal(shares, total_assets, total_supply);

        let new_total_supply = total_supply
            .checked_sub(shares)
            .expect("Total supply underflow");
        let new_total_assets = total_assets
            .checked_sub(assets)
            .expect("Total assets underflow");
        let new_total_withdrawals = Self::total_withdrawals(&env)
            .checked_add(assets)
            .expect("Total withdrawals overflow");

        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total_supply);
        env.storage()
            .instance()
            .set(&DataKey::TotalAssets, &new_total_assets);
        env.storage()
            .instance()
            .set(&DataKey::TotalWithdrawals, &new_total_withdrawals);

        let cleared = VaultUserDeposit {
            shares: 0,
            total_deposited: user_deposit.total_deposited,
            total_withdrawn: user_deposit
                .total_withdrawn
                .checked_add(assets)
                .expect("User withdrawn overflow"),
            last_deposit_at: user_deposit.last_deposit_at,
            last_withdrawal_at: env.ledger().timestamp(),
        };
        env.storage()
            .instance()
            .set(&DataKey::UserDeposit(user.clone()), &cleared);

        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &user, assets);
        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("v_emw"),),
            (user, shares, assets, new_total_assets, new_total_supply),
        );
        assets
    }

    // ── GOVERNANCE ─────────────────────────────────────────────

    pub fn governance_withdraw(env: Env, admin: Address, recipient: Address, amount: i128) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let total_assets = Self::total_assets(&env);
        if amount > total_assets {
            panic!("Insufficient vault assets");
        }

        let new_total_assets = total_assets.checked_sub(amount).expect("Underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalAssets, &new_total_assets);

        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &recipient, amount);
        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("v_gov"),),
            (admin, recipient, amount, new_total_assets),
        );
    }

    // ── PAUSE / RESUME ─────────────────────────────────────────

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if Self::is_paused_fn(&env) {
            panic!("Already paused");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((symbol_short!("v_pause"),), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if !Self::is_paused_fn(&env) {
            panic!("Not paused");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish((symbol_short!("v_unpse"),), admin);
    }

    // ── FEE MANAGEMENT ─────────────────────────────────────────

    pub fn set_performance_fee(env: Env, admin: Address, fee_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if fee_bps > 5_000 {
            panic!("Fee exceeds maximum of 50%");
        }
        env.storage()
            .instance()
            .set(&DataKey::PerformanceFeeBps, &fee_bps);
        env.events()
            .publish((symbol_short!("v_fee"),), (admin, fee_bps));
    }

    pub fn withdraw_fees(env: Env, admin: Address, recipient: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let total_fees = Self::total_fees_collected(&env);
        if total_fees <= 0 {
            panic!("No fees to withdraw");
        }

        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &recipient, total_fees);
        Self::exit_non_reentrant(&env);

        env.storage()
            .instance()
            .set(&DataKey::TotalFeesCollected, &0i128);
        env.events()
            .publish((symbol_short!("v_fewd"),), (admin, recipient, total_fees));
    }

    // ── STRATEGY MANAGEMENT ────────────────────────────────────

    pub fn add_strategy(env: Env, admin: Address, strategy_id: Symbol, strategy_address: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if env
            .storage()
            .instance()
            .has(&DataKey::Strategy(strategy_id.clone()))
        {
            panic!("Strategy already exists");
        }

        let strategy_ids = Self::strategy_ids(&env);
        if strategy_ids.len() >= MAX_STRATEGIES {
            panic!("Maximum strategies reached");
        }

        let config = StrategyConfig {
            strategy_id: strategy_id.clone(),
            strategy_address: strategy_address.clone(),
            allocated_assets: 0,
            current_balance: 0,
            total_gains: 0,
            total_losses: 0,
            is_active: true,
            created_at: env.ledger().timestamp(),
            last_harvest_at: 0,
        };
        env.storage()
            .instance()
            .set(&DataKey::Strategy(strategy_id.clone()), &config);
        env.storage()
            .instance()
            .set(&DataKey::StrategyAllocBps(strategy_id.clone()), &0u32);

        let mut ids = strategy_ids;
        ids.push_back(strategy_id.clone());
        let counter = Self::strategy_counter(&env);
        env.storage()
            .instance()
            .set(&DataKey::StrategyCounter, &(counter + 1));
        env.storage().instance().set(&DataKey::StrategyIds, &ids);

        env.events()
            .publish((symbol_short!("v_sadd"),), (strategy_id, strategy_address));
    }

    pub fn remove_strategy(env: Env, admin: Address, strategy_id: Symbol) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let mut config = Self::load_strategy(&env, &strategy_id);
        if !config.is_active {
            panic!("Strategy not active");
        }
        if config.allocated_assets > 0 {
            panic!("Cannot remove strategy with allocated assets. Migrate first.");
        }

        config.is_active = false;
        env.storage()
            .instance()
            .set(&DataKey::Strategy(strategy_id.clone()), &config);
        env.storage()
            .instance()
            .set(&DataKey::StrategyAllocBps(strategy_id.clone()), &0u32);
        env.events()
            .publish((symbol_short!("v_srem"),), (strategy_id,));
    }

    pub fn set_strategy_allocation(env: Env, admin: Address, strategy_id: Symbol, alloc_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let mut config = Self::load_strategy(&env, &strategy_id);
        if !config.is_active {
            panic!("Strategy not active");
        }

        let old_alloc = Self::strategy_alloc_bps(&env, &strategy_id) as i128;
        let total_alloc = Self::total_allocation_bps(&env) as i128;
        let new_total = total_alloc
            .checked_sub(old_alloc)
            .expect("Total alloc underflow")
            .checked_add(alloc_bps as i128)
            .expect("Total alloc overflow");
        if new_total > BPS_DENOMINATOR {
            panic!("Total allocations exceed 100%");
        }

        env.storage()
            .instance()
            .set(&DataKey::StrategyAllocBps(strategy_id.clone()), &alloc_bps);

        let total_assets = Self::total_assets(&env);
        let new_target = total_assets
            .checked_mul(alloc_bps as i128)
            .expect("Target calc overflow")
            / BPS_DENOMINATOR;

        config.allocated_assets = new_target;
        config.current_balance = new_target;

        env.storage()
            .instance()
            .set(&DataKey::Strategy(strategy_id.clone()), &config);
        env.events().publish(
            (symbol_short!("v_salc"),),
            (strategy_id, alloc_bps, config.allocated_assets),
        );
    }

    pub fn migrate_strategy(
        env: Env,
        admin: Address,
        from_strategy_id: Symbol,
        to_strategy_id: Symbol,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let from_config = Self::load_strategy(&env, &from_strategy_id);
        if !from_config.is_active {
            panic!("Source strategy not active");
        }
        let to_config = Self::load_strategy(&env, &to_strategy_id);
        if !to_config.is_active {
            panic!("Target strategy not active");
        }

        let migrated_amount = from_config.allocated_assets;
        if migrated_amount <= 0 {
            panic!("No assets to migrate");
        }
        // Accounting-only migration: vault retains custody of all tokens.

        let mut updated_from = from_config;
        updated_from.allocated_assets = 0;
        env.storage()
            .instance()
            .set(&DataKey::Strategy(from_strategy_id.clone()), &updated_from);

        let mut updated_to = to_config;
        updated_to.allocated_assets = updated_to
            .allocated_assets
            .checked_add(migrated_amount)
            .expect("Target alloc overflow");
        env.storage()
            .instance()
            .set(&DataKey::Strategy(to_strategy_id.clone()), &updated_to);

        env.events().publish(
            (symbol_short!("v_smig"),),
            (from_strategy_id, to_strategy_id, migrated_amount),
        );
    }

    // ── HARVEST / FEE COLLECTION ───────────────────────────────

    pub fn harvest_strategy(
        env: Env,
        strategy_id: Symbol,
        reported_balance: i128,
    ) -> HarvestResult {
        let mut config = Self::load_strategy(&env, &strategy_id);
        if !config.is_active {
            panic!("Strategy not active");
        }

        let current_balance = reported_balance;
        let gains = if current_balance > config.current_balance {
            current_balance
                .checked_sub(config.current_balance)
                .expect("Gains underflow")
        } else {
            0
        };
        let losses = if current_balance < config.current_balance {
            config
                .current_balance
                .checked_sub(current_balance)
                .expect("Losses underflow")
        } else {
            0
        };

        let performance_fee = if gains > 0 {
            gains
                .checked_mul(Self::performance_fee_bps(&env) as i128)
                .expect("Fee calc overflow")
                / BPS_DENOMINATOR
        } else {
            0
        };

        let total_assets = Self::total_assets(&env);
        let new_total_assets = if current_balance > config.current_balance {
            let net_gain = gains
                .checked_sub(performance_fee)
                .expect("Net gain underflow");
            total_assets
                .checked_add(net_gain)
                .expect("Total assets overflow")
        } else if current_balance < config.current_balance {
            let net_loss = config
                .current_balance
                .checked_sub(current_balance)
                .expect("Net loss underflow");
            total_assets
                .checked_sub(net_loss)
                .expect("Total assets underflow")
        } else {
            total_assets
        };

        let hwm = Self::high_water_mark(&env);
        let new_hwm = if new_total_assets > hwm {
            new_total_assets
        } else {
            hwm
        };

        config.current_balance = current_balance;
        config.total_gains = config
            .total_gains
            .checked_add(gains)
            .expect("Gains overflow");
        config.total_losses = config
            .total_losses
            .checked_add(losses)
            .expect("Losses overflow");
        config.last_harvest_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Strategy(strategy_id.clone()), &config);

        env.storage()
            .instance()
            .set(&DataKey::TotalAssets, &new_total_assets);
        env.storage()
            .instance()
            .set(&DataKey::HighWaterMark, &new_hwm);

        if performance_fee > 0 {
            Self::collect_fee(&env, performance_fee);
        }

        env.events().publish(
            (symbol_short!("v_harv"),),
            (
                strategy_id.clone(),
                gains,
                losses,
                performance_fee,
                new_total_assets,
                new_hwm,
            ),
        );

        HarvestResult {
            strategy_id,
            gains,
            performance_fee,
            new_total_assets,
            new_high_water_mark: new_hwm,
        }
    }

    pub fn emergency_strategy_withdrawal(env: Env, admin: Address, strategy_id: Symbol) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        let mut config = Self::load_strategy(&env, &strategy_id);
        if !config.is_active {
            panic!("Strategy not active");
        }

        let recalled_amount = config.allocated_assets;
        if recalled_amount <= 0 {
            return;
        }
        // Accounting-only: vault retains custody of all tokens
        config.allocated_assets = 0;
        config.current_balance = 0;
        env.storage()
            .instance()
            .set(&DataKey::Strategy(strategy_id.clone()), &config);
        env.events()
            .publish((symbol_short!("v_semg"),), (strategy_id, recalled_amount));
    }

    // ── WITHDRAWAL QUEUE ───────────────────────────────────────

    pub fn queue_withdrawal(env: Env, user: Address, shares: i128) -> u64 {
        user.require_auth();
        if shares <= 0 {
            panic!("Shares must be positive");
        }

        let user_deposit = Self::load_user_deposit(&env, &user);
        if user_deposit.shares < shares {
            panic!("Insufficient shares");
        }

        let total_assets = Self::total_assets(&env);
        let total_supply = Self::total_supply(&env);
        if total_supply <= 0 {
            panic!("No assets in vault");
        }

        let estimated_assets =
            Self::calculate_assets_for_withdrawal(shares, total_assets, total_supply);
        let min_queued = Self::min_queued_withdrawal(&env);
        if estimated_assets < min_queued {
            panic!("Below minimum queued withdrawal amount");
        }

        let mut updated = user_deposit;
        updated.shares = updated.shares.checked_sub(shares).expect("Underflow");
        updated.last_withdrawal_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::UserDeposit(user.clone()), &updated);

        let request_id = Self::next_withdrawal_queue_id(&env);
        let item = WithdrawalQueueItem {
            request_id,
            user: user.clone(),
            shares,
            estimated_assets,
            requested_at: env.ledger().timestamp(),
            processed: false,
        };
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalQueueItem(request_id), &item);

        let mut queue_ids = Self::withdrawal_queue_ids(&env);
        queue_ids.push_back(request_id);
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalQueueIds, &queue_ids);

        env.events().publish(
            (symbol_short!("v_wq"),),
            (user, request_id, shares, estimated_assets),
        );
        request_id
    }

    pub fn process_withdrawal_queue(env: Env, request_id: u64) -> i128 {
        let mut item = Self::load_queue_item(&env, &request_id);
        if item.processed {
            panic!("Already processed");
        }

        let total_assets = Self::total_assets(&env);
        let total_supply = Self::total_supply(&env);
        let assets = Self::calculate_assets_for_withdrawal(item.shares, total_assets, total_supply);

        let new_total_supply = total_supply.checked_sub(item.shares).expect("Underflow");
        let new_total_assets = total_assets.checked_sub(assets).expect("Underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total_supply);
        env.storage()
            .instance()
            .set(&DataKey::TotalAssets, &new_total_assets);

        let new_total_withdrawals = Self::total_withdrawals(&env)
            .checked_add(assets)
            .expect("Overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalWithdrawals, &new_total_withdrawals);

        item.processed = true;
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalQueueItem(request_id), &item);

        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        Self::transfer_token_unchecked(&env, &token, &contract_address, &item.user, assets);
        Self::exit_non_reentrant(&env);

        env.events().publish(
            (symbol_short!("v_wqpr"),),
            (request_id, item.user.clone(), item.shares, assets),
        );
        assets
    }

    // ── VIEW FUNCTIONS ─────────────────────────────────────────

    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }
    pub fn get_token(env: Env) -> Address {
        Self::token(&env)
    }
    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_fn(&env)
    }
    pub fn get_total_assets(env: Env) -> i128 {
        Self::total_assets(&env)
    }
    pub fn get_total_supply(env: Env) -> i128 {
        Self::total_supply(&env)
    }
    pub fn get_total_deposits(env: Env) -> i128 {
        Self::total_deposits(&env)
    }
    pub fn get_total_withdrawals(env: Env) -> i128 {
        Self::total_withdrawals(&env)
    }
    pub fn get_total_fees_collected(env: Env) -> i128 {
        Self::total_fees_collected(&env)
    }
    pub fn get_performance_fee_bps(env: Env) -> u32 {
        Self::performance_fee_bps(&env)
    }
    pub fn get_high_water_mark(env: Env) -> i128 {
        Self::high_water_mark(&env)
    }
    pub fn get_user_deposit(env: Env, user: Address) -> VaultUserDeposit {
        Self::load_user_deposit(&env, &user)
    }
    pub fn get_share_price(env: Env) -> i128 {
        Self::share_price(&env)
    }
    pub fn get_strategy(env: Env, strategy_id: Symbol) -> StrategyConfig {
        Self::load_strategy(&env, &strategy_id)
    }
    pub fn get_strategy_ids(env: Env) -> Vec<Symbol> {
        Self::strategy_ids(&env)
    }
    pub fn get_strategy_alloc_bps(env: Env, strategy_id: Symbol) -> u32 {
        Self::strategy_alloc_bps(&env, &strategy_id)
    }
    pub fn get_total_allocation_bps(env: Env) -> u32 {
        Self::total_allocation_bps(&env)
    }
    pub fn get_min_queued_withdrawal(env: Env) -> i128 {
        Self::min_queued_withdrawal(&env)
    }
    pub fn get_withdrawal_queue_counter(env: Env) -> u64 {
        Self::withdrawal_queue_counter(&env)
    }
    pub fn get_withdrawal_queue_item(env: Env, request_id: u64) -> WithdrawalQueueItem {
        Self::load_queue_item(&env, &request_id)
    }
    pub fn get_withdrawal_queue_ids(env: Env) -> Vec<u64> {
        Self::withdrawal_queue_ids(&env)
    }
    pub fn preview_deposit(env: Env, amount: i128) -> i128 {
        Self::calculate_shares_for_deposit(
            amount,
            Self::total_assets(&env),
            Self::total_supply(&env),
        )
    }
    pub fn preview_withdraw(env: Env, shares: i128) -> i128 {
        Self::calculate_assets_for_withdrawal(
            shares,
            Self::total_assets(&env),
            Self::total_supply(&env),
        )
    }

    pub fn get_vault_info(env: Env) -> VaultInfo {
        VaultInfo {
            admin: Self::admin(&env),
            token: Self::token(&env),
            total_assets: Self::total_assets(&env),
            total_supply: Self::total_supply(&env),
            total_deposits: Self::total_deposits(&env),
            total_withdrawals: Self::total_withdrawals(&env),
            total_fees_collected: Self::total_fees_collected(&env),
            share_price: Self::share_price(&env),
            performance_fee_bps: Self::performance_fee_bps(&env),
            high_water_mark: Self::high_water_mark(&env),
            paused: Self::is_paused_fn(&env),
        }
    }

    // ── INTERNAL HELPERS ───────────────────────────────────────

    fn calculate_shares_for_deposit(amount: i128, total_assets: i128, total_supply: i128) -> i128 {
        if total_assets <= 0 || total_supply <= 0 {
            amount
        } else {
            amount.checked_mul(total_supply).expect("Overflow") / total_assets
        }
    }

    fn calculate_assets_for_withdrawal(
        shares: i128,
        total_assets: i128,
        total_supply: i128,
    ) -> i128 {
        shares.checked_mul(total_assets).expect("Overflow") / total_supply
    }

    fn share_price(env: &Env) -> i128 {
        let total_supply = Self::total_supply(env);
        if total_supply <= 0 {
            return 10_000;
        }
        let total_assets = Self::total_assets(env);
        total_assets.checked_mul(10_000).expect("Price overflow") / total_supply
    }

    fn collect_fee(env: &Env, fee: i128) {
        if fee <= 0 {
            return;
        }
        let new_total = Self::total_fees_collected(env)
            .checked_add(fee)
            .expect("Overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalFeesCollected, &new_total);
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

    fn admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }
    fn token(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .expect("Not initialized")
    }
    fn is_paused_fn(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }
    fn assert_admin(env: &Env, caller: &Address) {
        let admin = Self::admin(env);
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    fn total_assets(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAssets)
            .unwrap_or(0)
    }
    fn total_supply(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }
    fn total_deposits(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalDeposits)
            .unwrap_or(0)
    }
    fn total_withdrawals(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalWithdrawals)
            .unwrap_or(0)
    }
    fn total_fees_collected(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalFeesCollected)
            .unwrap_or(0)
    }
    fn high_water_mark(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::HighWaterMark)
            .unwrap_or(0)
    }
    fn performance_fee_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::PerformanceFeeBps)
            .unwrap_or(DEFAULT_PERFORMANCE_FEE_BPS)
    }

    fn load_user_deposit(env: &Env, user: &Address) -> VaultUserDeposit {
        env.storage()
            .instance()
            .get(&DataKey::UserDeposit(user.clone()))
            .unwrap_or(VaultUserDeposit {
                shares: 0,
                total_deposited: 0,
                total_withdrawn: 0,
                last_deposit_at: 0,
                last_withdrawal_at: 0,
            })
    }

    fn load_strategy(env: &Env, strategy_id: &Symbol) -> StrategyConfig {
        env.storage()
            .instance()
            .get(&DataKey::Strategy(strategy_id.clone()))
            .expect("Strategy not found")
    }

    fn strategy_ids(env: &Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&DataKey::StrategyIds)
            .unwrap_or_else(|| Vec::new(env))
    }
    fn strategy_counter(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::StrategyCounter)
            .unwrap_or(0)
    }
    fn strategy_alloc_bps(env: &Env, strategy_id: &Symbol) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::StrategyAllocBps(strategy_id.clone()))
            .unwrap_or(0)
    }

    fn total_allocation_bps(env: &Env) -> u32 {
        let ids = Self::strategy_ids(env);
        let mut total: u32 = 0;
        for idx in 0..ids.len() {
            let id = ids.get_unchecked(idx);
            let config = Self::load_strategy(env, &id);
            if config.is_active {
                total = total
                    .checked_add(Self::strategy_alloc_bps(env, &id))
                    .expect("Overflow");
            }
        }
        total
    }

    fn load_queue_item(env: &Env, request_id: &u64) -> WithdrawalQueueItem {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalQueueItem(*request_id))
            .expect("Request not found")
    }

    fn withdrawal_queue_ids(env: &Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalQueueIds)
            .unwrap_or_else(|| Vec::new(env))
    }
    fn withdrawal_queue_counter(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawalQueueCounter)
            .unwrap_or(0)
    }
    fn next_withdrawal_queue_id(env: &Env) -> u64 {
        let current = Self::withdrawal_queue_counter(env);
        let next = current.checked_add(1).expect("Overflow");
        env.storage()
            .instance()
            .set(&DataKey::WithdrawalQueueCounter, &next);
        next
    }
    fn min_queued_withdrawal(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MinQueuedWithdrawal)
            .unwrap_or(DEFAULT_MIN_QUEUED_WITHDRAWAL)
    }
}

#[cfg(test)]
mod test;
