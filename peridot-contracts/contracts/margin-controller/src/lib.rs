#![no_std]
//! Custody-only margin controller (2026-11-24 assessment branch).
//!
//! The margin contract is an ordinary lending account: it deposits, enters markets,
//! borrows, repays and withdraws on the live cash receipts exactly like any user.
//! It never requires, and never calls, the receipts' MarginController hook
//! (`set_margin_controller`, `begin_margin_withdraw`, `borrow_for_margin`,
//! `locked_ptokens_in_market`, `liquidate_for_margin`). Lenders never call into it.
//! Per-position accounting (held pTokens, debt shares) lives here; the live
//! peridottroller health-checks the pooled account on every borrow/withdraw/transfer.

mod contract;
pub use contract::*;

