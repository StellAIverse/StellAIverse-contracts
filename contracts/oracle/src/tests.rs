use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{vec, Address, Env, String as SorobanString, Symbol, TryFromVal, Vec};

use crate::contract::OracleContractClient;
use crate::errors::OracleError;
use crate::types::{AggregatedPrice, SubscriptionTier};
use crate::OracleContract;

const ETH: &str = "ETHUSD";
const BTC: &str = "BTCUSD";
const WEATHER: &str = "WEATHER";

/// Deployed contract plus the addresses most scenarios need.
struct Fixture {
    env: Env,
    client: OracleContractClient<'static>,
    admin: Address,
    /// A caller holding an unlimited ETH subscription, so reads never trip
    /// any quota unless a test is explicitly about rate limiting.
    reader: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    let id = env.register(OracleContract, ());
    let client = OracleContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let reader = Address::generate(&env);

    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000;
    });

    client.initialize(&admin, &treasury);
    client.grant_subscription(
        &admin,
        &reader,
        &Symbol::new(&env, ETH),
        &SubscriptionTier::Unlimited,
        &(3600 * 24 * 365),
    );

    Fixture {
        env,
        client,
        admin,
        reader,
    }
}

impl Fixture {
    fn symbol(&self, name: &str) -> Symbol {
        Symbol::new(&self.env, name)
    }

    fn eth_feed(&self) -> Symbol {
        self.symbol(ETH)
    }

    fn register_provider(&self) -> Address {
        let provider = Address::generate(&self.env);
        self.client.register_provider(
            &self.admin,
            &provider,
            &crate::types::OracleProviderType::Chainlink,
            &false,
            &1_000,
            &500,
        );
        provider
    }

    /// Two registered providers behind the standard ETH feed.
    fn feed_with_max_change(&self, max_change_bps: u32) -> (Address, Address) {
        let a = self.register_provider();
        let b = self.register_provider();
        let providers = vec![&self.env, a.clone(), b.clone()];
        self.create_price_feed(&self.eth_feed(), &providers, max_change_bps, true);
        (a, b)
    }

    /// Same, but with an effectively unlimited change ceiling so ordinary
    /// test prices never trip the breaker incidentally; breaker tests pass
    /// their own tighter one instead.
    fn standard_feed(&self) -> (Address, Address) {
        self.feed_with_max_change(u32::MAX)
    }

    fn create_price_feed(
        &self,
        feed_id: &Symbol,
        providers: &Vec<Address>,
        max_change_bps: u32,
        circuit_breaker: bool,
    ) {
        self.client.create_price_feed(
            &self.admin,
            feed_id,
            &SorobanString::from_str(&self.env, "TESTASSET"),
            &SorobanString::from_str(&self.env, "test feed description"),
            &8,
            &1,       // min_update_interval (seconds)
            &100_000, // max_staleness (seconds)
            &max_change_bps,
            providers,
            &circuit_breaker,
        );
    }

    fn submit(&self, provider: &Address, price: i128) {
        self.client.submit_price(provider, &self.eth_feed(), &price);
    }

    fn advance(&self, seconds: u64) {
        self.env.ledger().with_mut(|li| {
            li.timestamp += seconds;
        });
    }

    fn aggregated(&self) -> AggregatedPrice {
        self.client
            .get_aggregated_price(&self.reader, &self.eth_feed())
    }
}

/// The failure shape produced by the generated `try_*` client methods in
/// soroban-sdk 22: contract errors surface as `Error::from_contract_error`
/// inside `Err(Ok(..))`. The success/wrap type is inferred per call site,
/// since it differs between structured and primitive return values.
fn expect_err<T>(
    code: OracleError,
) -> Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>> {
    Err(Ok(soroban_sdk::Error::from_contract_error(code as u32)))
}

/// Whether an event whose first topic is `name` was emitted by the latest
/// invocation. The event buffer only covers the most recent call, so
/// assertions run immediately after the emitting call.
fn has_event(f: &Fixture, name: &str) -> bool {
    let needle = Symbol::new(&f.env, name);
    f.env.events().all().iter().any(|(_cid, topics, _data)| {
        !topics.is_empty()
            && Symbol::try_from_val(&f.env, &topics.get(0).unwrap()) == Ok(needle.clone())
    })
}

// ---------------------------------------------------------------------------
// Initialization and administration
// ---------------------------------------------------------------------------

#[test]
fn double_initialization_is_rejected() {
    let f = setup();

    assert_eq!(
        f.client
            .try_initialize(&f.admin, &Address::generate(&f.env)),
        expect_err(OracleError::AlreadyInitialized)
    );
}

