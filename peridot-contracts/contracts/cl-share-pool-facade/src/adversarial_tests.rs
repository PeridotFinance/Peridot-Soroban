//! Failure after repayment must roll back the entire compiled storage state.
use super::*;
fn authorize(f:&F,liquidator:&Address,id:u64,max:u128){
 let e=&f.e;
 let transfer=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(liquidator.clone(),f.cash.clone(),max as i128).into_val(e),sub_invokes:&[]};let ts=[transfer];
 let repayment=MockAuthInvoke{contract:&f.cash,fn_name:"repay_full_for_margin",args:(id,liquidator.clone(),max).into_val(e),sub_invokes:&ts};
 e.mock_auths(&[MockAuth{address:liquidator,invoke:&MockAuthInvoke{contract:&f.margin,fn_name:"liquidate_funded_v3",args:(liquidator.clone(),id,max,1u128).into_val(e),sub_invokes:&[repayment]}}]);
}
fn fail(f:&F,liquidator:&Address,id:u64,label:&str,expected:&str,authorized:bool){
 let e=&f.e;let max=110_000_000u128;
 if authorized{authorize(f,liquidator,id,max);}else{e.mock_auths(&[]);}
 let before=e.to_ledger_snapshot().ledger_entries;
 #[cfg(feature="transaction-evidence")]
 crate::transaction_evidence::capture_rejection_before(e,&f.margin,"liquidate_funded_v3",&(liquidator.clone(),id,max,1u128).into_val(e),if authorized {Some(liquidator)} else {None},!authorized,label);
 crate::cl_resource_limits::reset(e);
 let result=e.try_invoke_contract::<(u128,u128),soroban_sdk::Error>(&f.margin,&Symbol::new(e,"liquidate_funded_v3"),(liquidator.clone(),id,max,1u128).into_val(e));
 assert!(result.is_err());
 let events=format!("{:?}",e.host().get_diagnostic_events().unwrap());
 assert!(events.contains(expected),"{label} failed at wrong stage: {events}");
 if authorized {assert!(!events.contains("Error(Auth"),"authorized negative must not fail auth");}
 if label=="funded_failed_collateral_transfer_rollback" {
  let returned=e.host().get_diagnostic_events().unwrap().0.iter().any(|v|{let s=format!("{v:?}");s.contains("fn_return")&&s.contains("repay_full_for_margin")});
  assert!(returned,"debt repayment must return before transfer rejection");
 }
 measure(e,label,true);
 assert_eq!(e.to_ledger_snapshot().ledger_entries,before,"{label}: complete ledger rollback, including reward history");
 std::println!("COMPILED_ATOMIC_ROLLBACK {label} all_ledger_entries_equal=true");
}
#[test]
fn compiled_funded_adversarial_atomicity(){
 let f=F::new();let e=&f.e;let id=funded_tests::open(&f);let liquidator=Address::generate(e);
 funded_tests::schedule_rewards(&f);funded_tests::advance_rewards(e,1100);
 assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));
 let old:RewardHolder=call(e,&f.share,"reward_account",(f.user.clone(),).into_val(e));assert!(old.pending>0);
 auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(f.share.clone()),55_000_000_000_000i128).into_val(e),&[]);
 fail(&f,&liquidator,id,"funded_unauthorized_rollback","Auth",false);
 fail(&f,&liquidator,id,"funded_insufficient_liquidator_rollback","repay_full_for_margin",true);
 auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),liquidator.clone(),200_000_000i128).into_val(e),&[]);
 auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(f.share.clone()),0i128).into_val(e),&[]);
 fail(&f,&liquidator,id,"funded_invalid_valuation_rollback","lastprice",true);
 auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(f.share.clone()),55_000_000_000_000i128).into_val(e),&[]);
 // This is an actual supported pause on the real receipt market, not a mock transfer.
 auth::<()>(e,&f.admin,&f.controller,"set_pause_redeem",(f.receipt.clone(),true).into_val(e),&[]);
 fail(&f,&liquidator,id,"funded_failed_collateral_transfer_rollback","is_redeem_paused",true);
 let after:RewardHolder=call(e,&f.share,"reward_account",(f.user.clone(),).into_val(e));assert_eq!(old.pending,after.pending);assert_eq!(old.fraction,after.fraction);
 auth::<()>(e,&f.admin,&f.controller,"set_pause_redeem",(f.receipt.clone(),false).into_val(e),&[]);
 authorize(&f,&liquidator,id,110_000_000);
 #[cfg(feature="transaction-evidence")]
 crate::transaction_evidence::capture_before(e,&f.margin,"liquidate_funded_v3",&(liquidator.clone(),id,110_000_000u128,1u128).into_val(e),Some(&liquidator));
 crate::cl_resource_limits::reset(e);
 let paid:(u128,u128)=e.invoke_contract(&f.margin,&Symbol::new(e,"liquidate_funded_v3"),(liquidator.clone(),id,110_000_000u128,1u128).into_val(e));
 measure(e,"funded_after_failures_success",true);assert!(paid.0>0&&paid.1>0);
 fail(&f,&liquidator,id,"funded_repeated_execution_rollback","UnreachableCodeReached",true);
 assert_eq!(call::<u128>(e,&f.cash,"get_margin_borrow_balance",(id,).into_val(e)),0);
}

