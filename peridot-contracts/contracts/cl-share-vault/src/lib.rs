#![no_std]
mod accounting;
mod constants;
mod contract;
mod events;
mod funded;
mod math;
mod oracle;
mod pool;
mod rewards;
mod storage;
pub use contract::{ClShareVault, ClShareVaultClient};
// This candidate is local validation only. Mainnet deployment is not authorized.
pub(crate) fn require_validation_network(env: &soroban_sdk::Env) {
    let public = env.crypto().sha256(&soroban_sdk::Bytes::from_slice(
        env,
        b"Public Global Stellar Network ; September 2015",
    ));
    assert_ne!(
        env.ledger().network_id(),
        public.to_bytes(),
        "CL validation: Mainnet disabled"
    );
}
#[cfg(test)]
extern crate std;
#[cfg(test)]
mod test;
