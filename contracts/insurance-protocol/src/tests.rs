use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    Address, Env, String as SorobanString, Symbol,
};

use crate::contract::InsuranceProtocolClient;
use crate::errors::InsuranceError;
use crate::types::*;
use crate::InsuranceProtocol;

#[contract]
pub struct MockToken;
#[derive(Clone)]
#[contracttype]
pub enum MockTokenKey {
    Balance(Address),
}
#[contractimpl]
impl MockToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = MockTokenKey::Balance(to);
        let c: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(c + amount));
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockTokenKey::Balance(id))
            .unwrap_or(0)
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let fk = MockTokenKey::Balance(from.clone());
        let fb: i128 = env.storage().instance().get(&fk).unwrap_or(0);
        env.storage().instance().set(&fk, &(fb - amount));
        let tk = MockTokenKey::Balance(to);
        let tb: i128 = env.storage().instance().get(&tk).unwrap_or(0);
        env.storage().instance().set(&tk, &(tb + amount));
    }
}

#[contract]
pub struct MockOracle;
#[contractimpl]
impl MockOracle {}

fn ok<T>(code: InsuranceError) -> Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>> {
    Err(Ok(soroban_sdk::Error::from_contract_error(code as u32)))
}

struct F {
    e: Env,
    c: InsuranceProtocolClient<'static>,
    t: MockTokenClient<'static>,
    a: Address,
}

fn s() -> F {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| {
        l.timestamp = 1_000_000;
    });
    let a = Address::generate(&e);
    let ti = e.register(MockToken, ());
    let t = MockTokenClient::new(&e, &ti);
    let oi = e.register(MockOracle, ());
    let ci = e.register(InsuranceProtocol, ());
    let c = InsuranceProtocolClient::new(&e, &ci);
    c.initialize(&a, &ti, &oi);
    F { e, c, t, a }
}

fn mp(e: &Env) -> Symbol {
    Symbol::new(e, "ETH")
}

fn sp() -> (F, Symbol) {
    let f = s();
    let p = mp(&f.e);
    f.c.create_pool(&f.a, &p, &CoverageType::SmartContractRisk, &3000);
    (f, p)
}

fn su() -> (F, Symbol, Address) {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &1_000_000);
    f.c.deposit_as_underwriter(&u, &p, &500_000);
    (f, p, u)
}

fn fut(e: &Env, n: u64) {
    e.ledger().with_mut(|l| {
        l.timestamp += n;
    });
}
fn ev(e: &Env, s: &str) -> SorobanString {
    SorobanString::from_str(e, s)
}

// ═══════════════════════════════════════════════════════════════
//  INIT
// ═══════════════════════════════════════════════════════════════

#[test]
fn init() {
    let f = s();
    assert_eq!(f.c.get_admin(), f.a);
    assert_eq!(f.c.get_token(), f.t.address);
    assert!(!f.c.is_paused());
}

#[test]
fn double_init() {
    let f = s();
    assert_eq!(
        f.c.try_initialize(&f.a, &f.t.address, &f.e.register(MockOracle, ())),
        ok(InsuranceError::AlreadyInitialized)
    );
}

// ═══════════════════════════════════════════════════════════════
//  POOL
// ═══════════════════════════════════════════════════════════════

#[test]
fn create_pool() {
    let (f, p) = sp();
    let i = f.c.get_pool_info(&p);
    assert_eq!(i.coverage_type, CoverageType::SmartContractRisk);
    assert_eq!(i.total_assets, 0);
    assert_eq!(i.reserve_ratio_bps, 3000);
    assert!(i.is_active);
}

#[test]
fn dup_pool() {
    let (f, p) = sp();
    assert_eq!(
        f.c.try_create_pool(&f.a, &p, &CoverageType::OracleFailure, &3000),
        ok(InsuranceError::PoolAlreadyExists)
    );
}

#[test]
fn three_pools() {
    let f = s();
    let p1 = Symbol::new(&f.e, "A");
    let p2 = Symbol::new(&f.e, "B");
    let p3 = Symbol::new(&f.e, "C");
    f.c.create_pool(&f.a, &p1, &CoverageType::SmartContractRisk, &3000);
    f.c.create_pool(&f.a, &p2, &CoverageType::OracleFailure, &4000);
    f.c.create_pool(&f.a, &p3, &CoverageType::LiquidationFailure, &5000);
    assert_eq!(f.c.get_pool_ids().len(), 3);
}

