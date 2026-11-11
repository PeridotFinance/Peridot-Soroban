use cl_stage1_interface::*;
use serde_json::{json, Value};
use soroban_sdk::{xdr::ToXdr, Address, BytesN, Env, String};
fn addr(e: &Env, n: u8) -> Address {
    use soroban_sdk::{
        xdr::{ContractId, Hash, ScAddress, ScVal},
        TryFromVal,
    };
    Address::try_from_val(
        e,
        &ScVal::Address(ScAddress::Contract(ContractId(Hash([n; 32])))),
    )
    .unwrap()
}
fn fixture(e: &Env) -> (BindingV1, CommandV1) {
    let binding = BindingV1 {
        schema: 1,
        receipt: addr(e, 1),
        vault: addr(e, 2),
        pool: addr(e, 3),
        token0: addr(e, 4),
        token1: addr(e, 5),
        decimals0: 7,
        decimals1: 7,
        tick_lower: -2000,
        tick_upper: 2000,
    };
    let terms = CommandTermsV1::Open(OpenTermsV1 {
        amount: 1000,
        swap_in: 500,
        min_swap: 490,
        min_shares: 900,
        max_strategy_cost: 10,
        deadline: 100,
        config_revision: 1,
    });
    let hash = terms_hash_v1(
        e,
        &BytesN::from_array(e, &[0; 32]),
        &binding.receipt,
        &binding.vault,
        42,
        &addr(e, 6),
        &binding_hash_v1(e, &BytesN::from_array(e, &[0; 32]), &binding),
        0,
        &terms,
    );
    let cmd = CommandV1 {
        schema: 1,
        id: 42,
        owner: addr(e, 6),
        terms,
        terms_hash: hash,
        phase: PhaseV1::Prepared,
        amounts: AmountsV1 {
            token0: 1000,
            token1: 0,
        },
        reserve0: 10,
        requested_shares: 0,
        shares: 0,
        result: ResultV1::None,
    };
    (binding, cmd)
}
fn hex(e: &Env, b: soroban_sdk::Bytes) -> std::string::String {
    let _ = e;
    b.iter().map(|n| format!("{n:02x}")).collect()
}
#[test]
fn canonical_terms_bind_network_contracts_owner_id_bounds_and_requested_close_shares() {
    let e = Env::default();
    let (b, c) = fixture(&e);
    let n = BytesN::from_array(&e, &[0; 32]);
    let bh = binding_hash_v1(&e, &n, &b);
    let hash =
        |net: &BytesN<32>,
         r: &Address,
         v: &Address,
         id,
         owner: &Address,
         bind: &BytesN<32>,
         shares,
         t: &CommandTermsV1| terms_hash_v1(&e, net, r, v, id, owner, bind, shares, t);
    let base = hash(&n, &b.receipt, &b.vault, c.id, &c.owner, &bh, 0, &c.terms);
    let changes = [
        hash(
            &BytesN::from_array(&e, &[1; 32]),
            &b.receipt,
            &b.vault,
            c.id,
            &c.owner,
            &bh,
            0,
            &c.terms,
        ),
        hash(&n, &addr(&e, 7), &b.vault, c.id, &c.owner, &bh, 0, &c.terms),
        hash(
            &n,
            &b.receipt,
            &addr(&e, 7),
            c.id,
            &c.owner,
            &bh,
            0,
            &c.terms,
        ),
        hash(
            &n,
            &b.receipt,
            &b.vault,
            c.id + 1,
            &c.owner,
            &bh,
            0,
            &c.terms,
        ),
        hash(
            &n,
            &b.receipt,
            &b.vault,
            c.id,
            &addr(&e, 7),
            &bh,
            0,
            &c.terms,
        ),
        hash(&n, &b.receipt, &b.vault, c.id, &c.owner, &n, 0, &c.terms),
        hash(&n, &b.receipt, &b.vault, c.id, &c.owner, &bh, 1, &c.terms),
    ];
    for h in changes {
        assert_ne!(base, h)
    }
    if let CommandTermsV1::Open(t) = c.terms {
        for i in 0..7 {
            let mut changed = t.clone();
            match i {
                0 => changed.amount += 1,
                1 => changed.swap_in += 1,
                2 => changed.min_swap += 1,
                3 => changed.min_shares += 1,
                4 => changed.max_strategy_cost += 1,
                5 => changed.deadline += 1,
                _ => changed.config_revision += 1,
            };
            assert_ne!(
                base,
                hash(
                    &n,
                    &b.receipt,
                    &b.vault,
                    c.id,
                    &c.owner,
                    &bh,
                    0,
                    &CommandTermsV1::Open(changed)
                )
            );
        }
    }
}
#[test]
fn frozen_specs_and_wire_vectors() {
    let e = Env::default();
    let (b, c) = fixture(&e);
    let mut specs = serde_json::Map::new();
    macro_rules! spec {($($t:ty),*)=>{$(specs.insert(stringify!($t).into(),json!(<$t>::spec_xdr().iter().map(|n|format!("{n:02x}")).collect::<std::string::String>()));)*}}
    spec!(
        PhaseV1,
        CloseModeV1,
        AmountsV1,
        OpenTermsV1,
        CloseTermsV1,
        CommandTermsV1,
        CommandCreatedV1,
        FundedV1,
        ConvertedV1,
        MintedV1,
        InvestedV1,
        ExitV1,
        PaidV1,
        ReleasedV1,
        ResultV1,
        CommandV1,
        LiabilityV1,
        ReserveTotalsV1,
        AllocationV1,
        WaiverConsentV1,
        HolderConsentV1,
        BindingV1,
        ReceiptConfigV1,
        RiskV1,
        VaultBindingV1,
        VaultConfigV1,
        TransitionV1,
        ReceiptKeyV1,
        VaultKeyV1,
        Stage1Error
    );
    let mut keys = serde_json::Map::new();
    for (k, name) in [
        (ReceiptKeyV1::Stage1ConfigV1, "receipt.config"),
        (ReceiptKeyV1::Stage1Next, "receipt.next"),
        (ReceiptKeyV1::Stage1Busy, "receipt.busy"),
        (ReceiptKeyV1::Stage1Job(42), "receipt.job"),
    ] {
        keys.insert(name.into(), json!(hex(&e, k.to_xdr(&e))));
    }
    for (k, name) in [
        (VaultKeyV1::Stage1Receipt, "vault.binding"),
        (VaultKeyV1::Stage1RiskV1, "vault.risk"),
        (VaultKeyV1::Stage1Reserved(addr(&e, 4)), "vault.reserved"),
        (VaultKeyV1::Stage1Liability(42), "vault.liability"),
        (VaultKeyV1::Stage1Locked(addr(&e, 6)), "vault.lock"),
        (VaultKeyV1::Stage1Allocation(42), "vault.allocation"),
    ] {
        keys.insert(name.into(), json!(hex(&e, k.to_xdr(&e))));
    }
    let got = json!({"schema":1,"spec_xdr_hex":specs,"keys_xdr_hex":keys,"binding_xdr_hex":hex(&e,b.to_xdr(&e)),"command_xdr_hex":hex(&e,c.clone().to_xdr(&e)),"terms_hash_hex":hex(&e,c.terms_hash.into()),"scope":"wire-only, no financial or resource gate"});
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("wire-golden.json");
    if std::env::var_os("STAGE1_WRITE_GOLDEN").is_some() {
        assert!(!p.exists(), "never overwrite frozen ABI");
        std::fs::write(&p, serde_json::to_vec_pretty(&got).unwrap()).unwrap();
    }
    let expected: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
    assert_eq!(got, expected);
}
#[test]
fn u128_payload_size_is_bounded_and_round_trips() {
    use soroban_sdk::{IntoVal, TryFromVal, Val};
    let e = Env::default();
    let (_, mut c) = fixture(&e);
    c.amounts = AmountsV1 {
        token0: i128::MAX as u128,
        token1: i128::MAX as u128,
    };
    c.shares = i128::MAX as u128;
    let v: Val = c.clone().into_val(&e);
    assert_eq!(CommandV1::try_from_val(&e, &v).unwrap(), c);
    // Fixed fields, no Vec/Map/String supplied by callers. A structural size ceiling, not gas.
    assert!(c.to_xdr(&e).len() < 4096);
    let _ = String::from_str(&e, "wire-only");
}
#[test]
fn close_terms_and_binding_commit_every_field() {
    let e = Env::default();
    let (b, c) = fixture(&e);
    let net = BytesN::from_array(&e, &[0; 32]);
    let baseline = binding_hash_v1(&e, &net, &b);
    for i in 0..11 {
        let mut v = b.clone();
        match i {
            0 => v.schema += 1,
            1 => v.receipt = addr(&e, 9),
            2 => v.vault = addr(&e, 9),
            3 => v.pool = addr(&e, 9),
            4 => v.token0 = addr(&e, 9),
            5 => v.token1 = addr(&e, 9),
            6 => v.decimals0 += 1,
            7 => v.decimals1 += 1,
            8 => v.tick_lower -= 1,
            9 => v.tick_upper += 1,
            _ => {
                assert_ne!(
                    baseline,
                    binding_hash_v1(&e, &BytesN::from_array(&e, &[1; 32]), &v)
                );
                continue;
            }
        }
        assert_ne!(baseline, binding_hash_v1(&e, &net, &v));
    }
    let t = CloseTermsV1 {
        mode: CloseModeV1::InKind,
        min_amounts: AmountsV1 {
            token0: 1,
            token1: 2,
        },
        min_underlying: 0,
        deadline: 100,
        config_revision: 1,
    };
    let hash = |x: &CloseTermsV1, n| {
        terms_hash_v1(
            &e,
            &net,
            &b.receipt,
            &b.vault,
            42,
            &c.owner,
            &baseline,
            n,
            &CommandTermsV1::Close(x.clone()),
        )
    };
    let original = hash(&t, 7);
    for i in 0..7 {
        let mut v = t.clone();
        match i {
            0 => v.mode = CloseModeV1::ConvertToUnderlying,
            1 => v.min_amounts.token0 += 1,
            2 => v.min_amounts.token1 += 1,
            3 => v.min_underlying += 1,
            4 => v.deadline += 1,
            5 => v.config_revision += 1,
            _ => {
                assert_ne!(original, hash(&v, 8));
                continue;
            }
        }
        assert_ne!(original, hash(&v, 7));
    }
}
