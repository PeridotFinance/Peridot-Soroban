#![no_std]

#[cfg(all(feature = "lp-receipt-prototype", target_arch = "wasm32"))]
compile_error!("lp-receipt-prototype selects a native test fixture, not a deployable release");

mod constants;
mod contract;
mod events;
mod math;
mod oracle;
mod pool;
mod storage;

pub use contract::{AquariusLpVault, AquariusLpVaultClient};

// Deliberately unavailable in deployable binaries until the complete receipt
// coordinator, migration and legacy-harvest exclusion are implemented.
#[cfg(all(
    feature = "hybrid-rewards",
    target_arch = "wasm32",
    not(feature = "hybrid-validation")
))]
compile_error!("hybrid-rewards is native integration work, not a deployable release");
// This opt-in permits compiled local integration WITHOUT authorizing Mainnet.
// Also called by every bridge operation, so upgrading a legacy instance cannot
// bypass the constructor/initialize restriction.
#[cfg(feature = "hybrid-validation")]
pub(crate) fn require_validation_network(env: &soroban_sdk::Env) {
    let public = env.crypto().sha256(&soroban_sdk::Bytes::from_slice(
        env,
        b"Public Global Stellar Network ; September 2015",
    ));
    assert_ne!(
        env.ledger().network_id(),
        public.to_bytes(),
        "LP release gates: Mainnet hybrid validation disabled"
    );
}
#[cfg(any(test, feature = "hybrid-rewards"))]
mod reward_bridge;
#[cfg(test)]
mod reward_bridge_test;
#[cfg(test)]
mod reward_lending_test;

#[cfg(test)]
mod test;
