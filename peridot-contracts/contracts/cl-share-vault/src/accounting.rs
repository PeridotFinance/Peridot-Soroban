//! Book idle principal explicitly; unsolicited token transfers are excluded.
use crate::storage::config;
use soroban_sdk::{contracttype, token, Address, Env};
#[contracttype]
#[derive(Clone)]
enum Key {
    Busy,
    Book(Address),
    Excluded(Address),
}
fn raw(e: &Env, t: &Address) -> u128 {
    token::Client::new(e, t)
        .balance(&e.current_contract_address())
        .try_into()
        .unwrap()
}
pub fn balance(e: &Env, t: &Address) -> u128 {
    let available = raw(e, t)
        .checked_sub(
            e.storage()
                .instance()
                .get(&Key::Excluded(t.clone()))
                .unwrap_or(0),
        )
        .expect("principal loss");
    if e.storage().instance().get(&Key::Busy).unwrap_or(false) {
        available
    } else {
        available.min(
            e.storage()
                .instance()
                .get(&Key::Book(t.clone()))
                .unwrap_or(0),
        )
    }
}
pub fn begin(e: &Env) {
    assert!(
        !e.storage().instance().get(&Key::Busy).unwrap_or(false),
        "operation already active"
    );
    let cfg = config(e);
    for t in [cfg.token0, cfg.token1] {
        let held = raw(e, &t);
        let book: u128 = e
            .storage()
            .instance()
            .get(&Key::Book(t.clone()))
            .unwrap_or(0);
        let excluded: u128 = e
            .storage()
            .instance()
            .get(&Key::Excluded(t.clone()))
            .unwrap_or(0);
        assert!(
            held >= book.checked_add(excluded).unwrap(),
            "principal loss"
        );
        e.storage()
            .instance()
            .set(&Key::Excluded(t), &(held - book));
    }
    e.storage().instance().set(&Key::Busy, &true);
}
pub fn end(e: &Env) {
    let cfg = config(e);
    for t in [cfg.token0, cfg.token1] {
        e.storage()
            .instance()
            .set(&Key::Book(t.clone()), &balance(e, &t));
    }
    e.storage().instance().set(&Key::Busy, &false);
}
