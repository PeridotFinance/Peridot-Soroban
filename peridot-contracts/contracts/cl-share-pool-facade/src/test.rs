//! Compiled local checkpoint. Test-only prices and maintenance margin; no deployment config.
use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Bytes, Val,
};
use std::format;
std::thread_local! {static MEASURE: std::cell::Cell<bool> = const {std::cell::Cell::new(false)};}
#[contracttype]
#[derive(Clone)]
enum Side {
    Long,
    Short,
}
const ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";
#[contracttype]
#[derive(Clone)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}
#[contracttype]
#[derive(Clone)]
pub struct Price {
    pub price: i128,
    pub timestamp: u64,
}
#[contract]
struct TestOracle;
#[contractimpl]
impl TestOracle {
    pub fn lastprice(e: Env, asset: Asset) -> Option<Price> {
        Some(Price {
            price: e.storage().instance().get(&asset).unwrap_or(100_000_000_000_000),
            timestamp: e.ledger().timestamp(),
        })
    }
    pub fn set_test_price(e: Env, admin: Address, asset: Asset, price: i128) {
        admin.require_auth(); // Synthetic oracle only; never a deployment oracle.
        e.storage().instance().set(&asset, &price);
    }
    pub fn decimals(_e: Env) -> u32 {
        14
    }
    pub fn resolution(_e: Env) -> u32 {
        300
    }
    pub fn base(e: Env) -> Asset {
        Asset::Other(Symbol::new(&e, "USD"))
    }
}
#[contract]
struct TestPlane;
#[contractimpl]
impl TestPlane {
    pub fn total_supply(_e: Env) -> u128 {
        0
    }
    pub fn update(_e: Env, _pool: Address, _kind: Symbol, _args: Vec<u128>, _reserves: Vec<u128>) {}
}
fn wasm(e: &Env, name: &str) -> Address {
    let base = std::env::var("CL_ARTIFACTS").expect("CL_ARTIFACTS required");
    let bytes = std::fs::read(format!("{base}/{name}.wasm")).unwrap();
    e.register(bytes.as_slice(), ())
}
fn call<T: soroban_sdk::TryFromVal<Env, Val>>(
    e: &Env,
    to: &Address,
    name: &str,
    args: Vec<Val>,
) -> T {
    e.mock_auths(&[]);
    crate::cl_resource_limits::reset(e);
    e.invoke_contract(to, &Symbol::new(e, name), args)
}
fn auth<T: soroban_sdk::TryFromVal<Env, Val>>(
    e: &Env,
    who: &Address,
    to: &Address,
    name: &str,
    args: Vec<Val>,
    subs: &[MockAuthInvoke],
) -> T {
    e.mock_auths(&[MockAuth {
        address: who,
        invoke: &MockAuthInvoke {
            contract: to,
            fn_name: name,
            args: args.clone(),
            sub_invokes: subs,
        },
    }]);
    crate::cl_resource_limits::reset(e);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.invoke_contract(to, &Symbol::new(e, name), args)
    }));
    measure(e, name, result.is_ok());
    match result {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error),
    }
}
fn measure(e: &Env, label: &str, ok: bool) {
    if !MEASURE.with(|m|m.get()) { return; }
    measure_scope(e,label,ok,"compiled-local-controlled-state");
}
pub(super) fn measure_scope(e:&Env,label:&str,ok:bool,scope:&str) {
    let profile=crate::cl_resource_limits::metadata();
    crate::resource_profile::capture(e,label,ok,profile["profile"].as_str().unwrap(),profile["run_mode"].as_str().unwrap());
    let r = e.cost_estimate().resources();
    let memory = r.mem_bytes;
    let line = format!(concat!(
        "{{\"flow\":\"{label}\",\"ok\":{ok},\"instructions\":{},\"memory_bytes\":{memory},",
        "\"read_entries\":{},\"write_entries\":{},\"read_bytes\":{},\"write_bytes\":{},",
        "\"disk_read_entries\":{},\"contract_events_size_bytes\":{},",
        "\"persistent_rent_ledger_bytes\":{},\"persistent_entry_rent_bumps\":{},",
        "\"temporary_rent_ledger_bytes\":{},\"temporary_entry_rent_bumps\":{},",
        "\"scope\":\"{scope}\"}}\n"),
        r.instructions,r.memory_read_entries,r.write_entries,r.disk_read_bytes,r.write_bytes,
        r.disk_read_entries,r.contract_events_size_bytes,r.persistent_rent_ledger_bytes,
        r.persistent_entry_rent_bumps,r.temporary_rent_ledger_bytes,r.temporary_entry_rent_bumps,
        label=label,ok=ok,memory=memory,scope=scope);
    let mut record:serde_json::Value=serde_json::from_str(&line).unwrap();
    for (key,value) in profile.as_object().unwrap() {record[key]=value.clone();}
    for (key,value) in crate::cl_resource_limits::diagnostics(e).as_object().unwrap(){record[key]=value.clone();}
    let line=format!("{}\n",serde_json::to_string(&record).unwrap());
    std::println!("{line}");
    if let Ok(path) = std::env::var("CL_RESOURCE_REPORT") {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
            .write_all(line.as_bytes())
            .unwrap();
    }
}
struct F {
    e: Env,
    admin: Address,
    user: Address,
    under: Address,
    paired: Address,
    reward: Address,
    pool: Address,
    share: Address,
    facade: Address,
    margin: Address,
    cash: Address,
    receipt: Address,
    binding: soroban_sdk::BytesN<32>,
    oracle: Address,
}
impl F {
    fn new() -> Self {
        MEASURE.with(|m|m.set(false));
        let mut e = Env::default();
        // Optional file snapshot at Drop reuses an exhausted execution meter and
        // can double-panic after a policy rejection. Resource/auth checks stay on.
        e.set_config(soroban_sdk::testutils::EnvTestConfig { capture_snapshot_at_drop: false, ..Default::default() });
        e.ledger().with_mut(|l| {
            l.timestamp = 1000;
            l.sequence_number = 100;
        });
        crate::cl_resource_limits::configure(&e);
        // Explicit synthetic contract-admin fixture, not a production identity.
        let admin = Address::generate(&e);
        assert_eq!(admin, Address::from_str(&e,"CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"));
        let user = Address::generate(&e);
        let a = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let b = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let (under, paired) = if a < b { (a, b) } else { (b, a) };
        let reward = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let boost = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let bytes = std::fs::read(std::env::var("AQUARIUS_CONCENTRATED_WASM").unwrap()).unwrap();
        let hash = e.crypto().sha256(&Bytes::from_slice(&e, &bytes)).to_array();
        let hex: std::string::String = hash.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6"
        );
        let pool = e.register(bytes.as_slice(), ());
        let plane = e.register(TestPlane, ());
        let roles = (
            admin.clone(),
            admin.clone(),
            admin.clone(),
            admin.clone(),
            soroban_sdk::vec![&e, admin.clone()],
            admin.clone(),
        );
        auth::<()>(
            &e,
            &admin,
            &pool,
            "initialize_all",
            (
                admin.clone(),
                roles,
                admin.clone(),
                soroban_sdk::vec![&e, under.clone(), paired.clone()],
                10u32,
                20i32,
                0u32,
                (reward.clone(), boost, plane.clone()),
                plane.clone(),
            )
                .into_val(&e),
            &[],
        );
        for t in [&under, &paired, &reward] {
            auth::<()>(
                &e,
                &admin,
                t,
                "mint",
                (user.clone(), 2_000_000_000_000i128).into_val(&e),
                &[],
            );
        }
        let p0 = MockAuthInvoke {
            contract: &under,
            fn_name: "transfer",
            args: (user.clone(), pool.clone(), 100_000_000_000i128).into_val(&e),
            sub_invokes: &[],
        };
        let p1 = MockAuthInvoke {
            contract: &paired,
            fn_name: "transfer",
            args: (user.clone(), pool.clone(), 100_000_000_000i128).into_val(&e),
            sub_invokes: &[],
        };
        // Pinned CL pool requires sender auth at deposit plus both transfers.
        let desired = soroban_sdk::vec![&e, 100_000_000_000u128, 100_000_000_000u128];
        let (actual, _): (Vec<u128>, u128) = call(
            &e,
            &pool,
            "estimate_deposit_position",
            (-2000i32, 2000i32, desired.clone()).into_val(&e),
        );
        let mut p0 = p0;
        let mut p1 = p1;
        p0.args = (user.clone(), pool.clone(), amount(actual.get(0).unwrap())).into_val(&e);
        p1.args = (user.clone(), pool.clone(), amount(actual.get(1).unwrap())).into_val(&e);
        let _: (Vec<u128>, u128) = auth(&e,&user,&pool,"deposit_position",
            (user.clone(), -2000i32, 2000i32, actual, 0u128).into_val(&e), &[p0,p1]);
        let oracle = e.register(TestOracle, ());
        let share = wasm(&e, "cl_share_vault");
        let facade = wasm(&e, "cl_share_pool_facade");
        let margin = wasm(&e, "margin_controller");
        let cash = wasm(&e, "receipt_vault");
        let receipt = wasm(&e, "receipt_vault");
        let controller = wasm(&e, "simple_peridottroller");
        let adapter = wasm(&e, "swap_adapter");
        auth::<()>(
            &e,
            &admin,
            &share,
            "initialize",
            (admin.clone(), pool.clone(), 0u32, oracle.clone()).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            &share,
            "set_primary_reward_token",
            (admin.clone(), Some(reward.clone())).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            &share,
            "set_max_deploy",
            (admin.clone(), 1_000_000_000u128).into_val(&e),
            &[],
        );
        let _: u128 = call(&e, &share, "refresh_nav_root", ().into_val(&e));
        auth::<()>(
            &e,
            &admin,
            &controller,
            "initialize",
            (admin.clone(),).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            &controller,
            "set_oracle",
            (oracle.clone(),).into_val(&e),
            &[],
        );
        for (v, t, cf) in [(&cash, &under, 600_000u128), (&receipt, &share, 10_000u128)] {
            auth::<()>(
                &e,
                &admin,
                v,
                "initialize",
                (t.clone(), 0u128, 0u128, admin.clone()).into_val(&e),
                &[],
            );
            auth::<()>(
                &e,
                &admin,
                v,
                "enable_static_rates",
                (admin.clone(),).into_val(&e),
                &[],
            );
            auth::<()>(
                &e,
                &admin,
                v,
                "set_peridottroller",
                (controller.clone(),).into_val(&e),
                &[],
            );
            auth::<()>(
                &e,
                &admin,
                &controller,
                "add_market",
                (v.clone(),).into_val(&e),
                &[],
            );
            auth::<()>(
                &e,
                &admin,
                &controller,
                "set_market_cf",
                (v.clone(), cf).into_val(&e),
                &[],
            );
            auth::<()>(
                &e,
                &admin,
                v,
                "set_collateral_factor",
                (cf,).into_val(&e),
                &[],
            );
            let _: Option<(u128, u128)> =
                call(&e, &controller, "cache_price", (t.clone(),).into_val(&e));
        }
        auth::<()>(
            &e,
            &admin,
            &adapter,
            "initialize",
            (admin.clone(), pool.clone()).into_val(&e),
            &[],
        );
        auth::<()>(&e,&admin,&adapter,"set_pool_allowed",(admin.clone(),margin.clone(),true).into_val(&e),&[]);
        auth::<()>(&e,&admin,&controller,"set_margin_liquidation_ctrl",(margin.clone(),true).into_val(&e),&[]);
        auth::<()>(
            &e,
            &admin,
            &margin,
            "initialize",
            (admin.clone(), controller.clone(), adapter.clone(), 2u128).into_val(&e),
            &[],
        );
        for (v, t) in [(&cash, &under), (&receipt, &share)] {
            auth::<()>(
                &e,
                &admin,
                &margin,
                "set_market",
                (admin.clone(), t.clone(), v.clone()).into_val(&e),
                &[],
            );
            auth::<()>(
                &e,
                &admin,
                v,
                "set_margin_controller",
                (admin.clone(), Some(margin.clone())).into_val(&e),
                &[],
            );
        }
        // Explicit validation-only custody wiring before any share holdings.
        for custodian in [&margin, &receipt] {
            auth::<()>(&e,&admin,&share,"register_custodian",(custodian.clone(),).into_val(&e),&[]);
        }
        auth::<()>(&e,&admin,&receipt,"set_cl_custody",(admin.clone(),).into_val(&e),&[]);
        auth::<()>(&e,&admin,&margin,"set_cl_share_asset",(admin.clone(),share.clone()).into_val(&e),&[]);
        auth::<()>(
            &e,
            &admin,
            &facade,
            "initialize",
            (
                admin.clone(),
                margin.clone(),
                under.clone(),
                share.clone(),
                admin.clone(),
            )
                .into_val(&e),
            &[],
        );
        let binding = soroban_sdk::BytesN::from_array(&e, &[17; 32]);
        auth::<()>(
            &e,
            &admin,
            &adapter,
            "set_pool_allowed",
            (admin.clone(), facade.clone(), true).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            &adapter,
            "set_pool_binding",
            (admin.clone(), binding.clone(), facade.clone(), true).into_val(&e),
            &[],
        );
        crate::resource_profile::register_contracts(&e,&[("v3",&margin),("facade",&facade),("vault",&share),("aquarius",&pool),("cash_receipt",&cash),("share_receipt",&receipt),("adapter",&adapter),("lending_controller",&controller),("oracle",&oracle),("plane",&plane),("underlying",&under),("paired",&paired),("reward",&reward)]);
        let f = Self {
            e,
            admin,
            user,
            under,
            paired,
            reward,
            pool,
            share,
            facade,
            margin,
            cash,
            receipt,
            binding,
            oracle,
        };
        f.deposit_cash(1_000_000_000);
        // Test-only maintenance margin: 25%; it is NOT an approved deployment value.
        let _: () = auth(
            &f.e,
            &f.admin,
            &f.margin,
            "set_perps_pair_config",
            (
                f.admin.clone(),
                f.under.clone(),
                f.share.clone(),
                Side::Long,
                2u128,
                250_000u128,
                80_000u128,
            )
                .into_val(&f.e),
            &[],
        );
        MEASURE.with(|m|m.set(true));
        f
    }
    fn deposit_cash(&self, n: u128) {
        let sub = MockAuthInvoke {
            contract: &self.under,
            fn_name: "transfer",
            args: (self.user.clone(), self.cash.clone(), amount(n)).into_val(&self.e),
            sub_invokes: &[],
        };
        let _: () = auth(
            &self.e,
            &self.user,
            &self.cash,
            "deposit",
            (self.user.clone(), n).into_val(&self.e),
            &[sub],
        );
    }
}
#[test]
fn compiled_v3_checkpoint() {
    let f = F::new();
    let e = &f.e;
    // Principal dust at facade must neither subsidize execution nor prevent it.
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), f.facade.clone(), 777i128).into_val(e),
        &[],
    );
    let id = begin(&f);
    measure(e, "begin_open", true);
    for name in [
        "swap_open_position_v3",
        "activate_open_position_v3",
        "prepare_close_position_v3",
    ] {
        crate::cl_resource_limits::reset(e);
        let _: () = auth(e,&f.user,&f.margin,name,(f.user.clone(), id).into_val(e),&[]);
        measure(e,name,true);
    }
    let shares = token::Client::new(e,&f.share).balance(&f.margin);
    let quote:u128=call(e,&f.facade,"estimate_swap",(1u32,0u32,shares as u128).into_val(e));
    auth::<()>(e,&f.user,&f.margin,"swap_close_position_v3",(f.user.clone(),id,quote).into_val(e),&[]);
    measure(e,"swap_close",true);
    call::<()>(e,&f.margin,"finish_close_position_v3",(id,).into_val(e));
    measure(e,"finish_close",true);
    assert_eq!(token::Client::new(e,&f.under).balance(&f.facade),777);
    assert_eq!(token::Client::new(e,&f.share).balance(&f.facade),0);
}