#[contracttype]
#[derive(Clone)]
enum PoolFaultKey { User(Address) }
#[test]
fn compiled_both_reward_calls_fail_forced_settlement_preserves_history(){
 let f=F::new();let e=&f.e;let id=funded_tests::open(&f);let liquidator=Address::generate(e);
 funded_tests::schedule_rewards(&f);funded_tests::advance_rewards(e,1100);
 assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));
 let old:RewardHolder=call(e,&f.share,"reward_account",(f.user.clone(),).into_val(e));assert!(old.pending>0);
 auth::<()>(e,&f.admin,&f.share,"enable_verified_reward_view",().into_val(e),&[]);
 funded_tests::advance_rewards(e,1200);
 // Controlled dependency fault: corrupt only Aquarius's per-vault user record.
 // Actual Aquarius bytecode remains pinned. This is not naturally reached live state.
 let key=PoolFaultKey::User(f.share.clone());
 let saved:Val=e.as_contract(&f.pool,||{let old=e.storage().persistent().get(&key).expect("pool user record exists");e.storage().persistent().set(&key,&123u32);old});
 e.mock_auths(&[]);crate::cl_resource_limits::reset(e);
 assert!(e.try_invoke_contract::<u128,soroban_sdk::Error>(&f.pool,&Symbol::new(e,"get_user_reward"),(f.share.clone(),).into_val(e)).is_err());
 measure(e,"actual_bytecode_pending_view_fault",true);
 assert!(!call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));
 measure(e,"actual_bytecode_claim_fault",true);
 auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),liquidator.clone(),200_000_000i128).into_val(e),&[]);
 auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(f.share.clone()),55_000_000_000_000i128).into_val(e),&[]);
 authorize(&f,&liquidator,id,110_000_000);
 #[cfg(feature="transaction-evidence")]
 crate::transaction_evidence::capture_before(e,&f.margin,"liquidate_funded_v3",&(liquidator.clone(),id,110_000_000u128,1u128).into_val(e),Some(&liquidator));
 crate::cl_resource_limits::reset(e);
 let paid:(u128,u128)=e.invoke_contract(&f.margin,&Symbol::new(e,"liquidate_funded_v3"),(liquidator.clone(),id,110_000_000u128,1u128).into_val(e));
 measure(e,"both_reward_calls_failed_funded_liquidation",true);assert!(paid.0>0&&paid.1>0);
 assert_eq!(call::<u128>(e,&f.cash,"get_margin_borrow_balance",(id,).into_val(e)),0);
 let after:RewardHolder=call(e,&f.share,"reward_account",(f.user.clone(),).into_val(e));assert_eq!(old.pending,after.pending);assert_eq!(old.fraction,after.fraction);
 e.as_contract(&f.pool,||e.storage().persistent().set(&key,&saved));
 assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));
 let totals:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));assert!(totals.quarantine>0);
 std::println!("FORCED_BOTH_CALLS_OUTAGE preserved={} late_quarantine={} no_waiver=true dependency_fault_injection=true",after.pending,totals.quarantine);
}
