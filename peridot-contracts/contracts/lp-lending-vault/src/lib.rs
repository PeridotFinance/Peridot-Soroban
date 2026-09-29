#![no_std]
//! Explicit LP lending ABI; shared engine primitives are NOT contract exports.
//! Validation candidate only: Mainnet initialization is blocked pending release.
#[cfg(not(all(feature = "lp-engine", feature = "hybrid-rewards")))]
compile_error!("LP lending requires its private engine and reward coordinator");
#[path = "../../receipt-vault/src/constants.rs"]
mod constants;
#[path = "../../receipt-vault/src/contract.rs"]
mod engine;
mod entrypoints;
#[path = "../../receipt-vault/src/events.rs"]
mod events;
#[path = "../../receipt-vault/src/helpers.rs"]
mod helpers;
#[path = "../../receipt-vault/src/reward_backing.rs"]
mod reward_backing;
#[path = "../../receipt-vault/src/reward_claims.rs"]
mod reward_claims;
#[path = "../../receipt-vault/src/reward_ledger.rs"]
mod reward_ledger;
#[path = "../../receipt-vault/src/reward_lending.rs"]
mod reward_lending;
#[path = "../../receipt-vault/src/reward_settlement.rs"]
mod reward_settlement;
#[path = "../../receipt-vault/src/reward_share_hooks.rs"]
mod reward_share_hooks;
#[path = "../../receipt-vault/src/storage.rs"]
mod storage;
pub(crate) use constants::*;
pub(crate) use engine::ReceiptVault;
pub use entrypoints::{LpLendingVault, LpLendingVaultClient};
pub use reward_settlement::Outcome;
pub use storage::SeizeContext;
pub(crate) use storage::*;

#[cfg(test)]
mod test;
