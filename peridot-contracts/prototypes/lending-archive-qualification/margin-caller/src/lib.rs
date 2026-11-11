#![no_std]
//! Synthetic TEST ONLY receipt ABI caller. Never deploy as a real controller.
use soroban_sdk::{contract,contractimpl,Address,Env,IntoVal,Symbol};
#[contract] pub struct CapTestCaller;
#[contractimpl] impl CapTestCaller {
 pub fn borrow(e:Env,receipt:Address,amount:u128){
  e.invoke_contract::<()>(&receipt,&Symbol::new(&e,"init_margin_borrow_state"),(1u64,).into_val(&e));
  e.invoke_contract::<()>(&receipt,&Symbol::new(&e,"borrow_for_margin"),(1u64,e.current_contract_address(),amount).into_val(&e));
 }
}