#[test]
fn provider_registration_validates_stake_and_uniqueness() {
    let f = setup();

    let poor = Address::generate(&f.env);
    assert_eq!(
        f.client.try_register_provider(
            &f.admin,
            &poor,
            &crate::types::OracleProviderType::Chainlink,
            &false,
            &100,
            &500,
        ),
        expect_err(OracleError::InsufficientStake)
    );

    let provider = f.register_provider();
    assert_eq!(
        f.client.try_register_provider(
            &f.admin,
            &provider,
            &crate::types::OracleProviderType::Pyth,
            &true,
            &2_000,
            &500,
        ),
        expect_err(OracleError::ProviderAlreadyExists)
    );
}

#[test]
fn feed_creation_requires_two_providers_and_uniqueness() {
    let f = setup();
    let a = f.register_provider();

    let one_provider = vec![&f.env, a.clone()];
    assert_eq!(
        f.client.try_create_price_feed(
            &f.admin,
            &f.eth_feed(),
            &SorobanString::from_str(&f.env, "ETHUSD"),
            &SorobanString::from_str(&f.env, "d"),
            &8,
            &1,
            &100_000,
            &u32::MAX,
            &one_provider,
            &true,
        ),
        expect_err(OracleError::InvalidInput)
    );

    let b = f.register_provider();
    let two = vec![&f.env, a, b];
    f.create_price_feed(&f.eth_feed(), &two, u32::MAX, true);

    // A second feed under the same identifier is refused whatever its list.
    assert_eq!(
        f.client.try_create_price_feed(
            &f.admin,
            &f.eth_feed(),
            &SorobanString::from_str(&f.env, "ETHUSD"),
            &SorobanString::from_str(&f.env, "d"),
            &8,
            &1,
            &100_000,
            &u32::MAX,
            &two,
            &true,
        ),
        expect_err(OracleError::FeedAlreadyExists)
    );
}

// ---------------------------------------------------------------------------
// Price submission and aggregation
// ---------------------------------------------------------------------------

#[test]
fn prices_aggregate_by_median_and_emit_update_events() {
    let f = setup();
    let (a, b) = f.standard_feed();

    f.submit(&a, 200_000);
    assert!(has_event(&f, "price_updated"));

    f.advance(2);
    f.submit(&b, 201_000);

    let agg = f.aggregated();
    assert_eq!(agg.price, 200_500); // mean of the two middle values
    assert_eq!(agg.sources_used, 2);
    assert_eq!(agg.min_price, 200_000);
    assert_eq!(agg.max_price, 201_000);
    assert!(agg.is_fresh);
}

#[test]
fn submissions_from_outsiders_are_rejected() {
    let f = setup();
    let (a, _b) = f.standard_feed();
    let stranger = Address::generate(&f.env);

    // Registered provider, but not on this feed's list.
    assert_eq!(
        f.client.try_submit_price(&stranger, &f.eth_feed(), &100),
        expect_err(OracleError::Unauthorized)
    );

    // On a list but never registered as a provider at all.
    let ghost = Address::generate(&f.env);
    let btc = f.symbol(BTC);
    let providers = vec![&f.env, a, ghost.clone()];
    f.create_price_feed(&btc, &providers, u32::MAX, true);
    assert_eq!(
        f.client.try_submit_price(&ghost, &btc, &100),
        expect_err(OracleError::ProviderNotFound)
    );
}

#[test]
fn non_positive_prices_are_rejected() {
    let f = setup();
    let (a, _b) = f.standard_feed();

    assert_eq!(
        f.client.try_submit_price(&a, &f.eth_feed(), &0),
        expect_err(OracleError::InvalidPrice)
    );
    assert_eq!(
        f.client.try_submit_price(&a, &f.eth_feed(), &-5),
        expect_err(OracleError::InvalidPrice)
    );
}

#[test]
fn updates_respect_the_minimum_interval() {
    let f = setup();
    let (a, b) = f.standard_feed();

    f.submit(&a, 100);

    // Same second: even another authorized provider must wait — the
    // interval protects the feed's history from burst rewrites.
    assert_eq!(
        f.client.try_submit_price(&b, &f.eth_feed(), &101),
        expect_err(OracleError::UpdateTooEarly)
    );

    f.advance(2);
    f.submit(&b, 101);
}