// ═══════════════════════════════════════════════════════════════
//  UNDERWRITERS
// ═══════════════════════════════════════════════════════════════

#[test]
fn deposit_first() {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &100_000);
    f.c.deposit_as_underwriter(&u, &p, &100_000);
    assert_eq!(f.c.get_pool_info(&p).total_assets, 100_000);
    assert_eq!(f.c.get_underwriter(&p, &u).shares, 100_000);
}

#[test]
fn deposit_proportional() {
    let (f, p) = sp();
    let a = Address::generate(&f.e);
    let b = Address::generate(&f.e);
    f.t.mint(&a, &100_000);
    f.t.mint(&b, &200_000);
    f.c.deposit_as_underwriter(&a, &p, &100_000);
    f.c.deposit_as_underwriter(&b, &p, &200_000);
    assert_eq!(f.c.get_pool_info(&p).total_shares, 300_000);
}

#[test]
fn withdraw_full() {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &100_000);
    f.c.deposit_as_underwriter(&u, &p, &100_000);
    assert_eq!(f.c.withdraw_as_underwriter(&u, &p, &100_000), 100_000);
    assert_eq!(f.c.get_pool_info(&p).total_assets, 0);
}

#[test]
fn withdraw_partial() {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &100_000);
    f.c.deposit_as_underwriter(&u, &p, &100_000);
    assert_eq!(f.c.withdraw_as_underwriter(&u, &p, &50_000), 50_000);
    assert_eq!(f.c.get_underwriter(&p, &u).shares, 50_000);
}

#[test]
fn withdraw_too_much() {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &100_000);
    f.c.deposit_as_underwriter(&u, &p, &100_000);
    assert_eq!(
        f.c.try_withdraw_as_underwriter(&u, &p, &200_000),
        ok(InsuranceError::InsufficientShares)
    );
}

#[test]
fn two_uws_share() {
    let (f, p) = sp();
    let a = Address::generate(&f.e);
    let b = Address::generate(&f.e);
    f.t.mint(&a, &300_000);
    f.t.mint(&b, &700_000);
    f.c.deposit_as_underwriter(&a, &p, &300_000);
    f.c.deposit_as_underwriter(&b, &p, &700_000);
    assert_eq!(f.c.withdraw_as_underwriter(&a, &p, &300_000), 300_000);
    assert_eq!(f.c.withdraw_as_underwriter(&b, &p, &700_000), 700_000);
    assert_eq!(f.c.get_pool_info(&p).total_assets, 0);
}

// ═══════════════════════════════════════════════════════════════
//  PREMIUM
// ═══════════════════════════════════════════════════════════════

#[test]
fn premium_positive() {
    let (f, p) = sp();
    assert!(f.c.preview_premium(&p, &100_000, &CoverageTier::Standard) > 0);
}

#[test]
fn premium_scales() {
    let (f, p) = sp();
    let a = f.c.preview_premium(&p, &10_000, &CoverageTier::Standard);
    let b = f.c.preview_premium(&p, &100_000, &CoverageTier::Standard);
    let c = f.c.preview_premium(&p, &1_000_000, &CoverageTier::Standard);
    assert!(b > a && c > b);
}

#[test]
fn premium_by_tier() {
    let (f, p) = sp();
    let b = f.c.preview_premium(&p, &100_000, &CoverageTier::Basic);
    let st = f.c.preview_premium(&p, &100_000, &CoverageTier::Standard);
    let pr = f.c.preview_premium(&p, &100_000, &CoverageTier::Premium);
    assert!(pr >= st && st >= b);
}

#[test]
fn premium_pool_depth() {
    let (f, p) = sp();
    let pe = f.c.preview_premium(&p, &100_000, &CoverageTier::Standard);
    let u1 = Address::generate(&f.e);
    let u2 = Address::generate(&f.e);
    f.t.mint(&u1, &1_000_000);
    f.t.mint(&u2, &1_000_000);
    f.c.deposit_as_underwriter(&u1, &p, &1_000_000);
    f.c.deposit_as_underwriter(&u2, &p, &1_000_000);
    let pd = f.c.preview_premium(&p, &100_000, &CoverageTier::Standard);
    assert!(pd < pe);
}

// ═══════════════════════════════════════════════════════════════
//  COVERAGE PURCHASE
// ═══════════════════════════════════════════════════════════════

