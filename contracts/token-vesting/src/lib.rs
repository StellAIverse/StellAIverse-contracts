#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, Address, Env, Vec,
};

const BPS_DENOMINATOR: i128 = 10_000;
const MAX_PENALTY_BPS: u32 = 10_000;
const MAX_BATCH_SIZE: u32 = 50;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    ScheduleCounter,
    Schedule(u64),
    BeneficiarySchedules(Address),
    ReentrancyLock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct VestingSchedule {
    pub schedule_id: u64,
    pub token: Address,
    pub funder: Address,
    pub beneficiary: Address,
    pub total_amount: i128,
    pub claimed_amount: i128,
    pub start_time: u64,
    pub cliff_time: u64,
    pub end_time: u64,
    pub revocable: bool,
    pub early_withdrawal_allowed: bool,
    pub early_penalty_bps: u32,
    pub active: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_claimed_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct EarlyWithdrawalResult {
    pub schedule_id: u64,
    pub vested_paid: i128,
    pub unvested_paid: i128,
    pub penalty_amount: i128,
    pub total_paid: i128,
}

#[contract]
pub struct TokenVesting;

#[contractimpl]
impl TokenVesting {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ScheduleCounter, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);

        env.events().publish((symbol_short!("vest_init"),), admin);
    }

    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    pub fn create_schedule(
        env: Env,
        funder: Address,
        token: Address,
        beneficiary: Address,
        total_amount: i128,
        start_time: u64,
        cliff_duration_seconds: u64,
        duration_seconds: u64,
        revocable: bool,
        early_withdrawal_allowed: bool,
        early_penalty_bps: u32,
    ) -> u64 {
        funder.require_auth();
        Self::validate_amount(total_amount);
        Self::validate_timing(start_time, cliff_duration_seconds, duration_seconds);
        Self::validate_penalty(early_penalty_bps);

        let contract_address = env.current_contract_address();
        Self::transfer_token(&env, &token, &funder, &contract_address, total_amount);

        let schedule_id = Self::next_schedule_id(&env);
        Self::store_schedule(
            &env,
            schedule_id,
            &funder,
            &token,
            &beneficiary,
            total_amount,
            start_time,
            cliff_duration_seconds,
            duration_seconds,
            revocable,
            early_withdrawal_allowed,
            early_penalty_bps,
        );

        schedule_id
    }

    pub fn create_schedules_batch(
        env: Env,
        funder: Address,
        token: Address,
        beneficiaries: Vec<Address>,
        amounts: Vec<i128>,
        start_time: u64,
        cliff_duration_seconds: u64,
        duration_seconds: u64,
        revocable: bool,
        early_withdrawal_allowed: bool,
        early_penalty_bps: u32,
    ) -> Vec<u64> {
        funder.require_auth();

        let count = beneficiaries.len();
        if count == 0 {
            panic!("Batch must contain at least one schedule");
        }
        if count > MAX_BATCH_SIZE {
            panic!("Batch exceeds maximum size");
        }
        if count != amounts.len() {
            panic!("Beneficiary and amount lengths differ");
        }
        Self::validate_timing(start_time, cliff_duration_seconds, duration_seconds);
        Self::validate_penalty(early_penalty_bps);

        let mut total_amount = 0i128;
        for idx in 0..count {
            let amount = amounts.get_unchecked(idx);
            Self::validate_amount(amount);
            total_amount = total_amount
                .checked_add(amount)
                .expect("Batch total overflow");
        }

        let contract_address = env.current_contract_address();
        Self::transfer_token(&env, &token, &funder, &contract_address, total_amount);

        let mut schedule_ids = Vec::new(&env);
        for idx in 0..count {
            let beneficiary = beneficiaries.get_unchecked(idx);
            let amount = amounts.get_unchecked(idx);
            let schedule_id = Self::next_schedule_id(&env);
            Self::store_schedule(
                &env,
                schedule_id,
                &funder,
                &token,
                &beneficiary,
                amount,
                start_time,
                cliff_duration_seconds,
                duration_seconds,
                revocable,
                early_withdrawal_allowed,
                early_penalty_bps,
            );
            schedule_ids.push_back(schedule_id);
        }

        env.events().publish(
            (symbol_short!("vest_bat"),),
            (funder, token, count, total_amount),
        );

        schedule_ids
    }

    pub fn claim(env: Env, schedule_id: u64, beneficiary: Address) -> i128 {
        beneficiary.require_auth();
        let paid = Self::claim_one(&env, schedule_id, &beneficiary, true);
        if paid <= 0 {
            panic!("No vested tokens available");
        }
        paid
    }

    pub fn claim_batch(env: Env, schedule_ids: Vec<u64>, beneficiary: Address) -> i128 {
        beneficiary.require_auth();
        if schedule_ids.is_empty() {
            panic!("No schedules provided");
        }
        if schedule_ids.len() > MAX_BATCH_SIZE {
            panic!("Batch exceeds maximum size");
        }

        let mut total_paid = 0i128;
        for idx in 0..schedule_ids.len() {
            let schedule_id = schedule_ids.get_unchecked(idx);
            let paid = Self::claim_one(&env, schedule_id, &beneficiary, false);
            total_paid = total_paid.checked_add(paid).expect("Claim total overflow");
        }
        if total_paid <= 0 {
            panic!("No vested tokens available");
        }

        env.events()
            .publish((symbol_short!("vest_bcl"),), (beneficiary, total_paid));
        total_paid
    }

    pub fn early_withdraw(
        env: Env,
        schedule_id: u64,
        beneficiary: Address,
    ) -> EarlyWithdrawalResult {
        beneficiary.require_auth();

        let mut schedule = Self::load_schedule(&env, schedule_id);
        if schedule.beneficiary != beneficiary {
            panic!("Only beneficiary can withdraw");
        }
        if !schedule.active {
            panic!("Schedule is not active");
        }
        if !schedule.early_withdrawal_allowed {
            panic!("Early withdrawal disabled");
        }

        let vested = Self::vested_for(&schedule, env.ledger().timestamp());
        let claimable = Self::claimable_from_vested(&schedule, vested);
        let unvested = schedule
            .total_amount
            .checked_sub(vested)
            .expect("Unvested underflow");
        let penalty_amount = Self::calculate_penalty(unvested, schedule.early_penalty_bps);
        let unvested_paid = unvested
            .checked_sub(penalty_amount)
            .expect("Penalty exceeds unvested amount");
        let total_paid = claimable
            .checked_add(unvested_paid)
            .expect("Early withdrawal payout overflow");

        if total_paid <= 0 && penalty_amount <= 0 {
            panic!("No tokens available for early withdrawal");
        }

        schedule.claimed_amount = schedule.total_amount;
        schedule.active = false;
        schedule.updated_at = env.ledger().timestamp();
        schedule.last_claimed_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);

        let contract_address = env.current_contract_address();
        Self::enter_non_reentrant(&env);
        if total_paid > 0 {
            Self::transfer_token_unchecked(
                &env,
                &schedule.token,
                &contract_address,
                &beneficiary,
                total_paid,
            );
        }
        if penalty_amount > 0 {
            Self::transfer_token_unchecked(
                &env,
                &schedule.token,
                &contract_address,
                &schedule.funder,
                penalty_amount,
            );
        }
        Self::exit_non_reentrant(&env);

        let result = EarlyWithdrawalResult {
            schedule_id,
            vested_paid: claimable,
            unvested_paid,
            penalty_amount,
            total_paid,
        };

        env.events().publish(
            (symbol_short!("vest_ear"),),
            (
                schedule_id,
                beneficiary,
                total_paid,
                penalty_amount,
                schedule.funder,
            ),
        );

        result
    }

    pub fn modify_schedule(
        env: Env,
        admin: Address,
        schedule_id: u64,
        new_beneficiary: Option<Address>,
        new_start_time: Option<u64>,
        new_cliff_duration_seconds: Option<u64>,
        new_duration_seconds: Option<u64>,
        new_early_withdrawal_allowed: Option<bool>,
        new_early_penalty_bps: Option<u32>,
    ) -> VestingSchedule {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut schedule = Self::load_schedule(&env, schedule_id);
        if !schedule.active {
            panic!("Cannot modify inactive schedule");
        }

        if let Some(beneficiary) = new_beneficiary {
            Self::remove_beneficiary_schedule(&env, &schedule.beneficiary, schedule_id);
            Self::add_beneficiary_schedule(&env, &beneficiary, schedule_id);
            schedule.beneficiary = beneficiary;
        }

        let current_cliff_duration = schedule
            .cliff_time
            .checked_sub(schedule.start_time)
            .expect("Stored cliff before start");
        let current_duration = schedule
            .end_time
            .checked_sub(schedule.start_time)
            .expect("Stored end before start");
        let start_time = new_start_time.unwrap_or(schedule.start_time);
        let cliff_duration = new_cliff_duration_seconds.unwrap_or(current_cliff_duration);
        let duration = new_duration_seconds.unwrap_or(current_duration);
        Self::validate_timing(start_time, cliff_duration, duration);

        schedule.start_time = start_time;
        schedule.cliff_time = start_time
            .checked_add(cliff_duration)
            .expect("Cliff timestamp overflow");
        schedule.end_time = start_time
            .checked_add(duration)
            .expect("End timestamp overflow");

        if let Some(allowed) = new_early_withdrawal_allowed {
            schedule.early_withdrawal_allowed = allowed;
        }
        if let Some(penalty_bps) = new_early_penalty_bps {
            Self::validate_penalty(penalty_bps);
            schedule.early_penalty_bps = penalty_bps;
        }

        let vested_after = Self::vested_for(&schedule, env.ledger().timestamp());
        if vested_after < schedule.claimed_amount {
            panic!("Modification would invalidate claimed tokens");
        }

        schedule.updated_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);

        env.events()
            .publish((symbol_short!("vest_mod"),), (schedule_id, admin));
        schedule
    }

    pub fn increase_schedule_amount(
        env: Env,
        funder: Address,
        schedule_id: u64,
        additional_amount: i128,
    ) -> VestingSchedule {
        funder.require_auth();
        Self::validate_amount(additional_amount);

        let mut schedule = Self::load_schedule(&env, schedule_id);
        if schedule.funder != funder {
            panic!("Only original funder can increase schedule");
        }
        if !schedule.active {
            panic!("Cannot fund inactive schedule");
        }

        schedule.total_amount = schedule
            .total_amount
            .checked_add(additional_amount)
            .expect("Schedule amount overflow");
        schedule.updated_at = env.ledger().timestamp();

        let contract_address = env.current_contract_address();
        Self::transfer_token(
            &env,
            &schedule.token,
            &funder,
            &contract_address,
            additional_amount,
        );
        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);

        env.events().publish(
            (symbol_short!("vest_add"),),
            (schedule_id, funder, additional_amount),
        );
        schedule
    }

    pub fn revoke_schedule(env: Env, admin: Address, schedule_id: u64) -> i128 {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut schedule = Self::load_schedule(&env, schedule_id);
        if !schedule.active {
            panic!("Schedule is not active");
        }
        if !schedule.revocable {
            panic!("Schedule is not revocable");
        }

        let now = env.ledger().timestamp();
        let vested = Self::vested_for(&schedule, now);
        let refund_amount = schedule
            .total_amount
            .checked_sub(vested)
            .expect("Refund underflow");

        schedule.total_amount = vested;
        if vested > schedule.claimed_amount {
            schedule.end_time = now;
        }
        schedule.active = schedule.claimed_amount < schedule.total_amount;
        schedule.updated_at = now;
        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);

        if refund_amount > 0 {
            let contract_address = env.current_contract_address();
            Self::transfer_token(
                &env,
                &schedule.token,
                &contract_address,
                &schedule.funder,
                refund_amount,
            );
        }

        env.events().publish(
            (symbol_short!("vest_rev"),),
            (schedule_id, admin, refund_amount),
        );

        refund_amount
    }

    pub fn get_schedule(env: Env, schedule_id: u64) -> VestingSchedule {
        Self::load_schedule(&env, schedule_id)
    }

    pub fn get_schedules_for_beneficiary(env: Env, beneficiary: Address) -> Vec<VestingSchedule> {
        let schedule_ids = Self::beneficiary_schedule_ids(&env, &beneficiary);
        let mut schedules = Vec::new(&env);
        for idx in 0..schedule_ids.len() {
            let schedule_id = schedule_ids.get_unchecked(idx);
            schedules.push_back(Self::load_schedule(&env, schedule_id));
        }
        schedules
    }

    pub fn vested_amount(env: Env, schedule_id: u64, timestamp: u64) -> i128 {
        let schedule = Self::load_schedule(&env, schedule_id);
        Self::vested_for(&schedule, timestamp)
    }

    pub fn claimable(env: Env, schedule_id: u64) -> i128 {
        let schedule = Self::load_schedule(&env, schedule_id);
        let vested = Self::vested_for(&schedule, env.ledger().timestamp());
        Self::claimable_from_vested(&schedule, vested)
    }

    pub fn get_schedule_counter(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::ScheduleCounter)
            .unwrap_or(0u64)
    }

    fn store_schedule(
        env: &Env,
        schedule_id: u64,
        funder: &Address,
        token: &Address,
        beneficiary: &Address,
        total_amount: i128,
        start_time: u64,
        cliff_duration_seconds: u64,
        duration_seconds: u64,
        revocable: bool,
        early_withdrawal_allowed: bool,
        early_penalty_bps: u32,
    ) {
        let now = env.ledger().timestamp();
        let schedule = VestingSchedule {
            schedule_id,
            token: token.clone(),
            funder: funder.clone(),
            beneficiary: beneficiary.clone(),
            total_amount,
            claimed_amount: 0,
            start_time,
            cliff_time: start_time
                .checked_add(cliff_duration_seconds)
                .expect("Cliff timestamp overflow"),
            end_time: start_time
                .checked_add(duration_seconds)
                .expect("End timestamp overflow"),
            revocable,
            early_withdrawal_allowed,
            early_penalty_bps,
            active: true,
            created_at: now,
            updated_at: now,
            last_claimed_at: 0,
        };

        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        Self::add_beneficiary_schedule(env, beneficiary, schedule_id);

        env.events().publish(
            (symbol_short!("vest_new"),),
            (
                schedule_id,
                funder.clone(),
                beneficiary.clone(),
                total_amount,
            ),
        );
    }

    fn claim_one(
        env: &Env,
        schedule_id: u64,
        beneficiary: &Address,
        require_positive: bool,
    ) -> i128 {
        let mut schedule = Self::load_schedule(env, schedule_id);
        if schedule.beneficiary != *beneficiary {
            panic!("Only beneficiary can claim");
        }
        if !schedule.active {
            if require_positive {
                panic!("Schedule is not active");
            }
            return 0;
        }

        let vested = Self::vested_for(&schedule, env.ledger().timestamp());
        let claimable = Self::claimable_from_vested(&schedule, vested);
        if claimable <= 0 {
            if require_positive {
                panic!("No vested tokens available");
            }
            return 0;
        }

        schedule.claimed_amount = schedule
            .claimed_amount
            .checked_add(claimable)
            .expect("Claim overflow");
        schedule.last_claimed_at = env.ledger().timestamp();
        schedule.updated_at = env.ledger().timestamp();
        if schedule.claimed_amount >= schedule.total_amount {
            schedule.active = false;
        }
        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);

        let contract_address = env.current_contract_address();
        Self::transfer_token(
            env,
            &schedule.token,
            &contract_address,
            beneficiary,
            claimable,
        );

        env.events().publish(
            (symbol_short!("vest_clm"),),
            (schedule_id, beneficiary.clone(), claimable),
        );

        claimable
    }

    fn vested_for(schedule: &VestingSchedule, timestamp: u64) -> i128 {
        if timestamp < schedule.cliff_time || timestamp <= schedule.start_time {
            return 0;
        }
        if timestamp >= schedule.end_time {
            return schedule.total_amount;
        }

        let elapsed = timestamp
            .checked_sub(schedule.start_time)
            .expect("Timestamp before schedule start");
        let duration = schedule
            .end_time
            .checked_sub(schedule.start_time)
            .expect("Schedule end before start");
        if duration == 0 {
            return schedule.total_amount;
        }

        schedule
            .total_amount
            .checked_mul(elapsed as i128)
            .expect("Vesting multiplication overflow")
            / duration as i128
    }

    fn claimable_from_vested(schedule: &VestingSchedule, vested: i128) -> i128 {
        if vested <= schedule.claimed_amount {
            0
        } else {
            vested
                .checked_sub(schedule.claimed_amount)
                .expect("Claimable underflow")
        }
    }

    fn calculate_penalty(unvested: i128, penalty_bps: u32) -> i128 {
        if unvested <= 0 || penalty_bps == 0 {
            return 0;
        }
        unvested
            .checked_mul(penalty_bps as i128)
            .expect("Penalty multiplication overflow")
            / BPS_DENOMINATOR
    }

    fn next_schedule_id(env: &Env) -> u64 {
        let current = env
            .storage()
            .instance()
            .get(&DataKey::ScheduleCounter)
            .unwrap_or(0u64);
        let next = current.checked_add(1).expect("Schedule ID overflow");
        env.storage()
            .instance()
            .set(&DataKey::ScheduleCounter, &next);
        next
    }

    fn load_schedule(env: &Env, schedule_id: u64) -> VestingSchedule {
        if schedule_id == 0 {
            panic!("Invalid schedule ID");
        }
        env.storage()
            .instance()
            .get(&DataKey::Schedule(schedule_id))
            .expect("Schedule not found")
    }

    fn beneficiary_schedule_ids(env: &Env, beneficiary: &Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::BeneficiarySchedules(beneficiary.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    fn add_beneficiary_schedule(env: &Env, beneficiary: &Address, schedule_id: u64) {
        let mut schedule_ids = Self::beneficiary_schedule_ids(env, beneficiary);
        schedule_ids.push_back(schedule_id);
        env.storage().instance().set(
            &DataKey::BeneficiarySchedules(beneficiary.clone()),
            &schedule_ids,
        );
    }

    fn remove_beneficiary_schedule(env: &Env, beneficiary: &Address, schedule_id: u64) {
        let mut schedule_ids = Self::beneficiary_schedule_ids(env, beneficiary);
        let mut idx = 0;
        while idx < schedule_ids.len() {
            if schedule_ids.get_unchecked(idx) == schedule_id {
                schedule_ids.remove(idx);
                break;
            }
            idx += 1;
        }
        env.storage().instance().set(
            &DataKey::BeneficiarySchedules(beneficiary.clone()),
            &schedule_ids,
        );
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
            .expect("Contract not initialized")
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin = Self::admin(env);
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    fn validate_amount(amount: i128) {
        if amount <= 0 {
            panic!("Amount must be positive");
        }
    }

    fn validate_penalty(penalty_bps: u32) {
        if penalty_bps > MAX_PENALTY_BPS {
            panic!("Penalty exceeds 100%");
        }
    }

    fn validate_timing(start_time: u64, cliff_duration_seconds: u64, duration_seconds: u64) {
        if duration_seconds == 0 {
            panic!("Duration must be positive");
        }
        if cliff_duration_seconds > duration_seconds {
            panic!("Cliff cannot exceed duration");
        }
        start_time
            .checked_add(cliff_duration_seconds)
            .expect("Cliff timestamp overflow");
        start_time
            .checked_add(duration_seconds)
            .expect("End timestamp overflow");
    }
}

#[cfg(test)]
mod test;
