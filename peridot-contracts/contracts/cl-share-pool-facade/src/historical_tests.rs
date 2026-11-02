//! Historical reward proofs on actual Aquarius bytecode with controlled state.
use super::*;
use soroban_sdk::{Map,TryFromVal,BytesN};
const SCALE:u128=1_000_000_000_000_000_000;
fn get<T:soroban_sdk::TryFromVal<Env,Val>>(e:&Env,m:&Map<Symbol,Val>,key:&str)->T {T::try_from_val(e,&m.get(Symbol::new(e,key)).unwrap()).ok().unwrap()}
fn segment(f:&F,id:u64)->Map<Symbol,Val>{call(&f.e,&f.share,"reward_segment",(id,).into_val(&f.e))}
fn interval_of(f:&F)->u64 {let b:Map<Symbol,Val>=call(&f.e,&f.share,"reward_boundary",(0u64,).into_val(&f.e));get(&f.e,&b,"interval")}
fn deposit(f:&F,owner:&Address,amount:i128)->i128 {
 let e=&f.e;let t=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(owner.clone(),f.share.clone(),amount).into_val(e),sub_invokes:&[]};
 auth(e,owner,&f.share,"deposit_underlying",(owner.clone(),amount,1i128).into_val(e),&[t])
}
fn time(e:&Env,t:u64){e.ledger().with_mut(|l|{l.timestamp=t;l.sequence_number+=1;});}
fn toggle(f:&F,kill:bool){auth::<()>(&f.e,&f.admin,&f.pool,if kill{"kill_claim"}else{"unkill_claim"},(f.admin.clone(),).into_val(&f.e),&[]);}
fn checkpoint_index(f:&F,a:&Address,revision:u64)->u32{
 let e=&f.e;let count:u32=call(e,&f.share,"reward_history_count",(a.clone(),).into_val(e));let mut selected=0;
 for n in 0..count {let w:Map<Symbol,Val>=call(e,&f.share,"reward_weight",(a.clone(),n).into_val(e));if get::<u64>(e,&w,"revision")<=revision{selected=n;}}
 selected
}
fn settle_interval(f:&F,id:u64,owners:&[Address],label:&str)->std::vec::Vec<u128>{
 let e=&f.e;let i:Map<Symbol,Val>=call(e,&f.share,"reward_interval",(id,).into_val(e));let first:u64=get(e,&i,"first_segment");let count:u32=get(e,&i,"segments");
 let mut credited=std::vec![0u128;owners.len()];
 for sid in first..first+count as u64 {
  assert!(call::<bool>(e,&f.share,"prepare_reward_segment",(sid,4u32).into_val(e)));measure(e,"historical_prepare_segment",true);
  let s=segment(f,sid);let rev:u64=get(e,&s,"revision");
  for (n,owner) in owners.iter().enumerate(){let version=checkpoint_index(f,owner,rev);let got:u128=call(e,&f.share,"settle_reward_segment",(owner.clone(),sid,version).into_val(e));measure(e,label,true);credited[n]+=got;
   let repeated:u128=call(e,&f.share,"settle_reward_segment",(owner.clone(),sid,version).into_val(e));measure(e,"historical_repeat_idempotent",true);assert_eq!(repeated,0);
  }
 }
 credited
}
fn conserve(f:&F,owners:&[Address]){
 let e=&f.e;let t:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));let mut credits=0;let mut fractions=0;
 for a in owners{let h:RewardHolder=call(e,&f.share,"reward_account",(a.clone(),).into_val(e));credits+=h.pending;fractions+=h.fraction;}
 assert_eq!(t.received*SCALE,(credits+t.paid+t.quarantine)*SCALE+fractions+t.rounding_scaled);
 assert_eq!(token::Client::new(e,&f.reward).balance(&f.share) as u128,t.received-t.paid);
 std::println!("HISTORICAL_CONSERVATION received={} paid={} pending={credits} fractions={fractions} quarantine={} rounding={}",t.received,t.paid,t.quarantine,t.rounding_scaled);
}
#[test]
fn compiled_automatic_history_exited_owner_and_later_accrual(){
 let f=F::new();let e=&f.e;let bob=Address::generate(e);let newcomer=Address::generate(e);
 let minted=deposit(&f,&f.user,100_000_000);
 auth::<()>(e,&f.user,&f.share,"transfer",(f.user.clone(),bob.clone(),minted/3).into_val(e),&[]);
 auth::<()>(e,&f.admin,&f.share,"enable_verified_reward_view",().into_val(e),&[]);
 funded_tests::schedule_rewards(&f);time(e,1100);toggle(&f,true);
 let first_pending:u128=call(e,&f.pool,"get_user_reward",(f.share.clone(),).into_val(e));
 let _:Vec<u128>=auth(e,&f.user,&f.share,"emergency_redeem",(f.user.clone(),(minted-minted/3) as u128,false).into_val(e),&[]);
 measure(e,"historical_proven_exit",true);assert_eq!(token::Client::new(e,&f.share).balance(&f.user),0);
 time(e,1120);
 let _:Vec<u128>=auth(e,&bob,&f.share,"emergency_redeem",(bob.clone(),(minted/6) as u128,false).into_val(e),&[]);
 time(e,1140);toggle(&f,false);
 let final_pending:u128=call(e,&f.pool,"get_user_reward",(f.share.clone(),).into_val(e));assert!(final_pending>first_pending);
 assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));measure(e,"historical_actual_claim_receipt",true);
 let t:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));assert_eq!(t.quarantine,final_pending);
 // A newcomer joins after the historical claim but before settlement.
 auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),newcomer.clone(),50_000_000i128).into_val(e),&[]);deposit(&f,&newcomer,50_000_000);
 let owners=[f.user.clone(),bob.clone(),newcomer.clone()];let id=interval_of(&f);
 let credits=settle_interval(&f,id,&owners,"quarantine_reconciliation");assert!(credits[0]>0&&credits[1]>0);assert_eq!(credits[2],0);
 let i:Map<Symbol,Val>=call(e,&f.share,"reward_interval",(id,).into_val(e));assert_eq!(get::<u32>(e,&i,"resolved"),get::<u32>(e,&i,"segments"));
 assert_eq!(get::<u128>(e,&i,"redistribution_scaled"),0);conserve(&f,&owners);
 std::println!("AUTOMATIC_HISTORICAL first_pending={first_pending} claim={final_pending} exited_credit={} coowner_credit={} newcomer=0",credits[0],credits[1]);
}
#[test]
fn compiled_governed_waiver_original_custody_coowner_exits_before_receipt(){
 let f=F::new();let e=&f.e;let id=funded_tests::open(&f);let waiving=Address::generate(e);let newcomer=Address::generate(e);
 auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),waiving.clone(),100_000_000i128).into_val(e),&[]);let waiving_shares=deposit(&f,&waiving,100_000_000);
 funded_tests::schedule_rewards(&f);time(e,1100);toggle(&f,true);
 let _:Vec<u128>=auth(e,&waiving,&f.share,"emergency_redeem",(waiving.clone(),waiving_shares as u128,true).into_val(e),&[]);
 // Original coowner is the borrower in receipt/margin custody. Forced seizure
 // exits their entire holding and assigns it to newcomer; it cannot waive them.
 auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),newcomer.clone(),200_000_000i128).into_val(e),&[]);
 auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(f.share.clone()),45_000_000_000_000i128).into_val(e),&[]);
 let transfer=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(newcomer.clone(),f.cash.clone(),110_000_000i128).into_val(e),sub_invokes:&[]};let ts=[transfer];
 let repay=MockAuthInvoke{contract:&f.cash,fn_name:"repay_full_for_margin",args:(id,newcomer.clone(),110_000_000u128).into_val(e),sub_invokes:&ts};
 let (repaid,seized): (u128,u128)=auth(e,&newcomer,&f.margin,"liquidate_funded_v3",(newcomer.clone(),id,110_000_000u128,1u128).into_val(e),&[repay]);
 assert!(repaid>0 && seized>0);
 // Establish the original coowner really exited all beneficial custody before
 // the later claim; a partially seized coowner would not prove this scenario.
 assert_eq!(call::<u128>(e,&f.share,"custody_allocation",(f.receipt.clone(),f.user.clone()).into_val(e)),0);
 assert_eq!(call::<u128>(e,&f.share,"custody_allocation",(f.margin.clone(),f.user.clone()).into_val(e)),0);
 assert_eq!(token::Client::new(e,&f.share).balance(&f.user),0);
 assert!(call::<u128>(e,&f.share,"custody_allocation",(f.receipt.clone(),newcomer.clone()).into_val(e))>0);
 time(e,1150);toggle(&f,false);assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));
 let interval=interval_of(&f);let i:Map<Symbol,Val>=call(e,&f.share,"reward_interval",(interval,).into_val(e));let first:u64=get(e,&i,"first_segment");let count:u32=get(e,&i,"segments");let received:u128=get(e,&i,"received");
 let mut evidence_rows=std::vec::Vec::new();let mut expected=0;
 for sid in first..first+count as u64 {let s=segment(&f,sid);let start:Option<u128>=get(e,&s,"reading_start");let end:Option<u128>=get(e,&s,"reading_end");let amount=end.unwrap()-start.unwrap();expected+=amount;evidence_rows.push((sid,amount,start.unwrap(),end.unwrap()));assert!(!get::<bool>(e,&s,"observed_verified"));}
 assert_eq!(expected,received,"governed amounts must agree with the actual receipt in this fixture");
 let evidence=serde_json::to_string(&serde_json::json!({"classification":"governed-controlled-state-attribution-not-automatic","pool_hash":"12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6","interval":interval,"received":received,"segments":evidence_rows})).unwrap();
 let digest=e.crypto().sha256(&Bytes::from_slice(e,evidence.as_bytes())).to_bytes();let digest=BytesN::<32>::try_from(digest).unwrap();
 std::println!("GOVERNED_PUBLISHED_EVIDENCE digest={digest:?} json={evidence}");
 if let Ok(dir)=std::env::var("CL_GOVERNED_EVIDENCE_DIR"){std::fs::create_dir_all(&dir).unwrap();std::fs::write(format!("{dir}/waiver-custody-interval-{interval}.json"),&evidence).unwrap();}
 // Without governed evidence, an old estimate is never released automatically.
 let s=segment(&f,first);let version=checkpoint_index(&f,&f.user,get(e,&s,"revision"));
 assert!(call::<bool>(e,&f.share,"prepare_reward_segment",(first,4u32).into_val(e)));
 e.mock_auths(&[]);crate::cl_resource_limits::reset(e);assert!(e.try_invoke_contract::<u128,soroban_sdk::Error>(&f.share,&Symbol::new(e,"settle_reward_segment"),(f.user.clone(),first,version).into_val(e)).is_err());measure(e,"unverified_history_stays_quarantined",true);
 for (sid,amount,_,_) in &evidence_rows{auth::<()>(e,&f.admin,&f.share,"propose_reward_segment",(*sid,*amount,digest.clone()).into_val(e),&[]);}
 time(e,87551);
 for (sid,amount,_,_) in &evidence_rows{auth::<()>(e,&f.admin,&f.share,"attribute_reward_segment",(*sid,*amount,digest.clone()).into_val(e),&[]);}
 // Observe the newcomer credit at the historical segment itself, separately
 // from its legitimate later-period accrual and the aggregate conservation sum.
 let original_segment=segment(&f,first);
 let newcomer_checkpoint=checkpoint_index(&f,&newcomer,get(e,&original_segment,"revision"));
 let newcomer_historical:u128=call(e,&f.share,"settle_reward_segment",(newcomer.clone(),first,newcomer_checkpoint).into_val(e));
 measure(e,"waiver_newcomer_historical_zero",true);
 assert_eq!(newcomer_historical,0,"newcomer cannot receive original entitlement or waiver redistribution");
 let owners=[waiving.clone(),f.user.clone(),newcomer.clone()];let credits=settle_interval(&f,interval,&owners,"waiver_redistribution");
 assert_eq!(credits[0],0);assert!(credits[1]>0&&credits[2]>0);
 let first_state=segment(&f,first);assert!(get::<bool>(e,&first_state,"governed"));assert!(!get::<bool>(e,&first_state,"observed_verified"));
 let newcomer_version=checkpoint_index(&f,&newcomer,get(e,&first_state,"revision"));let historical_weight:Map<Symbol,Val>=call(e,&f.share,"reward_weight",(newcomer.clone(),newcomer_version).into_val(e));assert!(get::<u64>(e,&historical_weight,"revision")>get::<u64>(e,&first_state,"revision"));
 let i:Map<Symbol,Val>=call(e,&f.share,"reward_interval",(interval,).into_val(e));assert!(get::<u128>(e,&i,"redistribution_scaled")>0);
 conserve(&f,&owners);
 std::println!("GOVERNED_WAIVER_EXITED_CUSTODY_OWNER received={received} borrower_historical={} newcomer_later={} waived=0 automatic=false",credits[1],credits[2]);
}

