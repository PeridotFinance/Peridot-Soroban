// Appended to isolated cl-share-vault/contract.rs only.
#[soroban_sdk::contracttype]
#[derive(Clone)]
enum StageVaultKey { Receipt, Allocation(u64) }
#[soroban_sdk::contracttype]
#[derive(Clone,Debug)]
pub struct StageAllocation {
    pub supply:u128,pub shares:u128,pub liquidity_burn:u128,
    pub fee0:u128,pub fee1:u128,pub principal0:u128,pub principal1:u128,
    pub allocated0:u128,pub allocated1:u128,pub retained0:u128,pub retained1:u128,
}
fn stage_raw(e:&Env,t:&Address)->u128 {to_u128(token::Client::new(e,t).balance(&e.current_contract_address()))}
fn stage_job(e:&Env,r:&Address,_id:u64,phase:u32,j:StageJob)->StageJob {
    assert_eq!(*r,e.storage().instance().get::<_,Address>(&StageVaultKey::Receipt).expect("stage unbound"));r.require_auth();
    assert_eq!(j.phase,phase);j
}
fn stage_pay(e:&Env,t:&Address,to:&Address,n:u128) {if n>0 {token::Client::new(e,t).transfer(&e.current_contract_address(),to,&to_i128(n));}}
#[contractimpl]
impl ClShareVault {
    pub fn stage_bind(e:Env,admin_addr:Address,receipt:Address) {
        Self::require_admin(&e,&admin_addr);crate::require_validation_network(&e);
        assert!(state(&e).total_shares==0 && !e.storage().instance().has(&StageVaultKey::Receipt));
        assert_eq!(Self::ticks(&e),(-2000,2000),"prototype fixed simple wide range");
        assert_eq!(config(&e).underlying_index,0);assert_eq!(config(&e).dec0,config(&e).dec1);
        e.storage().instance().set(&StageVaultKey::Receipt,&receipt);
    }
    fn stage_root(e:&Env)->u128 {
        let c=config(e);let prm=params(e);
        let pu=Self::read_price(e,&c.oracle,&c.token0,prm.oracle_max_age_mult).expect("fresh token0 price required");
        let po=Self::read_price(e,&c.oracle,&c.token1,prm.oracle_max_age_mult).expect("fresh token1 price required");
        let root=isqrt(mul_div(e,po,NAV_RATIO_SCALE,pu));assert!(root>0);
        let mut st=state(e);st.last_nav_root=root;st.last_nav_root_at=e.ledger().timestamp();set_state(e,&st);root
    }
    fn stage_band(e:&Env,root:u128) {
        let slot=ConcentratedPoolClient::new(e,&Self::pool(e)).get_slot0();
        let q=soroban_sdk::U256::from_u128(e,1u128<<96);
        let s=slot.sqrt_price_x96.mul(&soroban_sdk::U256::from_u128(e,1_000_000_000_000_000_000)).div(&q).to_u128().unwrap();
        let fair=mul_div(e,1_000_000_000_000_000_000,1_000_000_000,root);
        let b=params(e).max_pool_divergence_bps;assert!(b>0 && b<=500);
        let spot=s.checked_mul(s).unwrap();let fair_price=fair.checked_mul(fair).unwrap();
        let lower=apply_slippage_floor(e,fair_price,b);let upper=mul_div(e,fair_price,10_000+b as u128,10_000);
        assert!(spot>=lower && spot<=upper,"fresh pool/oracle band");
    }
    // Independent oracle-price CL value, not a current-spot mark that hides
    // price-reversion cost. Fixed wide-range sqrt constants are rounded inward.
    fn stage_fair_lp(e:&Env,l:u128,root:u128)->u128 {
        let q=1_000_000_000_000_000_000u128;
        let lo=904_841_941_932_768_879u128;let hi=1_105_165_392_603_232_697u128;
        let s=mul_div(e,q,1_000_000_000,root).max(lo).min(hi);
        let u=|n|soroban_sdk::U256::from_u128(e,n);
        let a=u(l).mul(&u(q)).mul(&u(hi-s)).div(&u(s).mul(&u(hi))).to_u128().unwrap();
        let b=mul_div(e,l,s-lo,q);
        a.checked_add(Self::other_amount_value_at_root(e,b,root)).unwrap()
    }
    fn stage_fair_nav(e:&Env,root:u128)->u128 {
        let l=Self::position_liquidity(e);
        let fees=if l>0 {ConcentratedPoolClient::new(e,&Self::pool(e)).get_all_position_fees(&e.current_contract_address())}else{soroban_sdk::vec![e,0,0]};
        Self::stage_fair_lp(e,l,root).checked_add(Self::balance_of_token(e,&Self::token(e,0))).unwrap()
            .checked_add(Self::other_amount_value_at_root(e,Self::balance_of_token(e,&Self::token(e,1)),root)).unwrap()
            .checked_add(fees.get(0).unwrap()).unwrap().checked_add(Self::other_amount_value_at_root(e,fees.get(1).unwrap(),root)).unwrap()
    }
    // Current estimate_withdraw_position includes accrued LP fees (part-a proof).
    // Call only after a fresh position snapshot; do not add pending fees twice.
    fn stage_actual_nav(e:&Env,root:u128)->u128 {
        Self::position_value_at_root(e,root,false)
            .checked_add(Self::balance_of_token(e,&Self::token(e,0))).unwrap()
            .checked_add(Self::other_amount_value_at_root(e,Self::balance_of_token(e,&Self::token(e,1)),root)).unwrap()
    }
    pub fn stage_check_open(e:Env,r:Address,id:u64,j:StageJob) {
        let j=stage_job(&e,&r,id,0,j);Self::require_not_paused(&e);
        assert!(j.kind==0 && j.a>0 && params(&e).max_deploy>0 && j.a<=params(&e).max_deploy);
        let root=Self::stage_root(&e);Self::stage_band(&e,root);
    }
    pub fn stage_accept_funds(e:Env,r:Address,id:u64,j:StageJob) {
        let j=stage_job(&e,&r,id,0,j);j.owner.require_auth();Self::require_not_paused(&e);
        crate::accounting::begin(&e);let t=Self::token(&e,0);let before=stage_raw(&e,&t);
        let n=j.a.checked_add(j.reserve).unwrap();
        token::Client::new(&e,&t).transfer(&j.owner,&e.current_contract_address(),&to_i128(n));
        assert_eq!(stage_raw(&e,&t)-before,n);crate::stage_storage::adjust(&e,&t,n,0);crate::accounting::end(&e);
    }
    fn stage_swap(e:&Env,i:u32,n:u128,min:u128)->u128 {
        let cfg=config(e);Self::require_not_paused(e);let root=Self::stage_root(e);Self::stage_band(e,root);
        let pool=ConcentratedPoolClient::new(e,&cfg.pool);assert!(!pool.get_is_killed_swap());
        let estimate=pool.estimate_swap(&i,&(1-i),&n);
        Self::require_quote_near_oracle(e,i,n,estimate,Some(root),params(e).max_pool_divergence_bps,true);
        let floor=apply_slippage_floor(e,estimate,Self::slippage_bps(e)).max(min);assert!(estimate>=floor && floor>0);
        let t=Self::token(e,i);let other=Self::token(e,1-i);let before=stage_raw(e,&t);let out_before=stage_raw(e,&other);
        e.authorize_as_current_contract(soroban_sdk::vec![e,auth_entry(e,&t,"transfer",(e.current_contract_address(),cfg.pool.clone(),to_i128(n)).into_val(e),Vec::new(e))]);
        let out=pool.swap(&e.current_contract_address(),&i,&(1-i),&n,&floor);
        assert_eq!(before-stage_raw(e,&t),n);assert_eq!(stage_raw(e,&other)-out_before,out);assert!(out>=floor);out
    }
    pub fn stage_convert_open(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,1,j);crate::accounting::begin(&e);
        let out=Self::stage_swap(&e,0,j.swap_in,j.min_swap);
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,j.swap_in);
        crate::stage_storage::adjust(&e,&Self::token(&e,1),out,0);crate::accounting::end(&e);out
    }
    pub fn stage_cash_mint(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,2,j);Self::require_not_paused(&e);crate::custody::require_direct_holder(&e,&j.owner);
        crate::accounting::begin(&e);crate::rewards::normal(&e,&j.owner);
        let root=Self::stage_root(&e);Self::stage_band(&e,root);Self::collect_fees(&e);
        let before=Self::stage_actual_nav(&e,root);let supply=Self::total_shares(&e);
        let added=j.a.checked_add(Self::other_amount_value_at_root(&e,j.b,root)).unwrap();
        let cap=params(&e).max_deploy;assert!(cap>0 && before.checked_add(added).unwrap()<=cap,"current exposure cap");
        let shares=if supply==0 {assert_eq!(before,0);added}else{mul_div(&e,added,supply,before)};
        assert!(shares>0 && shares>=j.min_shares);
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,j.a);
        crate::stage_storage::adjust(&e,&Self::token(&e,1),0,j.b);
        Self::mint_shares(&e,&j.owner,shares);crate::stage_storage::lock(&e,&j.owner,shares);
        crate::rewards::reset_debt(&e,&j.owner);crate::accounting::end(&e);shares
    }
    pub fn stage_invest_cash(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,3,j);Self::require_not_paused(&e);crate::accounting::begin(&e);
        assert_eq!(Self::ticks(&e),(-2000,2000));
        let root=Self::stage_root(&e);Self::stage_band(&e,root);
        let p=ConcentratedPoolClient::new(&e,&Self::pool(&e));assert!(!p.get_is_killed_deposit());
        assert!(Self::refresh_position_amounts_inner(&e));let before=Self::stage_fair_nav(&e,root);
        let actual_before=Self::stage_actual_nav(&e,root);
        assert!(params(&e).max_deploy>0 && actual_before<=params(&e).max_deploy);
        let a=Self::balance_of_token(&e,&Self::token(&e,0));let b=Self::balance_of_token(&e,&Self::token(&e,1));
        assert!(a>=j.a && b>=j.b,"command amount no longer available; cancel or retry");
        let result=Self::deposit_balances(&e,j.a,j.b,a,b);assert!(result.minted>0,"investment unavailable");
        assert!(Self::refresh_position_amounts_inner(&e));let after=Self::stage_fair_nav(&e,root);
        assert!(after<=before.checked_add(4).unwrap(),"unexplained investment gain");
        let cost=before.saturating_sub(after);assert!(cost<=j.reserve,"execution reserve insufficient");
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,j.reserve);
        stage_pay(&e,&Self::token(&e,0),&j.owner,j.reserve-cost);
        assert!(Self::stage_fair_nav(&e,root)>=before,"incumbents must not fund investment cost");
        assert!(Self::stage_actual_nav(&e,root)>=actual_before,"actual backing must also be preserved");
        crate::stage_storage::unlock(&e,&j.owner,j.shares);crate::accounting::end(&e);result.minted
    }
    pub fn stage_lock_close(e:Env,r:Address,id:u64,j:StageJob) {
        let j=stage_job(&e,&r,id,10,j);crate::custody::require_direct_holder(&e,&j.owner);
        assert!(j.kind==1 && j.shares>0 && j.shares<=crate::stage_storage::available(&e,&j.owner));
        crate::stage_storage::lock(&e,&j.owner,j.shares);
    }
    pub fn stage_allocation(e:Env,id:u64)->StageAllocation {e.storage().persistent().get(&StageVaultKey::Allocation(id)).unwrap()}
    pub fn stage_withdraw_kind(e:Env,r:Address,id:u64,j:StageJob)->Vec<u128> {
        let j=stage_job(&e,&r,id,10,j);crate::accounting::begin(&e);
        crate::custody::require_direct_holder(&e,&j.owner);crate::rewards::normal(&e,&j.owner);
        let supply=Self::total_shares(&e);let shares=j.shares;assert!(shares>0 && shares<=supply && shares<=shares_of(&e,&j.owner));
        let l=Self::position_liquidity(&e);let burn=mul_div(&e,l,shares,supply);
        let c=config(&e);let me=e.current_contract_address();let pool=ConcentratedPoolClient::new(&e,&c.pool);
        let fees=if l>0 {pool.get_all_position_fees(&me)}else{soroban_sdk::vec![&e,0u128,0u128]};
        let idle0=Self::balance_of_token(&e,&c.token0);let idle1=Self::balance_of_token(&e,&c.token1);
        let f0=fees.get(0).unwrap();let f1=fees.get(1).unwrap();
        let a0=mul_div(&e,idle0.checked_add(f0).unwrap(),shares,supply);
        let a1=mul_div(&e,idle1.checked_add(f1).unwrap(),shares,supply);
        let retained0=idle0+f0-a0;let retained1=idle1+f1-a1;
        let before0=stage_raw(&e,&c.token0);let before1=stage_raw(&e,&c.token1);
        let received=if burn>0 {
            pool.withdraw_position(&me,&c.tick_lower,&c.tick_upper,&burn,&soroban_sdk::vec![&e,0u128,0u128])
        } else if l>0 {pool.claim_all_position_fees(&me)}else{soroban_sdk::vec![&e,0u128,0u128]};
        assert_eq!(stage_raw(&e,&c.token0)-before0,received.get(0).unwrap());
        assert_eq!(stage_raw(&e,&c.token1)-before1,received.get(1).unwrap());
        let p0=received.get(0).unwrap().checked_sub(f0).expect("exact fee0 reconciliation");
        let p1=received.get(1).unwrap().checked_sub(f1).expect("exact fee1 reconciliation");
        let out0=p0.checked_add(a0).unwrap();let out1=p1.checked_add(a1).unwrap();
        crate::stage_storage::adjust(&e,&c.token0,out0,0);crate::stage_storage::adjust(&e,&c.token1,out1,0);
        // NAMED RULE: INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN.
        // Fees are allocated against OLD supply, exit liabilities fully funded,
        // and incumbent backing checked before any vault shares are burned.
        assert_eq!(Self::balance_of_token(&e,&c.token0),retained0,"INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN token0");
        assert_eq!(Self::balance_of_token(&e,&c.token1),retained1,"INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN token1");
        let allocation=StageAllocation{supply,shares,liquidity_burn:burn,fee0:f0,fee1:f1,principal0:p0,principal1:p1,allocated0:a0,allocated1:a1,retained0,retained1};
        let k=StageVaultKey::Allocation(id);e.storage().persistent().set(&k,&allocation);e.storage().persistent().extend_ttl(&k,500_000,1_000_000);
        e.events().publish((Symbol::new(&e,"feealloc"),id),allocation);
        if j.min_swap==0 {
            // Direct in-kind close completes in this transaction, not an
            // omitted prerequisite/payout transaction.
            crate::stage_storage::adjust(&e,&c.token0,0,out0);crate::stage_storage::adjust(&e,&c.token1,0,out1);
            stage_pay(&e,&c.token0,&j.owner,out0);stage_pay(&e,&c.token1,&j.owner,out1);
            assert_eq!(Self::balance_of_token(&e,&c.token0),retained0);
            assert_eq!(Self::balance_of_token(&e,&c.token1),retained1);
        }
        crate::stage_storage::unlock(&e,&j.owner,shares);Self::burn_shares(&e,&j.owner,shares);
        e.events().publish((Symbol::new(&e,"stageburn"),id),(shares,supply,Self::total_shares(&e)));
        crate::rewards::reset_debt(&e,&j.owner);Self::set_position_liquidity(&e,l-burn);
        assert!(Self::refresh_position_amounts_inner(&e));crate::accounting::end(&e);
        soroban_sdk::vec![&e,out0,out1]
    }
    pub fn stage_convert_exit(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,11,j);crate::accounting::begin(&e);
        let converted=if j.b>0 {Self::stage_swap(&e,1,j.b,j.min_swap)}else{0};
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,j.a);
        crate::stage_storage::adjust(&e,&Self::token(&e,1),0,j.b);
        let total=j.a.checked_add(converted).unwrap();stage_pay(&e,&Self::token(&e,0),&j.owner,total);
        crate::accounting::end(&e);total
    }
    pub fn stage_cancel_kind(e:Env,r:Address,id:u64,j:StageJob) {
        let bound:Address=e.storage().instance().get(&StageVaultKey::Receipt).unwrap();assert_eq!(bound,r);r.require_auth();
        let _ = id; // Receipt passes its stored command under real contract authorization; no reentry callback.
        crate::accounting::begin(&e);
        let (a,b)=match j.phase {
            0=>(0,0),1|2=>(j.a.checked_add(j.reserve).unwrap(),j.b),
            3=>{crate::stage_storage::unlock(&e,&j.owner,j.shares);(j.reserve,0)},
            10=>{crate::stage_storage::unlock(&e,&j.owner,j.shares);(0,0)},
            11=>(j.a,j.b),_=>panic!("terminal command"),
        };
        for (t,n) in [(Self::token(&e,0),a),(Self::token(&e,1),b)] {
            crate::stage_storage::adjust(&e,&t,0,n);stage_pay(&e,&t,&j.owner,n);
        }
        crate::accounting::end(&e);
    }
}