fn begin(f:&F)->u64 {
    let e=&f.e;
    let sub = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "transfer",
        args: (f.user.clone(), f.margin.clone(), 100_000_000i128).into_val(e),
        sub_invokes: &[],
    };
    let _: () = auth(
        e,
        &f.user,
        &f.margin,
        "transfer_spot_to_margin",
        (f.user.clone(), f.under.clone(), 100_000_000u128).into_val(e),
        &[sub],
    );
    let tokens = soroban_sdk::vec![e, f.under.clone(), f.share.clone()];
    let args = (
        f.user.clone(),
        f.under.clone(),
        f.share.clone(),
        100_000_000u128,
        2u128,
        Side::Long,
        tokens,
        f.binding.clone(),
        f.facade.clone(),
        194_040_000u128,
    )
        .into_val(e);
    crate::cl_resource_limits::reset(e);
    let id: u64 = auth(e, &f.user, &f.margin, "begin_open_position_v3", args, &[]);
    id
}

#[contract]
struct AdminSetupFixture;
#[contractimpl]
impl AdminSetupFixture {
    pub fn protected(_e: Env, admin: Address) { admin.require_auth(); }
}

#[test]
fn sdk_25_3_1_g_admin_mock_setup_regression() {
    let e=Env::default();
    let fixed=Address::from_str(&e,ADMIN);
    let target=e.register(AdminSetupFixture,());
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.mock_auths(&[MockAuth {address:&fixed, invoke:&MockAuthInvoke {
            contract:&target,fn_name:"protected",args:(fixed.clone(),).into_val(&e),sub_invokes:&[]
        }}]);
    }));
    assert!(result.is_err(),"SDK mock_auths must not register an account as a contract");
    std::println!("REPRODUCED: fixed G-address fails in mock setup, not an authorization rejection");
    let e=Env::default();let admin=Address::generate(&e);
    std::println!("SYNTHETIC_TEST_ADMIN={admin:?}");
    let target=e.register(AdminSetupFixture,());
    e.mock_auths(&[MockAuth {address:&admin,invoke:&MockAuthInvoke{contract:&target,fn_name:"protected",args:(admin.clone(),).into_val(&e),sub_invokes:&[]}}]);
    AdminSetupFixtureClient::new(&e,&target).protected(&admin);
    e.mock_auths(&[]);
    assert!(AdminSetupFixtureClient::new(&e,&target).try_protected(&admin).is_err());
    std::println!("PASS: generated synthetic admin exact auth accepted; no-auth invocation rejected. No real account-signature proof.");
}

