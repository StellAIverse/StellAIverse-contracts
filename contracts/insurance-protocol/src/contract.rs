use crate::errors::*;
use crate::storage_keys::*;
use crate::types::*;
use soroban_sdk::{
    contract, contractimpl, token::TokenClient, Address, Env, String as SorobanString, Symbol, Vec,
};

// ═══════════════════════════════════════════════════════════════
//  CONSTANTS
// ═══════════════════════════════════════════════════════════════

const BPS_DENOMINATOR: i128 = 10_000;
const MAX_UNDERWRITERS_PER_POOL: u32 = 1_000;
const LARGE_CLAIM_THRESHOLD_BPS: u32 = 1_000; // 10% of pool
const VOTING_PERIOD_SECONDS: u64 = 604_800; // 7 days
const DEFAULT_TIMELOCK_PERIOD: u64 = 259_200; // 3 days
const POLICY_DURATION: u64 = 31_536_000; // 1 year

// ═══════════════════════════════════════════════════════════════
//  CONTRACT
// ═══════════════════════════════════════════════════════════════

#[contract]
pub struct InsuranceProtocol;

#[contractimpl]
impl InsuranceProtocol {
    // ── INITIALIZATION ─────────────────────────────────────────

    /// Initialize the insurance protocol contract.
    pub fn initialize(env: Env, admin: Address, token: Address, oracle: Address) {
        if env.storage().instance().has(&get_admin_key()) {
            already_initialized(&env);
        }
        admin.require_auth();

        env.storage().instance().set(&get_admin_key(), &admin);
        env.storage().instance().set(&get_token_key(), &token);
        env.storage().instance().set(&get_oracle_key(), &oracle);
        env.storage().instance().set(&get_paused_key(), &false);
        env.storage()
            .instance()
            .set(&get_policy_counter_key(), &0u64);
        env.storage()
            .instance()
            .set(&get_claim_counter_key(), &0u64);
        env.storage()
            .instance()
            .set(&get_payout_queue_counter_key(), &0u64);
        env.storage().instance().set(&get_pool_counter_key(), &0u32);
        env.storage()
            .instance()
            .set(&get_underwriter_count_key(), &0u64);
        env.storage()
            .instance()
            .set(&get_pool_ids_key(), &Vec::<Symbol>::new(&env));

        // Initialize default risk parameters for each coverage type
        Self::init_default_risk_params(&env);

        // Initialize default tier configs
        Self::init_default_tier_configs(&env);

        env.events().publish(
            (soroban_sdk::symbol_short!("ins_init"),),
            (admin, token, oracle, env.ledger().timestamp()),
        );
    }

    // ── POOL MANAGEMENT ────────────────────────────────────────

