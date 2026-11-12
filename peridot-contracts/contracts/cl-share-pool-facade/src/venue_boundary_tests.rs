//! Read pending at the actual claim-first ownership boundary, not afterwards.
use super::*;
#[contracttype]
#[derive(Clone)]
struct Slot0 { sqrt_price_x96:soroban_sdk::U256, tick:i32 }
fn move_shares(f:&F,from:&Address,to:&Address,n:i128,label:&str) {
    let e=&f.e;
    let pending:u128=call(e,&f.pool,"get_user_reward",(f.share.clone(),).into_val(e));
    let before:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));
    auth::<()>(e,from,&f.share,"transfer",(from.clone(),to.clone(),n).into_val(e),&[]);
    measure(e,label,true);
    let after:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));
    assert_eq!(pending,after.received-before.received,"pending estimate differs at {label}");
    std::println!("VENUE_BOUNDARY {label} timestamp={} pending={pending} claimed={} supply_ownership_change={n}",e.ledger().timestamp(),after.received-before.received);
}
fn advance(e:&Env,seconds:u64) { e.ledger().with_mut(|l|{l.timestamp+=seconds;l.sequence_number+=1;}); }
fn pool_shift(f:&F,n:u128,reverse:bool) {
    let e=&f.e;let (i,j,t)=if reverse{(1u32,0u32,&f.paired)}else{(0u32,1u32,&f.under)};
    let quote:u128=call(e,&f.pool,"estimate_swap",(i,j,n).into_val(e));
    let child=MockAuthInvoke{contract:t,fn_name:"transfer",args:(f.user.clone(),f.pool.clone(),n as i128).into_val(e),sub_invokes:&[]};
    let actual:u128=auth(e,&f.user,&f.pool,"swap",(f.user.clone(),i,j,n,quote).into_val(e),&[child]);
    assert_eq!(actual,quote);
}
#[test]
fn compiled_pending_claim_boundary_matrix() {
    let f=F::new();let e=&f.e;let bob=Address::generate(e);let carol=Address::generate(e);
    let child=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(f.user.clone(),f.share.clone(),100_000_003i128).into_val(e),sub_invokes:&[]};
    let minted:i128=auth(e,&f.user,&f.share,"deposit_underlying",(f.user.clone(),100_000_003i128,1i128).into_val(e),&[child]);
    auth::<()>(e,&f.user,&f.reward,"transfer",(f.user.clone(),f.pool.clone(),100_000_000i128).into_val(e),&[]);
    auth::<()>(e,&f.admin,&f.pool,"set_rewards_config",(f.admin.clone(),3000u64,10_003u128).into_val(e),&[]);
    advance(e,13);
    move_shares(&f,&f.user,&bob,minted/3,"pending_boundary_fractional_first_transfer");
    advance(e,7);
    auth::<()>(e,&f.admin,&f.pool,"set_rewards_config",(f.admin.clone(),3001u64,90_007u128).into_val(e),&[]);
    advance(e,19);
    move_shares(&f,&bob,&carol,minted/9,"pending_boundary_emission_change");
    move_shares(&f,&carol,&bob,minted/18,"pending_boundary_same_ledger_zero_claim");
    let original_tick:Slot0=call(e,&f.pool,"get_slot0",().into_val(e));
    pool_shift(&f,5_000_000_000,false);
    let shifted:Slot0=call(e,&f.pool,"get_slot0",().into_val(e));
    assert!(shifted.tick < -40 || shifted.tick >= 40,"must leave actual vault range");
    std::println!("VENUE_RANGE_CROSS original={} shifted={}",original_tick.tick,shifted.tick);
    advance(e,17);
    move_shares(&f,&f.user,&carol,minted/7,"pending_boundary_shifted_range");
    pool_shift(&f,4_900_000_000,true);
    let returned:Slot0=call(e,&f.pool,"get_slot0",().into_val(e));
    assert!(returned.tick >= -40 && returned.tick < 40,"must return to actual vault range");
    std::println!("VENUE_RANGE_RETURN tick={}",returned.tick);
    advance(e,11);
    move_shares(&f,&bob,&f.user,minted/13,"pending_boundary_returned_range");
    let pending:u128=call(e,&f.pool,"get_user_reward",(f.share.clone(),).into_val(e));
    let before:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));
    let out:Vec<u128>=auth(e,&carol,&f.share,"emergency_redeem",(carol.clone(),(minted/23) as u128,false).into_val(e),&[]);
    measure(e,"pending_boundary_partial_exit",true);
    assert!(out.get(0).unwrap()+out.get(1).unwrap()>0);
    let after:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));
    assert_eq!(pending,after.received-before.received);
    advance(e,23);
    move_shares(&f,&f.user,&bob,minted/17,"pending_boundary_after_partial_exit");
    let mut pending_sum=0u128;let mut fractions=0u128;
    for holder in [&f.user,&bob,&carol] {
        let h:RewardHolder=call(e,&f.share,"reward_account",(holder.clone(),).into_val(e));
        pending_sum+=h.pending;fractions+=h.fraction;
    }
    let total:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));
    assert_eq!(total.received*1_000_000_000_000_000_000,(pending_sum+total.paid+total.quarantine)*1_000_000_000_000_000_000+fractions+total.rounding_scaled);
    std::println!("VENUE_MATRIX_CONSERVATION received={} pending={pending_sum} fractions={fractions} rounding={} quarantined={}",total.received,total.rounding_scaled,total.quarantine);
}
