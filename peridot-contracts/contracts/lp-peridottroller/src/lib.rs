#![no_std]
//! Separate LP controller artifact, using the exact shared lending/risk engine.
//! Validation-only: fresh Mainnet initialization is intentionally prohibited.
#[cfg(not(feature = "lp-zero-peri"))]
compile_error!("LP controller must be built with immutable zero-PERI policy");

#[path = "../../simple-peridottroller/src/constants.rs"]
mod constants;
#[path = "../../simple-peridottroller/src/contract.rs"]
mod contract;
#[path = "../../simple-peridottroller/src/events.rs"]
mod events;
#[path = "../../simple-peridottroller/src/reflector.rs"]
mod reflector;
#[path = "../../simple-peridottroller/src/storage.rs"]
mod storage;
pub use constants::*;
pub use contract::*;
pub use events::*;
pub use storage::*;

pub(crate) fn require_validation_network(env: &soroban_sdk::Env) {
    let public = env.crypto().sha256(&soroban_sdk::Bytes::from_slice(
        env,
        b"Public Global Stellar Network ; September 2015",
    ));
    assert_ne!(
        env.ledger().network_id(),
        public.to_bytes(),
        "LP release gates: Mainnet initialization disabled"
    );
}