#[test]
fn compiled_multiple_unsettled_intervals_exited_beneficiary(){
 let f=F::new();let e=&f.e;let bob=Address::generate(e);let newcomer=Address::generate(e);
 let minted=deposit(&f,&f.user,100_000_000);let share_b= minted/5;
 auth::<()>(e,&f.user,&f.share,"transfer",(f.user.clone(),bob.clone(),share_b).into_val(e),&[]);
 auth::<()>(e,&f.admin,&f.share,"enable_verified_reward_view",().into_val(e),&[]);
 funded_tests::schedule_rewards(&f);let mut remaining=minted-share_b;let mut ids=std::vec::Vec::new();
 for j in 0..3u64{
  time(e,1100+j*100);toggle(&f,true);let burn=if j==2{remaining}else{remaining/3};
  let _:Vec<u128>=auth(e,&f.user,&f.share,"emergency_redeem",(f.user.clone(),burn as u128,false).into_val(e),&[]);measure(e,"multiple_intervals_bounded_exit",true);remaining-=burn;
  let b:Map<Symbol,Val>=call(e,&f.share,"reward_boundary",(j,).into_val(e));ids.push(get::<u64>(e,&b,"interval"));
  time(e,1110+j*100);toggle(&f,false);assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));measure(e,"multiple_intervals_actual_claim",true);
 }
 assert_eq!(remaining,0);let t:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));assert!(t.quarantine>0);
 auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),newcomer.clone(),20_000_000i128).into_val(e),&[]);deposit(&f,&newcomer,20_000_000);
 let owners=[f.user.clone(),bob.clone(),newcomer.clone()];
 for id in ids{let credits=settle_interval(&f,id,&owners,"historical_multiple_intervals");assert!(credits[0]>0&&credits[1]>0);assert_eq!(credits[2],0);}
 conserve(&f,&owners);std::println!("MULTIPLE_HISTORICAL_INTERVALS count=3 exited_owner=true newcomer_old_credit=0 incremental=true");
}
