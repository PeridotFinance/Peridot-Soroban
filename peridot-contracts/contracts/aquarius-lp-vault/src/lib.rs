#![no_std]

mod constants;
mod contract;
mod events;
mod math;
mod oracle;
mod pool;
mod storage;

pub use contract::{AquariusLpVault, AquariusLpVaultClient};

#[cfg(test)]
mod test;
