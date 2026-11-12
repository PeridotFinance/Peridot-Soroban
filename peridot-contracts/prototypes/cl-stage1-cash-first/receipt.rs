// Appended only to an isolated copy of receipt-vault/contract.rs.
#[soroban_sdk::contracttype]
#[derive(Clone)]
enum StageReceiptKey { Vault, Next, Job(u64), Busy }
fn stage_receipt_vault(e:&Env)->Address {
    ensure_initialized_for_snapshot(e);
    e.storage().instance().get(&StageReceiptKey::Vault).expect("stage not bound")
}
fn stage_load(e:&Env,id:u64)->StageJob {
    let k=StageReceiptKey::Job(id);
    let j=e.storage().persistent().get(&k).expect("unknown stage command");
    e.storage().persistent().extend_ttl(&k,500_000,1_000_000);j
}
fn stage_save(e:&Env,id:u64,j:&StageJob) {
    let k=StageReceiptKey::Job(id);e.storage().persistent().set(&k,j);
    e.storage().persistent().extend_ttl(&k,500_000,1_000_000);
}
fn stage_enter(e:&Env) {
    assert!(!e.storage().instance().get(&StageReceiptKey::Busy).unwrap_or(false),"stage reentry");
    e.storage().instance().set(&StageReceiptKey::Busy,&true);
}
fn stage_leave(e:&Env){e.storage().instance().set(&StageReceiptKey::Busy,&false);}
fn stage_invoke<T:soroban_sdk::TryFromVal<Env,Val>>(e:&Env,v:&Address,name:&str,args:Vec<Val>)->T {
    e.invoke_contract(v,&Symbol::new(e,name),args)
}
#[contractimpl]
impl ReceiptVault {
    pub fn stage_bind(e:Env,admin:Address,vault:Address) {
        admin.require_auth();assert_eq!(admin,Self::get_admin(e.clone()));
        assert!(!e.storage().instance().has(&StageReceiptKey::Vault),"immutable stage binding");
        e.storage().instance().set(&StageReceiptKey::Vault,&vault);
    }
    pub fn stage_job(e:Env,id:u64)->StageJob {stage_load(&e,id)}
    pub fn stage_prepare(e:Env,owner:Address,amount:u128,swap_in:u128,min_swap:u128,min_shares:u128,reserve:u128,deadline:u64)->u64 {
        stage_enter(&e);owner.require_auth();let v=stage_receipt_vault(&e);
        assert!(amount>0 && swap_in>0 && swap_in<amount && min_swap>0 && min_shares>0);
        assert!(deadline>e.ledger().timestamp() && deadline-e.ledger().timestamp()<=3600,"TEST prototype command horizon");
        let id:u64=e.storage().instance().get(&StageReceiptKey::Next).unwrap_or(0);
        e.storage().instance().set(&StageReceiptKey::Next,&id.checked_add(1).unwrap());
        let j=StageJob{owner,kind:0,phase:0,deadline,a:amount,b:0,reserve,swap_in,min_swap,min_shares,shares:0};
        stage_save(&e,id,&j);
        stage_invoke::<()>(&e,&v,"stage_check_open",(e.current_contract_address(),id,j.clone()).into_val(&e));
        stage_leave(&e);id
    }
    pub fn stage_fund(e:Env,id:u64) {
        stage_enter(&e);let mut j=stage_load(&e,id);j.owner.require_auth();
        assert!(j.phase==0 && e.ledger().timestamp()<=j.deadline);
        let v=stage_receipt_vault(&e);
        stage_invoke::<()>(&e,&v,"stage_accept_funds",(e.current_contract_address(),id,j.clone()).into_val(&e));
        j.phase=1;stage_save(&e,id,&j);stage_leave(&e);
    }
    pub fn stage_convert(e:Env,id:u64) {
        stage_enter(&e);let mut j=stage_load(&e,id);assert!(j.phase==1 && e.ledger().timestamp()<=j.deadline);
        let v=stage_receipt_vault(&e);
        let out:u128=stage_invoke(&e,&v,"stage_convert_open",(e.current_contract_address(),id,j.clone()).into_val(&e));
        j.a-=j.swap_in;j.b=out;j.phase=2;stage_save(&e,id,&j);stage_leave(&e);
    }
    pub fn stage_mint(e:Env,id:u64)->u128 {
        stage_enter(&e);let mut j=stage_load(&e,id);assert!(j.phase==2 && e.ledger().timestamp()<=j.deadline);
        let v=stage_receipt_vault(&e);
        let q:u128=stage_invoke(&e,&v,"stage_cash_mint",(e.current_contract_address(),id,j.clone()).into_val(&e));
        j.shares=q;j.phase=3;stage_save(&e,id,&j);stage_leave(&e);q
    }
    pub fn stage_invest(e:Env,id:u64)->u128 {
        stage_enter(&e);let mut j=stage_load(&e,id);assert!(j.phase==3 && e.ledger().timestamp()<=j.deadline);
        let v=stage_receipt_vault(&e);
        let l:u128=stage_invoke(&e,&v,"stage_invest_cash",(e.current_contract_address(),id,j.clone()).into_val(&e));
        j.a=0;j.b=0;j.reserve=0;j.phase=4;stage_save(&e,id,&j);stage_leave(&e);l
    }
    pub fn stage_prepare_close(e:Env,owner:Address,shares:u128,min_swap:u128,deadline:u64)->u64 {
        stage_enter(&e);owner.require_auth();let v=stage_receipt_vault(&e);
        assert!(shares>0 && deadline>e.ledger().timestamp() && deadline-e.ledger().timestamp()<=3600);
        let id:u64=e.storage().instance().get(&StageReceiptKey::Next).unwrap_or(0);
        e.storage().instance().set(&StageReceiptKey::Next,&id.checked_add(1).unwrap());
        let j=StageJob{owner,kind:1,phase:10,deadline,a:0,b:0,reserve:0,swap_in:0,min_swap,min_shares:0,shares};
        stage_save(&e,id,&j);
        stage_invoke::<()>(&e,&v,"stage_lock_close",(e.current_contract_address(),id,j.clone()).into_val(&e));
        stage_leave(&e);id
    }
    pub fn stage_withdraw(e:Env,id:u64)->Vec<u128> {
        stage_enter(&e);let mut j=stage_load(&e,id);assert!(j.phase==10 && e.ledger().timestamp()<=j.deadline);
        let v=stage_receipt_vault(&e);
        let out:Vec<u128>=stage_invoke(&e,&v,"stage_withdraw_kind",(e.current_contract_address(),id,j.clone()).into_val(&e));
        if j.min_swap==0 {j.a=0;j.b=0;j.phase=12;} else {j.a=out.get(0).unwrap();j.b=out.get(1).unwrap();j.phase=11;}stage_save(&e,id,&j);stage_leave(&e);out
    }
    pub fn stage_close_convert(e:Env,id:u64)->u128 {
        stage_enter(&e);let mut j=stage_load(&e,id);assert!(j.phase==11 && e.ledger().timestamp()<=j.deadline && j.min_swap>0);
        let v=stage_receipt_vault(&e);
        let out:u128=stage_invoke(&e,&v,"stage_convert_exit",(e.current_contract_address(),id,j.clone()).into_val(&e));
        j.a=0;j.b=0;j.phase=12;stage_save(&e,id,&j);stage_leave(&e);out
    }
    // Owner cancellation or permissionless expiry, always to original owner.
    // 1x has no debt. Cash-backed shares are never converted into a second cash refund.
    pub fn stage_cancel(e:Env,id:u64) {
        stage_enter(&e);let mut j=stage_load(&e,id);
        assert!(j.phase!=4 && j.phase!=12 && j.phase!=99,"already terminal");
        if e.ledger().timestamp()<=j.deadline {j.owner.require_auth();}
        let v=stage_receipt_vault(&e);
        stage_invoke::<()>(&e,&v,"stage_cancel_kind",(e.current_contract_address(),id,j.clone()).into_val(&e));
        j.a=0;j.b=0;j.reserve=0;j.phase=99;stage_save(&e,id,&j);stage_leave(&e);
    }
}
