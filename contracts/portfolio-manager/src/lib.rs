#![no_std]

mod contract;
mod errors;
mod storage;
mod templates;
pub mod types;

pub use contract::PortfolioManager;

#[cfg(test)]
#[allow(unused)]
mod test;
