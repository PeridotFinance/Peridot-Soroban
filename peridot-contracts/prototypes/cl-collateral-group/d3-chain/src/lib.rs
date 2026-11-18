#![cfg_attr(not(test), no_std)]
//! Crate root for native tests; contracts include `d3_chain.rs` directly by #[path].
#[path = "d3_chain.rs"]
mod d3_chain;
pub use d3_chain::*;