#[test]
fn buy_coverage() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    assert!(pid > 0);
    let pol = f.c.get_policy(&pid);
    assert_eq!(pol.holder, b);
    assert_eq!(pol.pool_id, p);
    assert!(pol.coverage_limit > 0 && pol.premium_paid > 0);
    assert_eq!(f.c.get_pool_info(&p).active_policies, 1);
}

#[test]
fn buy_transfers_premium() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let before = f.t.balance(&b);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    assert_eq!(f.t.balance(&b), before - f.c.get_policy(&pid).premium_paid);
}

#[test]
fn cancel_refund() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let before = f.t.balance(&b);
    let ref_ = f.c.cancel_policy(&b, &pid);
    assert!(ref_ >= 0);
    assert_eq!(f.t.balance(&b), before + ref_);
    assert!(!f.c.get_policy(&pid).is_active);
}

// ═══════════════════════════════════════════════════════════════
//  CLAIMS
// ═══════════════════════════════════════════════════════════════

#[test]
fn submit_claim() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &50_000, &ev(&f.e, "tx"));
    assert!(cid > 0);
    let cl = f.c.get_claim(&cid);
    assert_eq!(cl.status, ClaimStatus::Pending);
    assert!(!cl.requires_voting);
}

#[test]
fn claim_checks_deductible() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let ded = f.c.get_policy(&pid).deductible;
    assert_eq!(
        f.c.try_submit_claim(&b, &pid, &ded, &ev(&f.e, "e")),
        ok(InsuranceError::DeductibleNotMet)
    );
}

#[test]
fn claim_checks_limit() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid = f.c.purchase_coverage(&b, &p, &CoverageTier::Basic, &10_000);
    let pol = f.c.get_policy(&pid);
    let amt = pol.coverage_limit + pol.deductible + 1;
    assert_eq!(
        f.c.try_submit_claim(&b, &pid, &amt, &ev(&f.e, "big")),
        ok(InsuranceError::ClaimAmountExceedsCoverage)
    );
}

#[test]
fn process_small_claim() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &15_000, &ev(&f.e, "bug"));
    fut(&f.e, 259_201);
    f.c.process_claim(&cid);
    assert_eq!(f.c.get_claim(&cid).status, ClaimStatus::Approved);
}

#[test]
fn pay_claim() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &20_000, &ev(&f.e, "loss"));
    fut(&f.e, 259_201);
    f.c.process_claim(&cid);
    let bb = f.t.balance(&b);
    let q = f.c.get_payout_queue_ids();
    f.c.pay_claim(&f.a, &q.get(0).unwrap());
    assert!(f.t.balance(&b) > bb);
    assert_eq!(f.c.get_claim(&cid).status, ClaimStatus::Paid);
}

// ═══════════════════════════════════════════════════════════════
//  VOTING
// ═══════════════════════════════════════════════════════════════

#[test]
fn large_claim_needs_vote() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &100_000, &ev(&f.e, "exploit"));
    assert!(f.c.get_claim(&cid).requires_voting);
}

#[test]
fn stranger_cannot_vote() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &100_000, &ev(&f.e, "exploit"));
    let str = Address::generate(&f.e);
    assert_eq!(
        f.c.try_vote_on_claim(&str, &cid, &true),
        ok(InsuranceError::NotAnUnderwriter)
    );
}

#[test]
fn no_double_vote() {
    let (f, p, u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &100_000, &ev(&f.e, "exploit"));
    f.c.vote_on_claim(&u, &cid, &true);
    assert_eq!(
        f.c.try_vote_on_claim(&u, &cid, &true),
        ok(InsuranceError::AlreadyVoted)
    );
}

#[test]
fn vote_again_denies() {
    let (f, p, u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &100_000, &ev(&f.e, "bad"));
    f.c.vote_on_claim(&u, &cid, &false);
    fut(&f.e, 604_801);
    f.c.process_claim(&cid);
    assert_eq!(f.c.get_claim(&cid).status, ClaimStatus::Denied);
}

// ═══════════════════════════════════════════════════════════════
//  RESERVE
// ═══════════════════════════════════════════════════════════════

#[test]
fn reserve_after_deposit() {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &1_000_000);
    f.c.deposit_as_underwriter(&u, &p, &1_000_000);
    assert_eq!(f.c.get_pool_info(&p).reserve_amount, 300_000);
}

#[test]
fn update_reserve() {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &1_000_000);
    f.c.deposit_as_underwriter(&u, &p, &1_000_000);
    f.c.update_reserve_ratio(&f.a, &p, &5000);
    assert_eq!(f.c.get_pool_info(&p).reserve_amount, 500_000);
}

