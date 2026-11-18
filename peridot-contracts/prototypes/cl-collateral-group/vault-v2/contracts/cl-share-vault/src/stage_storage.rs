use soroban_sdk::{contracttype,Address,Env};
// Every key this overlay adds is a StgV1* symbol; none equals a base vault key symbol.
#[contracttype]
#[derive(Clone)]
enum StageStorageKey { StgV1Reserved(Address), StgV1Locked(Address), StgV1Dirty }
pub fn reserved(e:&Env,t:&Address)->u128 {e.storage().instance().get(&StageStorageKey::StgV1Reserved(t.clone())).unwrap_or(0)}
pub fn adjust(e:&Env,t:&Address,add:u128,sub:u128) {
    let n=reserved(e,t).checked_add(add).unwrap().checked_sub(sub).expect("command liability underflow");
    e.storage().instance().set(&StageStorageKey::StgV1Reserved(t.clone()),&n);
}
pub fn locked(e:&Env,a:&Address)->u128 {let k=StageStorageKey::StgV1Locked(a.clone());let n=e.storage().persistent().get(&k).unwrap_or(0);if e.storage().persistent().has(&k){e.storage().persistent().extend_ttl(&k,500_000,1_000_000);}n}
pub fn lock(e:&Env,a:&Address,n:u128){let k=StageStorageKey::StgV1Locked(a.clone());e.storage().persistent().set(&k,&locked(e,a).checked_add(n).unwrap());e.storage().persistent().extend_ttl(&k,500_000,1_000_000);}
pub fn unlock(e:&Env,a:&Address,n:u128){let k=StageStorageKey::StgV1Locked(a.clone());e.storage().persistent().set(&k,&locked(e,a).checked_sub(n).expect("lock underflow"));e.storage().persistent().extend_ttl(&k,500_000,1_000_000);}
// Unlocked claim of `a`: direct shares (legacy, before binding) plus custody shares
// held for `a` as beneficiary (after binding there are no direct holders).
pub fn available(e:&Env,a:&Address)->u128 {crate::storage::shares_of(e,a).checked_add(crate::custody::beneficial(e,a)).unwrap().checked_sub(locked(e,a)).unwrap()}

pub fn composition_dirty(e:&Env)->bool {e.storage().instance().get(&StageStorageKey::StgV1Dirty).unwrap_or(false)}
pub fn set_composition_dirty(e:&Env,dirty:bool) {e.storage().instance().set(&StageStorageKey::StgV1Dirty,&dirty);}
