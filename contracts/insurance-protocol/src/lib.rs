#![no_std]
#![allow(clippy::too_many_arguments)]
extern crate alloc;

pub mod contract;
pub mod errors;
pub mod storage_keys;
pub mod types;

#[cfg(test)]
mod tests;

pub use contract::InsuranceProtocol;
