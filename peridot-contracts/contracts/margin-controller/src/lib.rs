#![no_std]
//! Pooled margin v2 (custody-only): backstop pool + isolated positions.
//!
//! The margin contract is an ordinary lending account on the live cash receipts. It never
//! requires, and never calls, the receipts' MarginController hook (`set_margin_controller`,
//! `begin_margin_withdraw`, `borrow_for_margin`, `locked_ptokens_in_market`,
//! `liquidate_for_margin`). Lenders never call into it. Per-position accounting lives here;
//! the live peridottroller health-checks the pooled account on every borrow/withdraw/transfer.
//!
//! Modules (WP0 freeze 25 Nov 2026, see research/pooled-margin-20261124/INTERFACE.md):
//! - `types`    errors, keys, structs (frozen)
//! - `common`   live ABIs + shared helpers (integrator)
//! - `contract` constructor, admin, positions: open/close/liquidate/claim (WP2)
//! - `backstop` backstop shares, exits, fees in, bad debt (WP1)
//! - `headroom` pool check, required backstop size, reconcile/Impaired (WP3)

/// Declared at the crate root so every module's `#[contractimpl]` (and tests) can use the
/// generated client/args types (E0616 otherwise).
#[soroban_sdk::contract]
pub struct MarginController;

mod backstop;
mod common;
mod contract;
mod headroom;
#[cfg(test)]
mod test_positions;
mod types;
#[cfg(test)]
mod test_headroom;

#[cfg(test)]
mod test_backstop;

pub use common::*;
pub use contract::*;
pub use types::*;

#[cfg(test)]
mod wp0_smoke {
    use crate::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};
    #[test]
    fn constructs() {
        let e = Env::default();
        let a = Address::generate(&e);
        let id = e.register(
            MarginController,
            (a.clone(), a.clone(), a.clone(), a.clone(), a.clone()),
        );
        let c = MarginControllerClient::new(&e, &id);
        assert!(c.opens_paused());
    }
}