#[test]
fn compiled_vault_core_flows() {
    let f=F::new();let e=&f.e;
    let other=Address::generate(e);
    let sub=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(f.user.clone(),f.share.clone(),100_000_000i128).into_val(e),sub_invokes:&[]};
    let minted:i128=auth(e,&f.user,&f.share,"deposit_underlying",(f.user.clone(),100_000_000i128,1i128).into_val(e),&[sub]);
    assert!(minted>0);measure(e,"deposit",true);
    auth::<()>(e,&f.user,&f.share,"transfer",(f.user.clone(),other.clone(),minted/2).into_val(e),&[]);
    let claimed:u128=auth(e,&f.user,&f.share,"claim_rewards",(f.user.clone(),).into_val(e),&[]);
    assert_eq!(claimed,0,"no reward schedule configured in this core fixture");
    let out:i128=auth(e,&other,&f.share,"redeem",(other.clone(),minted/2,1i128).into_val(e),&[]);
    assert!(out>0);
    let out:Vec<u128>=auth(e,&f.user,&f.share,"emergency_redeem",(f.user.clone(),(minted-minted/2)as u128,false).into_val(e),&[]);
    assert!(out.get(0).unwrap()+out.get(1).unwrap()>0);
    assert_eq!(token::Client::new(e,&f.share).balance(&f.user),0);
    assert_eq!(token::Client::new(e,&f.share).balance(&other),0);
}

