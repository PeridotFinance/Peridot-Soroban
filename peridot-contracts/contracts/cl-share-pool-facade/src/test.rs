//! Compiled local checkpoint. Test-only prices and maintenance margin; no deployment config.
use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Bytes, Val,
};
use std::format;
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
    pub fn lastprice(e: Env, _asset: Asset) -> Option<Price> {
        Some(Price {
            price: 100_000_000_000_000,
            timestamp: e.ledger().timestamp(),
        })
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
    e.cost_estimate().budget().reset_unlimited();
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
    let r = e.cost_estimate().resources();
    let memory = r.mem_bytes;
    let line=format!("{{\"flow\":\"{label}\",\"ok\":{ok},\"instructions\":{},\"memory_bytes\":{memory},\"read_entries\":{},\"write_entries\":{},\"read_bytes\":{},\"write_bytes\":{},\"scope\":\"compiled-local-controlled-state\"}}\n",r.instructions,r.memory_read_entries,r.write_entries,r.disk_read_bytes,r.write_bytes);
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
}
impl F {
    fn new() -> Self {
        let e = Env::default();
        e.ledger().with_mut(|l| {
            l.timestamp = 1000;
            l.sequence_number = 100;
        });
        e.cost_estimate().budget().reset_unlimited();
        let admin = Address::from_str(&e, ADMIN);
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
                plane,
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
        // Pool uses transfer-level auth without a sender require_auth root. Exact amounts
        // are read from estimate; token arguments must match the real pool's rounding.
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
        e.mock_auths(&[
            MockAuth {
                address: &user,
                invoke: &p0,
            },
            MockAuth {
                address: &user,
                invoke: &p1,
            },
        ]);
        let _: (Vec<u128>, u128) = e.invoke_contract(
            &pool,
            &Symbol::new(&e, "deposit_position"),
            (user.clone(), -2000i32, 2000i32, actual, 0u128).into_val(&e),
        );
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
        f
    }
    fn deposit_cash(&self, n: u128) {
        let sub = MockAuthInvoke {
            contract: &self.under,
            fn_name: "transfer",
            args: (self.user.clone(), self.cash.clone(), amount(n)).into_val(&self.e),
            sub_invokes: &[],
        };
        let _: u128 = auth(
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
        1u128,
    )
        .into_val(e);
    e.cost_estimate().budget().reset_unlimited();
    let id: u64 = auth(e, &f.user, &f.margin, "begin_open_position_v3", args, &[]);
    measure(e, "begin_open", true);
    for name in [
        "swap_open_position_v3",
        "activate_open_position_v3",
        "prepare_close_position_v3",
    ] {
        e.cost_estimate().budget().reset_unlimited();
        let _: () = auth(
            e,
            &f.user,
            &f.margin,
            name,
            (f.user.clone(), id).into_val(e),
            &[],
        );
        measure(e, name, true);
    }
    e.cost_estimate().budget().reset_unlimited();
    let _: () = auth(
        e,
        &f.user,
        &f.margin,
        "swap_close_position_v3",
        (f.user.clone(), id, 1u128).into_val(e),
        &[],
    );
    measure(e, "swap_close", true);
    e.cost_estimate().budget().reset_unlimited();
    let _: () = call(e, &f.margin, "finish_close_position_v3", (id,).into_val(e));
    measure(e, "finish_close", true);
    assert_eq!(token::Client::new(e, &f.under).balance(&f.facade), 777);
    assert_eq!(token::Client::new(e, &f.share).balance(&f.facade), 0);
    // Remaining required scenarios must not be reported as passed by this baseline.
}
