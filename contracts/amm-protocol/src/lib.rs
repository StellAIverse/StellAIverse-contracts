#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    TokenA,
    TokenB,
    ReserveA,
    ReserveB,
    FeeBps,
    TotalLpSupply,
    UserLpBalance(Address),
}

#[contract]
pub struct AmmProtocolContract;

#[contractimpl]
impl AmmProtocolContract {
    pub fn initialize(env: Env, admin: Address, token_a: Address, token_b: Address, fee_bps: u32) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TokenA, &token_a);
        env.storage().instance().set(&DataKey::TokenB, &token_b);
        env.storage().instance().set(&DataKey::ReserveA, &0i128);
        env.storage().instance().set(&DataKey::ReserveB, &0i128);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage().instance().set(&DataKey::TotalLpSupply, &0i128);
    }

    pub fn deposit_liquidity(env: Env, user: Address, amount_a: i128, amount_b: i128) -> i128 {
        user.require_auth();
        if amount_a <= 0 || amount_b <= 0 {
            panic!("amounts must be positive");
        }

        let reserve_a: i128 = env.storage().instance().get(&DataKey::ReserveA).unwrap_or(0);
        let reserve_b: i128 = env.storage().instance().get(&DataKey::ReserveB).unwrap_or(0);
        let total_lp: i128 = env.storage().instance().get(&DataKey::TotalLpSupply).unwrap_or(0);

        let lp_minted = if total_lp == 0 {
            amount_a + amount_b
        } else {
            let share_a = (amount_a * total_lp) / reserve_a;
            let share_b = (amount_b * total_lp) / reserve_b;
            if share_a < share_b { share_a } else { share_b }
        };

        env.storage().instance().set(&DataKey::ReserveA, &(reserve_a + amount_a));
        env.storage().instance().set(&DataKey::ReserveB, &(reserve_b + amount_b));
        env.storage().instance().set(&DataKey::TotalLpSupply, &(total_lp + lp_minted));

        let user_key = DataKey::UserLpBalance(user);
        let current_lp: i128 = env.storage().persistent().get(&user_key).unwrap_or(0);
        env.storage().persistent().set(&user_key, &(current_lp + lp_minted));

        lp_minted
    }

    pub fn withdraw_liquidity(env: Env, user: Address, lp_amount: i128) -> (i128, i128) {
        user.require_auth();
        if lp_amount <= 0 {
            panic!("lp amount must be positive");
        }

        let user_key = DataKey::UserLpBalance(user.clone());
        let current_lp: i128 = env.storage().persistent().get(&user_key).unwrap_or(0);
        if current_lp < lp_amount {
            panic!("insufficient LP balance");
        }

        let reserve_a: i128 = env.storage().instance().get(&DataKey::ReserveA).unwrap_or(0);
        let reserve_b: i128 = env.storage().instance().get(&DataKey::ReserveB).unwrap_or(0);
        let total_lp: i128 = env.storage().instance().get(&DataKey::TotalLpSupply).unwrap_or(0);

        let amount_a = (lp_amount * reserve_a) / total_lp;
        let amount_b = (lp_amount * reserve_b) / total_lp;

        env.storage().instance().set(&DataKey::ReserveA, &(reserve_a - amount_a));
        env.storage().instance().set(&DataKey::ReserveB, &(reserve_b - amount_b));
        env.storage().instance().set(&DataKey::TotalLpSupply, &(total_lp - lp_amount));
        env.storage().persistent().set(&user_key, &(current_lp - lp_amount));

        (amount_a, amount_b)
    }

    pub fn swap_exact_tokens_for_tokens(
        env: Env,
        user: Address,
        amount_in: i128,
        min_amount_out: i128,
        token_in: Address,
    ) -> i128 {
        user.require_auth();
        if amount_in <= 0 {
            panic!("amount_in must be positive");
        }

        let token_a: Address = env.storage().instance().get(&DataKey::TokenA).unwrap();
        let is_a = token_in == token_a;

        let (reserve_in, reserve_out) = if is_a {
            (
                env.storage().instance().get::<_, i128>(&DataKey::ReserveA).unwrap(),
                env.storage().instance().get::<_, i128>(&DataKey::ReserveB).unwrap(),
            )
        } else {
            (
                env.storage().instance().get::<_, i128>(&DataKey::ReserveB).unwrap(),
                env.storage().instance().get::<_, i128>(&DataKey::ReserveA).unwrap(),
            )
        };

        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(30); // 0.3%
        let amount_in_with_fee = amount_in * (10000 - fee_bps as i128);
        let amount_out = (amount_in_with_fee * reserve_out) / (reserve_in * 10000 + amount_in_with_fee);

        if amount_out < min_amount_out {
            panic!("insufficient output amount due to slippage");
        }

        if is_a {
            env.storage().instance().set(&DataKey::ReserveA, &(reserve_in + amount_in));
            env.storage().instance().set(&DataKey::ReserveB, &(reserve_out - amount_out));
        } else {
            env.storage().instance().set(&DataKey::ReserveB, &(reserve_in + amount_in));
            env.storage().instance().set(&DataKey::ReserveA, &(reserve_out - amount_out));
        }

        amount_out
    }

    pub fn get_reserves(env: Env) -> (i128, i128) {
        let r_a: i128 = env.storage().instance().get(&DataKey::ReserveA).unwrap_or(0);
        let r_b: i128 = env.storage().instance().get(&DataKey::ReserveB).unwrap_or(0);
        (r_a, r_b)
    }

    pub fn get_lp_balance(env: Env, user: Address) -> i128 {
        let user_key = DataKey::UserLpBalance(user);
        env.storage().persistent().get(&user_key).unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
