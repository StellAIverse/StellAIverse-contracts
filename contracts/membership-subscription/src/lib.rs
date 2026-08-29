#![no_std]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, Address, Env, String,
    Vec,
};

const BPS_DENOMINATOR: i128 = 10_000;
const MAX_REFERRAL_REWARD_BPS: u32 = 5_000;
const MAX_TIER_NAME_LENGTH: u32 = 64;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Tier(u32),
    TierIds,
    Subscription(Address),
    Analytics,
    ReferralRewards(Address),
    ReentrancyLock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum SubscriptionStatus {
    Trial = 0,
    Active = 1,
    Paused = 2,
    Cancelled = 3,
    Expired = 4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MembershipTier {
    pub tier_id: u32,
    pub name: String,
    pub price_per_period: i128,
    pub period_seconds: u64,
    pub trial_seconds: u64,
    pub benefit_flags: u32,
    pub referral_reward_bps: u32,
    pub active: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Subscription {
    pub member: Address,
    pub tier_id: u32,
    pub token: Address,
    pub status: SubscriptionStatus,
    pub auto_renew: bool,
    pub started_at: u64,
    pub current_period_start: u64,
    pub current_period_end: u64,
    pub trial_end: u64,
    pub paused_at: u64,
    pub paused_accumulated_seconds: u64,
    pub referrer: Option<Address>,
    pub total_paid: i128,
    pub total_referral_rewards: i128,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SubscriptionAnalytics {
    pub total_subscriptions: u64,
    pub active_subscriptions: u64,
    pub cancelled_subscriptions: u64,
    pub expired_subscriptions: u64,
    pub paused_subscriptions: u64,
    pub total_revenue: i128,
    pub total_refunds: i128,
    pub total_referral_rewards: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RenewalResult {
    pub charged_amount: i128,
    pub referral_reward: i128,
    pub period_start: u64,
    pub period_end: u64,
    pub status: SubscriptionStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TierChangeResult {
    pub old_tier_id: u32,
    pub new_tier_id: u32,
    pub charge_amount: i128,
    pub refund_amount: i128,
    pub period_start: u64,
    pub period_end: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ProrationQuote {
    pub remaining_credit: i128,
    pub charge_amount: i128,
    pub refund_amount: i128,
}

#[contract]
pub struct MembershipSubscription;

#[contractimpl]
impl MembershipSubscription {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::TierIds, &Vec::<u32>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &SubscriptionAnalytics::empty());
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);
        env.events().publish((symbol_short!("sub_init"),), admin);
    }

    pub fn create_tier(
        env: Env,
        admin: Address,
        tier_id: u32,
        name: String,
        price_per_period: i128,
        period_seconds: u64,
        trial_seconds: u64,
        benefit_flags: u32,
        referral_reward_bps: u32,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        if tier_id == 0 {
            panic!("Invalid tier ID");
        }
        if env.storage().instance().has(&DataKey::Tier(tier_id)) {
            panic!("Tier already exists");
        }
        Self::validate_tier_terms(&name, price_per_period, period_seconds, referral_reward_bps);

        let now = env.ledger().timestamp();
        let tier = MembershipTier {
            tier_id,
            name,
            price_per_period,
            period_seconds,
            trial_seconds,
            benefit_flags,
            referral_reward_bps,
            active: true,
            created_at: now,
            updated_at: now,
        };
        env.storage().instance().set(&DataKey::Tier(tier_id), &tier);

        let mut tier_ids = Self::tier_ids(&env);
        tier_ids.push_back(tier_id);
        env.storage().instance().set(&DataKey::TierIds, &tier_ids);
        env.events()
            .publish((symbol_short!("tier_new"),), (admin, tier_id));
    }

    pub fn update_tier(
        env: Env,
        admin: Address,
        tier_id: u32,
        name: String,
        price_per_period: i128,
        period_seconds: u64,
        trial_seconds: u64,
        benefit_flags: u32,
        referral_reward_bps: u32,
        active: bool,
    ) -> MembershipTier {
        admin.require_auth();
        Self::assert_admin(&env, &admin);
        Self::validate_tier_terms(&name, price_per_period, period_seconds, referral_reward_bps);

        let mut tier = Self::load_tier(&env, tier_id);
        tier.name = name;
        tier.price_per_period = price_per_period;
        tier.period_seconds = period_seconds;
        tier.trial_seconds = trial_seconds;
        tier.benefit_flags = benefit_flags;
        tier.referral_reward_bps = referral_reward_bps;
        tier.active = active;
        tier.updated_at = env.ledger().timestamp();

        env.storage().instance().set(&DataKey::Tier(tier_id), &tier);
        env.events()
            .publish((symbol_short!("tier_upd"),), (admin, tier_id, active));
        tier
    }

    pub fn subscribe(
        env: Env,
        member: Address,
        token: Address,
        tier_id: u32,
        auto_renew: bool,
        referrer: Option<Address>,
    ) -> Subscription {
        member.require_auth();
        let tier = Self::load_active_tier(&env, tier_id);
        if let Some(existing) = Self::maybe_subscription(&env, &member) {
            if Self::is_non_terminal(existing.status) {
                panic!("Member already has subscription");
            }
        }
        if let Some(ref referrer_address) = referrer {
            if referrer_address == &member {
                panic!("Member cannot refer themselves");
            }
        }

        let now = env.ledger().timestamp();
        let has_trial = tier.trial_seconds > 0;
        let status = if has_trial {
            SubscriptionStatus::Trial
        } else {
            SubscriptionStatus::Active
        };
        let period_end = if has_trial {
            now.checked_add(tier.trial_seconds)
                .expect("Trial end overflow")
        } else {
            now.checked_add(tier.period_seconds)
                .expect("Period end overflow")
        };

        let mut subscription = Subscription {
            member: member.clone(),
            tier_id,
            token: token.clone(),
            status,
            auto_renew,
            started_at: now,
            current_period_start: now,
            current_period_end: period_end,
            trial_end: if has_trial { period_end } else { 0 },
            paused_at: 0,
            paused_accumulated_seconds: 0,
            referrer,
            total_paid: 0,
            total_referral_rewards: 0,
            updated_at: now,
        };

        if !has_trial {
            let reward =
                Self::charge_member_direct(&env, &member, &token, &tier, &mut subscription);
            Self::record_revenue(&env, tier.price_per_period, reward);
        }

        env.storage()
            .instance()
            .set(&DataKey::Subscription(member.clone()), &subscription);
        Self::record_subscription_created(&env);
        env.events()
            .publish((symbol_short!("sub_new"),), (member, tier_id, status));
        subscription
    }

    pub fn renew_subscription(env: Env, member: Address) -> RenewalResult {
        let mut subscription = Self::load_subscription(&env, &member);
        if subscription.status == SubscriptionStatus::Paused {
            panic!("Paused subscription cannot renew");
        }
        if !Self::is_non_terminal(subscription.status) {
            panic!("Subscription is not renewable");
        }
        if env.ledger().timestamp() < subscription.current_period_end {
            panic!("Subscription is not due");
        }

        let tier = Self::load_active_tier(&env, subscription.tier_id);
        if !subscription.auto_renew {
            Self::expire_subscription(&env, &mut subscription);
            return RenewalResult {
                charged_amount: 0,
                referral_reward: 0,
                period_start: subscription.current_period_start,
                period_end: subscription.current_period_end,
                status: subscription.status,
            };
        }

        let token_client = TokenClient::new(&env, &subscription.token);
        let wallet = env.current_contract_address();
        let allowance = token_client.allowance(&member, &wallet);
        let balance = token_client.balance(&member);
        if allowance < tier.price_per_period || balance < tier.price_per_period {
            Self::expire_subscription(&env, &mut subscription);
            env.events()
                .publish((symbol_short!("sub_fail"),), (member, tier.tier_id));
            return RenewalResult {
                charged_amount: 0,
                referral_reward: 0,
                period_start: subscription.current_period_start,
                period_end: subscription.current_period_end,
                status: subscription.status,
            };
        }

        Self::enter_non_reentrant(&env);
        token_client.transfer_from(&wallet, &member, &wallet, &tier.price_per_period);
        Self::exit_non_reentrant(&env);

        let reward = Self::pay_referral_reward(&env, &tier, &mut subscription);
        subscription.total_paid = subscription
            .total_paid
            .checked_add(tier.price_per_period)
            .expect("Subscription paid overflow");
        subscription.status = SubscriptionStatus::Active;
        subscription.current_period_start = env.ledger().timestamp();
        subscription.current_period_end = env
            .ledger()
            .timestamp()
            .checked_add(tier.period_seconds)
            .expect("Period end overflow");
        subscription.trial_end = 0;
        subscription.updated_at = env.ledger().timestamp();

        env.storage()
            .instance()
            .set(&DataKey::Subscription(member.clone()), &subscription);
        Self::record_revenue(&env, tier.price_per_period, reward);
        env.events().publish(
            (symbol_short!("sub_ren"),),
            (member, tier.tier_id, tier.price_per_period, reward),
        );

        RenewalResult {
            charged_amount: tier.price_per_period,
            referral_reward: reward,
            period_start: subscription.current_period_start,
            period_end: subscription.current_period_end,
            status: subscription.status,
        }
    }

    pub fn cancel_subscription(env: Env, member: Address) -> i128 {
        member.require_auth();
        let mut subscription = Self::load_subscription(&env, &member);
        if !Self::is_non_terminal(subscription.status) {
            panic!("Subscription already terminal");
        }

        let refund = Self::unused_period_value(&env, &subscription);
        subscription.status = SubscriptionStatus::Cancelled;
        subscription.auto_renew = false;
        subscription.updated_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Subscription(member.clone()), &subscription);
        Self::record_cancelled(&env, refund);

        if refund > 0 {
            let wallet = env.current_contract_address();
            Self::transfer_token(&env, &subscription.token, &wallet, &member, refund);
        }

        env.events()
            .publish((symbol_short!("sub_can"),), (member, refund));
        refund
    }

    pub fn pause_subscription(env: Env, member: Address) {
        member.require_auth();
        let mut subscription = Self::load_subscription(&env, &member);
        if subscription.status != SubscriptionStatus::Active
            && subscription.status != SubscriptionStatus::Trial
        {
            panic!("Subscription cannot be paused");
        }

        subscription.status = SubscriptionStatus::Paused;
        subscription.paused_at = env.ledger().timestamp();
        subscription.updated_at = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::Subscription(member.clone()), &subscription);
        Self::record_paused(&env);
        env.events().publish((symbol_short!("sub_pau"),), member);
    }

    pub fn resume_subscription(env: Env, member: Address) -> Subscription {
        member.require_auth();
        let mut subscription = Self::load_subscription(&env, &member);
        if subscription.status != SubscriptionStatus::Paused {
            panic!("Subscription is not paused");
        }

        let now = env.ledger().timestamp();
        let paused_duration = now
            .checked_sub(subscription.paused_at)
            .expect("Resume before pause");
        subscription.paused_accumulated_seconds = subscription
            .paused_accumulated_seconds
            .checked_add(paused_duration)
            .expect("Paused duration overflow");
        subscription.current_period_end = subscription
            .current_period_end
            .checked_add(paused_duration)
            .expect("Period extension overflow");
        if subscription.trial_end >= subscription.paused_at && subscription.trial_end > 0 {
            subscription.trial_end = subscription
                .trial_end
                .checked_add(paused_duration)
                .expect("Trial extension overflow");
        }
        subscription.status = if subscription.trial_end > now {
            SubscriptionStatus::Trial
        } else {
            SubscriptionStatus::Active
        };
        subscription.paused_at = 0;
        subscription.updated_at = now;

        env.storage()
            .instance()
            .set(&DataKey::Subscription(member.clone()), &subscription);
        Self::record_resumed(&env);
        env.events().publish(
            (symbol_short!("sub_res"),),
            (member, subscription.current_period_end),
        );
        subscription
    }

    pub fn change_tier(env: Env, member: Address, new_tier_id: u32) -> TierChangeResult {
        member.require_auth();
        let mut subscription = Self::load_subscription(&env, &member);
        if subscription.status != SubscriptionStatus::Active
            && subscription.status != SubscriptionStatus::Trial
        {
            panic!("Subscription is not active");
        }

        let old_tier = Self::load_tier(&env, subscription.tier_id);
        let new_tier = Self::load_active_tier(&env, new_tier_id);
        let quote = Self::quote_proration_inner(&env, &subscription, &new_tier);
        if quote.charge_amount > 0 {
            Self::transfer_token(
                &env,
                &subscription.token,
                &member,
                &env.current_contract_address(),
                quote.charge_amount,
            );
            subscription.total_paid = subscription
                .total_paid
                .checked_add(quote.charge_amount)
                .expect("Subscription paid overflow");
            Self::record_revenue(&env, quote.charge_amount, 0);
        }
        if quote.refund_amount > 0 {
            Self::transfer_token(
                &env,
                &subscription.token,
                &env.current_contract_address(),
                &member,
                quote.refund_amount,
            );
            Self::record_refund(&env, quote.refund_amount);
        }

        let now = env.ledger().timestamp();
        subscription.tier_id = new_tier_id;
        subscription.status = SubscriptionStatus::Active;
        subscription.current_period_start = now;
        subscription.current_period_end = now
            .checked_add(new_tier.period_seconds)
            .expect("Period end overflow");
        subscription.trial_end = 0;
        subscription.updated_at = now;
        env.storage()
            .instance()
            .set(&DataKey::Subscription(member.clone()), &subscription);

        env.events().publish(
            (symbol_short!("sub_tier"),),
            (
                member,
                old_tier.tier_id,
                new_tier_id,
                quote.charge_amount,
                quote.refund_amount,
            ),
        );

        TierChangeResult {
            old_tier_id: old_tier.tier_id,
            new_tier_id,
            charge_amount: quote.charge_amount,
            refund_amount: quote.refund_amount,
            period_start: subscription.current_period_start,
            period_end: subscription.current_period_end,
        }
    }

    pub fn quote_tier_change(env: Env, member: Address, new_tier_id: u32) -> ProrationQuote {
        let subscription = Self::load_subscription(&env, &member);
        let new_tier = Self::load_active_tier(&env, new_tier_id);
        Self::quote_proration_inner(&env, &subscription, &new_tier)
    }

    pub fn has_benefit(env: Env, member: Address, benefit_flag: u32) -> bool {
        if benefit_flag == 0 {
            return false;
        }
        if !Self::is_active_inner(&env, &member) {
            return false;
        }
        let subscription = Self::load_subscription(&env, &member);
        let tier = Self::load_tier(&env, subscription.tier_id);
        tier.benefit_flags & benefit_flag == benefit_flag
    }

    pub fn is_active(env: Env, member: Address) -> bool {
        Self::is_active_inner(&env, &member)
    }

    pub fn get_subscription(env: Env, member: Address) -> Subscription {
        Self::load_subscription(&env, &member)
    }

    pub fn get_tier(env: Env, tier_id: u32) -> MembershipTier {
        Self::load_tier(&env, tier_id)
    }

    pub fn get_tiers(env: Env) -> Vec<MembershipTier> {
        let tier_ids = Self::tier_ids(&env);
        let mut tiers = Vec::new(&env);
        for idx in 0..tier_ids.len() {
            tiers.push_back(Self::load_tier(&env, tier_ids.get_unchecked(idx)));
        }
        tiers
    }

    pub fn get_analytics(env: Env) -> SubscriptionAnalytics {
        Self::analytics(&env)
    }

    pub fn referral_rewards(env: Env, referrer: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::ReferralRewards(referrer))
            .unwrap_or(0)
    }

    fn charge_member_direct(
        env: &Env,
        member: &Address,
        token: &Address,
        tier: &MembershipTier,
        subscription: &mut Subscription,
    ) -> i128 {
        Self::transfer_token(
            env,
            token,
            member,
            &env.current_contract_address(),
            tier.price_per_period,
        );
        let reward = Self::pay_referral_reward(env, tier, subscription);
        subscription.total_paid = subscription
            .total_paid
            .checked_add(tier.price_per_period)
            .expect("Subscription paid overflow");
        reward
    }

    fn pay_referral_reward(
        env: &Env,
        tier: &MembershipTier,
        subscription: &mut Subscription,
    ) -> i128 {
        let Some(referrer) = subscription.referrer.clone() else {
            return 0;
        };
        let reward = tier
            .price_per_period
            .checked_mul(tier.referral_reward_bps as i128)
            .expect("Referral reward overflow")
            / BPS_DENOMINATOR;
        if reward <= 0 {
            return 0;
        }

        Self::transfer_token(
            env,
            &subscription.token,
            &env.current_contract_address(),
            &referrer,
            reward,
        );
        let current: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ReferralRewards(referrer.clone()))
            .unwrap_or(0);
        env.storage().instance().set(
            &DataKey::ReferralRewards(referrer),
            &(current
                .checked_add(reward)
                .expect("Referral total overflow")),
        );
        subscription.total_referral_rewards = subscription
            .total_referral_rewards
            .checked_add(reward)
            .expect("Subscription reward overflow");
        reward
    }

    fn quote_proration_inner(
        env: &Env,
        subscription: &Subscription,
        new_tier: &MembershipTier,
    ) -> ProrationQuote {
        let remaining_credit = Self::unused_period_value(env, subscription);
        if new_tier.price_per_period > remaining_credit {
            ProrationQuote {
                remaining_credit,
                charge_amount: new_tier
                    .price_per_period
                    .checked_sub(remaining_credit)
                    .expect("Charge underflow"),
                refund_amount: 0,
            }
        } else {
            ProrationQuote {
                remaining_credit,
                charge_amount: 0,
                refund_amount: remaining_credit
                    .checked_sub(new_tier.price_per_period)
                    .expect("Refund underflow"),
            }
        }
    }

    fn unused_period_value(env: &Env, subscription: &Subscription) -> i128 {
        if subscription.status == SubscriptionStatus::Trial {
            return 0;
        }
        if subscription.current_period_end <= env.ledger().timestamp() {
            return 0;
        }
        let tier = Self::load_tier(env, subscription.tier_id);
        let remaining = subscription
            .current_period_end
            .checked_sub(env.ledger().timestamp())
            .expect("Remaining time underflow");
        tier.price_per_period
            .checked_mul(remaining as i128)
            .expect("Proration multiplication overflow")
            / tier.period_seconds as i128
    }

    fn expire_subscription(env: &Env, subscription: &mut Subscription) {
        subscription.status = SubscriptionStatus::Expired;
        subscription.auto_renew = false;
        subscription.updated_at = env.ledger().timestamp();
        env.storage().instance().set(
            &DataKey::Subscription(subscription.member.clone()),
            subscription,
        );
        Self::record_expired(env);
    }

    fn is_active_inner(env: &Env, member: &Address) -> bool {
        let Some(subscription) = Self::maybe_subscription(env, member) else {
            return false;
        };
        if subscription.status != SubscriptionStatus::Active
            && subscription.status != SubscriptionStatus::Trial
        {
            return false;
        }
        env.ledger().timestamp() <= subscription.current_period_end
    }

    fn is_non_terminal(status: SubscriptionStatus) -> bool {
        status == SubscriptionStatus::Active
            || status == SubscriptionStatus::Trial
            || status == SubscriptionStatus::Paused
    }

    fn record_subscription_created(env: &Env) {
        let mut analytics = Self::analytics(env);
        analytics.total_subscriptions = analytics
            .total_subscriptions
            .checked_add(1)
            .expect("Subscription count overflow");
        analytics.active_subscriptions = analytics
            .active_subscriptions
            .checked_add(1)
            .expect("Active count overflow");
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &analytics);
    }

    fn record_revenue(env: &Env, revenue: i128, referral_reward: i128) {
        let mut analytics = Self::analytics(env);
        analytics.total_revenue = analytics
            .total_revenue
            .checked_add(revenue)
            .expect("Revenue overflow");
        analytics.total_referral_rewards = analytics
            .total_referral_rewards
            .checked_add(referral_reward)
            .expect("Referral analytics overflow");
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &analytics);
    }

    fn record_refund(env: &Env, refund: i128) {
        let mut analytics = Self::analytics(env);
        analytics.total_refunds = analytics
            .total_refunds
            .checked_add(refund)
            .expect("Refund analytics overflow");
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &analytics);
    }

    fn record_cancelled(env: &Env, refund: i128) {
        let mut analytics = Self::analytics(env);
        analytics.cancelled_subscriptions = analytics
            .cancelled_subscriptions
            .checked_add(1)
            .expect("Cancelled count overflow");
        analytics.active_subscriptions = analytics.active_subscriptions.saturating_sub(1);
        analytics.total_refunds = analytics
            .total_refunds
            .checked_add(refund)
            .expect("Refund analytics overflow");
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &analytics);
    }

    fn record_expired(env: &Env) {
        let mut analytics = Self::analytics(env);
        analytics.expired_subscriptions = analytics
            .expired_subscriptions
            .checked_add(1)
            .expect("Expired count overflow");
        analytics.active_subscriptions = analytics.active_subscriptions.saturating_sub(1);
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &analytics);
    }

    fn record_paused(env: &Env) {
        let mut analytics = Self::analytics(env);
        analytics.paused_subscriptions = analytics
            .paused_subscriptions
            .checked_add(1)
            .expect("Paused count overflow");
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &analytics);
    }

    fn record_resumed(env: &Env) {
        let mut analytics = Self::analytics(env);
        analytics.paused_subscriptions = analytics.paused_subscriptions.saturating_sub(1);
        env.storage()
            .instance()
            .set(&DataKey::Analytics, &analytics);
    }

    fn transfer_token(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
        if amount <= 0 {
            panic!("Transfer amount must be positive");
        }
        Self::enter_non_reentrant(env);
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

    fn load_active_tier(env: &Env, tier_id: u32) -> MembershipTier {
        let tier = Self::load_tier(env, tier_id);
        if !tier.active {
            panic!("Tier is inactive");
        }
        tier
    }

    fn load_tier(env: &Env, tier_id: u32) -> MembershipTier {
        if tier_id == 0 {
            panic!("Invalid tier ID");
        }
        env.storage()
            .instance()
            .get(&DataKey::Tier(tier_id))
            .expect("Tier not found")
    }

    fn load_subscription(env: &Env, member: &Address) -> Subscription {
        env.storage()
            .instance()
            .get(&DataKey::Subscription(member.clone()))
            .expect("Subscription not found")
    }

    fn maybe_subscription(env: &Env, member: &Address) -> Option<Subscription> {
        env.storage()
            .instance()
            .get(&DataKey::Subscription(member.clone()))
    }

    fn tier_ids(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&DataKey::TierIds)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn analytics(env: &Env) -> SubscriptionAnalytics {
        env.storage()
            .instance()
            .get(&DataKey::Analytics)
            .unwrap_or_else(SubscriptionAnalytics::empty)
    }

    fn assert_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    fn validate_tier_terms(
        name: &String,
        price_per_period: i128,
        period_seconds: u64,
        referral_reward_bps: u32,
    ) {
        if name.len() > MAX_TIER_NAME_LENGTH {
            panic!("Tier name exceeds maximum length");
        }
        if price_per_period <= 0 {
            panic!("Tier price must be positive");
        }
        if period_seconds == 0 {
            panic!("Period must be positive");
        }
        if referral_reward_bps > MAX_REFERRAL_REWARD_BPS {
            panic!("Referral reward too high");
        }
    }
}

impl SubscriptionAnalytics {
    fn empty() -> Self {
        Self {
            total_subscriptions: 0,
            active_subscriptions: 0,
            cancelled_subscriptions: 0,
            expired_subscriptions: 0,
            paused_subscriptions: 0,
            total_revenue: 0,
            total_refunds: 0,
            total_referral_rewards: 0,
        }
    }
}

#[cfg(test)]
mod test;