#[test]
fn stale_feeds_refuse_reads_while_freshness_reports_honestly() {
    let f = setup();

    // Three sources: one goes silent long enough to age out, two keep
    // reporting, so aggregation ends up with a stale/fresh mixture.
    let a = f.register_provider();
    let b = f.register_provider();
    let c = f.register_provider();
    let providers = vec![&f.env, a.clone(), b.clone(), c.clone()];
    f.create_price_feed(&f.eth_feed(), &providers, u32::MAX, true);

    f.client.submit_price(&c, &f.eth_feed(), &90);
    f.advance(100_002);
    f.client.submit_price(&a, &f.eth_feed(), &100);
    f.advance(2);
    f.client.submit_price(&b, &f.eth_feed(), &102);

    // One stale plus two fresh entries: the read refuses rather than
    // silently averaging over a partially outdated picture. The plain
    // freshness flag tracks the newest entry, so it still reads true here.
    assert_eq!(
        f.client.try_get_aggregated_price(&f.reader, &f.eth_feed()),
        expect_err(OracleError::StalePrice)
    );
    assert!(f.client.is_data_fresh(&f.eth_feed()));

    // Once every entry is past max_staleness there is literally nothing
    // left to aggregate, which surfaces as an empty-source failure.
    f.advance(100_001);
    assert_eq!(
        f.client.try_get_aggregated_price(&f.reader, &f.eth_feed()),
        expect_err(OracleError::NotEnoughSources)
    );
    assert!(!f.client.is_data_fresh(&f.eth_feed()));
}

#[test]
fn outlier_prices_do_not_drag_the_median() {
    let f = setup();
    let a = f.register_provider();
    let b = f.register_provider();
    let c = f.register_provider();
    let d = f.register_provider();
    let e = f.register_provider();
    let providers = vec![
        &f.env,
        a.clone(),
        b.clone(),
        c.clone(),
        d.clone(),
        e.clone(),
    ];

    // Outlier filtering needs volume, and the huge jump to the bad value
    // must not trip the breaker, so this feed runs without both guards on
    // the change ceiling.
    f.create_price_feed(&f.eth_feed(), &providers, u32::MAX, false);

    f.submit(&a, 100);
    f.advance(2);
    f.submit(&b, 101);
    f.advance(2);
    f.submit(&c, 102);
    f.advance(2);
    f.submit(&d, 103);
    f.advance(2);
    f.submit(&e, 100_000);

    let agg = f.aggregated();
    assert_eq!(agg.sources_used, 4); // the 100_000 entry was filtered
    assert_eq!(agg.price, 101); // median of [100, 101, 102, 103]
}

// ---------------------------------------------------------------------------
// Circuit breaker
// ---------------------------------------------------------------------------

#[test]
fn price_anomalies_trip_the_circuit_breaker() {
    let f = setup();
    let (a, b) = f.feed_with_max_change(100); // 1% ceiling

    f.submit(&a, 1_000);
    f.advance(2);
    f.submit(&b, 1_200); // +20%: far past the ceiling

    let state = f
        .client
        .get_circuit_breaker_state(&f.eth_feed())
        .expect("breaker state should exist");
    assert!(state.triggered);
    assert_eq!(state.price_change_bps, Some(2000));

    // While tripped, both directions are halted...
    assert_eq!(
        f.client.try_get_aggregated_price(&f.reader, &f.eth_feed()),
        expect_err(OracleError::CircuitBreakerTriggered)
    );
    f.advance(3);
    assert_eq!(
        f.client.try_submit_price(&a, &f.eth_feed(), &1_201),
        expect_err(OracleError::CircuitBreakerTriggered)
    );
}

#[test]
fn the_circuit_breaker_auto_resumes_after_cooldown() {
    let f = setup();
    let (a, b) = f.feed_with_max_change(100);

    f.submit(&a, 1_000);
    f.advance(2);
    f.submit(&b, 1_200); // trips the breaker; cooldown 1h

    f.advance(3_601); // cooldown elapsed

    // Reads resume without manual intervention. Both entries survived the
    // trip, so aggregation sees two sources around a 1100 midpoint.
    let agg = f.aggregated();
    assert_eq!(agg.sources_used, 2);
    assert_eq!(agg.price, 1_100);

    // ...and so do updates within the ceiling again.
    f.advance(2);
    f.submit(&a, 1_210); // ~+0.83%
}

#[test]
fn admins_can_reset_a_tripped_breaker_early() {
    let f = setup();
    let (a, b) = f.feed_with_max_change(100);

    f.submit(&a, 1_000);
    f.advance(2);
    f.submit(&b, 1_500); // +50%

    f.client.reset_circuit_breaker(&f.admin, &f.eth_feed());

    // Would have panicked with CircuitBreakerTriggered before the reset.
    let agg = f.aggregated();
    assert_eq!(agg.sources_used, 2);
    assert_eq!(agg.price, 1_250);
}