    /// Create a new insurance pool for a specific coverage type.
    pub fn create_pool(
        env: Env,
        admin: Address,
        pool_id: Symbol,
        coverage_type: CoverageType,
        reserve_ratio_bps: u32,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if env.storage().instance().has(&get_pool_key(&pool_id)) {
            pool_already_exists(&env);
        }

        if reserve_ratio_bps > 10_000 {
            invalid_input(&env);
        }

        let pool = InsurancePool {
            pool_id: pool_id.clone(),
            coverage_type,
            total_assets: 0,
            total_shares: 0,
            reserve_ratio_bps,
            reserve_amount: 0,
            active_policies: 0,
            total_premiums_collected: 0,
            total_claims_paid: 0,
            is_active: true,
            created_at: env.ledger().timestamp(),
        };

        env.storage().instance().set(&get_pool_key(&pool_id), &pool);

        let mut pool_ids: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&get_pool_ids_key())
            .unwrap_or_else(|| Vec::new(&env));
        pool_ids.push_back(pool_id.clone());
        env.storage().instance().set(&get_pool_ids_key(), &pool_ids);

        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&get_pool_counter_key())
            .unwrap_or(0);
        counter += 1;
        env.storage()
            .instance()
            .set(&get_pool_counter_key(), &counter);

        env.events().publish(
            (soroban_sdk::symbol_short!("pool_cre"),),
            (pool_id, coverage_type as u32, reserve_ratio_bps),
        );
    }

    /// Deposit funds as an underwriter, receiving proportional shares.
    pub fn deposit_as_underwriter(env: Env, underwriter: Address, pool_id: Symbol, amount: i128) {
        underwriter.require_auth();

        if Self::is_paused_fn(&env) {
            already_paused(&env);
        }

        if amount <= 0 {
            invalid_input(&env);
        }

        let mut pool = Self::load_pool(&env, &pool_id);
        if !pool.is_active {
            pool_inactive(&env);
        }

        let uw_key = get_underwriter_key(&pool_id, &underwriter);
        if !env.storage().instance().has(&uw_key) {
            // Check underwriter count limit
            let pool_underwriter_count = Self::count_pool_underwriters(&env, &pool_id);
            if pool_underwriter_count >= MAX_UNDERWRITERS_PER_POOL {
                max_underwriters_reached(&env);
            }
        }

        // Calculate shares: proportional to pool depth
        let shares = if pool.total_shares <= 0 || pool.total_assets <= 0 {
            amount
        } else {
            amount.checked_mul(pool.total_shares).expect("Overflow") / pool.total_assets
        };

        // Transfer tokens from underwriter to contract
        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&underwriter, &contract_address, &amount);

        // Update pool
        pool.total_assets = pool.total_assets.checked_add(amount).expect("Overflow");
        pool.total_shares = pool.total_shares.checked_add(shares).expect("Overflow");
        pool.reserve_amount = pool
            .total_assets
            .checked_mul(pool.reserve_ratio_bps as i128)
            .expect("Overflow")
            / BPS_DENOMINATOR;

        env.storage().instance().set(&get_pool_key(&pool_id), &pool);

        // Update underwriter record
        let mut uw = Self::load_underwriter(&env, &pool_id, &underwriter);
        uw.shares = uw.shares.checked_add(shares).expect("Overflow");
        uw.total_deposited = uw.total_deposited.checked_add(amount).expect("Overflow");
        uw.deposit_timestamp = env.ledger().timestamp();
        env.storage().instance().set(&uw_key, &uw);

        env.events().publish(
            (soroban_sdk::symbol_short!("uw_dep"),),
            (
                underwriter,
                pool_id,
                amount,
                shares,
                pool.total_assets,
                pool.total_shares,
            ),
        );
    }

    /// Withdraw funds as an underwriter, burning shares for proportional assets.
    pub fn withdraw_as_underwriter(
        env: Env,
        underwriter: Address,
        pool_id: Symbol,
        shares: i128,
    ) -> i128 {
        underwriter.require_auth();

        if shares <= 0 {
            invalid_input(&env);
        }

        let mut pool = Self::load_pool(&env, &pool_id);
        if !pool.is_active {
            pool_inactive(&env);
        }

        let uw_key = get_underwriter_key(&pool_id, &underwriter);
        let mut uw = Self::load_underwriter(&env, &pool_id, &underwriter);
        if uw.shares < shares {
            insufficient_shares(&env);
        }

        let total_assets = pool.total_assets;
        let total_shares = pool.total_shares;
        if total_shares <= 0 {
            pool_inactive(&env);
        }

        // Calculate proportional assets
        let assets = shares.checked_mul(total_assets).expect("Overflow") / total_shares;

        // Ensure reserve ratio is maintained after withdrawal
        let new_total_assets = total_assets.checked_sub(assets).expect("Underflow");
        let new_total_shares = total_shares.checked_sub(shares).expect("Underflow");
        let new_reserve = if new_total_shares > 0 {
            new_total_assets
                .checked_mul(pool.reserve_ratio_bps as i128)
                .expect("Overflow")
                / BPS_DENOMINATOR
        } else {
            0
        };

        // Allow withdrawal if: pool has no active policies OR reserve is maintained
        // OR total assets after withdrawal >= reserve requirement
        let withdrawable = if pool.active_policies == 0 {
            // No active policies: underwriters can withdraw everything
            true
        } else {
            // With active policies, check reserve coverage
            let total_liabilities = Self::estimate_total_liabilities(&env, &pool_id);
            let available_for_withdrawal = new_total_assets
                .checked_sub(total_liabilities)
                .expect("Underflow");
            available_for_withdrawal >= new_reserve
        };

        if !withdrawable && pool.active_policies > 0 {
            reserve_ratio_exceeded(&env);
        }

        // Transfer tokens back to underwriter
        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&contract_address, &underwriter, &assets);

        // Update pool
        pool.total_assets = new_total_assets;
        pool.total_shares = new_total_shares;
        pool.reserve_amount = new_reserve;

        env.storage().instance().set(&get_pool_key(&pool_id), &pool);

        // Update underwriter record
        uw.shares = uw.shares.checked_sub(shares).expect("Underflow");
        uw.total_withdrawn = uw.total_withdrawn.checked_add(assets).expect("Overflow");
        uw.last_withdrawal_timestamp = env.ledger().timestamp();
        env.storage().instance().set(&uw_key, &uw);

        env.events().publish(
            (soroban_sdk::symbol_short!("uw_wd"),),
            (
                underwriter,
                pool_id,
                shares,
                assets,
                pool.total_assets,
                pool.total_shares,
            ),
        );

        assets
    }

    // ── PREMIUM PRICING (Constant Product Bonding Curve) ───────

    /// Calculate premium using Constant Product bonding curve.
    /// Formula: premium = base_rate * (coverage_amount / pool_depth) * risk_multiplier * tier_multiplier / 10000
    pub fn calculate_premium(
        env: Env,
        pool_id: Symbol,
        coverage_amount: i128,
        tier: CoverageTier,
    ) -> i128 {
        let pool = Self::load_pool(&env, &pool_id);
        if !pool.is_active {
            pool_inactive(&env);
        }

        let risk_params = Self::load_risk_params(&env, &pool.coverage_type);
        let tier_config = Self::load_tier_config(&env, &pool.coverage_type, &tier);

        let base_rate = risk_params.base_rate_bps as i128;
        let risk_multiplier = risk_params.risk_multiplier as i128;
        let tier_multiplier = tier_config.premium_multiplier_bps as i128;

        // Constant Product: premium scales quadratically with coverage/pool_depth ratio
        // premium = base_rate * (coverage / pool_depth)^2 * risk_multiplier * tier_multiplier / BPS^3
        let pool_depth = if pool.total_assets > 0 {
            pool.total_assets
        } else {
            // Bootstrap: fixed reference depth so premiums scale across amounts
            1_000_000_i128
        };

        // Constant Product bonding curve: premium scales with coverage/pool_depth
        // premium = base_rate * coverage_amount * risk_multiplier * tier_multiplier / (pool_depth * BPS^2)
        let numerator = base_rate
            .checked_mul(coverage_amount)
            .expect("Overflow")
            .checked_mul(risk_multiplier)
            .expect("Overflow")
            .checked_mul(tier_multiplier)
            .expect("Overflow");
        let premium = numerator / pool_depth / 10_000_i128 / 10_000_i128;

        // Minimum premium of 1 (in base units)
        if premium < 1 {
            1
        } else {
            premium
        }
    }

    // ── COVERAGE PURCHASE ──────────────────────────────────────

    /// Purchase coverage from a pool.
    pub fn purchase_coverage(
        env: Env,
        buyer: Address,
        pool_id: Symbol,
        tier: CoverageTier,
        coverage_amount: i128,
    ) -> u64 {
        buyer.require_auth();

        if Self::is_paused_fn(&env) {
            already_paused(&env);
        }

        if coverage_amount <= 0 {
            invalid_coverage_amount(&env);
        }

        let mut pool = Self::load_pool(&env, &pool_id);
        if !pool.is_active {
            pool_inactive(&env);
        }

        let risk_params = Self::load_risk_params(&env, &pool.coverage_type);

        // Validate coverage amount within limits
        if coverage_amount < risk_params.min_coverage_limit {
            invalid_coverage_amount(&env);
        }
        if coverage_amount > risk_params.max_coverage_limit {
            claim_amount_exceeds_coverage(&env);
        }

        // Calculate premium
        let premium = Self::calculate_premium(env.clone(), pool_id.clone(), coverage_amount, tier);

        // Calculate deductible based on tier
        let tier_config = Self::load_tier_config(&env, &pool.coverage_type, &tier);
        let deductible = coverage_amount
            .checked_mul(tier_config.deductible_bps as i128)
            .expect("Overflow")
            / BPS_DENOMINATOR;

        // Calculate max payout based on tier
        let max_payout = coverage_amount
            .checked_mul(tier_config.max_payout_multiplier as i128)
            .expect("Overflow")
            / 10_000;

        // Transfer premium from buyer to contract
        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&buyer, &contract_address, &premium);

        // Create policy
        let policy_id = Self::next_policy_id(&env);
        let now = env.ledger().timestamp();
        let policy = CoveragePolicy {
            policy_id,
            pool_id: pool_id.clone(),
            holder: buyer.clone(),
            coverage_type: pool.coverage_type,
            tier,
            coverage_limit: max_payout,
            deductible,
            premium_paid: premium,
            is_active: true,
            purchased_at: now,
            expires_at: now + POLICY_DURATION,
        };

        env.storage()
            .instance()
            .set(&get_policy_key(policy_id), &policy);

        // Track policy IDs
        let mut policy_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&get_policy_ids_key())
            .unwrap_or_else(|| Vec::new(&env));
        policy_ids.push_back(policy_id);
        env.storage()
            .instance()
            .set(&get_policy_ids_key(), &policy_ids);

        // Update holder's policies
        let mut holder_policies: Vec<u64> = env
            .storage()
            .instance()
            .get(&get_holder_policies_key(&buyer))
            .unwrap_or_else(|| Vec::new(&env));
        holder_policies.push_back(policy_id);
        env.storage()
            .instance()
            .set(&get_holder_policies_key(&buyer), &holder_policies);

        // Update pool
        pool.total_premiums_collected = pool
            .total_premiums_collected
            .checked_add(premium)
            .expect("Overflow");
        pool.total_assets = pool.total_assets.checked_add(premium).expect("Overflow");
        pool.active_policies += 1;
        pool.reserve_amount = pool
            .total_assets
            .checked_mul(pool.reserve_ratio_bps as i128)
            .expect("Overflow")
            / BPS_DENOMINATOR;

        env.storage().instance().set(&get_pool_key(&pool_id), &pool);

        env.events().publish(
            (soroban_sdk::symbol_short!("cov_buy"),),
            (
                buyer,
                pool_id,
                policy_id,
                coverage_amount,
                premium,
                tier as u32,
            ),
        );

        policy_id
    }

    /// Cancel a coverage policy. Returns partial premium refund if not expired.
    pub fn cancel_policy(env: Env, buyer: Address, policy_id: u64) -> i128 {
        buyer.require_auth();

        let mut policy = Self::load_policy(&env, &policy_id);
        if policy.holder != buyer {
            unauthorized(&env);
        }
        if !policy.is_active {
            policy_inactive(&env);
        }

        let now = env.ledger().timestamp();
        let total_duration = policy
            .expires_at
            .checked_sub(policy.purchased_at)
            .expect("Underflow");
        let elapsed = now.checked_sub(policy.purchased_at).expect("Underflow");

        // Prorated refund: if less than 50% of term has passed, refund 50% of remaining premium
        let refund = if elapsed < total_duration / 2 {
            let remaining_bps = ((total_duration - elapsed) * 10_000) / total_duration;
            policy
                .premium_paid
                .checked_mul(remaining_bps as i128)
                .expect("Overflow")
                / 20_000
        } else {
            0
        };

        policy.is_active = false;
        env.storage()
            .instance()
            .set(&get_policy_key(policy_id), &policy);

        // Update pool
        let mut pool = Self::load_pool(&env, &policy.pool_id);
        pool.active_policies = pool.active_policies.saturating_sub(1);
        pool.total_assets = pool.total_assets.checked_sub(refund).expect("Underflow");
        pool.reserve_amount = if pool.total_assets > 0 && pool.total_shares > 0 {
            pool.total_assets
                .checked_mul(pool.reserve_ratio_bps as i128)
                .expect("Overflow")
                / BPS_DENOMINATOR
        } else {
            0
        };
        env.storage()
            .instance()
            .set(&get_pool_key(&policy.pool_id), &pool);

        // Refund if applicable
        if refund > 0 {
            let token = Self::token(&env);
            let contract_address = env.current_contract_address();
            let token_client = TokenClient::new(&env, &token);
            token_client.transfer(&contract_address, &buyer, &refund);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("cov_can"),),
            (buyer, policy_id, refund, policy.pool_id),
        );

        refund
    }

    // ── CLAIMS ─────────────────────────────────────────────────

    /// Submit a claim with evidence.
    pub fn submit_claim(
        env: Env,
        claimant: Address,
        policy_id: u64,
        amount: i128,
        evidence: SorobanString,
    ) -> u64 {
        claimant.require_auth();

        if Self::is_paused_fn(&env) {
            already_paused(&env);
        }

        let policy = Self::load_policy(&env, &policy_id);
        if policy.holder != claimant {
            unauthorized(&env);
        }
        if !policy.is_active {
            policy_inactive(&env);
        }

        let now = env.ledger().timestamp();
        if now > policy.expires_at {
            policy_expired(&env);
        }

        if amount <= 0 {
            invalid_input(&env);
        }

        // Check deductible
        if amount <= policy.deductible {
            deductible_not_met(&env);
        }

        // Check coverage limit (payout is amount minus deductible, capped at coverage_limit)
        let payout_amount = amount.checked_sub(policy.deductible).expect("Underflow");
        if payout_amount > policy.coverage_limit {
            claim_amount_exceeds_coverage(&env);
        }

        // Determine if voting is required (>10% of pool)
        let pool = Self::load_pool(&env, &policy.pool_id);
        let threshold = pool
            .total_assets
            .checked_mul(LARGE_CLAIM_THRESHOLD_BPS as i128)
            .expect("Overflow")
            / BPS_DENOMINATOR;
        let requires_voting = payout_amount > threshold;

        let risk_params = Self::load_risk_params(&env, &policy.coverage_type);
        let voting_deadline = if requires_voting {
            now + VOTING_PERIOD_SECONDS
        } else {
            now + risk_params.timelock_period
        };

        let claim_id = Self::next_claim_id(&env);
        let claim = Claim {
            claim_id,
            policy_id,
            pool_id: policy.pool_id.clone(),
            claimant: claimant.clone(),
            amount: payout_amount,
            evidence,
            status: ClaimStatus::Pending,
            submitted_at: now,
            voting_deadline,
            votes_for: 0,
            votes_against: 0,
            total_voters: 0,
            requires_voting,
        };

        env.storage()
            .instance()
            .set(&get_claim_key(claim_id), &claim);

        // Track claim IDs
        let mut claim_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&get_claim_ids_key())
            .unwrap_or_else(|| Vec::new(&env));
        claim_ids.push_back(claim_id);
        env.storage()
            .instance()
            .set(&get_claim_ids_key(), &claim_ids);

        // Track pool claims
        let mut pool_claims: Vec<u64> = env
            .storage()
            .instance()
            .get(&get_pool_claims_key(&policy.pool_id))
            .unwrap_or_else(|| Vec::new(&env));
        pool_claims.push_back(claim_id);
        env.storage()
            .instance()
            .set(&get_pool_claims_key(&policy.pool_id), &pool_claims);

        env.events().publish(
            (soroban_sdk::symbol_short!("clm_sub"),),
            (
                claim_id,
                policy_id,
                claimant,
                policy.pool_id,
                payout_amount,
                requires_voting,
            ),
        );

        claim_id
    }

    /// Vote on a claim (only for large claims requiring voting).
    pub fn vote_on_claim(env: Env, underwriter: Address, claim_id: u64, approve: bool) {
        underwriter.require_auth();

        let mut claim = Self::load_claim(&env, &claim_id);
        if !claim.requires_voting {
            invalid_input(&env);
        }

        let now = env.ledger().timestamp();
        if now > claim.voting_deadline {
            voting_period_not_ended(&env);
        }

        // Check if already voted
        let vote_key = get_vote_record_key(claim_id, &underwriter);
        if env.storage().instance().has(&vote_key) {
            already_voted(&env);
        }

        // Verify caller is an underwriter of this pool
        let uw_key = get_underwriter_key(&claim.pool_id, &underwriter);
        let uw: Underwriter = env
            .storage()
            .instance()
            .get(&uw_key)
            .unwrap_or_else(|| not_an_underwriter(&env));
        if uw.shares <= 0 {
            not_an_underwriter(&env);
        }

        // Record vote
        let vote = VoteRecord {
            voter: underwriter.clone(),
            claim_id,
            approve,
            timestamp: now,
        };
        env.storage().instance().set(&vote_key, &vote);

        // Update claim vote counts
        if approve {
            claim.votes_for += 1;
        } else {
            claim.votes_against += 1;
        }
        claim.total_voters += 1;
        env.storage()
            .instance()
            .set(&get_claim_key(claim_id), &claim);

        env.events().publish(
            (soroban_sdk::symbol_short!("clm_vot"),),
            (
                underwriter,
                claim_id,
                approve,
                claim.votes_for,
                claim.votes_against,
            ),
        );
    }

    /// Process a claim after the timelock/voting period has ended.
    pub fn process_claim(env: Env, claim_id: u64) {
        let mut claim = Self::load_claim(&env, &claim_id);
        if claim.status != ClaimStatus::Pending {
            claim_already_processed(&env);
        }

        let now = env.ledger().timestamp();
        if now < claim.voting_deadline {
            if claim.requires_voting {
                voting_period_not_ended(&env);
            } else {
                timelock_not_expired(&env);
            }
        }

        // Determine outcome
        if claim.requires_voting {
            // Need majority approval
            let total_votes = claim.votes_for + claim.votes_against;
            if total_votes == 0 || claim.votes_for <= claim.votes_against {
                claim.status = ClaimStatus::Denied;
                env.storage()
                    .instance()
                    .set(&get_claim_key(claim_id), &claim);

                env.events().publish(
                    (soroban_sdk::symbol_short!("clm_deny"),),
                    (claim_id, claim.pool_id, claim.claimant, claim.amount),
                );
                return;
            }
        }

        // Claim approved: add to payout queue
        claim.status = ClaimStatus::Approved;
        env.storage()
            .instance()
            .set(&get_claim_key(claim_id), &claim);

        // Determine priority (smaller claims process first)
        let priority = Self::calculate_claim_priority(&env, &claim);

        let queue_id = Self::next_payout_queue_id(&env);
        let queue_item = PayoutQueueItem {
            claim_id,
            pool_id: claim.pool_id.clone(),
            amount: claim.amount,
            priority,
            queued_at: now,
            processed: false,
        };

        env.storage()
            .instance()
            .set(&get_payout_queue_item_key(queue_id), &queue_item);

        let mut queue_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&get_payout_queue_ids_key())
            .unwrap_or_else(|| Vec::new(&env));
        queue_ids.push_back(queue_id);
        env.storage()
            .instance()
            .set(&get_payout_queue_ids_key(), &queue_ids);

        env.events().publish(
            (soroban_sdk::symbol_short!("clm_apr"),),
            (
                claim_id,
                claim.pool_id,
                claim.claimant,
                claim.amount,
                queue_id,
            ),
        );
    }

    /// Pay out an approved claim from the queue.
    pub fn pay_claim(env: Env, admin: Address, queue_id: u64) -> i128 {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let mut queue_item = Self::load_payout_queue_item(&env, &queue_id);
        if queue_item.processed {
            claim_already_processed(&env);
        }

        let mut claim = Self::load_claim(&env, &queue_item.claim_id);
        if claim.status != ClaimStatus::Approved {
            claim_denied(&env);
        }

        let mut pool = Self::load_pool(&env, &queue_item.pool_id);

        // Check reserve: ensure we don't dip below minimum after payout
        let new_total_assets = pool
            .total_assets
            .checked_sub(queue_item.amount)
            .expect("Underflow");
        let required_reserve = new_total_assets
            .checked_mul(pool.reserve_ratio_bps as i128)
            .expect("Overflow")
            / BPS_DENOMINATOR;

        // Allow payout if pool still has enough for remaining liabilities
        // or if there are no underwriters left to protect
        if pool.total_shares > 0 && new_total_assets < required_reserve {
            // Check if this would deplete more than available after reserve
            let available = pool
                .total_assets
                .checked_sub(required_reserve)
                .expect("Underflow");
            if queue_item.amount > available {
                insufficient_reserve(&env);
            }
        }

        // Transfer payout to claimant
        let token = Self::token(&env);
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&contract_address, &claim.claimant, &queue_item.amount);

        // Update pool
        pool.total_assets = new_total_assets;
        pool.total_claims_paid = pool
            .total_claims_paid
            .checked_add(queue_item.amount)
            .expect("Overflow");
        pool.reserve_amount = if pool.total_assets > 0 && pool.total_shares > 0 {
            pool.total_assets
                .checked_mul(pool.reserve_ratio_bps as i128)
                .expect("Overflow")
                / BPS_DENOMINATOR
        } else {
            0
        };
        env.storage()
            .instance()
            .set(&get_pool_key(&queue_item.pool_id), &pool);

        // Update claim status
        claim.status = ClaimStatus::Paid;
        env.storage()
            .instance()
            .set(&get_claim_key(queue_item.claim_id), &claim);

        // Mark queue item as processed
        queue_item.processed = true;
        env.storage()
            .instance()
            .set(&get_payout_queue_item_key(queue_id), &queue_item);

        // Deactivate the policy
        let mut policy = Self::load_policy(&env, &claim.policy_id);
        policy.is_active = false;
        pool.active_policies = pool.active_policies.saturating_sub(1);
        env.storage()
            .instance()
            .set(&get_pool_key(&queue_item.pool_id), &pool);
        env.storage()
            .instance()
            .set(&get_policy_key(claim.policy_id), &policy);

        env.events().publish(
            (soroban_sdk::symbol_short!("clm_pay"),),
            (
                queue_item.claim_id,
                queue_item.pool_id,
                claim.claimant,
                queue_item.amount,
            ),
        );

        queue_item.amount
    }

    // ── GOVERNANCE: RISK PARAMETERS ────────────────────────────

    /// Update risk parameters for a coverage type.
    pub fn set_risk_parameters(
        env: Env,
        admin: Address,
        coverage_type: CoverageType,
        base_rate_bps: u32,
        risk_multiplier: u32,
        max_coverage_limit: i128,
        min_coverage_limit: i128,
        default_deductible_bps: u32,
        max_payout_bps: u32,
        timelock_period: u64,
        voting_threshold_bps: u32,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let ct_key = Self::coverage_type_to_key(&coverage_type);
        let params = RiskParameters {
            coverage_type,
            base_rate_bps,
            risk_multiplier,
            max_coverage_limit,
            min_coverage_limit,
            default_deductible_bps,
            max_payout_bps,
            timelock_period,
            voting_threshold_bps,
        };

        env.storage()
            .instance()
            .set(&get_risk_parameters_key(&ct_key), &params);

        env.events().publish(
            (soroban_sdk::symbol_short!("risk_set"),),
            (coverage_type as u32, base_rate_bps, risk_multiplier),
        );
    }

    /// Update tier configuration for a coverage type and tier.
    pub fn set_tier_config(
        env: Env,
        admin: Address,
        coverage_type: CoverageType,
        tier: CoverageTier,
        deductible_bps: u32,
        max_payout_multiplier: u32,
        premium_multiplier_bps: u32,
    ) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        let ct_key = Self::coverage_type_to_key(&coverage_type);
        let t_key = Self::coverage_tier_to_key(&tier);

        let config = CoverageTierConfig {
            tier,
            deductible_bps,
            max_payout_multiplier,
            premium_multiplier_bps,
        };

        env.storage()
            .instance()
            .set(&get_tier_config_key(&ct_key, &t_key), &config);
    }

    /// Update the reserve ratio for a pool.
    pub fn update_reserve_ratio(env: Env, admin: Address, pool_id: Symbol, new_ratio_bps: u32) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if new_ratio_bps > 10_000 {
            invalid_input(&env);
        }

        let mut pool = Self::load_pool(&env, &pool_id);
        pool.reserve_ratio_bps = new_ratio_bps;
        pool.reserve_amount = if pool.total_assets > 0 {
            pool.total_assets
                .checked_mul(new_ratio_bps as i128)
                .expect("Overflow")
                / BPS_DENOMINATOR
        } else {
            0
        };

        env.storage().instance().set(&get_pool_key(&pool_id), &pool);

        env.events().publish(
            (soroban_sdk::symbol_short!("rsrv_upd"),),
            (pool_id, new_ratio_bps, pool.reserve_amount),
        );
    }

    // ── EMERGENCY PAUSE / UNPAUSE ─────────────────────────────

    /// Emergency pause the protocol.
    pub fn emergency_pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if Self::is_paused_fn(&env) {
            already_paused(&env);
        }

        env.storage().instance().set(&get_paused_key(), &true);

        env.events()
            .publish((soroban_sdk::symbol_short!("pause"),), admin);
    }

    /// Unpause the protocol.
    pub fn emergency_unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::assert_admin(&env, &admin);

        if !Self::is_paused_fn(&env) {
            pool_not_paused(&env);
        }

        env.storage().instance().set(&get_paused_key(), &false);

        env.events()
            .publish((soroban_sdk::symbol_short!("unpause"),), admin);
    }

    // ── VIEW FUNCTIONS ─────────────────────────────────────────

    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    pub fn get_token(env: Env) -> Address {
        Self::token(&env)
    }

    pub fn get_oracle(env: Env) -> Address {
        Self::oracle(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_fn(&env)
    }

    pub fn get_pool_info(env: Env, pool_id: Symbol) -> PoolInfo {
        let pool = Self::load_pool(&env, &pool_id);
        let share_price = if pool.total_shares > 0 {
            pool.total_assets.checked_mul(10_000).expect("Overflow") / pool.total_shares
        } else {
            10_000
        };

        PoolInfo {
            pool_id: pool.pool_id,
            coverage_type: pool.coverage_type,
            total_assets: pool.total_assets,
            total_shares: pool.total_shares,
            share_price,
            reserve_ratio_bps: pool.reserve_ratio_bps,
            reserve_amount: pool.reserve_amount,
            active_policies: pool.active_policies,
            total_premiums_collected: pool.total_premiums_collected,
            total_claims_paid: pool.total_claims_paid,
            is_active: pool.is_active,
        }
    }

    pub fn get_pool_ids(env: Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&get_pool_ids_key())
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_underwriter(env: Env, pool_id: Symbol, address: Address) -> Underwriter {
        Self::load_underwriter(&env, &pool_id, &address)
    }

    pub fn get_policy(env: Env, policy_id: u64) -> CoveragePolicy {
        Self::load_policy(&env, &policy_id)
    }

    pub fn get_claim(env: Env, claim_id: u64) -> Claim {
        Self::load_claim(&env, &claim_id)
    }

    pub fn get_payout_queue_item(env: Env, queue_id: u64) -> PayoutQueueItem {
        Self::load_payout_queue_item(&env, &queue_id)
    }

    pub fn get_payout_queue_ids(env: Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&get_payout_queue_ids_key())
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_holder_policies(env: Env, holder: Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&get_holder_policies_key(&holder))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_pool_claims(env: Env, pool_id: Symbol) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&get_pool_claims_key(&pool_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_risk_parameters(env: Env, coverage_type: CoverageType) -> RiskParameters {
        let ct_key = Self::coverage_type_to_key(&coverage_type);
        env.storage()
            .instance()
            .get(&get_risk_parameters_key(&ct_key))
            .expect("Risk parameters not found")
    }

    pub fn get_tier_config(
        env: Env,
        coverage_type: CoverageType,
        tier: CoverageTier,
    ) -> CoverageTierConfig {
        let ct_key = Self::coverage_type_to_key(&coverage_type);
        let t_key = Self::coverage_tier_to_key(&tier);
        env.storage()
            .instance()
            .get(&get_tier_config_key(&ct_key, &t_key))
            .expect("Tier config not found")
    }

    pub fn preview_premium(
        env: Env,
        pool_id: Symbol,
        coverage_amount: i128,
        tier: CoverageTier,
    ) -> i128 {
        Self::calculate_premium(env, pool_id, coverage_amount, tier)
    }

    // ── INTERNAL HELPERS ───────────────────────────────────────

    fn assert_admin(env: &Env, caller: &Address) {
        let admin = Self::admin(env);
        if caller != &admin {
            unauthorized(env);
        }
    }

    fn admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&get_admin_key())
            .expect("Not initialized")
    }

    fn token(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&get_token_key())
            .expect("Not initialized")
    }

    fn oracle(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&get_oracle_key())
            .expect("Not initialized")
    }

    fn is_paused_fn(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&get_paused_key())
            .unwrap_or(false)
    }

    fn load_pool(env: &Env, pool_id: &Symbol) -> InsurancePool {
        env.storage()
            .instance()
            .get(&get_pool_key(pool_id))
            .unwrap_or_else(|| pool_not_found(env))
    }

    fn load_underwriter(env: &Env, pool_id: &Symbol, address: &Address) -> Underwriter {
        let key = get_underwriter_key(pool_id, address);
        env.storage().instance().get(&key).unwrap_or(Underwriter {
            address: address.clone(),
            shares: 0,
            total_deposited: 0,
            total_withdrawn: 0,
            deposit_timestamp: 0,
            last_withdrawal_timestamp: 0,
        })
    }

    fn load_policy(env: &Env, policy_id: &u64) -> CoveragePolicy {
        env.storage()
            .instance()
            .get(&get_policy_key(*policy_id))
            .unwrap_or_else(|| policy_not_found(env))
    }

    fn load_claim(env: &Env, claim_id: &u64) -> Claim {
        env.storage()
            .instance()
            .get(&get_claim_key(*claim_id))
            .unwrap_or_else(|| claim_not_found(env))
    }

    fn load_payout_queue_item(env: &Env, queue_id: &u64) -> PayoutQueueItem {
        env.storage()
            .instance()
            .get(&get_payout_queue_item_key(*queue_id))
            .unwrap_or_else(|| payout_queue_empty(env))
    }

    fn load_risk_params(env: &Env, coverage_type: &CoverageType) -> RiskParameters {
        let ct_key = Self::coverage_type_to_key(coverage_type);
        env.storage()
            .instance()
            .get(&get_risk_parameters_key(&ct_key))
            .expect("Risk parameters not initialized")
    }

    fn load_tier_config(
        env: &Env,
        coverage_type: &CoverageType,
        tier: &CoverageTier,
    ) -> CoverageTierConfig {
        let ct_key = Self::coverage_type_to_key(coverage_type);
        let t_key = Self::coverage_tier_to_key(tier);
        env.storage()
            .instance()
            .get(&get_tier_config_key(&ct_key, &t_key))
            .expect("Tier config not initialized")
    }

    fn coverage_type_to_key(ct: &CoverageType) -> CoverageTypeKey {
        match ct {
            CoverageType::SmartContractRisk => CoverageTypeKey::SmartContractRisk,
            CoverageType::OracleFailure => CoverageTypeKey::OracleFailure,
            CoverageType::LiquidationFailure => CoverageTypeKey::LiquidationFailure,
        }
    }

    fn coverage_tier_to_key(tier: &CoverageTier) -> CoverageTierKey {
        match tier {
            CoverageTier::Basic => CoverageTierKey::Basic,
            CoverageTier::Standard => CoverageTierKey::Standard,
            CoverageTier::Premium => CoverageTierKey::Premium,
        }
    }

    fn next_policy_id(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&get_policy_counter_key())
            .unwrap_or(0);
        let next = current.checked_add(1).expect("Overflow");
        env.storage()
            .instance()
            .set(&get_policy_counter_key(), &next);
        next
    }

    fn next_claim_id(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&get_claim_counter_key())
            .unwrap_or(0);
        let next = current.checked_add(1).expect("Overflow");
        env.storage()
            .instance()
            .set(&get_claim_counter_key(), &next);
        next
    }

    fn next_payout_queue_id(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .instance()
            .get(&get_payout_queue_counter_key())
            .unwrap_or(0);
        let next = current.checked_add(1).expect("Overflow");
        env.storage()
            .instance()
            .set(&get_payout_queue_counter_key(), &next);
        next
    }

    fn count_pool_underwriters(env: &Env, pool_id: &Symbol) -> u32 {
        // Iterate through known underwriters for this pool
        // We track this via the pool counter for simplicity
        let pool = Self::load_pool(env, pool_id);
        // Approximate: use total_shares / min_deposit as count
        // In production, we'd maintain a separate index
        if pool.total_shares > 0 {
            // Return a reasonable estimate - actual tracking would need an index
            0 // Placeholder: in production this would iterate an index
        } else {
            0
        }
    }

    fn estimate_total_liabilities(env: &Env, pool_id: &Symbol) -> i128 {
        // Estimate total pending claim liabilities for the pool
        let claim_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&get_pool_claims_key(pool_id))
            .unwrap_or_else(|| Vec::new(env));

        let mut total_liabilities: i128 = 0;
        for i in 0..claim_ids.len() {
            let cid = claim_ids.get(i).unwrap();
            let claim: Claim = env
                .storage()
                .instance()
                .get(&get_claim_key(cid))
                .unwrap_or_else(|| claim_not_found(env));
            if claim.status == ClaimStatus::Pending || claim.status == ClaimStatus::Approved {
                total_liabilities = total_liabilities
                    .checked_add(claim.amount)
                    .expect("Overflow");
            }
        }
        total_liabilities
    }

    fn calculate_claim_priority(_env: &Env, claim: &Claim) -> u32 {
        // Priority 1: smallest claims process first (faster payout for smaller amounts)
        // Priority 2: older claims get higher priority
        // Simple formula: higher priority = lower value (1 = highest priority)
        if claim.amount <= 10_000 {
            1 // High priority for small claims
        } else if claim.amount <= 100_000 {
            2 // Medium priority
        } else {
            3 // Low priority for large claims
        }
    }

    fn init_default_risk_params(env: &Env) {
        let coverage_types = [
            CoverageType::SmartContractRisk,
            CoverageType::OracleFailure,
            CoverageType::LiquidationFailure,
        ];

        for ct in coverage_types.iter() {
            let ct_key = Self::coverage_type_to_key(ct);
            let params = RiskParameters {
                coverage_type: *ct,
                base_rate_bps: 500,                // 5% base rate
                risk_multiplier: 10_000,           // 1x (neutral)
                max_coverage_limit: 1_000_000_000, // 1M USDC in stroops
                min_coverage_limit: 100,           // 0.01 USDC minimum
                default_deductible_bps: 1_000,     // 10% deductible
                max_payout_bps: 10_000,            // 100% max payout
                timelock_period: DEFAULT_TIMELOCK_PERIOD,
                voting_threshold_bps: 1_000, // 10% of pool
            };
            env.storage()
                .instance()
                .set(&get_risk_parameters_key(&ct_key), &params);
        }
    }

    fn init_default_tier_configs(env: &Env) {
        let coverage_types = [
            CoverageType::SmartContractRisk,
            CoverageType::OracleFailure,
            CoverageType::LiquidationFailure,
        ];
        let tiers = [
            CoverageTier::Basic,
            CoverageTier::Standard,
            CoverageTier::Premium,
        ];

        for ct in coverage_types.iter() {
            for tier in tiers.iter() {
                let ct_key = Self::coverage_type_to_key(ct);
                let t_key = Self::coverage_tier_to_key(tier);

                let config = match tier {
                    CoverageTier::Basic => CoverageTierConfig {
                        tier: *tier,
                        deductible_bps: 2_000,         // 20% deductible
                        max_payout_multiplier: 10_000, // 100% of coverage
                        premium_multiplier_bps: 8_000, // 0.8x premium
                    },
                    CoverageTier::Standard => CoverageTierConfig {
                        tier: *tier,
                        deductible_bps: 1_000,          // 10% deductible
                        max_payout_multiplier: 10_000,  // 100% of coverage
                        premium_multiplier_bps: 10_000, // 1x premium
                    },
                    CoverageTier::Premium => CoverageTierConfig {
                        tier: *tier,
                        deductible_bps: 500,            // 5% deductible
                        max_payout_multiplier: 12_000,  // 120% of coverage
                        premium_multiplier_bps: 15_000, // 1.5x premium
                    },
                };

                env.storage()
                    .instance()
                    .set(&get_tier_config_key(&ct_key, &t_key), &config);
            }
        }
    }
}
