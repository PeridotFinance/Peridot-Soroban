//! Controlled compiled research: no production policy/stage adoption.
use super::*;
const SETTLE: &str = "settle_stalled_funded_v3";

fn settle(f: &F, payer: &Address, id: u64, max: u128, min_t: u128, min_u: u128) -> (u128,u128,u128,u128) {
    let e = &f.e;
    let transfers = [MockAuthInvoke { contract: &f.under, fn_name: "transfer",
        args: (payer.clone(),f.cash.clone(),max as i128).into_val(e), sub_invokes: &[] }];
    let repay = [MockAuthInvoke { contract: &f.cash, fn_name: "repay_full_for_margin",
        args: (id,payer.clone(),max).into_val(e), sub_invokes: &transfers }];
    auth(e,payer,&f.margin,SETTLE,(payer.clone(),id,max,min_t,min_u).into_val(e), if max > 0 { &repay } else { &[] })
}

fn guard() {
    use sha2::{Digest,Sha256};
    let build = std::path::PathBuf::from(std::env::var("CL_ATOMIC_BUILD_DIR").unwrap());
    let m:serde_json::Value=serde_json::from_slice(&std::fs::read(build.join("build-manifest.json")).unwrap()).unwrap();
    assert_eq!(m["qualification_variant"],"option1-stalled-funded-recovery-unadopted");
    assert_eq!(m["closing_overlay_sha256"],format!("{:x}",Sha256::digest(include_bytes!("closing-recovery.patch"))));
    assert_eq!(m["margin_build_feature"],"funded-liquidation-validation");
    assert_eq!(m["release_approval"],false);
    for name in ["margin_controller.wasm","receipt_vault.wasm","simple_peridottroller.wasm"] {
        let b=std::fs::read(build.join("artifacts").join(name)).unwrap();
        assert_eq!(m["wasm"][name]["sha256"],format!("{:x}",Sha256::digest(&b)));
    }
}

fn reject_typed(f:&F,payer:&Address,id:u64,max:u128,min_u:u128,code:u32,label:&str) {
    let e=&f.e;
    let args:Vec<Val>=(payer.clone(),id,max,0u128,min_u).into_val(e);
    e.mock_auths(&[MockAuth{address:payer,invoke:&MockAuthInvoke{contract:&f.margin,fn_name:SETTLE,args:args.clone(),sub_invokes:&[]}}]);
    crate::cl_resource_limits::reset(e);
    let before=e.to_ledger_snapshot();
    #[cfg(feature="transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(e,&f.margin,SETTLE,&args,Some(payer),false,label);
    crate::cl_resource_limits::reset(e);
    let r=e.try_invoke_contract::<Val,soroban_sdk::Error>(&f.margin,&Symbol::new(e,SETTLE),args);
    assert!(r.is_err());
    let events=e.host().get_diagnostic_events().unwrap();
    assert!(format!("{:?}",events).contains(&format!("Contract({code})")),"wrong rejection: {:?}",events);
    assert_eq!(before.ledger_entries,e.to_ledger_snapshot().ledger_entries,"rejection ledger/TTL rollback");
    // Proof outcome PASS only after typed rejection and full rollback above.
    // Raw unsigned invocation remains expected_rejection=true and failed.
    measure(e,label,true);
}