// ---------------------------------------------------------------------------
// Fallback oracle
// ---------------------------------------------------------------------------

#[test]
fn fallback_providers_activate_when_primaries_go_silent() {
    let f = setup();
    let (_a, _b) = f.standard_feed();

    // A third registered provider joins only as fallback.
    let c = f.register_provider();
    f.client.add_fallback_provider(&f.admin, &f.eth_feed(), &c);

    // The primaries never report; the fallback does. Aggregation relaxes
    // its usual two-source floor while running on fallbacks.
    f.submit(&c, 777);

    let agg = f.aggregated();
    assert_eq!(agg.price, 777);
    assert_eq!(agg.sources_used, 1);
}

#[test]
fn feeds_without_any_source_report_it() {
    let f = setup();
    f.standard_feed();

    // Nobody has submitted anything yet.
    assert_eq!(
        f.client.try_get_aggregated_price(&f.reader, &f.eth_feed()),
        expect_err(OracleError::NotEnoughSources)
    );
}

// ---------------------------------------------------------------------------
// TWAP and batch retrieval
// ---------------------------------------------------------------------------

#[test]
fn twap_weights_samples_by_their_lifespan() {
    let f = setup();
    let (a, b) = f.standard_feed();

    // t=1_000_000: 100 becomes known and stays newest for 100 seconds.
    f.submit(&a, 100);
    f.advance(100);
    // t=1_000_100: 200 takes over for 100 seconds.
    f.submit(&b, 200);
    f.advance(100);
    // t=1_000_200: 300 arrives exactly now, so it carries no weight yet.
    f.submit(&a, 300);

    // (100*100 + 200*100 + 300*0) / 200
    assert_eq!(f.client.get_twap(&f.reader, &f.eth_feed(), &600), 150);

    // After 50 more seconds the 300 sample has earned some weight.
    f.advance(50);
    // (100*100 + 200*100 + 300*50) / 250
    assert_eq!(f.client.get_twap(&f.reader, &f.eth_feed(), &600), 180);

    // A window that excludes everything fails honestly.
    f.advance(60_000);
    assert_eq!(
        f.client.try_get_twap(&f.reader, &f.eth_feed(), &600),
        expect_err(OracleError::NotEnoughSources)
    );
}

#[test]
fn twap_rejects_nonsensical_windows() {
    let f = setup();
    f.standard_feed();

    assert_eq!(
        f.client.try_get_twap(&f.reader, &f.eth_feed(), &30),
        expect_err(OracleError::InvalidInput)
    );
}

#[test]
fn batch_reads_return_every_feed_in_one_call() {
    let f = setup();
    let (a, b) = f.standard_feed();

    // A second, independent feed sharing the same providers.
    let btc = f.symbol(BTC);
    let providers = vec![&f.env, a.clone(), b.clone()];
    f.create_price_feed(&btc, &providers, u32::MAX, true);

    f.client.submit_price(&a, &f.eth_feed(), &2_000);
    f.client.submit_price(&b, &btc, &60_000);

    let unknown = f.symbol("NOPE");
    let result = f
        .client
        .get_latest_prices_batch(&f.reader, &vec![&f.env, f.eth_feed(), btc, unknown]);

    assert_eq!(result.len(), 3);
    assert_eq!(result.get(0).unwrap().unwrap().price, 2_000);
    assert_eq!(result.get(1).unwrap().unwrap().price, 60_000);
    assert!(result.get(2).unwrap().is_none());
}

#[test]
fn multiple_feeds_track_state_independently() {
    let f = setup();
    let (a, b) = f.standard_feed();

    let btc = f.symbol(BTC);
    let providers = vec![&f.env, a.clone(), b.clone()];
    f.create_price_feed(&btc, &providers, u32::MAX, true);

    f.client.submit_price(&a, &f.eth_feed(), &2_000);
    f.client.submit_price(&b, &btc, &60_000);

    let eth_agg = f.aggregated();
    assert_eq!(eth_agg.price, 2_000);

    let btc_agg: AggregatedPrice = f.client.get_aggregated_price(&f.reader, &btc);
    assert_eq!(btc_agg.price, 60_000);
}

// ---------------------------------------------------------------------------
// Rate limiting and subscriptions
// ---------------------------------------------------------------------------

