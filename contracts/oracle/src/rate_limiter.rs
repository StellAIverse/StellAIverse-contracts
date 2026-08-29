use crate::errors::*;
use crate::storage_keys::*;
use crate::types::{FeedSubscription, QueryRateLimit, SubscriptionTier};
use soroban_sdk::{Address, Env};

pub struct RateLimiter;

impl RateLimiter {
    /// Check and consume a query for a user
    pub fn consume_query(
        env: &Env,
        user: &Address,
        feed_id: &soroban_sdk::Symbol,
        global_window: u64,
        global_max_queries: u32,
    ) {
        // Check subscription first
        let sub_key = get_subscription_key(user, feed_id);
        let current_time = env.ledger().timestamp();

        let subscription: Option<FeedSubscription> = env.storage().instance().get(&sub_key);
        if let Some(mut subscription) = subscription {
            if subscription.expires_at < current_time {
                subscription_expired(env);
            }

            // Apply subscription-based limits
            if subscription.queries_remaining == 0
                && subscription.tier != SubscriptionTier::Unlimited
            {
                rate_limit_exceeded(env);
            }

            if subscription.tier != SubscriptionTier::Unlimited {
                subscription.queries_remaining -= 1;
                env.storage().instance().set(&sub_key, &subscription);
            }

            // Also check global rate limits
            Self::check_global_rate_limit(env, user, global_window, global_max_queries);
        } else {
            // No subscription - check guest rate limits
            Self::check_guest_rate_limit(env, user);
        }
    }

    /// Check global rate limits for all users
    pub fn check_global_rate_limit(
        env: &Env,
        user: &Address,
        window_seconds: u64,
        max_queries: u32,
    ) {
        let rl_key = get_rate_limit_key(user);
        let current_time = env.ledger().timestamp();

        let stored: Option<QueryRateLimit> = env.storage().instance().get(&rl_key);
        let mut rate_limit: QueryRateLimit = stored.unwrap_or(QueryRateLimit {
            window_seconds,
            max_queries,
            queries_used: 0,
            window_start: current_time,
        });

        // Reset window if it has expired
        if current_time - rate_limit.window_start > rate_limit.window_seconds {
            rate_limit.window_start = current_time;
            rate_limit.queries_used = 0;
        }

        if rate_limit.queries_used >= rate_limit.max_queries {
            rate_limit_exceeded(env);
        }

        rate_limit.queries_used += 1;
        env.storage().instance().set(&rl_key, &rate_limit);
    }

    /// Check guest rate limits for unsubscribed users
    fn check_guest_rate_limit(env: &Env, user: &Address) {
        // Guests get very limited access - 5 queries per day
        let daily_window: u64 = 86400; // 24 hours
        let max_guest_queries: u32 = 5;
        Self::check_global_rate_limit(env, user, daily_window, max_guest_queries);
    }

    /// Account for one batch query.
    ///
    /// A batch carries many feed lookups in a single invocation, so it is
    /// charged once against the global per-window cap rather than once per
    /// feed — that is part of what makes batching cheaper than N separate
    /// calls. Subscriptions do not apply; the global cap does.
    pub fn consume_batch_query(env: &Env, user: &Address) {
        Self::check_global_rate_limit(env, user, 3600, 1000);
    }

    /// Reset rate limits for a specific user
    pub fn reset_user_limits(env: &Env, user: &Address) {
        let rl_key = get_rate_limit_key(user);
        let current_time = env.ledger().timestamp();
        let rate_limit = QueryRateLimit {
            window_seconds: 3600,
            max_queries: 1000,
            queries_used: 0,
            window_start: current_time,
        };
        env.storage().instance().set(&rl_key, &rate_limit);
    }

    /// Update rate limit configuration
    pub fn update_rate_limit_config(
        env: &Env,
        user: &Address,
        new_window: u64,
        new_max_queries: u32,
    ) {
        let rl_key = get_rate_limit_key(user);
        let mut rate_limit: QueryRateLimit =
            env.storage()
                .instance()
                .get(&rl_key)
                .unwrap_or_else(|| QueryRateLimit {
                    window_seconds: 3600,
                    max_queries: 100,
                    queries_used: 0,
                    window_start: env.ledger().timestamp(),
                });

        rate_limit.window_seconds = new_window;
        rate_limit.max_queries = new_max_queries;
        env.storage().instance().set(&rl_key, &rate_limit);
    }

    /// Get current rate limit status for a user
    pub fn get_user_rate_limit(env: &Env, user: &Address) -> Option<QueryRateLimit> {
        let rl_key = get_rate_limit_key(user);
        env.storage().instance().get(&rl_key)
    }

    /// Add query allowance to a user (for premium subscriptions)
    pub fn add_queries(env: &Env, user: &Address, additional_queries: u32) {
        let rl_key = get_rate_limit_key(user);
        let stored: Option<QueryRateLimit> = env.storage().instance().get(&rl_key);
        if let Some(mut rate_limit) = stored {
            rate_limit.queries_used = rate_limit.queries_used.saturating_sub(additional_queries);
            env.storage().instance().set(&rl_key, &rate_limit);
        }
    }
}
