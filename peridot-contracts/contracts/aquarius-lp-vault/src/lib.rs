#![no_std]

#[cfg(all(feature = "lp-receipt-prototype", target_arch = "wasm32"))]
compile_error!("lp-receipt-prototype selects a native test fixture, not a deployable release");

mod constants;
mod contract;
mod events;
mod math;
mod oracle;
mod pool;
mod storage;

pub use contract::{AquariusLpVault, AquariusLpVaultClient};

// Deliberately unavailable in deployable binaries until the complete receipt
// coordinator, migration and legacy-harvest exclusion are implemented.
#[cfg(all(feature = "hybrid-rewards", target_arch = "wasm32"))]
compile_error!("hybrid-rewards is native integration work, not a deployable release");
#[cfg(any(test, feature = "hybrid-rewards"))]
mod reward_bridge;
#[cfg(test)]
mod reward_bridge_test;

#[cfg(test)]
mod test;