#[test]
fn guests_are_capped_at_five_queries_per_day() {
    let f = setup();
    let (a, b) = f.standard_feed();
    f.submit(&a, 100);
    f.advance(2);
    f.submit(&b, 102);

    let guest = Address::generate(&f.env);
    for _ in 0..5 {
        f.client.get_aggregated_price(&guest, &f.eth_feed());
    }

    assert_eq!(
        f.client.try_get_aggregated_price(&guest, &f.eth_feed()),
        expect_err(OracleError::RateLimitExceeded)
    );
}

#[test]
fn unlimited_subscriptions_are_not_throttled_per_query() {
    let f = setup();
    let (a, b) = f.standard_feed();
    f.submit(&a, 100);
    f.advance(2);
    f.submit(&b, 102);

    // Far beyond the guest cap of five.
    for _ in 0..20 {
        let agg = f.aggregated();
        assert_eq!(agg.price, 101);
    }
}

#[test]
fn expired_subscriptions_fail_closed() {
    let f = setup();
    let (a, b) = f.standard_feed();
    f.submit(&a, 100);
    f.advance(2);
    f.submit(&b, 102);

    let user = Address::generate(&f.env);
    f.client.grant_subscription(
        &f.admin,
        &user,
        &f.eth_feed(),
        &SubscriptionTier::Basic,
        &10,
    );

    f.advance(11); // subscription expired

    assert_eq!(
        f.client.try_get_aggregated_price(&user, &f.eth_feed()),
        expect_err(OracleError::SubscriptionExpired)
    );
}

// ---------------------------------------------------------------------------
// Custom (non-price) feeds
// ---------------------------------------------------------------------------

#[test]
fn custom_feeds_validate_access_and_staleness() {
    let f = setup();
    let p = f.register_provider();
    let weather = f.symbol(WEATHER);
    let providers = vec![&f.env, p.clone()];

    f.client.create_custom_feed(
        &f.admin,
        &weather,
        &SorobanString::from_str(&f.env, "local weather"),
        &SorobanString::from_str(&f.env, "temperature"),
        &10, // max_staleness seconds
        &providers,
    );

    // Duplicate identifiers are refused here too.
    assert_eq!(
        f.client.try_create_custom_feed(
            &f.admin,
            &weather,
            &SorobanString::from_str(&f.env, "again"),
            &SorobanString::from_str(&f.env, "t"),
            &10,
            &providers,
        ),
        expect_err(OracleError::FeedAlreadyExists)
    );

    // Non-authorized writers are refused.
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.client.try_submit_custom_data(
            &stranger,
            &weather,
            &SorobanString::from_str(&f.env, "21c"),
        ),
        expect_err(OracleError::Unauthorized)
    );

    // Authorized write lands and reads back.
    f.client
        .submit_custom_data(&p, &weather, &SorobanString::from_str(&f.env, "21c"));
    assert!(has_event(&f, "custom_data_updated"));
    let latest = f
        .client
        .get_custom_data(&f.reader, &weather)
        .expect("entry should exist");
    assert_eq!(latest.data, SorobanString::from_str(&f.env, "21c"));

    // Old data goes stale like price data does.
    f.advance(11);
    assert_eq!(
        f.client.try_get_custom_data(&f.reader, &weather),
        expect_err(OracleError::StalePrice)
    );
}

// ---------------------------------------------------------------------------
// Incentives and history
// ---------------------------------------------------------------------------

#[test]
fn providers_earn_rewards_they_can_withdraw() {
    let f = setup();
    let (a, b) = f.standard_feed();

    f.submit(&a, 100);
    f.advance(2);
    f.submit(&b, 102);

    let balance_a = f.client.get_provider_balance(&a);
    assert!(balance_a > 0, "successful updates must earn rewards");

    let withdrawn = f.client.withdraw_incentives(&a);
    assert_eq!(withdrawn, balance_a);
    assert_eq!(f.client.get_provider_balance(&a), 0);

    assert_eq!(
        f.client.try_withdraw_incentives(&a),
        expect_err(OracleError::InsufficientBalance)
    );
}

#[test]
fn historical_reads_respect_the_limit() {
    let f = setup();
    let (a, b) = f.standard_feed();

    f.submit(&a, 100);
    f.advance(2);
    f.submit(&b, 102);
    f.advance(2);
    f.submit(&a, 104);

    let last_two = f.client.get_historical_prices(&f.reader, &f.eth_feed(), &2);
    assert_eq!(last_two.len(), 2);
    assert_eq!(last_two.get(0).unwrap().price, 102);
    assert_eq!(last_two.get(1).unwrap().price, 104);

    // Oversized limits are rejected rather than truncated silently.
    assert_eq!(
        f.client
            .try_get_historical_prices(&f.reader, &f.eth_feed(), &501),
        expect_err(OracleError::InvalidInput)
    );
}
