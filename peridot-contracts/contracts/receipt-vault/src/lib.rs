#![no_std]

mod constants;
mod contract;
mod events;
mod helpers;
mod storage;

// Internal-custody building block. Not a callable contract interface or activation.
#[cfg(any(test, feature = "hybrid-rewards"))]
pub mod reward_backing;
#[cfg(any(test, feature = "hybrid-rewards"))]
pub mod reward_claims;
#[cfg(any(test, feature = "hybrid-rewards"))]
pub mod reward_ledger;
#[cfg(any(test, feature = "hybrid-rewards"))]
pub mod reward_lending;
#[cfg(any(test, feature = "hybrid-rewards"))]
pub mod reward_settlement;
#[cfg(any(test, feature = "hybrid-rewards"))]
pub mod reward_share_hooks;
#[cfg(all(feature = "hybrid-rewards", target_arch = "wasm32"))]
compile_error!("hybrid-rewards is native integration work, not a deployable release");

pub use constants::*;
pub use contract::*;
pub use events::*;
pub use helpers::*;
pub use storage::*;

mod test;

// Executable economic prototype only: no storage keys, ABI or deployed behavior.
#[cfg(test)]
mod reward_accounting_model;

#[cfg(test)]
mod reward_backing_test;

#[cfg(test)]
mod lp_lending_test;
