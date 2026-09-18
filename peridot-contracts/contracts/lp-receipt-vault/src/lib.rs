#![no_std]
//! Native-only supply-only receipt prototype for the three Aquarius markets.
//! No exported contract ABI, deployment artifact, or legacy migration yet.
//! The test coordinator MUST wrap every share mutation with reward checkpoints.

#[cfg(target_arch = "wasm32")]
compile_error!("lp-receipt-vault is a native prototype, not a deployable release");

mod contract;
mod storage;
pub use contract::LpReceiptVault as ReceiptVault;
pub use storage::*;
pub const SCALE_1E6: u128 = 1_000_000;

// Share the existing DEVELOPMENT accounting implementation instead of forking
// its economic rules. Its crate-local imports resolve to this lean receipt core.
pub mod exit_request;
pub mod migration;
#[path = "../../receipt-vault/src/reward_backing.rs"]
pub mod reward_backing;
#[path = "../../receipt-vault/src/reward_claims.rs"]
pub mod reward_claims;
pub mod reward_coordinator;
#[path = "../../receipt-vault/src/reward_ledger.rs"]
pub mod reward_ledger;
#[path = "../../receipt-vault/src/reward_settlement.rs"]
pub mod reward_settlement;

#[cfg(test)]
mod test;

#[cfg(test)]
mod migration_test;
