//! Isolated compiled 1x cash-first transactions on the existing runtime29 harness.
use super::*;
use soroban_sdk::testutils::Events;
include!("../command_types.rs");
#[contracttype]
#[derive(Clone,Debug)]
struct StageAllocation {
    supply:u128,shares:u128,liquidity_burn:u128,fee0:u128,fee1:u128,
    principal0:u128,principal1:u128,allocated0:u128,allocated1:u128,retained0:u128,retained1:u128,
}
struct ProfileRestore(std::string::String);
impl Drop for ProfileRestore {fn drop(&mut self){std::env::set_var("CL_RESOURCE_PROFILE",&self.0);}}
fn quiet<T>(action:impl FnOnce()->T)->T {
    let old=MEASURE.with(|m|m.replace(false));let result=action();MEASURE.with(|m|m.set(old));result
}
fn prestate(f:&F,action:impl FnOnce()) {
    let p=std::env::var("CL_RESOURCE_PROFILE").unwrap();let guard=ProfileRestore(p);
    std::env::set_var("CL_RESOURCE_PROFILE",std::env::var("CL_STAGE_SETUP_PROFILE").unwrap());
    crate::cl_resource_limits::configure(&f.e);quiet(action);drop(guard);crate::cl_resource_limits::configure(&f.e);
}
fn setup()->(F,Address) {
    let p=std::env::var("CL_RESOURCE_PROFILE").unwrap();let guard=ProfileRestore(p);
    std::env::set_var("CL_RESOURCE_PROFILE",std::env::var("CL_STAGE_SETUP_PROFILE").unwrap());
    let f=F::new();let e=&f.e;MEASURE.with(|m|m.set(false));
    auth::<()>(e,&f.admin,&f.share,"stage_bind",(f.admin.clone(),f.cash.clone()).into_val(e),&[]);
    auth::<()>(e,&f.admin,&f.cash,"stage_bind",(f.admin.clone(),f.share.clone()).into_val(e),&[]);
    auth::<()>(e,&f.admin,&f.share,"set_max_deploy",(f.admin.clone(),2_000_000_000u128).into_val(e),&[]);
    // TEST ONLY finite cap; no production cap/borrowing approval.
    let n=20_000_003i128;
    let child=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(f.user.clone(),f.share.clone(),n).into_val(e),sub_invokes:&[]};
    let seeded:i128=auth(e,&f.user,&f.share,"deposit_underlying",(f.user.clone(),n,1i128).into_val(e),&[child]);assert!(seeded>0);
    let entrant=Address::generate(e);
    auth::<()>(e,&f.admin,&f.under,"mint",(entrant.clone(),2_000_000_000i128).into_val(e),&[]);
    auth::<()>(e,&f.user,&f.reward,"transfer",(f.user.clone(),f.pool.clone(),100_000_000i128).into_val(e),&[]);
    auth::<()>(e,&f.admin,&f.pool,"set_rewards_config",(f.admin.clone(),10_000u64,10_003u128).into_val(e),&[]);
    // Donation is never part of common backing or command funding.
    for (t,n) in [(&f.under,777i128),(&f.paired,333i128)] {auth::<()>(e,&f.user,t,"transfer",(f.user.clone(),f.share.clone(),n).into_val(e),&[]);}
    drop(guard);crate::cl_resource_limits::configure(e);MEASURE.with(|m|m.set(true));(f,entrant)
}
fn next(e:&Env){e.ledger().with_mut(|l|{l.timestamp+=5;l.sequence_number+=1;});}
fn job(f:&F,id:u64)->StageJob {quiet(||call(&f.e,&f.cash,"stage_job",(id,).into_val(&f.e)))}
fn root<T:soroban_sdk::TryFromVal<Env,Val>>(f:&F,name:&str,args:Vec<Val>,label:&str)->T {
    next(&f.e);
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||call(&f.e,&f.cash,name,args)));
    measure(&f.e,label,result.is_ok());match result{Ok(v)=>v,Err(err)=>std::panic::resume_unwind(err)}
}
fn prep(f:&F,who:&Address)->u64 {
    next(&f.e);let e=&f.e;
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||auth(e,who,&f.cash,"stage_prepare",(who.clone(),200_000_003u128,100_000_000u128,99_000_000u128,190_000_000u128,100_000u128,e.ledger().timestamp()+1800).into_val(e),&[])));
    measure(e,"stage1_prepare",result.is_ok());match result{Ok(v)=>v,Err(err)=>std::panic::resume_unwind(err)}
}
fn fund(f:&F,who:&Address,id:u64) {
    let e=&f.e;next(e);let j=job(f,id);
    let transfer=[MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(who.clone(),f.share.clone(),(j.a+j.reserve)as i128).into_val(e),sub_invokes:&[]}];
    let vault=[MockAuthInvoke{contract:&f.share,fn_name:"stage_accept_funds",args:(f.cash.clone(),id,j.clone()).into_val(e),sub_invokes:&transfer}];
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||auth::<()>(e,who,&f.cash,"stage_fund",(id,).into_val(e),&vault)));
    measure(e,"stage1_fund",result.is_ok());if let Err(err)=result{std::panic::resume_unwind(err)}
    assert_eq!(job(f,id).phase,1);
}
fn convert(f:&F,id:u64){root::<()>(f,"stage_convert",(id,).into_val(&f.e),"stage1_convert");assert_eq!(job(f,id).phase,2);}
fn mint(f:&F,id:u64)->u128{let n=root(f,"stage_mint",(id,).into_val(&f.e),"stage1_fee_checkpoint_cash_mint");assert_eq!(job(f,id).phase,3);n}
fn invest(f:&F,id:u64){let l:u128=root(f,"stage_invest",(id,).into_val(&f.e),"stage1_keeper_investment");assert!(l>0);assert_eq!(job(f,id).phase,4);}
fn prep_close(f:&F,who:&Address,q:u128,convert:bool)->u64 {
    let e=&f.e;next(e);
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||auth(e,who,&f.cash,"stage_prepare_close",(who.clone(),q,if convert{1u128}else{0u128},e.ledger().timestamp()+1800).into_val(e),&[])));
    measure(e,"stage1_prepare_close",result.is_ok());match result{Ok(v)=>v,Err(err)=>std::panic::resume_unwind(err)}
}
fn withdraw(f:&F,id:u64,convert:bool)->Vec<u128>{root(f,"stage_withdraw",(id,).into_val(&f.e),if convert{"stage1_withdraw_for_conversion"}else{"stage1_withdraw_in_kind"})}
fn exit_convert(f:&F,id:u64)->u128{root(f,"stage_close_convert",(id,).into_val(&f.e),"stage1_optional_close_conversion")}
fn conservation(f:&F,entrant:&Address){quiet(||{
    let e=&f.e;let t:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));let mut pending=0;let mut fraction=0;
    for who in [&f.user,entrant] {let h:RewardHolder=call(e,&f.share,"reward_account",(who.clone(),).into_val(e));pending+=h.pending;fraction+=h.fraction;}
    assert_eq!(t.received*1_000_000_000_000_000_000,(t.paid+t.quarantine+pending)*1_000_000_000_000_000_000+fraction+t.rounding_scaled);
    assert_eq!(token::Client::new(e,&f.reward).balance(&f.share)as u128,t.received-t.paid);
    std::println!("STAGE1_REWARD_CONSERVATION received={} pending={pending} quarantine={} fractions={fraction} rounding={}",t.received,t.quarantine,t.rounding_scaled);
});}
fn trade(f:&F){quiet(||{
    let e=&f.e;let n=100_000_003u128;let quote:u128=call(e,&f.pool,"estimate_swap",(0u32,1u32,n).into_val(e));
    let child=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(f.user.clone(),f.pool.clone(),n as i128).into_val(e),sub_invokes:&[]};
    let out:u128=auth(e,&f.user,&f.pool,"swap",(f.user.clone(),0u32,1u32,n,quote).into_val(e),&[child]);assert_eq!(out,quote);
});}

