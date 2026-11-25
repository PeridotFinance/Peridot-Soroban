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

mod backstop;
mod common;
mod contract;
mod headroom;
mod types;

pub use common::*;
pub use contract::*;
pub use types::*;