fn rejection(e:&Env,to:&Address,name:&str,args:Vec<Val>,label:&str,auth_error:bool) {
    e.mock_auths(&[]);
    crate::cl_resource_limits::reset(e);
    let result=e.try_invoke_contract::<Val,soroban_sdk::Error>(to,&Symbol::new(e,name),args);
    assert!(result.is_err(),"negative case unexpectedly succeeded");
    let detail=format!("{result:?}");
    if auth_error {
        let events=e.host().get_diagnostic_events().unwrap();
        let diagnostics=format!("{:?}",events);
        assert!(diagnostics.contains("Auth") && diagnostics.contains(name),"expected authorization rejection, got {detail}; {diagnostics}");
    }
    std::println!("EXPECTED_REJECTION {label}: {detail}");
    measure(e,label,true);
}
#[test]
fn compiled_authorization_rejections_after_setup() {
    let f=F::new();let e=&f.e;
    rejection(e,&f.facade,"swap",(f.user.clone(),0u32,1u32,1u128,1u128).into_val(e),"wrong_caller",false);
    rejection(e,&f.facade,"swap",(f.margin.clone(),0u32,1u32,1u128,1u128).into_val(e),"controller_impersonation",true);
    rejection(e,&f.share,"custody_prepare_transfer",(f.margin.clone(),f.user.clone(),f.user.clone(),1u128,true).into_val(e),"unauthorized_vault_recovery",true);
    rejection(e,&f.receipt,"cl_prepare_margin_rewards",(f.user.clone(),1u128,true).into_val(e),"unauthorized_receipt_recovery",true);
    let id=begin(&f);
    rejection(e,&f.margin,"recover_pending_open_funded",(f.user.clone(),id,100_000_000u128).into_val(e),"unauthorized_pending_recovery",true);
    auth::<()>(e,&f.user,&f.margin,"cancel_pending_open_v3",(f.user.clone(),id).into_val(e),&[]);
    measure(e,"cancel_before_execute",true);
}

