use crate::errors::*;
use crate::types::{AggregatedPrice, HistoricalPrice, PriceEntry};
use alloc::vec as alloc_vec;
use soroban_sdk::{Env, Vec};

pub struct PriceAggregator;

impl PriceAggregator {
    /// Aggregate prices from multiple providers using median aggregation
    pub fn aggregate_prices(
        env: &Env,
        prices: &Vec<PriceEntry>,
        min_sources: u32,
        max_staleness: u64,
    ) -> AggregatedPrice {
        if prices.len() < min_sources {
            not_enough_sources(env);
        }

        // Filter out stale prices
        let current_time = env.ledger().timestamp();
        let mut valid_prices = Vec::new(env);
        let mut is_fresh = true;

        for price in prices.iter() {
            if current_time - price.timestamp <= max_staleness {
                valid_prices.push_back(price.price);
            } else {
                is_fresh = false;
            }
        }

        if valid_prices.len() < min_sources {
            not_enough_sources(env);
        }

        // Sort prices for median calculation. Soroban's host-side Vec has
        // no in-place sort, so the values are sorted in a scratch vector.
        let mut sorted_prices: alloc_vec::Vec<i128> = alloc_vec::Vec::new();
        for price in valid_prices.iter() {
            sorted_prices.push(price);
        }
        sorted_prices.sort();

        // Calculate statistics
        let n = sorted_prices.len();
        let min_price = sorted_prices[0];
        let max_price = sorted_prices[n - 1];

        let median_idx = n / 2;
        let median_price = if n.is_multiple_of(2) {
            // Average of two middle values for even length
            (sorted_prices[median_idx - 1] + sorted_prices[median_idx]) / 2
        } else {
            sorted_prices[median_idx]
        };

        // Get the most recent timestamp from valid prices
        let mut latest_timestamp: u64 = 0;
        for entry in prices.iter() {
            if entry.timestamp > latest_timestamp {
                latest_timestamp = entry.timestamp;
            }
        }

        AggregatedPrice {
            price: median_price,
            timestamp: latest_timestamp,
            sources_used: valid_prices.len(),
            min_price,
            max_price,
            median_price,
            is_fresh,
        }
    }

    /// Calculate weighted average based on provider reputation
    pub fn weighted_aggregate(
        env: &Env,
        prices: &Vec<(PriceEntry, u32)>, // (price entry, reputation score)
        min_sources: u32,
        max_staleness: u64,
    ) -> AggregatedPrice {
        if prices.len() < min_sources {
            not_enough_sources(env);
        }

        let current_time = env.ledger().timestamp();
        let mut total_weight: u128 = 0;
        let mut weighted_sum: i128 = 0;
        let mut valid_count: u32 = 0;
        let mut is_fresh = true;
        let mut min_price = i128::MAX;
        let mut max_price = i128::MIN;
        let mut latest_timestamp: u64 = 0;

        for item in prices.iter() {
            let (entry, reputation) = item;
            if current_time - entry.timestamp <= max_staleness {
                let weight = reputation as u128;
                total_weight += weight;
                weighted_sum += entry.price * (weight as i128);
                valid_count += 1;

                if entry.price < min_price {
                    min_price = entry.price;
                }
                if entry.price > max_price {
                    max_price = entry.price;
                }
                if entry.timestamp > latest_timestamp {
                    latest_timestamp = entry.timestamp;
                }
            } else {
                is_fresh = false;
            }
        }

        if valid_count < min_sources {
            not_enough_sources(env);
        }

        let final_price = weighted_sum / (total_weight as i128);

        AggregatedPrice {
            price: final_price,
            timestamp: latest_timestamp,
            sources_used: valid_count,
            min_price,
            max_price,
            median_price: final_price,
            is_fresh,
        }
    }

    /// Detect outliers using IQR method
    pub fn remove_outliers(env: &Env, prices: Vec<PriceEntry>) -> Vec<PriceEntry> {
        if prices.len() < 4 {
            return prices; // Not enough data to filter outliers
        }

        // Extract prices for IQR calculation
        let mut price_values: alloc_vec::Vec<i128> = alloc_vec::Vec::new();
        for entry in prices.iter() {
            price_values.push(entry.price);
        }
        price_values.sort();

        let len = price_values.len();
        let q1_idx = len / 4;
        let q3_idx = (3 * len) / 4;
        let q1 = price_values[q1_idx];
        let q3 = price_values[q3_idx];
        let iqr = q3 - q1;
        let lower_bound = q1 - (i128::from(150) * iqr) / i128::from(100); // 1.5 * IQR
        let upper_bound = q3 + (i128::from(150) * iqr) / i128::from(100);

        // Filter out outliers
        let mut filtered = Vec::new(env);
        for entry in prices.iter() {
            if entry.price >= lower_bound && entry.price <= upper_bound {
                filtered.push_back(entry);
            }
        }

        filtered
    }

    /// Validate price divergence between providers
    pub fn validate_price_divergence(prices: &Vec<PriceEntry>, max_divergence_bps: u32) -> bool {
        if prices.len() < 2 {
            return true;
        }

        let mut min_price = i128::MAX;
        let mut max_price = i128::MIN;

        for entry in prices.iter() {
            if entry.price < min_price {
                min_price = entry.price;
            }
            if entry.price > max_price {
                max_price = entry.price;
            }
        }

        // Calculate percentage difference
        if min_price == 0 {
            return false;
        }
        let divergence = ((max_price - min_price) * 10000) / min_price;
        divergence <= max_divergence_bps as i128
    }

    /// Time-weighted average price over a trailing window.
    ///
    /// Each sample is weighted by how long it remained the newest price:
    /// from its own timestamp until the next sample arrived, and until the
    /// window's end for the most recent one. Returns `None` when no samples
    /// fall inside the window.
    pub fn calculate_twap(
        env: &Env,
        history: &Vec<HistoricalPrice>,
        window_seconds: u64,
    ) -> Option<i128> {
        let now = env.ledger().timestamp();
        let window_start = now.saturating_sub(window_seconds);

        // Samples inside the window, in chronological order.
        let mut samples: alloc_vec::Vec<(u64, i128)> = alloc_vec::Vec::new();
        for entry in history.iter() {
            if entry.timestamp >= window_start && entry.timestamp <= now {
                samples.push((entry.timestamp, entry.price));
            }
        }

        if samples.is_empty() {
            return None;
        }

        let mut weighted_sum: i128 = 0;
        let mut total_weight: u64 = 0;

        let mut i = 0usize;
        while i < samples.len() {
            let (t_i, p_i) = samples[i];
            let t_next = if i + 1 < samples.len() {
                samples[i + 1].0
            } else {
                now
            };
            let segment = t_next.saturating_sub(t_i);
            weighted_sum += p_i * (segment as i128);
            total_weight += segment;
            i += 1;
        }

        if total_weight == 0 {
            // All samples share one timestamp; the plain last value is the
            // best time-weighted answer available.
            return Some(samples[samples.len() - 1].1);
        }

        Some(weighted_sum / (total_weight as i128))
    }
}