#[test]
fn compiled_stage1_full_open_close() {
    let (f,who)=setup();let id=prep(&f,&who);fund(&f,&who,id);convert(&f,id);let q=mint(&f,id);
    conservation(&f,&who);trade(&f);invest(&f,id);trade(&f);
    let c=prep_close(&f,&who,q,true);let out=withdraw(&f,c,true);assert!(out.get(0).unwrap()+out.get(1).unwrap()>0);
    let paid=exit_convert(&f,c);assert!(paid>0);assert_eq!(job(&f,c).phase,12);conservation(&f,&who);
    std::println!("STAGE1_FULL_LIFECYCLE_PASS debt=0 open_owner_auth_after_fund=false close_prepare_owner_auth=true actual_intervening_trades=2");
}

#[test]
fn compiled_stage1_full_open_in_kind_close() {
    let (f,who)=setup();let id=prep(&f,&who);fund(&f,&who,id);convert(&f,id);let q=mint(&f,id);
    conservation(&f,&who);trade(&f);invest(&f,id);trade(&f);
    let c=prep_close(&f,&who,q,false);let out=withdraw(&f,c,false);
    assert!(out.get(0).unwrap()+out.get(1).unwrap()>0);
    assert_eq!(job(&f,c).phase,12);conservation(&f,&who);
    std::println!("STAGE1_FULL_IN_KIND_LIFECYCLE_PASS debt=0 payouts_in_withdraw_transaction=true actual_intervening_trades=2");
}

