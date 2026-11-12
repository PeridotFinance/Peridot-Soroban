//! Part (a) only: pinned Aquarius bytecode through the existing protocol29 harness.
//! Synthetic external LP owner, exact nested SAC auth; not the future vault ABI.
use super::*;
use soroban_sdk::U256;
use serde_json::json;

#[contracttype]
#[derive(Clone)]
struct Position {
    fee_growth_inside_0_last_x128: U256,
    fee_growth_inside_1_last_x128: U256,
    liquidity: u128,
    tokens_owed_0: u128,
    tokens_owed_1: u128,
}
#[contracttype]
#[derive(Clone)]
struct Slot { sqrt_price_x96: U256, tick: i32 }

fn pair(v: Vec<u128>) -> [u128; 2] { assert_eq!(v.len(),2); [v.get(0).unwrap(),v.get(1).unwrap()] }
fn fees(f:&F, owner:&Address)->[u128;2] {
    pair(call(&f.e,&f.pool,"get_all_position_fees",(owner.clone(),).into_val(&f.e)))
}
fn wallet(f:&F, owner:&Address)->[i128;2] {
    [token::Client::new(&f.e,&f.under).balance(owner),token::Client::new(&f.e,&f.paired).balance(owner)]
}
fn deposit(f:&F, owner:&Address, lo:i32, hi:i32, n:u128, label:&str)->([u128;2],u128) {
    let e=&f.e;
    let desired=soroban_sdk::vec![e,n,n];
    let (amounts,l):(Vec<u128>,u128)=call(e,&f.pool,"estimate_deposit_position",(lo,hi,desired.clone()).into_val(e));
    measure(e,&format!("{label}_quote"),true);
    let a=pair(amounts);
    let children=[MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(owner.clone(),f.pool.clone(),n as i128).into_val(e),sub_invokes:&[]},
                  MockAuthInvoke{contract:&f.paired,fn_name:"transfer",args:(owner.clone(),f.pool.clone(),n as i128).into_val(e),sub_invokes:&[]}];
    let (actual,added):(Vec<u128>,u128)=auth(e,owner,&f.pool,"deposit_position",(owner.clone(),lo,hi,desired,l).into_val(e),&children);
    measure(e,label,true);
    let actual=pair(actual);
    for i in 0..2 { assert!(actual[i]<=n); assert!(actual[i].abs_diff(a[i])<=1,"quote/actual rounding exceeds one raw unit"); }
    assert_eq!(added,l);assert!(added>0);
    (actual,added)
}
fn trade(f:&F,n:u128,reverse:bool) {
    let e=&f.e;let (i,j,t)=if reverse{(1u32,0u32,&f.paired)}else{(0u32,1u32,&f.under)};
    let quote:u128=call(e,&f.pool,"estimate_swap",(i,j,n).into_val(e));
    let child=MockAuthInvoke{contract:t,fn_name:"transfer",args:(f.user.clone(),f.pool.clone(),n as i128).into_val(e),sub_invokes:&[]};
    let actual:u128=auth(e,&f.user,&f.pool,"swap",(f.user.clone(),i,j,n,quote).into_val(e),&[child]);
    assert_eq!(actual,quote);
}
fn emit(row:serde_json::Value) {
    use std::io::Write;
    std::println!("FEE_PENDING {row}");
    let mut file=std::fs::OpenOptions::new().create(true).append(true)
        .open(std::env::var("CL_FEE_PENDING_REPORT").unwrap()).unwrap();
    writeln!(file,"{row}").unwrap();
}

#[derive(Debug,PartialEq,Eq)]
struct Outcome { principal:[u128;2], liquidity:u128, total_fees:[u128;2], wallet_delta:[i128;2], prestate:String }

struct ProfileRestore(String);
impl Drop for ProfileRestore { fn drop(&mut self) { std::env::set_var("CL_RESOURCE_PROFILE", &self.0); } }