#[test]
fn compiled_venue_rewards_and_claim_failure() {
    let f=F::new();let e=&f.e;
    let sub=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(f.user.clone(),f.share.clone(),100_000_000i128).into_val(e),sub_invokes:&[]};
    let minted:i128=auth(e,&f.user,&f.share,"deposit_underlying",(f.user.clone(),100_000_000i128,1i128).into_val(e),&[sub]);
    auth::<()>(e,&f.user,&f.reward,"transfer",(f.user.clone(),f.pool.clone(),100_000_000i128).into_val(e),&[]);
    auth::<()>(e,&f.admin,&f.pool,"set_rewards_config",(f.admin.clone(),2000u64,10_000u128).into_val(e),&[]);
    e.ledger().with_mut(|l|{l.timestamp=1100;l.sequence_number+=20;});
    let pending:u128=call(e,&f.pool,"get_user_reward",(f.share.clone(),).into_val(e));
    let before=token::Client::new(e,&f.reward).balance(&f.share);
    let claimed:bool=call(e,&f.share,"checkpoint_rewards",().into_val(e));
    assert!(claimed);
    let delta=token::Client::new(e,&f.reward).balance(&f.share)-before;
    assert_eq!(delta as u128,pending);
    assert!(pending>0,"fixture must exercise nonzero actual rewards");
    std::println!("PINNED_POOL_SAME_STATE in-range pending={pending} actual_claim_delta={delta}; one controlled state only, not universal equality proof");
    let paid:u128=auth(e,&f.user,&f.share,"claim_rewards",(f.user.clone(),).into_val(e),&[]);
    let totals:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));
    let holder:RewardHolder=call(e,&f.share,"reward_account",(f.user.clone(),).into_val(e));
    assert_eq!(totals.received,pending);assert_eq!(totals.paid,paid);
    assert_eq!(pending*1_000_000_000_000_000_000u128,
        (paid+holder.pending+totals.quarantine)*1_000_000_000_000_000_000u128+holder.fraction+totals.rounding_scaled);
    assert_eq!(token::Client::new(e,&f.reward).balance(&f.share)as u128,pending-paid);
    auth::<()>(e,&f.admin,&f.pool,"kill_claim",(f.admin.clone(),).into_val(e),&[]);
    e.ledger().with_mut(|l|{l.timestamp=1200;l.sequence_number+=20;});
    let balance=token::Client::new(e,&f.share).balance(&f.user);
    let other=Address::generate(e);
    e.mock_auths(&[MockAuth{address:&f.user,invoke:&MockAuthInvoke{contract:&f.share,fn_name:"transfer",args:(f.user.clone(),other.clone(),1i128).into_val(e),sub_invokes:&[]}}]);
    let result=e.try_invoke_contract::<(),soroban_sdk::Error>(&f.share,&Symbol::new(e,"transfer"),(f.user.clone(),other.clone(),1i128).into_val(e));
    assert_eq!(result,Err(Ok(soroban_sdk::Error::from_contract_error(801))));
    measure(e,"normal_transfer_claim_failure_rejected",true);
    assert_eq!(token::Client::new(e,&f.share).balance(&f.user),balance);
    e.mock_auths(&[MockAuth{address:&f.user,invoke:&MockAuthInvoke{contract:&f.share,fn_name:"emergency_redeem",args:(f.user.clone(),minted as u128,false).into_val(e),sub_invokes:&[]}}]);
    let result=e.try_invoke_contract::<Vec<u128>,soroban_sdk::Error>(&f.share,&Symbol::new(e,"emergency_redeem"),(f.user.clone(),minted as u128,false).into_val(e));
    assert_eq!(result,Err(Ok(soroban_sdk::Error::from_contract_error(802))));
    measure(e,"waiver_false_rejected_named_error",true);
    let out:Vec<u128>=auth(e,&f.user,&f.share,"emergency_redeem",(f.user.clone(),minted as u128,true).into_val(e),&[]);
    assert!(out.get(0).unwrap()+out.get(1).unwrap()>0);
    measure(e,"claim_failure_recovery",true);
    assert_eq!(token::Client::new(e,&f.share).balance(&f.user),0);
    std::println!("REWARD_CONSERVATION actual_pool_received={pending} actual_holder_paid={paid} rounding_backing={}; remaining failed-claim interval quarantined/unproven; no reallocation clearance",pending-paid);
}