fn isolated(which:&str) {
    let (f,who)=setup();let mut id=0;let mut q=0;let mut c=0;
    if which=="prepare" {prep(&f,&who);return;}
    prestate(&f,||{id=prep(&f,&who);});
    if which=="fund" {fund(&f,&who,id);return;}
    prestate(&f,||fund(&f,&who,id));
    if which=="convert" {convert(&f,id);return;}
    prestate(&f,||convert(&f,id));
    if which=="mint" {mint(&f,id);conservation(&f,&who);return;}
    prestate(&f,||{q=mint(&f,id);trade(&f);});
    if which=="invest" {invest(&f,id);conservation(&f,&who);return;}
    prestate(&f,||{invest(&f,id);trade(&f);});
    if which=="prepare_close" {prep_close(&f,&who,q,false);return;}
    prestate(&f,||{c=prep_close(&f,&who,q,which!="in_kind");});
    if which=="in_kind" {withdraw(&f,c,false);assert_eq!(job(&f,c).phase,12);conservation(&f,&who);return;}
    if which=="withdraw" {withdraw(&f,c,true);conservation(&f,&who);return;}
    prestate(&f,||{withdraw(&f,c,true);});exit_convert(&f,c);conservation(&f,&who);
}
#[test]fn compiled_stage1_isolated_prepare(){isolated("prepare");}
#[test]fn compiled_stage1_isolated_fund(){isolated("fund");}
#[test]fn compiled_stage1_isolated_convert(){isolated("convert");}
#[test]fn compiled_stage1_isolated_mint(){isolated("mint");}
#[test]fn compiled_stage1_isolated_invest(){isolated("invest");}
#[test]fn compiled_stage1_isolated_prepare_close(){isolated("prepare_close");}
#[test]fn compiled_stage1_isolated_withdraw_in_kind(){isolated("in_kind");}
#[test]fn compiled_stage1_isolated_withdraw_for_conversion(){isolated("withdraw");}
#[test]fn compiled_stage1_isolated_close_convert(){isolated("close_convert");}