#[test]
fn compiled_option1_closing_and_liquidated_six_states_independent_full_settlement() {
    guard();
    let (f,owners,mut ids,selected)=fixture(true,true);
    // One additional real active position supplies NONZERO contingent close
    // fees for the live converted-close rejection. Existing eight positions
    // and their historical entitlements remain untouched. All setup calls
    // enforce the captured profile; their costs are explicitly setup-only.
    auth::<()>(&f.e,&f.admin,&f.margin,"set_close_fee_bps",(f.admin.clone(),100u128).into_val(&f.e),&[]);
    let fee_id=begin_for(&f,&owners[2]);
    auth::<()>(&f.e,&owners[2],&f.margin,"swap_open_position_v3",(owners[2].clone(),fee_id).into_val(&f.e),&[]);
    auth::<()>(&f.e,&owners[2],&f.margin,"activate_open_position_v3",(owners[2].clone(),fee_id).into_val(&f.e),&[]);
    ids[1]=fee_id;
    std::println!("STALLED_NONZERO_FEE_FIXTURE initial_borrowers=8 extra_active_position=1 close_fee_bps=100 setup_only=true");
    let (f,owners)=rebase(f,owners);
    let e=&f.e;
    let payer=fresh_liquidator(&f,&owners);
    auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),payer.clone(),300_000_000i128).into_val(e),&[]);
    enable_measurement(&f,&selected);
    for n in 0..5 {
        let owner=&owners[n+1];let id=ids[n];
        // Real transitions, no private storage injection for the recovery stage.
        if n < 2 || n == 4 {
            assert!(call::<u128>(e,&f.share,"refresh_nav_root",().into_val(e))>0);
            measure(e,"option1_stalled_refresh_nav",true);
            auth::<()>(e,owner,&f.margin,"prepare_close_position_v3",(owner.clone(),id).into_val(e),&[]);
            measure(e,"option1_stalled_real_prepare_close",true);
            if n == 1 || n == 4 {
                let p:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_pending_perps_close",(id,).into_val(e));
                let held:u128=field(e,&p.unwrap(),"collateral_underlying");
                let quote:u128=inspect(&f,&f.facade,"estimate_swap",(1u32,0u32,held).into_val(e));
                auth::<()>(e,owner,&f.margin,"swap_close_position_v3",(owner.clone(),id,quote).into_val(e),&[]);
                measure(e,"option1_stalled_real_swap_close",true);
            }
            let p:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_pending_perps_close",(id,).into_val(e));
            let expiry:u64=field(e,&p.unwrap(),"expires_at");
            if n==1 {
                let terms:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_position_fee_terms",(id,).into_val(e));
                assert!(field::<u128>(e,terms.as_ref().unwrap(),"close_fee_underlying")>0);
                let debt:u128=inspect(&f,&f.cash,"get_margin_borrow_balance",(id,).into_val(e));
                let reward=holder(&f,owner);
                let pending:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_pending_perps_close",(id,).into_val(e));
                reject_typed(&f,&payer,id,0,0,9102,"option1_stalled_live_converted_close_reject");
                assert_eq!(inspect::<Option<Map<Symbol,Val>>>(&f,&f.margin,"get_position_fee_terms",(id,).into_val(e)),terms);
                assert_eq!(inspect::<Option<Map<Symbol,Val>>>(&f,&f.margin,"get_pending_perps_close",(id,).into_val(e)),pending);
                assert_eq!(inspect::<u128>(&f,&f.cash,"get_margin_borrow_balance",(id,).into_val(e)),debt);
                assert_eq!(holder(&f,owner),reward);
            }
            advance(&f,expiry+1);
            if n == 4 {
                // Deliberately artificial long timestamp horizon, with only20
                // ledger ticks: isolate actual accrued residual-debt semantics,
                // not realistic latency, archival or keeper economics.
                advance(&f,expiry+30*365*24*60*60);
                call::<()>(e,&f.margin,"finish_close_position_v3",(id,).into_val(e));
                measure(e,"option1_stalled_real_partial_finish",true);
                let p:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_pending_perps_close",(id,).into_val(e));
                assert_eq!(field::<u128>(e,&p.unwrap(),"received_debt_asset"),0);
                assert!(inspect::<u128>(&f,&f.cash,"get_margin_borrow_balance",(id,).into_val(e))>0);
                assert!(inspect::<Option<Map<Symbol,Val>>>(&f,&f.margin,"get_position",(id,).into_val(e)).is_some());
                // Fresh test prices after artificial timestamp advance. No
                // cached-price bypass in the actual settlement implementation.
                for asset in [&f.under,&f.share] {
                    auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(asset.clone()),100_000_000_000_000i128).into_val(e),&[]);
                    measure(e,"option1_partial_residual_fresh_test_price",true);
                }
            }
        } else {
            auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(f.share.clone()),45_000_000_000_000i128).into_val(e),&[]);
            measure(e,"option1_stalled_test_unhealthy_price",true);
            auth::<()>(e,&payer,&f.margin,"begin_liquidation_v3",(payer.clone(),id).into_val(e),&[]);
            measure(e,"option1_stalled_real_begin_liquidation",true);
            if n==3 {
                assert!(call::<u128>(e,&f.share,"refresh_nav_root",().into_val(e))>0);
                measure(e,"option1_stalled_liquidation_refresh_nav",true);
                let quote:Map<Symbol,Val>=inspect(&f,&f.margin,"preview_liquidation_v3",(id,).into_val(e));
                let min:u128=field(e,&quote,"pool_estimated_out");
                auth::<()>(e,&payer,&f.margin,"swap_liquidation_v3",(payer.clone(),id,min).into_val(e),&[]);
                measure(e,"option1_stalled_real_swap_liquidation",true);
            }
        }
        auth::<()>(e,&f.admin,&f.cash,"set_cash_borrowing_enabled",(f.admin.clone(),false).into_val(e),&[]);
        measure(e,"option1_stalled_borrow_disabled",true);
        let booked=holder(&f,owner);
        let saved=fault(&f);prove_both_calls_fail(&f);
        if n==0 {reject_typed(&f,&payer,id,MAX_REPAY,u128::MAX,9101,"option1_stalled_impossible_seizure_rollback");}
        let wallet=balance(&f,&f.under,&payer);
        let max=if n==4 {150_000_000}else if n==0 || n==2 {MAX_REPAY}else{0};
        let r=settle(&f,&payer,id,max,if n==2{1}else{0},if n==0{1}else{0});
        measure(e,match n {0=>"option1_closing_withdrawn_funded",1=>"option1_closing_converted_proceeds",2=>"option1_liquidated_started_funded",3=>"option1_liquidated_converted_proceeds",_=>"option1_closing_partial_finish_residual_funded"},true);
        assert_eq!(wallet-balance(&f,&f.under,&payer),r.1 as i128);
        if max>0 {assert!(r.1>0&&r.1<max);} else {assert!(r.0>0);assert_eq!(r.1,0);assert_eq!(r.2,0);assert_eq!(r.3,0);}
        if n==4 {assert_eq!((r.0,r.2,r.3),(0,0,0),"no phantom collateral after partial finish");}
        debt_zero(&f,id);
        assert!(inspect::<Option<Map<Symbol,Val>>>(&f,&f.margin,"get_position",(id,).into_val(e)).is_none());
        assert_eq!(holder(&f,owner),booked,"booked rewards survive forced settlement");
        restore(&f,saved);
        assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));
        measure(e,"option1_stalled_late_quarantine",true);
        conserve(&f,&owners,Some(&payer),"five_state_recovery");
        std::println!("STALLED_STATE_SETTLED case={n} id={id} proceeds={} external={} seized_ptokens={} seized_underlying={} no_borrower_authorization=true",r.0,r.1,r.2,r.3);
    }

    // Sixth reachable asset shape: liquidation inherits actual underlying from
    // a real withdrawn Closing, then a DIFFERENT payer waits out exclusivity.
    let owner=&owners[6];let id=ids[5];
    assert!(call::<u128>(e,&f.share,"refresh_nav_root",().into_val(e))>0);
    measure(e,"option1_inherited_close_refresh_nav",true);
    auth::<()>(e,owner,&f.margin,"prepare_close_position_v3",(owner.clone(),id).into_val(e),&[]);
    measure(e,"option1_inherited_real_prepare_close",true);
    let held:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_pending_perps_close",(id,).into_val(e));
    assert!(field::<u128>(e,&held.unwrap(),"collateral_underlying")>0);
    auth::<()>(e,&f.admin,&f.oracle,"set_test_price",(f.admin.clone(),Asset::Stellar(f.share.clone()),45_000_000_000_000i128).into_val(e),&[]);
    measure(e,"option1_inherited_unhealthy_test_price",true);
    auth::<()>(e,&payer,&f.margin,"begin_liquidation_v3",(payer.clone(),id).into_val(e),&[]);
    measure(e,"option1_inherited_real_begin_liquidation",true);
    let position:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_position",(id,).into_val(e));
    assert_eq!(field::<u128>(e,&position.unwrap(),"collateral_ptokens"),0);
    let deadline:Option<u64>=inspect(&f,&f.margin,"get_liquidation_takeover_after",(id,).into_val(e));
    let deadline=deadline.unwrap();assert!(e.ledger().timestamp()<deadline);
    let mut beneficiaries=owners.clone();beneficiaries.push(payer.clone());
    let successor=fresh_liquidator(&f,&beneficiaries);assert_ne!(successor,payer);
    auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),successor.clone(),200_000_000i128).into_val(e),&[]);
    measure(e,"option1_inherited_successor_cash_funding",true);
    let booked=holder(&f,owner);let saved=fault(&f);prove_both_calls_fail(&f);
    let args:Vec<Val>=(successor.clone(),id,150_000_000u128,0u128,1u128).into_val(e);
    let spend=[MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(successor.clone(),f.cash.clone(),150_000_000i128).into_val(e),sub_invokes:&[]}];
    let repayment=[MockAuthInvoke{contract:&f.cash,fn_name:"repay_full_for_margin",args:(id,successor.clone(),150_000_000u128).into_val(e),sub_invokes:&spend}];
    e.mock_auths(&[MockAuth{address:&successor,invoke:&MockAuthInvoke{contract:&f.margin,fn_name:SETTLE,args:args.clone(),sub_invokes:&repayment}}]);
    crate::cl_resource_limits::reset(e);let before=e.to_ledger_snapshot();
    #[cfg(feature="transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(e,&f.margin,SETTLE,&args,Some(&successor),false,"stalled_liquidator_exclusivity");
    crate::cl_resource_limits::reset(e);
    let rejected=e.try_invoke_contract::<Val,soroban_sdk::Error>(&f.margin,&Symbol::new(e,SETTLE),args);
    // The legacy assertion compiles to a generic WASM panic. This is a real
    // root business-state rejection, NOT a setup or account-signature proof;
    // the paired success below uses the same complete narrow auth tree and
    // arguments; only the ledger timestamp/deadline eligibility changes.
    assert!(rejected.is_err(), "live successor must fail before takeover deadline");
    assert_eq!(before.ledger_entries,e.to_ledger_snapshot().ledger_entries);
    measure(e,"option1_inherited_early_takeover_reject",true);
    advance(&f,deadline+1);
    let wallet=balance(&f,&f.under,&successor);
    let result=settle(&f,&successor,id,150_000_000,0,1);
    measure(e,"option1_liquidated_inherited_underlying_successor_funded",true);
    assert!(result.1>0&&result.1<150_000_000&&result.3>0);
    assert_eq!((result.0,result.2),(0,0));
    assert_eq!(wallet-balance(&f,&f.under,&successor),result.1 as i128);
    debt_zero(&f,id);
    assert!(inspect::<Option<Map<Symbol,Val>>>(&f,&f.margin,"get_position",(id,).into_val(e)).is_none());
    assert_eq!(holder(&f,owner),booked);
    restore(&f,saved);assert!(call::<bool>(e,&f.share,"checkpoint_rewards",().into_val(e)));
    measure(e,"option1_inherited_late_quarantine",true);
    conserve(&f,&beneficiaries,Some(&successor),"six_state_recovery");
    std::println!("STALLED_STATE_SETTLED case=5 id={id} proceeds={} external={} seized_ptokens={} seized_underlying={} different_successor=true no_borrower_authorization=true",result.0,result.1,result.2,result.3);
}
