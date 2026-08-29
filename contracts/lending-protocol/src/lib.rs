#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserAccountData {
    pub total_collateral_usd: i128,
    pub total_debt_usd: i128,
    pub health_factor: u32, // 10000 = 1.0 (Health factor >= 1.0 is healthy)
    pub ltv_bps: u32,        // e.g. 7500 = 75%
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    ReserveFactor,
    Paused,
    UserCollateral(Address),
    UserDebt(Address),
}

#[contract]
pub struct LendingProtocolContract;

#[contractimpl]
impl LendingProtocolContract {
    pub fn initialize(env: Env, admin: Address, reserve_factor: u32) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ReserveFactor, &reserve_factor);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    pub fn deposit_collateral(env: Env, user: Address, _token: Address, amount: i128) {
        user.require_auth();
        Self::ensure_not_paused(&env);
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let key = DataKey::UserCollateral(user.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + amount));
    }

    pub fn borrow(env: Env, user: Address, _asset: Address, amount: i128) {
        user.require_auth();
        Self::ensure_not_paused(&env);
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let collateral_key = DataKey::UserCollateral(user.clone());
        let debt_key = DataKey::UserDebt(user.clone());

        let collateral: i128 = env.storage().persistent().get(&collateral_key).unwrap_or(0);
        let current_debt: i128 = env.storage().persistent().get(&debt_key).unwrap_or(0);
        let new_debt = current_debt + amount;

        // Max borrow = 75% of collateral (LTV = 7500 bps)
        let max_borrow = (collateral * 7500) / 10000;
        if new_debt > max_borrow {
            panic!("borrow exceeds LTV limit");
        }

        env.storage().persistent().set(&debt_key, &new_debt);
    }

    pub fn repay(env: Env, user: Address, _asset: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let debt_key = DataKey::UserDebt(user.clone());
        let current_debt: i128 = env.storage().persistent().get(&debt_key).unwrap_or(0);
        let repay_amount = if amount > current_debt { current_debt } else { amount };

        env.storage().persistent().set(&debt_key, &(current_debt - repay_amount));
    }

    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        _collateral_token: Address,
        _debt_token: Address,
    ) {
        liquidator.require_auth();
        let account_data = Self::get_user_account_data(env.clone(), borrower.clone());

        // Liquidation triggered if health factor drops below 1.0 (10000 bps)
        if account_data.health_factor >= 10000 {
            panic!("account is healthy, cannot liquidate");
        }

        let debt_key = DataKey::UserDebt(borrower.clone());
        let collateral_key = DataKey::UserCollateral(borrower.clone());

        let _debt: i128 = env.storage().persistent().get(&debt_key).unwrap_or(0);
        let collateral: i128 = env.storage().persistent().get(&collateral_key).unwrap_or(0);

        // Liquidator repays debt, receives collateral with 10% incentive bonus
        let reward = (collateral * 11000) / 10000;
        let final_collateral = if reward > collateral { 0 } else { collateral - reward };

        env.storage().persistent().set(&debt_key, &0i128);
        env.storage().persistent().set(&collateral_key, &final_collateral);
    }

    pub fn get_health_factor(env: Env, user: Address) -> u32 {
        let account_data = Self::get_user_account_data(env, user);
        account_data.health_factor
    }

    pub fn get_user_account_data(env: Env, user: Address) -> UserAccountData {
        let collateral_key = DataKey::UserCollateral(user.clone());
        let debt_key = DataKey::UserDebt(user);

        let collateral: i128 = env.storage().persistent().get(&collateral_key).unwrap_or(0);
        let debt: i128 = env.storage().persistent().get(&debt_key).unwrap_or(0);

        let health_factor = if debt == 0 {
            20000 // Infinite health factor (represented as 2.0 = 20000 bps)
        } else {
            let max_borrow = (collateral * 7500) / 10000;
            ((max_borrow * 10000) / debt) as u32
        };

        UserAccountData {
            total_collateral_usd: collateral,
            total_debt_usd: debt,
            health_factor,
            ltv_bps: 7500,
        }
    }

    pub fn set_emergency_pause(env: Env, paused: bool) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &paused);
    }

    fn ensure_not_paused(env: &Env) {
        let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        if paused {
            panic!("protocol is paused");
        }
    }
}

#[cfg(test)]
mod test;
