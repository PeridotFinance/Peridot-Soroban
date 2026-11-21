//! CL collateral GROUP measurement harness (18 Nov 2026). LOCAL host-only, mock_all_auths.
//! No network, no signing, no submission. `common.rs` is the item-S spike harness infrastructure
//! (copied; the spike folder is unchanged) and `group.rs` holds the group stages and tests.
#![cfg(test)]
extern crate std;
include!("common.rs");
include!("group.rs");
include!("real.rs");
include!("fresh.rs");
