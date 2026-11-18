#![no_std]
//! MOCK group receipt (StgV2 sleeve class) + staged vault, as one contract.
//!
//! It implements only what `cl-leveraged` and the keeper call, with the frozen stage-1 wire types
//! (`interfaces/cl-stage1-interface`): command records with typed results, fixed-owner payouts,
//! owner-before-deadline / anyone-after cancellation, share locks, and the DESIGN §2/§4 checkpoints.
//! Not modelled: Aquarius, fees, rewards, reserves categories, liabilities, revision fences.
//! `prepare_open_pair` and `assign_beneficiary` are NOT in DESIGN.md or the freeze (X1, X3).
//! The receipt holds a pre-funded token inventory and acts as its own conversion venue at the oracle
//! price, so token balances stay physical. All `mock_*` setters are test knobs with no auth.
use cl_lev_interface::{Asset, Ck, PairOpenTermsV1, ReflectorClient, PRICE_SCALE, SCALE_1E6};
use cl_lev_interface::stage1::{
    AmountsV1, CloseTermsV1, CommandCreatedV1, CommandTermsV1, CommandV1, ConvertedV1, ExitV1, FundedV1, InvestedV1,
    MintedV1, OpenTermsV1, PhaseV1, ReleasedV1, ResultV1,
};
use soroban_sdk::{contract, contracttype, contractimpl, token, Address, BytesN, Env};

#[contracttype]
#[derive(Clone)]
pub struct Cfg {
    pub token0: Address,
    pub token1: Address,
    pub oracle: Address,
}
#[contracttype]
enum K {
    Cfg,
    Next,
    Total,
    Back0,
    Back1,
    Nav,
    Fb,
    NavFail,
    Haircut,
    FailMask,
    Revision,
    Job(u64),
    Pair(u64),
    Units(Address),
    Locked(Address),
    Ben(Address),
}

pub const FAIL_CONVERT: u32 = 1;
pub const FAIL_MINT: u32 = 2;
pub const FAIL_INVEST: u32 = 4;
pub const FAIL_FUND: u32 = 8;
pub const FAIL_WITHDRAW: u32 = 16;

#[contract]
pub struct MockReceipt;

fn i<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(e: &Env, k: &K) -> Option<T> {
    e.storage().instance().get(k)
}
fn iset<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(e: &Env, k: &K, v: &T) {
    e.storage().instance().set(k, v)
}
fn p_get(e: &Env, k: &K) -> u128 {
    e.storage().persistent().get(k).unwrap_or(0)
}
fn p_set(e: &Env, k: &K, v: u128) {
    e.storage().persistent().set(k, &v)
}
fn cfg(e: &Env) -> Cfg {
    i(e, &K::Cfg).unwrap()
}
fn job(e: &Env, id: u64) -> CommandV1 {
    e.storage().persistent().get(&K::Job(id)).expect("unknown command")
}
fn save(e: &Env, j: &CommandV1) {
    e.storage().persistent().set(&K::Job(j.id), j)
}
fn fail(e: &Env, bit: u32) -> bool {
    i::<u32>(e, &K::FailMask).unwrap_or(0) & bit != 0
}
/// Oracle price (14 dp) and its age check (600 s, DESIGN §1.3).
fn price(e: &Env) -> u128 {
    let c = cfg(e);
    let pd = ReflectorClient::new(e, &c.oracle).lastprice(&Asset::Stellar(c.token0.clone())).expect("no price");
    let now = e.ledger().timestamp();
    assert!(pd.timestamp <= now && now - pd.timestamp <= 600, "OracleStale");
    pd.price as u128
}
fn depegged(p: u128) -> bool {
    let d = if p > PRICE_SCALE { p - PRICE_SCALE } else { PRICE_SCALE - p };
    d * 10_000 > PRICE_SCALE * 100
}
fn value(a0: u128, a1: u128, p: u128) -> u128 {
    a1 + a0 * p / PRICE_SCALE
}
fn pay(e: &Env, tok: &Address, to: &Address, n: u128) {
    if n > 0 {
        token::Client::new(e, tok).transfer(&e.current_contract_address(), to, &(n as i128));
    }
}
fn take(e: &Env, tok: &Address, from: &Address, n: u128) {
    if n > 0 {
        token::Client::new(e, tok).transfer(from, &e.current_contract_address(), &(n as i128));
    }
}
fn forward_ok(e: &Env, j: &CommandV1) {
    let deadline = match &j.terms {
        CommandTermsV1::Open(t) => t.deadline,
        CommandTermsV1::Close(t) => t.deadline,
    };
    assert!(e.ledger().timestamp() <= deadline, "Expired");
}

