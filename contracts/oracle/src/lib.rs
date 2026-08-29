#![no_std]
#![allow(clippy::too_many_arguments)]
extern crate alloc;

pub mod circuit_breaker;
pub mod contract;
pub mod errors;
pub mod incentives;
pub mod price_aggregator;
pub mod rate_limiter;
pub mod storage_keys;
pub mod types;

#[cfg(test)]
mod tests;

pub use contract::OracleContract;
