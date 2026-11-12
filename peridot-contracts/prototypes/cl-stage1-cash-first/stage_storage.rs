use soroban_sdk::{contracttype,Address,Env};
#[contracttype]
#[derive(Clone)]
enum StageStorageKey { Reserved(Address), Locked(Address) }
pub fn reserved(e:&Env,t:&Address)->u128 {e.storage().instance().get(&StageStorageKey::Reserved(t.clone())).unwrap_or(0)}
pub fn adjust(e:&Env,t:&Address,add:u128,sub:u128) {
    let n=reserved(e,t).checked_add(add).unwrap().checked_sub(sub).expect("command liability underflow");
    e.storage().instance().set(&StageStorageKey::Reserved(t.clone()),&n);
}
pub fn locked(e:&Env,a:&Address)->u128 {let k=StageStorageKey::Locked(a.clone());let n=e.storage().persistent().get(&k).unwrap_or(0);if e.storage().persistent().has(&k){e.storage().persistent().extend_ttl(&k,500_000,1_000_000);}n}
pub fn lock(e:&Env,a:&Address,n:u128){let k=StageStorageKey::Locked(a.clone());e.storage().persistent().set(&k,&locked(e,a).checked_add(n).unwrap());e.storage().persistent().extend_ttl(&k,500_000,1_000_000);}
pub fn unlock(e:&Env,a:&Address,n:u128){let k=StageStorageKey::Locked(a.clone());e.storage().persistent().set(&k,&locked(e,a).checked_sub(n).expect("lock underflow"));e.storage().persistent().extend_ttl(&k,500_000,1_000_000);}
pub fn available(e:&Env,a:&Address)->u128 {crate::storage::shares_of(e,a).checked_sub(locked(e,a)).unwrap()}