#[contracttype]
#[derive(Clone)]
struct RewardTotals { received:u128,paid:u128,quarantine:u128,rounding_scaled:u128 }
#[contracttype]
#[derive(Clone)]
struct RewardHolder { debt:u128,pending:u128,fraction:u128 }

#[test]
fn compiled_receipt_margin_reward_custody() {
    let f=F::new();let e=&f.e;let other=Address::generate(e);
    let sub=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(f.user.clone(),f.share.clone(),100_000_000i128).into_val(e),sub_invokes:&[]};
    let minted:i128=auth(e,&f.user,&f.share,"deposit_underlying",(f.user.clone(),100_000_000i128,1i128).into_val(e),&[sub]);
    let sub=MockAuthInvoke{contract:&f.share,fn_name:"transfer",args:(f.user.clone(),f.receipt.clone(),minted).into_val(e),sub_invokes:&[]};
    auth::<()>(e,&f.user,&f.receipt,"deposit",(f.user.clone(),minted as u128).into_val(e),&[sub]);
    measure(e,"receipt_custody_deposit",true);
    auth::<()>(e,&f.user,&f.reward,"transfer",(f.user.clone(),f.pool.clone(),100_000_000i128).into_val(e),&[]);
    auth::<()>(e,&f.admin,&f.pool,"set_rewards_config",(f.admin.clone(),2000u64,10_000u128).into_val(e),&[]);
    e.ledger().with_mut(|l|{l.timestamp=1100;l.sequence_number+=20;});
    auth::<()>(e,&f.user,&f.receipt,"transfer",(f.user.clone(),other.clone(),minted/2).into_val(e),&[]);
    measure(e,"receipt_custody_ownership_change",true);
    let old:RewardHolder=call(e,&f.share,"reward_account",(f.user.clone(),).into_val(e));
    assert!(old.pending>0);
    let newcomer:RewardHolder=call(e,&f.share,"reward_account",(other.clone(),).into_val(e));
    assert_eq!(newcomer.pending,0,"new owner must not capture historical rewards");
    let sub=MockAuthInvoke{contract:&f.receipt,fn_name:"transfer",args:(other.clone(),f.margin.clone(),minted/2).into_val(e),sub_invokes:&[]};
    auth::<()>(e,&other,&f.margin,"transfer_spot_to_margin",(other.clone(),f.share.clone(),(minted/2)as u128).into_val(e),&[sub]);
    measure(e,"margin_custody_deposit",true);
    e.ledger().with_mut(|l|{l.timestamp=1200;l.sequence_number+=20;});
    let paid_other:u128=auth(e,&other,&f.share,"claim_rewards",(other.clone(),).into_val(e),&[]);
    let paid_old:u128=auth(e,&f.user,&f.share,"claim_rewards",(f.user.clone(),).into_val(e),&[]);
    assert!(paid_other>0);assert!(paid_old>paid_other);
    for custodian in [&f.receipt,&f.margin] {
        let holder:RewardHolder=call(e,&f.share,"reward_account",(custodian.clone(),).into_val(e));
        assert_eq!(holder.pending,0,"custody must not own user rewards");
    }
    let totals:RewardTotals=call(e,&f.share,"reward_totals",().into_val(e));
    assert_eq!(totals.paid,paid_old+paid_other);
    assert_eq!(totals.received,totals.paid+token::Client::new(e,&f.reward).balance(&f.share)as u128);
    std::println!("COMPILED_CUSTODY_CONSERVATION received={} old_owner={paid_old} new_owner={paid_other} custody_owners=0 backing={}; free margin custody only, not activated position proof",totals.received,totals.received-totals.paid);
}

#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;

#[path = "stress_tests.rs"]
mod stress_tests;
#[path = "funded_tests.rs"]
mod funded_tests;