#[contractimpl]
impl MockReceipt {
    pub fn mock_init(e: Env, token0: Address, token1: Address, oracle: Address) {
        iset(&e, &K::Cfg, &Cfg { token0, token1, oracle });
        iset(&e, &K::Haircut, &200u32);
    }
    pub fn mock_fail(e: Env, mask: u32) {
        iset(&e, &K::FailMask, &mask);
    }
    pub fn mock_nav_fail(e: Env, on: bool) {
        iset(&e, &K::NavFail, &on);
    }
    /// Realised venue loss (bps > 0) or fee gain (bps < 0) on common backing, both legs.
    /// Gains are backed by the pre-funded inventory, so payouts stay physical.
    pub fn mock_loss(e: Env, bps: i32) {
        for k in [K::Back0, K::Back1] {
            let b: u128 = i(&e, &k).unwrap_or(0);
            let nb = (b as i128 - b as i128 * bps as i128 / 10_000) as u128;
            iset(&e, &k, &nb);
        }
    }
    pub fn mock_bump_revision(e: Env) {
        let r: u64 = i(&e, &K::Revision).unwrap_or(0);
        iset(&e, &K::Revision, &(r + 1));
    }
    pub fn units(e: Env, owner: Address) -> u128 {
        p_get(&e, &K::Units(owner))
    }
    pub fn locked(e: Env, owner: Address) -> u128 {
        p_get(&e, &K::Locked(owner))
    }
    pub fn beneficial(e: Env, owner: Address) -> u128 {
        p_get(&e, &K::Ben(owner))
    }
    pub fn stage1_revision(e: Env) -> u64 {
        i(&e, &K::Revision).unwrap_or(0)
    }

    // ---------------------------------------------------------------- checkpoints (DESIGN §2, §4)
    pub fn checkpoint_nav(e: Env) -> u128 {
        let p = price(&e);
        assert!(!depegged(p), "DepegGuard");
        assert!(!i::<bool>(&e, &K::NavFail).unwrap_or(false), "PoolOutOfBand");
        let total: u128 = i(&e, &K::Total).unwrap_or(0);
        assert!(total > 0, "empty");
        let rate = value(i(&e, &K::Back0).unwrap_or(0), i(&e, &K::Back1).unwrap_or(0), p) * SCALE_1E6 / total;
        iset(&e, &K::Nav, &Ck { rate, ts: e.ledger().timestamp(), ledger: e.ledger().sequence() });
        rate
    }
    pub fn checkpoint_fallback(e: Env) -> u128 {
        let p = price(&e);
        let cause = depegged(p) || i::<bool>(&e, &K::NavFail).unwrap_or(false);
        assert!(cause, "NavRefreshable");
        let total: u128 = i(&e, &K::Total).unwrap_or(0);
        let h: u32 = i(&e, &K::Haircut).unwrap();
        // Simplified DESIGN §2.1 lower bound: everything priced as token0 at min(p, 1), less the haircut.
        let pf = core::cmp::min(p, PRICE_SCALE) * (10_000 - h as u128) / 10_000;
        let b0: u128 = i(&e, &K::Back0).unwrap_or(0);
        let b1: u128 = i(&e, &K::Back1).unwrap_or(0);
        let rate = (b0 + b1) * pf / PRICE_SCALE * SCALE_1E6 / total;
        iset(&e, &K::Fb, &Ck { rate, ts: e.ledger().timestamp(), ledger: e.ledger().sequence() });
        rate
    }
    pub fn nav_checkpoint(e: Env) -> Option<Ck> {
        i(&e, &K::Nav)
    }
    pub fn fallback_checkpoint(e: Env) -> Option<Ck> {
        i(&e, &K::Fb)
    }