fn branch(range:&str, path:&str, op:&str, swept:bool)->Outcome {
    let selected=std::env::var("CL_RESOURCE_PROFILE").unwrap();
    let _restore=ProfileRestore(selected.clone());
    std::env::set_var("CL_RESOURCE_PROFILE",std::env::var("CL_FEE_SETUP_PROFILE").unwrap());
    let f=F::new();let e=&f.e;MEASURE.with(|m|m.set(false));
    let owner=Address::generate(e);
    for t in [&f.under,&f.paired] {
        auth::<()>(e,&f.admin,t,"mint",(owner.clone(),10_000_000_000i128).into_val(e),&[]);
    }
    let (lo,hi)=match range {"narrow"=>(-40,40),"wide"=>(-2000,2000),"full"=>(-887260,887260),_=>panic!("range")};
    deposit(&f,&owner,lo,hi,1_000_000_003,"setup");
    if path!="zero" {
        for (n,reverse) in [(3_000_017u128,false),(2_999_983,true),(10_003,false),(10_007,true)] {
            trade(&f,n,reverse);
        }
    }
    if path=="outside" || path=="return" {
        trade(&f,5_000_000_000,false);
        let slot:Slot=call(e,&f.pool,"get_slot0",().into_val(e));
        assert!(slot.tick<lo || slot.tick>=hi,"actual range crossing required");
        if path=="return" {
            trade(&f,4_900_000_000,true);
            let slot:Slot=call(e,&f.pool,"get_slot0",().into_val(e));
            assert!(slot.tick>=lo && slot.tick<hi,"actual range return required");
        }
    }
    let pre=fees(&f,&owner);
    if path=="zero" {assert_eq!(pre,[0,0]);} else {assert!(pre[0]+pre[1]>0,"must have authentic pending fees");}
    use sha2::{Digest,Sha256};
    let prestate=format!("{:x}",Sha256::digest(serde_json::to_vec(&e.to_ledger_snapshot()).unwrap()));
    let initial=wallet(&f,&owner);
    let before:Position=call(e,&f.pool,"get_position",(owner.clone(),lo,hi).into_val(e));
    let slot:Slot=call(e,&f.pool,"get_slot0",().into_val(e));
    std::env::set_var("CL_RESOURCE_PROFILE",selected);
    crate::cl_resource_limits::configure(e);
    MEASURE.with(|m|m.set(true));
    let case=format!("fee_pending_{range}_{path}_{op}_{}",if swept{"swept"}else{"pending"});
    assert_eq!(fees(&f,&owner),pre);
    measure(e,&format!("{case}_fee_view"),true);
    let collected=if swept {
        let c=pair(auth(e,&owner,&f.pool,"claim_all_position_fees",(owner.clone(),).into_val(e),&[]));
        measure(e,&format!("{case}_sweep"),true);assert_eq!(c,pre);c
    } else {[0,0]};
    let (principal,liquidity)=if op.starts_with("deposit") {
        let n=if op=="deposit_dust"{3}else{100_000_003};
        deposit(&f,&owner,lo,hi,n,&case)
    } else {
        let n=match op {"withdraw_dust"=>1,"withdraw_partial"=>before.liquidity/7,"withdraw_full"=>before.liquidity,_=>panic!("op")};
        let quote:Vec<u128>=call(e,&f.pool,"estimate_withdraw_position",(owner.clone(),lo,hi,n).into_val(e));
        measure(e,&format!("{case}_quote"),true);
        let q=pair(quote);let pending=if swept{[0,0]}else{pre};
        let out=pair(auth(e,&owner,&f.pool,"withdraw_position",(owner.clone(),lo,hi,n,soroban_sdk::vec![e,0u128,0u128]).into_val(e),&[]));
        measure(e,&case,true);assert_eq!(q,out,"quote must match actual receipt at same state");
        let p=[out[0].checked_sub(pending[0]).unwrap(),out[1].checked_sub(pending[1]).unwrap()];
        // Arithmetic for a partial share burn: never give every collected fee to
        // the exiter. Verify exact conservation and <1 raw-unit rounding/token.
        let (qshares,supply)=(143u128,1001u128);
        for i in 0..2 {let exit=p[i]+pre[i]*qshares/supply;let retained=pre[i]-pre[i]*qshares/supply;
            assert_eq!(exit+retained,p[i]+pre[i]);assert!((pre[i]*qshares)%supply<supply);}
        (p,n)
    };
    MEASURE.with(|m|m.set(false));
    let remaining=fees(&f,&owner);let final_wallet=wallet(&f,&owner);
    let delta=[final_wallet[0]-initial[0],final_wallet[1]-initial[1]];
    let mut total_fees=[0u128;2];
    for i in 0..2 {
        let paid=if op.starts_with("deposit"){delta[i]+principal[i] as i128}else{delta[i]-principal[i] as i128};
        assert!(paid>=0);total_fees[i]=paid as u128+remaining[i];
        assert_eq!(total_fees[i],pre[i],"fee conservation including wallet, pending and sweep");
    }
    if op=="withdraw_full" {
        e.mock_auths(&[]);crate::cl_resource_limits::reset(e);
        match e.try_invoke_contract::<Val,soroban_sdk::Error>(&f.pool,&Symbol::new(e,"get_position"),(owner.clone(),lo,hi).into_val(e)) {
            Err(Ok(err))=>assert_eq!(err,soroban_sdk::Error::from_contract_error(2118)),
            _=>panic!("full exit must remove position with the specific venue missing-position error"),
        }
        assert_eq!(remaining,[0,0]);
    } else {
        let after:Position=call(e,&f.pool,"get_position",(owner.clone(),lo,hi).into_val(e));
        assert_eq!(after.liquidity,if op.starts_with("deposit"){before.liquidity+liquidity}else{before.liquidity-liquidity});
    }
    emit(json!({"case":case,"range":[lo,hi],"tick":slot.tick,"operation":op,"swept":swept,"prestate_sha256":prestate,"pre_fees":pre,"collected_before":collected,"remaining_fees":remaining,"principal":principal,"wallet_delta":delta,"liquidity":liquidity,"fee_conservation_exact":true,"scope":"actual-aquarius-bytecode-controlled-state-external-owner-not-vault-transaction"}));
    Outcome{principal,liquidity,total_fees,wallet_delta:delta,prestate}
}
fn compare(range:&str,path:&str) {
    for op in ["deposit_dust","deposit_normal","withdraw_dust","withdraw_partial","withdraw_full"] {
        let pending=branch(range,path,op,false);let swept=branch(range,path,op,true);
        assert_eq!(pending.prestate,swept.prestate,"branches must start from identical complete ledger state");
        assert_eq!(pending.principal,swept.principal);assert_eq!(pending.liquidity,swept.liquidity);
        assert_eq!(pending.total_fees,swept.total_fees);
        // Deposits may leave fees pending rather than pay them. Withdrawal must
        // reconcile the same total actual wallet payout with/without pre-sweep.
        if op.starts_with("withdraw"){assert_eq!(pending.wallet_delta,swept.wallet_delta);}
    }
}
#[test] fn compiled_fee_pending_narrow_zero(){compare("narrow","zero");}
#[test] fn compiled_fee_pending_narrow_rounding(){compare("narrow","fees");}
#[test] fn compiled_fee_pending_narrow_outside(){compare("narrow","outside");}
#[test] fn compiled_fee_pending_narrow_return(){compare("narrow","return");}
#[test] fn compiled_fee_pending_wide_rounding(){compare("wide","fees");}
#[test] fn compiled_fee_pending_full_rounding(){compare("full","fees");}
