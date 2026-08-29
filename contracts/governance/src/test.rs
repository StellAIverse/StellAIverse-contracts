#[cfg(test)]
mod tests {
    use crate::contract::GovernanceContractClient;
    use crate::{GovernanceContract, ProposalAction, ProposalState, VoteType};
    use crate::{MAX_LOCK_DURATION, MIN_LOCK_DURATION};
    use soroban_sdk::{
        contract, contractimpl, contracttype,
        testutils::{Address as _, Ledger as _},
        Address, Bytes, Env, String, Vec,
    };

    // ── Mock Token Contract ──────────────────────────────────────────────

    #[contract]
    pub struct MockToken;

    #[derive(Clone)]
    #[contracttype]
    pub enum MockTokenKey {
        Balance(Address),
        Supply,
    }

    #[contractimpl]
    impl MockToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let key = MockTokenKey::Balance(to.clone());
            let current: i128 = env.storage().instance().get(&key).unwrap_or(0);
            env.storage()
                .instance()
                .set(&key, &(current.checked_add(amount).unwrap()));
            let supply_key = MockTokenKey::Supply;
            let supply: i128 = env.storage().instance().get(&supply_key).unwrap_or(0);
            env.storage()
                .instance()
                .set(&supply_key, &(supply.checked_add(amount).unwrap()));
        }

        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage()
                .instance()
                .get(&MockTokenKey::Balance(id))
                .unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let from_key = MockTokenKey::Balance(from.clone());
            let from_balance: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
            if from_balance < amount {
                panic!("Insufficient balance");
            }
            env.storage()
                .instance()
                .set(&from_key, &(from_balance - amount));
            let to_key = MockTokenKey::Balance(to.clone());
            let to_balance: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
            env.storage()
                .instance()
                .set(&to_key, &(to_balance.checked_add(amount).unwrap()));
        }

        pub fn total_supply(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&MockTokenKey::Supply)
                .unwrap_or(0)
        }
    }

    // ── Test Helpers ─────────────────────────────────────────────────────

    struct TestContext {
        env: Env,
        gov: GovernanceContractClient<'static>,
        token: MockTokenClient<'static>,
        admin: Address,
        token_id: Address,
    }

    fn setup() -> TestContext {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1000);
        let admin = Address::generate(&env);
        let token_id = env.register(MockToken, ());
        let token = MockTokenClient::new(&env, &token_id);
        let gov_id = env.register(GovernanceContract, ());
        let gov = GovernanceContractClient::new(&env, &gov_id);
        gov.initialize(
            &admin, &token_id, &1, &100, &86400, &2000, &5100, &100, &1_000_000,
        );
        TestContext {
            env,
            gov,
            token,
            admin,
            token_id,
        }
    }

    fn empty_args(
        ctx: &TestContext,
    ) -> (
        String,
        Vec<Address>,
        Vec<String>,
        Vec<Bytes>,
        Vec<ProposalAction>,
    ) {
        (
            String::from_str(&ctx.env, "Test proposal"),
            Vec::new(&ctx.env),
            Vec::new(&ctx.env),
            Vec::new(&ctx.env),
            Vec::new(&ctx.env),
        )
    }

    fn create_proposal(ctx: &TestContext, voter: &Address, amount: i128) -> u64 {
        ctx.token.mint(voter, &amount);
        let (d, t, f, c, a) = empty_args(ctx);
        ctx.gov.propose(voter, &d, &t, &f, &c, &a)
    }

    // ── Initialization Tests ─────────────────────────────────────────────

    #[test]
    fn initialize_contract() {
        let ctx = setup();
        assert_eq!(ctx.gov.get_proposal_ids().len(), 0);
    }

    #[test]
    #[should_panic(expected = "1")]
    fn cannot_double_initialize() {
        let ctx = setup();
        ctx.gov.initialize(
            &ctx.admin,
            &ctx.token_id,
            &1,
            &100,
            &86400,
            &2000,
            &5100,
            &100,
            &1_000_000,
        );
    }

    #[test]
    fn get_settings_returns_correct_values() {
        let ctx = setup();
        let s = ctx.gov.get_settings();
        assert_eq!(s.admin, ctx.admin);
        assert_eq!(s.voting_token, ctx.token_id);
        assert_eq!(s.voting_delay, 1);
        assert_eq!(s.voting_period, 100);
        assert_eq!(s.timelock_delay, 86400);
        assert_eq!(s.quorum, 2000);
        assert_eq!(s.approval_threshold, 5100);
        assert_eq!(s.proposal_threshold, 100);
    }

    // ── Proposal Tests ───────────────────────────────────────────────────

    #[test]
    fn create_proposal_basic() {
        let ctx = setup();
        let proposer = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &proposer, 10_000);
        assert_eq!(ctx.gov.get_proposal_ids().len(), 1);
        let p = ctx.gov.get_proposal(&pid);
        assert_eq!(p.proposer, proposer);
        assert_eq!(p.description, String::from_str(&ctx.env, "Test proposal"));
        assert_eq!(p.vote_start, 1); // block 0 + delay 1
        assert_eq!(p.vote_end, 101); // 1 + period 100
    }

    #[test]
    #[should_panic(expected = "9")]
    fn proposal_insufficient_tokens() {
        let ctx = setup();
        let proposer = Address::generate(&ctx.env);
        let (d, t, f, c, a) = empty_args(&ctx);
        ctx.gov.propose(&proposer, &d, &t, &f, &c, &a);
    }

    #[test]
    fn create_multiple_proposals() {
        let ctx = setup();
        let proposer = Address::generate(&ctx.env);
        ctx.token.mint(&proposer, &10_000);
        for _ in 0..3 {
            let (d, t, f, c, a) = empty_args(&ctx);
            ctx.gov.propose(&proposer, &d, &t, &f, &c, &a);
        }
        assert_eq!(ctx.gov.get_proposal_ids().len(), 3);
    }

    // ── Voting Tests ─────────────────────────────────────────────────────

    #[test]
    fn cast_vote_for() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 10_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        let (f, ag, ab, st) = ctx.gov.get_proposal_results(&pid);
        assert_eq!(f, 10_000);
        assert_eq!(ag, 0);
        assert_eq!(ab, 0);
        assert_eq!(st, ProposalState::Active);
    }

    #[test]
    fn cast_vote_against() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 5_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::Against);
        let (f, ag, ab, _) = ctx.gov.get_proposal_results(&pid);
        assert_eq!(f, 0);
        assert_eq!(ag, 5_000);
        assert_eq!(ab, 0);
    }

    #[test]
    fn cast_vote_abstain() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 3_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::Abstain);
        let (f, ag, ab, _) = ctx.gov.get_proposal_results(&pid);
        assert_eq!(f, 0);
        assert_eq!(ag, 0);
        assert_eq!(ab, 3_000);
    }

    #[test]
    #[should_panic(expected = "8")]
    fn cannot_vote_twice() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 10_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::Against);
    }

    #[test]
    #[should_panic(expected = "5")]
    fn vote_on_pending_proposal_fails() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 10_000);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
    }

    #[test]
    fn multiple_voters() {
        let ctx = setup();
        let v1 = Address::generate(&ctx.env);
        let v2 = Address::generate(&ctx.env);
        ctx.token.mint(&v1, &10_000);
        ctx.token.mint(&v2, &5_000);
        let (d, t, f, c, a) = empty_args(&ctx);
        let pid = ctx.gov.propose(&v1, &d, &t, &f, &c, &a);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&v1, &pid, &VoteType::For);
        ctx.gov.cast_vote(&v2, &pid, &VoteType::Against);
        let (fv, av, _, _) = ctx.gov.get_proposal_results(&pid);
        assert_eq!(fv, 10_000);
        assert_eq!(av, 5_000);
    }

    #[test]
    fn vote_info_queried_correctly() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 10_000);
        assert!(ctx.gov.get_vote_info(&pid, &voter).is_none());
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        let vote = ctx.gov.get_vote_info(&pid, &voter).unwrap();
        assert!(vote.has_voted);
        assert_eq!(vote.vote_type, VoteType::For);
        assert_eq!(vote.weight, 10_000);
    }

    // ── Lifecycle Tests ──────────────────────────────────────────────────

    #[test]
    fn proposal_state_transitions() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 600_000);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Pending);
        ctx.env.ledger().set_sequence_number(2);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Active);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(50);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Active);
        ctx.env.ledger().set_sequence_number(103);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Succeeded);
    }

    #[test]
    fn proposal_defeated_no_quorum() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 100);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(103);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Defeated);
    }

    #[test]
    fn proposal_defeated_insufficient_approval() {
        let ctx = setup();
        let v1 = Address::generate(&ctx.env);
        let v2 = Address::generate(&ctx.env);
        ctx.token.mint(&v1, &300_000);
        ctx.token.mint(&v2, &700_000);
        let (d, t, f, c, a) = empty_args(&ctx);
        let pid = ctx.gov.propose(&v1, &d, &t, &f, &c, &a);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&v1, &pid, &VoteType::For);
        ctx.gov.cast_vote(&v2, &pid, &VoteType::Against);
        ctx.env.ledger().set_sequence_number(103);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Defeated);
    }

    #[test]
    fn proposal_succeeded() {
        let ctx = setup();
        let v1 = Address::generate(&ctx.env);
        let v2 = Address::generate(&ctx.env);
        ctx.token.mint(&v1, &600_000);
        ctx.token.mint(&v2, &400_000);
        let (d, t, f, c, a) = empty_args(&ctx);
        let pid = ctx.gov.propose(&v1, &d, &t, &f, &c, &a);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&v1, &pid, &VoteType::For);
        ctx.gov.cast_vote(&v2, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(103);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Succeeded);
    }

    // ── Queue and Execute Tests ──────────────────────────────────────────

    #[test]
    fn queue_proposal() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 600_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(103);
        ctx.gov.queue(&ctx.admin, &pid);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Queued);
        let p = ctx.gov.get_proposal(&pid);
        assert_eq!(p.eta, 1000 + 86400);
    }

    #[test]
    #[should_panic(expected = "13")]
    fn queue_defeated_proposal_fails() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 100);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(103);
        ctx.gov.queue(&ctx.admin, &pid);
    }

    #[test]
    fn execute_after_timelock() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 600_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(103);
        ctx.gov.queue(&ctx.admin, &pid);
        ctx.env.ledger().set_timestamp(87400);
        ctx.gov.execute(&ctx.admin, &pid);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Executed);
    }

    #[test]
    #[should_panic(expected = "12")]
    fn execute_before_timelock_fails() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 600_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(103);
        ctx.gov.queue(&ctx.admin, &pid);
        ctx.gov.execute(&ctx.admin, &pid);
    }

    #[test]
    #[should_panic(expected = "22")]
    fn execute_non_queued_proposal_fails() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 600_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.execute(&ctx.admin, &pid);
    }

    // ── Cancel Tests ─────────────────────────────────────────────────────

    #[test]
    fn cancel_by_proposer() {
        let ctx = setup();
        let proposer = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &proposer, 10_000);
        ctx.gov.cancel(&proposer, &pid);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Canceled);
    }

    #[test]
    fn cancel_by_admin() {
        let ctx = setup();
        let proposer = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &proposer, 10_000);
        ctx.gov.cancel(&ctx.admin, &pid);
        assert_eq!(ctx.gov.get_proposal_state(&pid), ProposalState::Canceled);
    }

    #[test]
    #[should_panic(expected = "2")]
    fn unauthorized_cancel_fails() {
        let ctx = setup();
        let proposer = Address::generate(&ctx.env);
        let rando = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &proposer, 10_000);
        ctx.gov.cancel(&rando, &pid);
    }

    #[test]
    #[should_panic(expected = "18")]
    fn cancel_executed_proposal_fails() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let pid = create_proposal(&ctx, &voter, 600_000);
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);
        ctx.env.ledger().set_sequence_number(103);
        ctx.gov.queue(&ctx.admin, &pid);
        ctx.env.ledger().set_timestamp(87400);
        ctx.gov.execute(&ctx.admin, &pid);
        ctx.gov.cancel(&ctx.admin, &pid);
    }

    // ── Delegation Tests ─────────────────────────────────────────────────

    #[test]
    fn delegate_voting_power() {
        let ctx = setup();
        let delegator = Address::generate(&ctx.env);
        let delegatee = Address::generate(&ctx.env);
        ctx.token.mint(&delegator, &5_000);
        ctx.gov.delegate(&delegator, &delegatee, &5_000);
        let d = ctx.gov.get_delegation_info(&delegator).unwrap();
        assert_eq!(d.delegator, delegator);
        assert_eq!(d.delegatee, delegatee);
        assert_eq!(d.amount, 5_000);
    }

    #[test]
    #[should_panic(expected = "16")]
    fn cannot_delegate_to_self() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &1_000);
        ctx.gov.delegate(&user, &user, &1_000);
    }

    #[test]
    #[should_panic(expected = "17")]
    fn cannot_delegate_twice() {
        let ctx = setup();
        let delegator = Address::generate(&ctx.env);
        let d1 = Address::generate(&ctx.env);
        let d2 = Address::generate(&ctx.env);
        ctx.token.mint(&delegator, &10_000);
        ctx.gov.delegate(&delegator, &d1, &5_000);
        ctx.gov.delegate(&delegator, &d2, &5_000);
    }

    #[test]
    fn revoke_delegation() {
        let ctx = setup();
        let delegator = Address::generate(&ctx.env);
        let delegatee = Address::generate(&ctx.env);
        ctx.token.mint(&delegator, &5_000);
        ctx.gov.delegate(&delegator, &delegatee, &5_000);
        assert!(ctx.gov.get_delegation_info(&delegator).is_some());

        ctx.gov.revoke_delegation(&delegator);
        assert!(ctx.gov.get_delegation_info(&delegator).is_none());
    }

    // ── veToken Tests ────────────────────────────────────────────────────

    #[test]
    fn lock_tokens_for_ve() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &10_000);
        let dur = 365 * 24 * 60 * 60;
        ctx.gov.lock_tokens(&user, &10_000, &dur);
        let lock = ctx.gov.get_ve_lock(&user).unwrap();
        assert_eq!(lock.amount, 10_000);
        assert_eq!(lock.lock_end, 1000 + dur);
        assert_eq!(lock.created_at, 1000);
    }

    #[test]
    #[should_panic(expected = "23")]
    fn cannot_lock_twice() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &20_000);
        let dur = 365 * 24 * 60 * 60;
        ctx.gov.lock_tokens(&user, &10_000, &dur);
        ctx.gov.lock_tokens(&user, &10_000, &dur);
    }

    #[test]
    #[should_panic(expected = "26")]
    fn lock_zero_tokens_fails() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &1_000);
        ctx.gov.lock_tokens(&user, &0, &(365 * 24 * 60 * 60));
    }

    #[test]
    #[should_panic(expected = "27")]
    fn lock_invalid_duration_fails() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &1_000);
        ctx.gov.lock_tokens(&user, &1_000, &(24 * 60 * 60));
    }

    #[test]
    #[should_panic(expected = "27")]
    fn lock_excessive_duration_fails() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &1_000);
        ctx.gov
            .lock_tokens(&user, &1_000, &(5 * 365 * 24 * 60 * 60));
    }

    #[test]
    fn withdraw_after_lock_expires() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &10_000);
        let dur = MIN_LOCK_DURATION;
        ctx.gov.lock_tokens(&user, &10_000, &dur);
        ctx.env.ledger().set_timestamp(1000 + dur);
        ctx.gov.withdraw_tokens(&user);
        assert!(ctx.gov.get_ve_lock(&user).is_none());
    }

    #[test]
    #[should_panic(expected = "24")]
    fn withdraw_before_expiry_fails() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &10_000);
        let dur = 365 * 24 * 60 * 60;
        ctx.gov.lock_tokens(&user, &10_000, &dur);
        ctx.env.ledger().set_timestamp(1000 + 100);
        ctx.gov.withdraw_tokens(&user);
    }

    #[test]
    #[should_panic(expected = "25")]
    fn withdraw_no_lock_fails() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.gov.withdraw_tokens(&user);
    }

    #[test]
    fn ve_token_boosts_voting_power() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &10_000);
        assert_eq!(ctx.gov.get_voting_power_view(&user), 10_000);
        let dur = 365 * 24 * 60 * 60;
        ctx.gov.lock_tokens(&user, &10_000, &dur);
        assert_eq!(ctx.gov.get_voting_power_view(&user), 20_000);
        ctx.env.ledger().set_timestamp(1000 + dur / 2);
        let half = ctx.gov.get_voting_power_view(&user);
        assert!(half > 10_000);
        assert!(half < 20_000);
        ctx.env.ledger().set_timestamp(1000 + dur);
        assert_eq!(ctx.gov.get_voting_power_view(&user), 10_000);
    }

    #[test]
    fn ve_token_enables_proposal_creation() {
        let ctx = setup();
        let user = Address::generate(&ctx.env);
        ctx.token.mint(&user, &100);
        let dur = MAX_LOCK_DURATION;
        ctx.gov.lock_tokens(&user, &100, &dur);
        let (d, t, f, c, a) = empty_args(&ctx);
        let pid = ctx.gov.propose(&user, &d, &t, &f, &c, &a);
        assert_eq!(pid, 0);
    }

    // ── Admin Tests ──────────────────────────────────────────────────────

    #[test]
    fn update_parameters() {
        let ctx = setup();
        ctx.gov.set_voting_params(
            &ctx.admin,
            &None,
            &Some(200u64),
            &None,
            &Some(2500i128),
            &None,
        );
        let s = ctx.gov.get_settings();
        assert_eq!(s.voting_delay, 1);
        assert_eq!(s.voting_period, 200);
        assert_eq!(s.quorum, 2500);
        assert_eq!(s.approval_threshold, 5100);
    }

    #[test]
    #[should_panic(expected = "2")]
    fn unauthorized_param_update_fails() {
        let ctx = setup();
        let rando = Address::generate(&ctx.env);
        ctx.gov
            .set_voting_params(&rando, &None, &None, &None, &None, &Some(9900i128));
    }

    // ── Action Execution Tests ───────────────────────────────────────────

    #[test]
    fn execute_triggers_token_transfer() {
        let ctx = setup();
        let voter = Address::generate(&ctx.env);
        let recipient = Address::generate(&ctx.env);

        // Give governance contract some tokens (it needs to be able to transfer)
        ctx.token.mint(&voter, &600_000);
        // Fund the governance contract itself
        let gov_addr = ctx.gov.address.clone();
        ctx.token.mint(&gov_addr, &50_000);

        // Create proposal with a Transfer action
        let mut actions = Vec::new(&ctx.env);
        actions.push_back(ProposalAction::Transfer(
            ctx.token_id.clone(),
            recipient.clone(),
            10_000,
        ));
        let desc = String::from_str(&ctx.env, "Transfer tokens");
        let targets = Vec::new(&ctx.env);
        let functions = Vec::new(&ctx.env);
        let calldatas = Vec::new(&ctx.env);
        let pid = ctx
            .gov
            .propose(&voter, &desc, &targets, &functions, &calldatas, &actions);

        // Vote to pass
        ctx.env.ledger().set_sequence_number(2);
        ctx.gov.cast_vote(&voter, &pid, &VoteType::For);

        // Queue and execute
        ctx.env.ledger().set_sequence_number(103);
        ctx.gov.queue(&ctx.admin, &pid);
        ctx.env.ledger().set_timestamp(87400);
        ctx.gov.execute(&ctx.admin, &pid);

        // Verify tokens were transferred
        assert_eq!(ctx.token.balance(&recipient), 10_000);
        assert_eq!(ctx.token.balance(&gov_addr), 40_000);
    }

    // ── Delegation Voting Power Tests ────────────────────────────────────

    #[test]
    fn delegation_adds_to_voting_power() {
        let ctx = setup();
        let delegator = Address::generate(&ctx.env);
        let delegatee = Address::generate(&ctx.env);
        ctx.token.mint(&delegator, &5_000);
        ctx.token.mint(&delegatee, &3_000);

        // Before delegation: delegatee has 3000
        assert_eq!(ctx.gov.get_voting_power_view(&delegatee), 3_000);

        // Delegate 5000 from delegator to delegatee
        ctx.gov.delegate(&delegator, &delegatee, &5_000);

        // After delegation: delegatee has 3000 (own) + 5000 (delegated) = 8000
        assert_eq!(ctx.gov.get_voting_power_view(&delegatee), 8_000);
    }

    #[test]
    fn delegation_revocation_removes_voting_power() {
        let ctx = setup();
        let delegator = Address::generate(&ctx.env);
        let delegatee = Address::generate(&ctx.env);
        ctx.token.mint(&delegator, &5_000);
        ctx.token.mint(&delegatee, &3_000);

        ctx.gov.delegate(&delegator, &delegatee, &5_000);
        assert_eq!(ctx.gov.get_voting_power_view(&delegatee), 8_000);

        ctx.gov.revoke_delegation(&delegator);
        assert_eq!(ctx.gov.get_voting_power_view(&delegatee), 3_000);
    }

    #[test]
    fn delegation_enables_proposal_creation() {
        let ctx = setup();
        let delegator = Address::generate(&ctx.env);
        let delegatee = Address::generate(&ctx.env);
        // Delegatee has only 50 tokens (< threshold 100)
        ctx.token.mint(&delegatee, &50);
        // Delegator has 100 tokens
        ctx.token.mint(&delegator, &100);

        // Before delegation, delegatee can't propose (50 < 100)
        let (d, t, f, c, a) = empty_args(&ctx);
        // This would fail if we tried, but let's skip the panic test and verify after delegation

        // Delegate 100 tokens to delegatee
        ctx.gov.delegate(&delegator, &delegatee, &100);

        // Now delegatee has 50 (own) + 100 (delegated) = 150 >= 100 threshold
        assert_eq!(ctx.gov.get_voting_power_view(&delegatee), 150);
        let pid = ctx.gov.propose(&delegatee, &d, &t, &f, &c, &a);
        assert_eq!(pid, 0);
    }
}
