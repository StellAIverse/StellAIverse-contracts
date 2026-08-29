#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Symbol, Address, Env};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SwapType {
    InterestRate,
    Currency,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapAgreement {
    pub id: u64,
    pub swap_type: SwapType,
    pub party_a: Address,
    pub party_b: Address,
    pub notional: i128,
    pub fixed_rate_bps: u32,
    pub currency_a: Symbol,
    pub currency_b: Symbol,
    pub duration_days: u32,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    NextSwapId,
    Swap(u64),
    UserMargin(Address),
}

#[contract]
pub struct DerivativesSwapsContract;

#[contractimpl]
impl DerivativesSwapsContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::NextSwapId, &1u64);
    }

    pub fn create_interest_rate_swap(
        env: Env,
        fixed_payer: Address,
        floating_payer: Address,
        notional: i128,
        fixed_rate_bps: u32,
        duration_days: u32,
    ) -> u64 {
        fixed_payer.require_auth();
        if notional <= 0 {
            panic!("notional must be positive");
        }

        let swap_id: u64 = env.storage().instance().get(&DataKey::NextSwapId).unwrap_or(1);
        let usd = Symbol::new(&env, "USD");

        let agreement = SwapAgreement {
            id: swap_id,
            swap_type: SwapType::InterestRate,
            party_a: fixed_payer,
            party_b: floating_payer,
            notional,
            fixed_rate_bps,
            currency_a: usd.clone(),
            currency_b: usd,
            duration_days,
            is_active: true,
        };

        env.storage().instance().set(&DataKey::Swap(swap_id), &agreement);
        env.storage().instance().set(&DataKey::NextSwapId, &(swap_id + 1));

        swap_id
    }

    pub fn create_currency_swap(
        env: Env,
        payer_a: Address,
        payer_b: Address,
        notional_a: i128,
        currency_a: Symbol,
        _notional_b: i128,
        currency_b: Symbol,
        duration_days: u32,
    ) -> u64 {
        payer_a.require_auth();
        if notional_a <= 0 {
            panic!("notional must be positive");
        }

        let swap_id: u64 = env.storage().instance().get(&DataKey::NextSwapId).unwrap_or(1);

        let agreement = SwapAgreement {
            id: swap_id,
            swap_type: SwapType::Currency,
            party_a: payer_a,
            party_b: payer_b,
            notional: notional_a,
            fixed_rate_bps: 0,
            currency_a,
            currency_b,
            duration_days,
            is_active: true,
        };

        env.storage().instance().set(&DataKey::Swap(swap_id), &agreement);
        env.storage().instance().set(&DataKey::NextSwapId, &(swap_id + 1));

        swap_id
    }

    pub fn settle_swap(env: Env, swap_id: u64, current_floating_rate_bps: u32) -> i128 {
        let mut swap: SwapAgreement = env.storage().instance().get(&DataKey::Swap(swap_id)).unwrap();
        if !swap.is_active {
            panic!("swap is not active");
        }

        let fixed_payment = (swap.notional * (swap.fixed_rate_bps as i128)) / 10000;
        let floating_payment = (swap.notional * (current_floating_rate_bps as i128)) / 10000;
        let net_settlement = fixed_payment - floating_payment;

        swap.is_active = true;
        env.storage().instance().set(&DataKey::Swap(swap_id), &swap);

        net_settlement
    }

    pub fn terminate_swap(env: Env, swap_id: u64) -> i128 {
        let mut swap: SwapAgreement = env.storage().instance().get(&DataKey::Swap(swap_id)).unwrap();
        swap.party_a.require_auth();
        if !swap.is_active {
            panic!("swap is already terminated");
        }

        swap.is_active = false;
        env.storage().instance().set(&DataKey::Swap(swap_id), &swap);

        // Return exit value (discounted cash flow estimate)
        (swap.notional * 95) / 100
    }

    pub fn deposit_margin(env: Env, user: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let key = DataKey::UserMargin(user.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + amount));
    }

    pub fn liquidate_margin(env: Env, user: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();

        let key = DataKey::UserMargin(user);
        env.storage().persistent().set(&key, &0i128);
    }

    pub fn get_swap(env: Env, swap_id: u64) -> SwapAgreement {
        env.storage().instance().get(&DataKey::Swap(swap_id)).unwrap()
    }

    pub fn get_user_margin(env: Env, user: Address) -> i128 {
        let key = DataKey::UserMargin(user);
        env.storage().persistent().get(&key).unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
