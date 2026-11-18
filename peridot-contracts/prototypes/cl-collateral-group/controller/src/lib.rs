#![no_std]
// CL collateral group controller PROTOTYPE (18 Nov 2026): copy of
// contracts/simple-peridottroller/src at build commit 4c98bdd3 plus the patches
// applied by apply-patches.py (see research/cl-collateral-group-20261118/DESIGN.md §7). Measurement prototype only; NOT a release candidate.
#[cfg(feature = "lp-zero-peri")]
compile_error!("lp-zero-peri is reserved for the lp-peridottroller source consumer");

mod constants;
mod contract;
mod events;
pub(crate) mod reflector;
mod storage;

pub use constants::*;
pub use contract::*;
pub use events::*;
pub use storage::*;
