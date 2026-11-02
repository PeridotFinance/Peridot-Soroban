//! Protocol29 native test host for unchanged compiled contract artifacts.
//! This is controlled-state runtime evidence, not a live-state transaction.
#![cfg(test)]
use soroban_sdk::{token, Address, Env, IntoVal, Symbol, Vec};
fn amount(n: u128) -> i128 { n.try_into().expect("amount too large") }

#[path = "../../../contracts/cl-share-pool-facade/src/test.rs"]
mod test;
#[path = "../../../test-support/cl_resource_limits.rs"]
mod cl_resource_limits;
#[path = "../../../contracts/cl-share-pool-facade/src/resource_profile.rs"]
mod resource_profile;

#[test]
fn runtime_integrity_and_captured_cost_models() {
    use soroban_sdk::testutils::Ledger;
    let env = Env::default();
    assert_eq!(soroban_env_host::VERSION.pkg, "29.0.1");
    assert_eq!(soroban_env_host::meta::INTERFACE_VERSION.protocol, 29);
    assert_eq!(env.ledger().get().protocol_version, 29);
    cl_resource_limits::configure(&env);
    println!("RUNTIME_INTEGRITY {:?}", soroban_env_host::VERSION);
}

#[cfg(feature = "transaction-evidence")]
#[path = "transaction_evidence.rs"]
mod transaction_evidence;