#[test]
fn compiled_stage1_incumbent_fee_entitlement_allocated_before_share_burn_one_unit() {
    let (f,who)=setup();let e=&f.e;let mut id=0;
    prestate(&f,||{id=prep(&f,&who);fund(&f,&who,id);convert(&f,id);mint(&f,id);trade(&f);});
    let supply:u128=quiet(||call::<i128>(e,&f.share,"total_supply",().into_val(e))as u128);
    let l:u128=quiet(||call(e,&f.share,"get_position_liquidity",().into_val(e)));
    let q=supply.div_ceil(l);assert_eq!(l*q/supply,1,"genuine one-unit Aquarius burn through staged share exit");
    let incumbent_before=token::Client::new(e,&f.share).balance(&f.user);
    let bob_before=token::Client::new(e,&f.share).balance(&who);
    let c=prep_close(&f,&f.user,q,false);withdraw(&f,c,false);
    // Capture before getter invocations replace the SDK last-invocation events.
    let events=e.events().all().filter_by_contract(&f.share);
    let a:StageAllocation=quiet(||call(e,&f.share,"stage_allocation",(c,).into_val(e)));
    assert_eq!(a.liquidity_burn,1);assert_eq!((a.principal0,a.principal1),(0,0));assert!(a.fee0+a.fee1>0);
    assert_eq!(a.supply,supply);assert_eq!(a.shares,q);assert!(a.retained0>=a.fee0 && a.retained1>=a.fee1);
    assert_eq!(token::Client::new(e,&f.share).balance(&f.user),incumbent_before-q as i128);
    assert_eq!(token::Client::new(e,&f.share).balance(&who),bob_before);
    let mut allocation_index=None;let mut burn_index=None;
    for (i,event) in events.events().iter().enumerate() {
        let soroban_sdk::xdr::ContractEventBody::V0(body)=&event.body;
        if body.topics.len()==2 && body.topics[1]==soroban_sdk::xdr::ScVal::U64(c) {
            if let soroban_sdk::xdr::ScVal::Symbol(symbol)=&body.topics[0] {
                if symbol.0.as_slice()==b"feealloc" {allocation_index=Some(i);}
                if symbol.0.as_slice()==b"stageburn" {burn_index=Some(i);}
            }
        }
    }
    assert!(allocation_index.unwrap()<burn_index.unwrap());conservation(&f,&who);
    std::println!("NAMED_INVARIANT_PASS INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN one_unit=true old_supply={supply} burned_shares={q} fee0={} fee1={} principal0=0 principal1=0",a.fee0,a.fee1);
}

#[test]
fn compiled_stage1_cash_mint_uses_actual_fee_inclusive_backing() {
    let (f,who)=setup();let e=&f.e;let mut id=0;
    prestate(&f,||{id=prep(&f,&who);fund(&f,&who,id);convert(&f,id);
        let n=5_000_000_003u128;
        let quote:u128=call(e,&f.pool,"estimate_swap",(0u32,1u32,n).into_val(e));
        let child=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(f.user.clone(),f.pool.clone(),n as i128).into_val(e),sub_invokes:&[]};
        let out:u128=auth(e,&f.user,&f.pool,"swap",(f.user.clone(),0u32,1u32,n,quote).into_val(e),&[child]);assert_eq!(out,quote);
    });
    let j=job(&f,id);
    let (s,l,basket,fees,held)=quiet(||{
        let s=call::<i128>(e,&f.share,"total_supply",().into_val(e))as u128;
        let l:u128=call(e,&f.share,"get_position_liquidity",().into_val(e));
        let basket:Vec<u128>=call(e,&f.pool,"estimate_withdraw_position",(f.share.clone(),-2000i32,2000i32,l).into_val(e));
        let fees:Vec<u128>=call(e,&f.pool,"get_all_position_fees",(f.share.clone(),).into_val(e));
        let held=token::Client::new(e,&f.under).balance(&f.share)as u128+token::Client::new(e,&f.paired).balance(&f.share)as u128;
        (s,l,basket,fees,held)
    });
    // Oracle ratio is exactly one. Actual quote already contains fees once.
    let idle=held-j.a-j.b-j.reserve-777-333;
    let actual_nav=idle+basket.get(0).unwrap()+basket.get(1).unwrap();
    let q=1_000_000_000_000_000_000u128;let lo=904_841_941_932_768_879u128;let hi=1_105_165_392_603_232_697u128;
    let old_fair_nav=idle+l*(hi-q)/hi+l*(q-lo)/q+fees.get(0).unwrap()+fees.get(1).unwrap();
    assert!(actual_nav>old_fair_nav,"off-oracle case must distinguish actual backing from convergence value");
    let expected=(j.a+j.b)*s/actual_nav;let old_overmint=(j.a+j.b)*s/old_fair_nav;
    assert!(old_overmint>expected);
    let received=mint(&f,id);assert_eq!(received,expected);conservation(&f,&who);
    std::println!("ACTUAL_BACKING_MINT_PASS actual_nav={actual_nav} convergence_nav={old_fair_nav} minted={received} rejected_old_overmint={old_overmint}");
}
