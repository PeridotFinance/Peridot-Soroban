use soroban_sdk::{contracttype, Address, Env};
use stellar_tokens::fungible::Base as TokenBase;

// Exact existing serialized key type; unused lending keys are never renewed by
// the LP hot path. Reusing keys is NOT proof of safe legacy upgrade compatibility.
pub use receipt_vault::DataKey;

pub(crate) const TTL_LOW: u32 = 500_000;
pub(crate) const TTL_HIGH: u32 = 1_000_000;

#[contracttype]
pub(crate) enum LpKey {
    LpReceiptVersion,
    LpDepositPaused,
}

pub fn ensure_initialized(env: &Env) -> Address {
    // An upgraded generic receipt is deliberately rejected until a reviewed
    // migration proves zero debt, zero collateral use and reconciled liabilities.
    assert_eq!(
        env.storage()
            .instance()
            .get::<_, u32>(&LpKey::LpReceiptVersion),
        Some(1),
        "LP receipt activation required"
    );
    env.storage().instance().extend_ttl(TTL_LOW, TTL_HIGH);
    let p = env.storage().persistent();
    assert_eq!(p.get::<_, bool>(&DataKey::Initialized), Some(true));
    for key in [
        DataKey::Initialized,
        DataKey::UnderlyingToken,
        DataKey::Admin,
        DataKey::ManagedCash,
        DataKey::TotalDeposited,
        DataKey::InitialExchangeRate,
        DataKey::SupplyCap,
        DataKey::IdleCashBufferBps,
        DataKey::BoostedVault,
        DataKey::BoostedUnderlyingCached,
    ] {
        if p.has(&key) {
            p.extend_ttl(&key, TTL_LOW, TTL_HIGH);
        }
    }
    p.get(&DataKey::UnderlyingToken)
        .expect("underlying missing")
}

pub fn total_ptokens_supply(env: &Env) -> u128 {
    TokenBase::total_supply(env)
        .try_into()
        .expect("negative supply")
}

pub fn to_i128(value: u128) -> i128 {
    value.try_into().expect("amount exceeds token range")
}