// ═══════════════════════════════════════════════════════════════
//  TIERS
// ═══════════════════════════════════════════════════════════════

#[test]
fn tier_deductibles() {
    let f = s();
    let b =
        f.c.get_tier_config(&CoverageType::SmartContractRisk, &CoverageTier::Basic);
    let st =
        f.c.get_tier_config(&CoverageType::SmartContractRisk, &CoverageTier::Standard);
    let pr =
        f.c.get_tier_config(&CoverageType::SmartContractRisk, &CoverageTier::Premium);
    assert!(b.deductible_bps > st.deductible_bps && st.deductible_bps > pr.deductible_bps);
}

#[test]
fn tier_payouts() {
    let f = s();
    let b =
        f.c.get_tier_config(&CoverageType::SmartContractRisk, &CoverageTier::Basic);
    let st =
        f.c.get_tier_config(&CoverageType::SmartContractRisk, &CoverageTier::Standard);
    let pr =
        f.c.get_tier_config(&CoverageType::SmartContractRisk, &CoverageTier::Premium);
    assert!(pr.max_payout_multiplier >= st.max_payout_multiplier);
    assert!(st.max_payout_multiplier >= b.max_payout_multiplier);
}

// ═══════════════════════════════════════════════════════════════
//  QUEUE
// ═══════════════════════════════════════════════════════════════

#[test]
fn queue_priority() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    f.c.submit_claim(&b, &pid, &15_000, &ev(&f.e, "small"));
    f.c.submit_claim(&b, &pid, &50_000, &ev(&f.e, "big"));
    fut(&f.e, 259_201);
    f.c.process_claim(&1);
    f.c.process_claim(&2);
    let q = f.c.get_payout_queue_ids();
    assert_eq!(q.len(), 2);
    let i1 = f.c.get_payout_queue_item(&q.get(0).unwrap());
    let i2 = f.c.get_payout_queue_item(&q.get(1).unwrap());
    assert!(i1.priority <= i2.priority);
}

// ═══════════════════════════════════════════════════════════════
//  RISK PARAMS
// ═══════════════════════════════════════════════════════════════

#[test]
fn default_risk() {
    let f = s();
    let rp = f.c.get_risk_parameters(&CoverageType::SmartContractRisk);
    assert_eq!(rp.base_rate_bps, 500);
    assert_eq!(rp.risk_multiplier, 10_000);
}

#[test]
fn update_risk() {
    let f = s();
    f.c.set_risk_parameters(
        &f.a,
        &CoverageType::SmartContractRisk,
        &1000,
        &15_000,
        &5_000_000,
        &100,
        &500,
        &10_000,
        &1800,
        &1_000,
    );
    assert_eq!(
        f.c.get_risk_parameters(&CoverageType::SmartContractRisk)
            .base_rate_bps,
        1000
    );
}

#[test]
fn update_tier() {
    let f = s();
    f.c.set_tier_config(
        &f.a,
        &CoverageType::SmartContractRisk,
        &CoverageTier::Premium,
        &250,
        &15_000,
        &20_000,
    );
    let tc =
        f.c.get_tier_config(&CoverageType::SmartContractRisk, &CoverageTier::Premium);
    assert_eq!(tc.deductible_bps, 250);
    assert_eq!(tc.max_payout_multiplier, 15_000);
}

// ═══════════════════════════════════════════════════════════════
//  PAUSE
// ═══════════════════════════════════════════════════════════════

#[test]
fn pause_blocks_deposit() {
    let (f, p) = sp();
    f.c.emergency_pause(&f.a);
    let u = Address::generate(&f.e);
    f.t.mint(&u, &100_000);
    assert_eq!(
        f.c.try_deposit_as_underwriter(&u, &p, &100_000),
        ok(InsuranceError::AlreadyPaused)
    );
}

#[test]
fn pause_blocks_buy() {
    let (f, p, _u) = su();
    f.c.emergency_pause(&f.a);
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    assert_eq!(
        f.c.try_purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000),
        ok(InsuranceError::AlreadyPaused)
    );
}

#[test]
fn unpause_works() {
    let (f, p) = sp();
    f.c.emergency_pause(&f.a);
    f.c.emergency_unpause(&f.a);
    let u = Address::generate(&f.e);
    f.t.mint(&u, &100_000);
    f.c.deposit_as_underwriter(&u, &p, &100_000);
    assert_eq!(f.c.get_pool_info(&p).total_assets, 100_000);
}