    // ---------------------------------------------------------------- commands (frozen names)
    pub fn prepare_open_pair(e: Env, owner: Address, t: PairOpenTermsV1) -> CommandCreatedV1 {
        owner.require_auth();
        let now = e.ledger().timestamp();
        assert!(t.amount0 + t.amount1 > 0 && t.deadline > now && t.deadline - now <= 3600, "InvalidTerms");
        assert!(t.swap_dir <= 1 && t.swap_in <= if t.swap_dir == 0 { t.amount0 } else { t.amount1 }, "InvalidTerms");
        let id: u64 = i(&e, &K::Next).unwrap_or(0);
        iset(&e, &K::Next, &(id + 1));
        let terms_hash = BytesN::from_array(&e, &[0u8; 32]);
        let j = CommandV1 {
            schema: 1,
            id,
            owner,
            terms: CommandTermsV1::Open(OpenTermsV1 {
                amount: t.amount0,
                swap_in: t.swap_in,
                min_swap: t.min_swap,
                min_shares: t.min_shares,
                max_strategy_cost: t.reserve0,
                deadline: t.deadline,
                config_revision: t.config_revision,
            }),
            terms_hash: terms_hash.clone(),
            phase: PhaseV1::Prepared,
            amounts: AmountsV1 { token0: t.amount0, token1: t.amount1 },
            reserve0: t.reserve0,
            requested_shares: 0,
            shares: 0,
            result: ResultV1::None,
        };
        save(&e, &j);
        e.storage().persistent().set(&K::Pair(id), &t);
        CommandCreatedV1 { id, terms_hash }
    }
    pub fn fund_open(e: Env, id: u64) -> FundedV1 {
        let mut j = job(&e, id);
        assert!(j.phase == PhaseV1::Prepared, "BadPhase");
        j.owner.require_auth();
        forward_ok(&e, &j);
        assert!(!fail(&e, FAIL_FUND), "fund rejected");
        let c = cfg(&e);
        take(&e, &c.token0, &j.owner, j.amounts.token0 + j.reserve0);
        take(&e, &c.token1, &j.owner, j.amounts.token1);
        let r = FundedV1 { amount0: j.amounts.token0, reserve0: j.reserve0 };
        j.phase = PhaseV1::Funded;
        j.result = ResultV1::Funded(r.clone());
        save(&e, &j);
        r
    }
    pub fn convert_open(e: Env, id: u64) -> ConvertedV1 {
        let mut j = job(&e, id);
        assert!(j.phase == PhaseV1::Funded, "BadPhase");
        forward_ok(&e, &j);
        assert!(!fail(&e, FAIL_CONVERT), "convert guard");
        let t: PairOpenTermsV1 = e.storage().persistent().get(&K::Pair(id)).unwrap();
        let p = price(&e);
        if t.swap_in > 0 {
            if t.swap_dir == 0 {
                let out = t.swap_in * p / PRICE_SCALE;
                assert!(out >= t.min_swap, "MinimumNotMet");
                j.amounts.token0 -= t.swap_in;
                j.amounts.token1 += out;
            } else {
                let out = t.swap_in * PRICE_SCALE / p;
                assert!(out >= t.min_swap, "MinimumNotMet");
                j.amounts.token1 -= t.swap_in;
                j.amounts.token0 += out;
            }
        }
        j.phase = PhaseV1::Converted;
        let r = ConvertedV1 { amounts: j.amounts.clone() };
        j.result = ResultV1::Converted(r.clone());
        save(&e, &j);
        r
    }
    pub fn mint_cash(e: Env, id: u64) -> MintedV1 {
        let mut j = job(&e, id);
        assert!(j.phase == PhaseV1::Converted, "BadPhase");
        forward_ok(&e, &j);
        assert!(!fail(&e, FAIL_MINT), "mint guard");
        let p = price(&e);
        let total: u128 = i(&e, &K::Total).unwrap_or(0);
        let (b0, b1): (u128, u128) = (i(&e, &K::Back0).unwrap_or(0), i(&e, &K::Back1).unwrap_or(0));
        let v = value(j.amounts.token0, j.amounts.token1, p);
        let shares = if total == 0 { v } else { v * total / value(b0, b1, p) };
        let min_shares = match &j.terms {
            CommandTermsV1::Open(t) => t.min_shares,
            _ => panic!("kind"),
        };
        assert!(shares >= min_shares && shares > 0, "MinimumNotMet");
        iset(&e, &K::Back0, &(b0 + j.amounts.token0));
        iset(&e, &K::Back1, &(b1 + j.amounts.token1));
        iset(&e, &K::Total, &(total + shares));
        p_set(&e, &K::Units(j.owner.clone()), p_get(&e, &K::Units(j.owner.clone())) + shares);
        p_set(&e, &K::Locked(j.owner.clone()), p_get(&e, &K::Locked(j.owner.clone())) + shares);
        j.shares = shares;
        j.phase = PhaseV1::MintedPendingInvestment;
        let r = MintedV1 { shares, locked: shares };
        j.result = ResultV1::Minted(r.clone());
        save(&e, &j);
        r
    }
    pub fn invest_cash(e: Env, id: u64) -> InvestedV1 {
        let mut j = job(&e, id);
        assert!(j.phase == PhaseV1::MintedPendingInvestment, "BadPhase");
        forward_ok(&e, &j);
        assert!(!fail(&e, FAIL_INVEST), "invest guard");
        let k = K::Locked(j.owner.clone());
        p_set(&e, &k, p_get(&e, &k) - j.shares);
        pay(&e, &cfg(&e).token0, &j.owner, j.reserve0);
        let r = InvestedV1 { liquidity: 0, cost0: 0, refund0: j.reserve0, unlocked: j.shares };
        j.reserve0 = 0;
        j.phase = PhaseV1::Invested;
        j.result = ResultV1::Invested(r.clone());
        save(&e, &j);
        r
    }
    pub fn prepare_close(e: Env, owner: Address, shares: u128, terms: CloseTermsV1) -> CommandCreatedV1 {
        owner.require_auth();
        let now = e.ledger().timestamp();
        assert!(shares > 0 && terms.deadline > now && terms.deadline - now <= 3600, "InvalidTerms");
        let (ku, kl) = (K::Units(owner.clone()), K::Locked(owner.clone()));
        let locked = p_get(&e, &kl);
        assert!(p_get(&e, &ku) - locked >= shares, "SharesLocked");
        p_set(&e, &kl, locked + shares);
        let id: u64 = i(&e, &K::Next).unwrap_or(0);
        iset(&e, &K::Next, &(id + 1));
        let terms_hash = BytesN::from_array(&e, &[0u8; 32]);
        save(&e, &CommandV1 {
            schema: 1,
            id,
            owner,
            terms: CommandTermsV1::Close(terms),
            terms_hash: terms_hash.clone(),
            phase: PhaseV1::CloseLocked,
            amounts: AmountsV1 { token0: 0, token1: 0 },
            reserve0: 0,
            requested_shares: shares,
            shares,
            result: ResultV1::None,
        });
        CommandCreatedV1 { id, terms_hash }
    }
    /// In-kind close (C2a): proportional backing paid to the fixed owner.
    pub fn withdraw_close(e: Env, id: u64) -> ExitV1 {
        let mut j = job(&e, id);
        assert!(j.phase == PhaseV1::CloseLocked, "BadPhase");
        forward_ok(&e, &j);
        assert!(!fail(&e, FAIL_WITHDRAW), "withdraw guard");
        let total: u128 = i(&e, &K::Total).unwrap();
        let (b0, b1): (u128, u128) = (i(&e, &K::Back0).unwrap_or(0), i(&e, &K::Back1).unwrap_or(0));
        let (o0, o1) = (b0 * j.shares / total, b1 * j.shares / total);
        iset(&e, &K::Back0, &(b0 - o0));
        iset(&e, &K::Back1, &(b1 - o1));
        iset(&e, &K::Total, &(total - j.shares));
        let (ku, kl) = (K::Units(j.owner.clone()), K::Locked(j.owner.clone()));
        p_set(&e, &ku, p_get(&e, &ku) - j.shares);
        p_set(&e, &kl, p_get(&e, &kl) - j.shares);
        let c = cfg(&e);
        pay(&e, &c.token0, &j.owner, o0);
        pay(&e, &c.token1, &j.owner, o1);
        let r = ExitV1 { amounts: AmountsV1 { token0: o0, token1: o1 }, paid: true };
        j.amounts = r.amounts.clone();
        j.phase = PhaseV1::Paid;
        j.result = ResultV1::Exit(r.clone());
        save(&e, &j);
        r
    }
    /// Owner before the deadline, anyone strictly after. Always to the fixed owner.
    pub fn cancel(e: Env, id: u64) -> ReleasedV1 {
        let mut j = job(&e, id);
        let deadline = match &j.terms {
            CommandTermsV1::Open(t) => t.deadline,
            CommandTermsV1::Close(t) => t.deadline,
        };
        if e.ledger().timestamp() <= deadline {
            j.owner.require_auth();
        }
        let c = cfg(&e);
        let r = match j.phase {
            PhaseV1::Prepared => ReleasedV1 { amounts: AmountsV1 { token0: 0, token1: 0 }, shares: 0 },
            PhaseV1::Funded | PhaseV1::Converted => {
                let a = AmountsV1 { token0: j.amounts.token0 + j.reserve0, token1: j.amounts.token1 };
                pay(&e, &c.token0, &j.owner, a.token0);
                pay(&e, &c.token1, &j.owner, a.token1);
                ReleasedV1 { amounts: a, shares: 0 }
            }
            PhaseV1::MintedPendingInvestment | PhaseV1::CloseLocked => {
                let k = K::Locked(j.owner.clone());
                p_set(&e, &k, p_get(&e, &k) - j.shares);
                pay(&e, &c.token0, &j.owner, j.reserve0);
                ReleasedV1 { amounts: AmountsV1 { token0: j.reserve0, token1: 0 }, shares: j.shares }
            }
            _ => panic!("AlreadyTerminal"),
        };
        j.reserve0 = 0;
        j.phase = PhaseV1::Cancelled;
        j.result = ResultV1::Released(r.clone());
        save(&e, &j);
        r
    }
    pub fn stage1_job(e: Env, id: u64) -> Option<CommandV1> {
        e.storage().persistent().get(&K::Job(id))
    }
    /// X3: custodian labels its own units with a beneficiary for AQUA accrual.
    pub fn assign_beneficiary(e: Env, custodian: Address, beneficiary: Address, shares: u128, add: bool) {
        custodian.require_auth();
        let k = K::Ben(beneficiary);
        let b = p_get(&e, &k);
        p_set(&e, &k, if add { b + shares } else { b - shares });
    }
}