// ═══════════════════════════════════════════════════════════════
//  FUNDING SCENARIOS
// ═══════════════════════════════════════════════════════════════

#[test]
fn claim_within_capacity() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &30_000, &ev(&f.e, "loss"));
    fut(&f.e, 259_201);
    f.c.process_claim(&cid);
    let q = f.c.get_payout_queue_ids();
    f.c.pay_claim(&f.a, &q.get(0).unwrap());
    assert_eq!(f.c.get_claim(&cid).status, ClaimStatus::Paid);
}

#[test]
fn cascading_claims() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let p1 =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let p2 =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let c1 = f.c.submit_claim(&b, &p1, &30_000, &ev(&f.e, "L1"));
    let c2 = f.c.submit_claim(&b, &p2, &30_000, &ev(&f.e, "L2"));
    fut(&f.e, 259_201);
    f.c.process_claim(&c1);
    f.c.process_claim(&c2);
    let q = f.c.get_payout_queue_ids();
    f.c.pay_claim(&f.a, &q.get(0).unwrap());
    let r = f.c.try_pay_claim(&f.a, &q.get(1).unwrap());
    if r.is_err() {
        assert_eq!(f.c.get_claim(&c2).status, ClaimStatus::Approved);
    }
}

// ═══════════════════════════════════════════════════════════════
//  SECURITY
// ═══════════════════════════════════════════════════════════════

#[test]
fn non_admin_no_pool() {
    let f = s();
    let na = Address::generate(&f.e);
    let p = mp(&f.e);
    assert_eq!(
        f.c.try_create_pool(&na, &p, &CoverageType::SmartContractRisk, &3000),
        ok(InsuranceError::Unauthorized)
    );
}

#[test]
fn non_admin_no_risk() {
    let f = s();
    let na = Address::generate(&f.e);
    assert_eq!(
        f.c.try_set_risk_parameters(
            &na,
            &CoverageType::SmartContractRisk,
            &1000,
            &15_000,
            &5_000_000,
            &100,
            &500,
            &10_000,
            &1800,
            &1_000
        ),
        ok(InsuranceError::Unauthorized)
    );
}

#[test]
fn non_admin_no_pay() {
    let (f, p, _u) = su();
    let na = Address::generate(&f.e);
    let b = Address::generate(&f.e);
    f.t.mint(&b, &5_000_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    let cid = f.c.submit_claim(&b, &pid, &15_000, &ev(&f.e, "ev"));
    fut(&f.e, 259_201);
    f.c.process_claim(&cid);
    let q = f.c.get_payout_queue_ids();
    assert_eq!(
        f.c.try_pay_claim(&na, &q.get(0).unwrap()),
        ok(InsuranceError::Unauthorized)
    );
}

#[test]
fn premium_deterministic() {
    let (f, p) = sp();
    let a = f.c.preview_premium(&p, &100_000, &CoverageTier::Standard);
    assert_eq!(
        a,
        f.c.preview_premium(&p, &100_000, &CoverageTier::Standard)
    );
    assert_ne!(
        a,
        f.c.preview_premium(&p, &200_000, &CoverageTier::Standard)
    );
}

// ═══════════════════════════════════════════════════════════════
//  EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[test]
fn empty_pool_withdraw() {
    let (f, p) = sp();
    let u = Address::generate(&f.e);
    f.t.mint(&u, &100_000);
    f.c.deposit_as_underwriter(&u, &p, &100_000);
    f.c.withdraw_as_underwriter(&u, &p, &100_000);
    assert_eq!(f.c.get_pool_info(&p).total_assets, 0);
}

#[test]
fn no_double_cancel() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    f.c.cancel_policy(&b, &pid);
    assert_eq!(
        f.c.try_cancel_policy(&b, &pid),
        ok(InsuranceError::PolicyInactive)
    );
}

#[test]
fn views_ok() {
    let (f, p, _u) = su();
    let b = Address::generate(&f.e);
    f.t.mint(&b, &500_000);
    let pid =
        f.c.purchase_coverage(&b, &p, &CoverageTier::Standard, &100_000);
    assert!(f.c.get_pool_info(&p).total_assets > 0);
    assert_eq!(f.c.get_pool_info(&p).active_policies, 1);
    assert_eq!(f.c.get_policy(&pid).holder, b);
    assert_eq!(f.c.get_holder_policies(&b).len(), 1);
    assert_eq!(f.c.get_pool_claims(&p).len(), 0);
}
