//! Native tests: SentoraStrategy + native ReceiptVault/SimplePeridottroller
//! against the REAL Sentora vault and escrow WASM fetched from Mainnet
//! (sha256 asserted). Tokens are real Stellar Asset Contracts.
extern crate std;

use super::*;
use receipt_vault::{ReceiptVault, ReceiptVaultClient};
use sha2::{Digest, Sha256};
use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{contract, contractimpl, contracttype, token, Bytes};

// Sentora's DEPLOYED bytes are not vendored (third-party code, no licence file). They are fetched read-only
// from Mainnet into a git-ignored directory and hash-pinned here:
//   cd harness/sentora-fork/tools && npm ci && node fetch-fork.cjs --out ../.scratch/fork3.json --wasm-dir ../.scratch/wasm
const WASM_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../harness/sentora-fork/.scratch/wasm"
);
fn deployed_wasm(hash: &str) -> std::vec::Vec<u8> {
    let path = std::format!("{WASM_DIR}/{hash}.wasm");
    let b = std::fs::read(&path).unwrap_or_else(|_| {
        panic!("{path} missing: fetch Sentora's deployed wasm with tools/fetch-fork.cjs --wasm-dir (see test.rs header)")
    });
    assert_eq!(hex(&Sha256::digest(&b)), hash, "deployed wasm hash pin");
    b
}
fn pins(e: &Env) -> (BytesN<32>, BytesN<32>) {
    let h = |x: &str| {
        let mut b = [0u8; 32];
        for i in 0..32 {
            b[i] = u8::from_str_radix(&x[2 * i..2 * i + 2], 16).unwrap();
        }
        BytesN::from_array(e, &b)
    };
    (h(VAULT_SHA), h(ESCROW_SHA))
}
const VAULT_SHA: &str = "ca6b85a13e53353912aeee2d4742830fc58076660dde64fdad4a6afbcca9674f";
const ESCROW_SHA: &str = "f22e4a52893d04b277c79bd19b3ff0b8771e1c0ca396a35d9339dd3a027ae299";
const E7: i128 = 10_000_000;
const WEEK: u64 = 7 * 24 * 3600;
const SEED: u128 = 1_000;
/// Absolute Sentora cap used by the fixture (large; the cap itself is tested separately).
const CAP: u128 = 1_000_000 * E7 as u128;
/// Raw-unit tolerance for figures shifted by the 1000-raw seed and buffer rounding.
fn close(a: i128, b: i128) -> bool {
    (a - b).abs() <= 10_000
}

fn hex(b: &[u8]) -> std::string::String {
    b.iter().map(|x| std::format!("{x:02x}")).collect()
}

// ------------------------------------------------------------------ Reflector-shaped mock oracle
#[contracttype]
#[derive(Clone)]
pub enum OAsset {
    Stellar(Address),
    Other(Symbol),
}
#[contracttype]
#[derive(Clone)]
pub struct OPrice {
    pub price: i128,
    pub timestamp: u64,
}
#[contract]
pub struct Oracle;
#[contractimpl]
impl Oracle {
    pub fn set(e: Env, asset: Address, price: i128) {
        e.storage().instance().set(&asset, &price);
    }
    pub fn decimals(_e: Env) -> u32 {
        7
    }
    pub fn resolution(_e: Env) -> u32 {
        300
    }
    pub fn lastprice(e: Env, asset: OAsset) -> Option<OPrice> {
        match asset {
            OAsset::Stellar(a) => e
                .storage()
                .instance()
                .get::<_, i128>(&a)
                .map(|price| OPrice {
                    price,
                    timestamp: e.ledger().timestamp(),
                }),
            _ => None,
        }
    }
}

struct Market {
    asset: Address,
    rv: Address,
    sentora: Option<Address>,
    strat: Option<Address>,
}

struct T {
    e: Env,
    admin: Address,
    sentora_admin: Address,
    escrow: Address,
    ctrl: Address,
    oracle: Address,
    usdc: Market,
    xlm: Market,
}

impl T {
    fn rv(&self, m: &Market) -> ReceiptVaultClient<'_> {
        ReceiptVaultClient::new(&self.e, &m.rv)
    }
    fn strat(&self, m: &Market) -> SentoraStrategyClient<'_> {
        SentoraStrategyClient::new(&self.e, m.strat.as_ref().unwrap())
    }
    fn ctrl(&self) -> SimplePeridottrollerClient<'_> {
        SimplePeridottrollerClient::new(&self.e, &self.ctrl)
    }
    fn mint(&self, asset: &Address, to: &Address, amt: i128) {
        token::StellarAssetClient::new(&self.e, asset).mint(to, &amt);
    }
    fn bal(&self, asset: &Address, who: &Address) -> i128 {
        token::TokenClient::new(&self.e, asset).balance(who)
    }
    fn price(&self, asset: &Address, p: i128) {
        OracleClient::new(&self.e, &self.oracle).set(asset, &p);
        self.ctrl().cache_price(asset);
    }
    fn advance(&self, secs: u64) {
        self.e.ledger().with_mut(|l| {
            l.timestamp += secs;
            l.sequence_number += (secs / 5) as u32;
        });
    }
    /// Keeper duty: ReceiptVault account health fails closed after 5 minutes.
    fn refresh(&self) {
        for m in [&self.usdc, &self.xlm] {
            if m.strat.is_some() {
                self.rv(m).refresh_boosted_underlying();
            }
        }
        self.price(&self.usdc.asset, E7);
        self.price(&self.xlm.asset, 3 * E7 / 10);
    }
}

fn new_market(
    e: &Env,
    admin: &Address,
    sentora_admin: &Address,
    escrow: &Address,
    with_sentora: bool,
    idle_bps: u32,
) -> Market {
    let asset = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let rv = e.register(ReceiptVault, ());
    let c = ReceiptVaultClient::new(e, &rv);
    c.initialize(&asset, &0u128, &0u128, admin);
    c.enable_static_rates(admin);
    c.set_cash_borrowing_enabled(admin, &true);
    c.set_idle_cash_buffer_bps(admin, &idle_bps);
    // Fresh cash-policy receipts require a permanent dead-share seed before any
    // strategy binding (ReceiptVault::seed_minimum_supply).
    token::StellarAssetClient::new(e, &asset).mint(admin, &(SEED as i128));
    c.seed_minimum_supply(admin, &SEED);
    if !with_sentora {
        return Market {
            asset,
            rv,
            sentora: None,
            strat: None,
        };
    }
    let sentora = e.register(
        deployed_wasm(VAULT_SHA).as_slice(),
        (sentora_admin.clone(), asset.clone(), escrow.clone()),
    );
    let strat = e.register(
        SentoraStrategy,
        (
            admin.clone(),
            rv.clone(),
            asset.clone(),
            sentora.clone(),
            escrow.clone(),
            WEEK,
            CAP,
            pins(e),
        ),
    );
    Market {
        asset,
        rv,
        sentora: Some(sentora),
        strat: Some(strat),
    }
}

/// Isolated group: one fresh controller, a Sentora-backed USDC market and a
/// Sentora-backed XLM market, both borrowable, CF 70 %.
fn setup() -> T {
    let e = Env::default();
    e.cost_estimate().disable_resource_limits();
    e.ledger().with_mut(|l| {
        l.timestamp = 1_795_600_000;
        l.sequence_number = 65_660_000;
    });
    e.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&e);
    let sentora_admin = Address::generate(&e);
    let escrow = e.register(
        deployed_wasm(ESCROW_SHA).as_slice(),
        (sentora_admin.clone(),),
    );
    let usdc = new_market(&e, &admin, &sentora_admin, &escrow, true, 2_000);
    let xlm = new_market(&e, &admin, &sentora_admin, &escrow, true, 2_000);
    let oracle = e.register(Oracle, ());
    let ctrl = e.register(SimplePeridottroller, ());
    let t = T {
        e,
        admin,
        sentora_admin,
        escrow,
        ctrl,
        oracle,
        usdc,
        xlm,
    };
    let c = t.ctrl();
    c.initialize(&t.admin);
    c.set_oracle(&t.oracle);
    for m in [&t.usdc, &t.xlm] {
        c.add_market(&m.rv);
        c.set_market_cf(&m.rv, &700_000u128);
        t.rv(m).set_collateral_factor(&700_000u128);
        t.rv(m).set_peridottroller(&t.ctrl);
        t.rv(m)
            .set_boosted_vault(&t.admin, m.strat.as_ref().unwrap());
    }
    t.refresh();
    t
}

fn sentora_bal(t: &T, m: &Market) -> i128 {
    t.bal(&m.asset, m.sentora.as_ref().unwrap())
}

#[test]
fn deposit_lands_in_real_sentora_under_strict_auth() {
    let t = setup();
    let e = &t.e;
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    let amt: u128 = 1_000 * E7 as u128;
    // Only the user signs: RV.deposit + its USDC transfer to the RV. Every
    // nested auth (RV -> strategy -> Sentora -> SAC) must come from the
    // contracts' own authorize_as_current_contract trees.
    e.mock_auths(&[MockAuth {
        address: &user,
        invoke: &MockAuthInvoke {
            contract: &t.usdc.rv,
            fn_name: "deposit",
            args: (user.clone(), amt).into_val(e),
            sub_invokes: &[MockAuthInvoke {
                contract: &t.usdc.asset,
                fn_name: "transfer",
                args: (user.clone(), t.usdc.rv.clone(), amt as i128).into_val(e),
                sub_invokes: &[],
            }],
        },
    }]);
    t.rv(&t.usdc).deposit(&user, &amt);
    // 20 % idle buffer stays in the receipt; 80 % reached the real Sentora vault.
    assert!(close(t.bal(&t.usdc.asset, &t.usdc.rv), 200 * E7));
    assert!(close(sentora_bal(&t, &t.usdc), 800 * E7));
    let s = t.strat(&t.usdc).get_state();
    assert_eq!(s.principal as i128, sentora_bal(&t, &t.usdc));
    assert_eq!(
        (s.idle, s.shares, s.total_assets),
        (0, s.principal, s.principal)
    );
    assert_eq!(t.rv(&t.usdc).get_exchange_rate(), 1_000_000);

    // Strict exit beyond the idle buffer: only the user signs RV.withdraw.
    let pt: u128 = 600 * E7 as u128;
    e.mock_auths(&[MockAuth {
        address: &user,
        invoke: &MockAuthInvoke {
            contract: &t.usdc.rv,
            fn_name: "withdraw",
            args: (user.clone(), pt).into_val(e),
            sub_invokes: &[],
        },
    }]);
    t.rv(&t.usdc).withdraw(&user, &pt);
    assert!(close(t.bal(&t.usdc.asset, &user), 600 * E7));
    assert!(close(sentora_bal(&t, &t.usdc), 400 * E7));

    // Permissionless claim with NO signer at all: the adapter authorizes the
    // escrow claim as its direct invoker; the escrow pays from its own balance.
    let claim = publish_epoch(
        &t,
        &t.usdc,
        t.usdc.strat.as_ref().unwrap(),
        1,
        5 * E7 as u128,
        1,
    );
    e.mock_auths(&[]);
    assert_eq!(t.strat(&t.usdc).claim_rewards(&1, &claim), 5 * E7 as u128);
}

#[test]
fn withdraw_exits_through_sentora_and_final_exit_is_complete() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    // 500 > 200 idle: the receipt redeems strategy shares, the strategy withdraws from Sentora.
    t.rv(&t.usdc).withdraw(&user, &(500 * E7 as u128));
    assert_eq!(t.bal(&t.usdc.asset, &user), 500 * E7);
    assert!(sentora_bal(&t, &t.usdc) < 800 * E7);
    let rest = t.rv(&t.usdc).get_ptoken_balance(&user);
    t.rv(&t.usdc).withdraw(&user, &rest);
    assert!(close(t.bal(&t.usdc.asset, &user), 1_000 * E7));
    // The seed's dead shares keep the receipt non-empty, so this is not the
    // strategy's final exit; at most rounding dust stays deployed.
    assert!(sentora_bal(&t, &t.usdc) <= 10_000);
}

#[test]
fn only_bound_receipt_may_deposit_or_withdraw() {
    let t = setup();
    let e = &t.e;
    let rando = Address::generate(e);
    t.mint(&t.usdc.asset, &rando, 10 * E7);
    let s = t.strat(&t.usdc);
    let v = soroban_sdk::vec![e, 10 * E7];
    assert!(s.try_deposit(&v, &v, &rando, &true).is_err());
    assert!(s
        .try_withdraw(&1, &soroban_sdk::vec![e, 0i128], &rando)
        .is_err());
}

/// One-leaf Merkle epoch on the real escrow for `owner` (root = leaf hash).
fn publish_epoch(
    t: &T,
    m: &Market,
    owner: &Address,
    deposit_id: u64,
    amount: u128,
    epoch: u32,
) -> ClaimParams {
    let e = &t.e;
    let vault = m.sentora.clone().unwrap();
    let mut payload = Bytes::from_slice(e, &[0u8]);
    payload.append(&(vault.clone(), epoch, deposit_id, owner.clone(), amount).to_xdr(e));
    let root: BytesN<32> = e.crypto().sha256(&payload).into();
    e.mock_all_auths_allowing_non_root_auth();
    t.mint(&m.asset, &t.escrow, amount as i128);
    let now = e.ledger().timestamp();
    let _: () = e.invoke_contract(
        &t.escrow,
        &Symbol::new(e, "set_rewards_root"),
        (m.asset.clone(), vault, epoch, root, amount, 1u32, now).into_val(e),
    );
    ClaimParams {
        amount,
        deposit_id,
        leaf_index: 0,
        proof: Vec::new(e),
    }
}

#[test]
fn reward_claim_vests_linearly_and_donations_are_ignored() {
    let t = setup();
    let user = Address::generate(&t.e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    let s = t.strat(&t.usdc);
    let nav0 = s.get_state().total_assets;

    // Donation to the adapter changes nothing.
    t.mint(&t.usdc.asset, t.usdc.strat.as_ref().unwrap(), 50 * E7);
    assert_eq!(s.get_state().total_assets, nav0);

    // Deposit id 1 is the adapter's first Sentora deposit (real vault counter).
    let claim = publish_epoch(
        &t,
        &t.usdc,
        t.usdc.strat.as_ref().unwrap(),
        1,
        40 * E7 as u128,
        1,
    );
    // A proof for someone else's leaf cannot be redirected to the adapter.
    let stranger = Address::generate(&t.e);
    let other = publish_epoch(&t, &t.usdc, &stranger, 1, 40 * E7 as u128, 2);
    assert!(s.try_claim_rewards(&2, &other).is_err());

    let got = s.claim_rewards(&1, &claim);
    assert_eq!(got, 40 * E7 as u128);
    assert_eq!(s.get_state().total_assets, nav0, "no step jump at claim");
    assert!(s.try_claim_rewards(&1, &claim).is_err(), "double claim");
    t.advance(WEEK / 2);
    let half = s.get_state().total_assets;
    assert!(close(half as i128, (nav0 + 20 * E7 as u128) as i128));
    t.advance(WEEK);
    assert_eq!(s.get_state().total_assets, nav0 + 40 * E7 as u128);
    // Supplier exchange rate rises once the receipt refreshes its cache.
    t.refresh();
    let rate = t.rv(&t.usdc).get_exchange_rate();
    assert!((1_039_990..=1_040_000).contains(&rate), "rate {rate}");
    // Admin can compound vested idle rewards back into Sentora as one deposit.
    let p0 = s.get_state().principal;
    s.invest_idle(&t.admin, &(40 * E7 as u128));
    assert_eq!(s.get_state().principal, p0 + 40 * E7 as u128);
}

#[test]
fn sentora_pause_limits_exits_to_idle_buffer() {
    let t = setup();
    let e = &t.e;
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    let sentora = t.usdc.sentora.clone().unwrap();
    let _: () = e.invoke_contract(&sentora, &Symbol::new(e, "set_status"), (true,).into_val(e));
    assert!(t
        .rv(&t.usdc)
        .try_withdraw(&user, &(500 * E7 as u128))
        .is_err());
    t.rv(&t.usdc).withdraw(&user, &(150 * E7 as u128));
    assert_eq!(t.bal(&t.usdc.asset, &user), 150 * E7);
    let _ = t.sentora_admin.clone();
}

#[test]
fn drained_sentora_marks_the_market_down() {
    let t = setup();
    let e = &t.e;
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    // Model an upgrade-and-drain by the Sentora admin: half the vault balance leaves.
    let sentora = t.usdc.sentora.clone().unwrap();
    let sink = Address::generate(e);
    token::TokenClient::new(e, &t.usdc.asset).transfer(&sentora, &sink, &(400 * E7));
    let s = t.strat(&t.usdc).get_state();
    assert_eq!(s.principal_marked as i128, s.principal as i128 - 400 * E7);
    t.refresh();
    let rate = t.rv(&t.usdc).get_exchange_rate();
    assert!((599_990..=600_010).contains(&rate), "rate {rate}");
}

/// deposit -> borrow -> withdraw -> price shock -> liquidation -> liquidator exit,
/// every exit path going through the real Sentora vault bytes.
#[test]
fn isolated_group_borrow_liquidate_and_exit() {
    let t = setup();
    let e = &t.e;
    let (lender, borrower, liq) = (
        Address::generate(e),
        Address::generate(e),
        Address::generate(e),
    );
    t.mint(&t.xlm.asset, &lender, 100_000 * E7);
    t.mint(&t.usdc.asset, &borrower, 1_000 * E7);
    t.mint(&t.xlm.asset, &liq, 10_000 * E7);
    t.rv(&t.xlm).deposit(&lender, &(100_000 * E7 as u128));
    t.rv(&t.usdc).deposit(&borrower, &(1_000 * E7 as u128));
    t.ctrl().enter_market(&borrower, &t.usdc.rv);
    t.ctrl().enter_market(&borrower, &t.xlm.rv);
    // Borrowable cash is only the receipt's idle buffer (20 % of 100k XLM).
    assert!(t
        .rv(&t.xlm)
        .try_borrow(&borrower, &(20_001 * E7 as u128))
        .is_err());
    // $1000 USDC at CF 70 % -> $700 -> 2333 XLM at $0.30. Borrow 2200.
    t.rv(&t.xlm).borrow(&borrower, &(2_200 * E7 as u128));
    assert_eq!(t.bal(&t.xlm.asset, &borrower), 2_200 * E7);
    // Lender partial exit beyond idle: redeemed from Sentora.
    let s_before = sentora_bal(&t, &t.xlm);
    t.rv(&t.xlm).withdraw(&lender, &(30_000 * E7 as u128));
    assert!(sentora_bal(&t, &t.xlm) < s_before);
    // XLM +20 % -> debt $792 vs $700 borrow limit.
    t.advance(60);
    t.rv(&t.usdc).refresh_boosted_underlying();
    t.rv(&t.xlm).refresh_boosted_underlying();
    t.price(&t.usdc.asset, E7);
    t.price(&t.xlm.asset, 36 * E7 / 100);
    approve(&t, &t.xlm.asset, &liq, &t.xlm.rv, 10_000 * E7);
    t.ctrl().liquidate(
        &borrower,
        &t.xlm.rv,
        &t.usdc.rv,
        &(1_000 * E7 as u128),
        &liq,
    );
    let seized = t.rv(&t.usdc).get_ptoken_balance(&liq);
    assert!(seized > 0);
    assert!(t.rv(&t.xlm).get_user_borrow_balance(&borrower) < 2_200 * E7 as u128);
    // Liquidator exits the seized pTokens; the receipt pulls the USDC from Sentora.
    let sent_before = sentora_bal(&t, &t.usdc);
    t.rv(&t.usdc).withdraw(&liq, &seized);
    assert!(t.bal(&t.usdc.asset, &liq) > 0);
    assert!(sentora_bal(&t, &t.usdc) < sent_before);
}

#[test]
fn stale_health_cache_blocks_borrow_until_refreshed() {
    let t = setup();
    let e = &t.e;
    let (lender, borrower) = (Address::generate(e), Address::generate(e));
    t.mint(&t.xlm.asset, &lender, 10_000 * E7);
    t.mint(&t.usdc.asset, &borrower, 1_000 * E7);
    t.rv(&t.xlm).deposit(&lender, &(10_000 * E7 as u128));
    t.rv(&t.usdc).deposit(&borrower, &(1_000 * E7 as u128));
    t.ctrl().enter_market(&borrower, &t.usdc.rv);
    t.ctrl().enter_market(&borrower, &t.xlm.rv);
    // Positive control: fresh cache, borrow works.
    t.rv(&t.xlm).borrow(&borrower, &(10 * E7 as u128));
    t.advance(301);
    t.price(&t.usdc.asset, E7);
    t.price(&t.xlm.asset, 3 * E7 / 10);
    // Prices are fresh, but the boosted health cache is 301 s old: fails closed.
    assert!(t
        .rv(&t.xlm)
        .try_borrow(&borrower, &(100 * E7 as u128))
        .is_err());
    t.refresh();
    t.rv(&t.xlm).borrow(&borrower, &(100 * E7 as u128));
}

fn approve(t: &T, asset: &Address, owner: &Address, spender: &Address, amt: i128) {
    let live = t.e.ledger().sequence() + 100_000;
    token::TokenClient::new(&t.e, asset).approve(owner, spender, &amt, &live);
}

#[test]
fn constructor_rejects_asset_mismatch() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let a1 = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let a2 = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let rv = e.register(ReceiptVault, ());
    ReceiptVaultClient::new(&e, &rv).initialize(&a1, &0u128, &0u128, &admin);
    let x = Address::generate(&e);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.register(
            SentoraStrategy,
            (
                admin.clone(),
                rv.clone(),
                a2.clone(),
                x.clone(),
                x.clone(),
                WEEK,
                CAP,
                pins(&e),
            ),
        );
    }));
    assert!(r.is_err());
}

/// Sentora paused: (1) receipt deposits must not be bricked by auto-invest, (2) liquidation still works
/// (it only moves pTokens), (3) the liquidator can exit only within the receipt's idle buffer.
#[test]
fn sentora_pause_deposits_liquidation_and_exits() {
    let t = setup();
    let e = &t.e;
    let (lender, borrower, liq, late) = (
        Address::generate(e),
        Address::generate(e),
        Address::generate(e),
        Address::generate(e),
    );
    t.mint(&t.xlm.asset, &lender, 100_000 * E7);
    t.mint(&t.usdc.asset, &borrower, 1_000 * E7);
    t.mint(&t.usdc.asset, &late, 10 * E7);
    t.mint(&t.xlm.asset, &liq, 10_000 * E7);
    t.rv(&t.xlm).deposit(&lender, &(100_000 * E7 as u128));
    t.rv(&t.usdc).deposit(&borrower, &(1_000 * E7 as u128));
    t.ctrl().enter_market(&borrower, &t.usdc.rv);
    t.ctrl().enter_market(&borrower, &t.xlm.rv);
    t.rv(&t.xlm).borrow(&borrower, &(2_200 * E7 as u128));
    let sentora = t.usdc.sentora.clone().unwrap();
    let _: () = e.invoke_contract(&sentora, &Symbol::new(e, "set_status"), (true,).into_val(e));
    // (1) a new USDC deposit still succeeds; the cash stays idle in the adapter.
    let p0 = t.strat(&t.usdc).get_state().principal;
    t.rv(&t.usdc).deposit(&late, &(10 * E7 as u128));
    let st = t.strat(&t.usdc).get_state();
    assert_eq!(st.principal, p0);
    assert!(st.idle > 0);
    // (2) liquidation while paused.
    t.advance(60);
    t.refresh();
    t.price(&t.xlm.asset, 36 * E7 / 100);
    approve(&t, &t.xlm.asset, &liq, &t.xlm.rv, 10_000 * E7);
    t.ctrl().liquidate(
        &borrower,
        &t.xlm.rv,
        &t.usdc.rv,
        &(1_000 * E7 as u128),
        &liq,
    );
    let seized = t.rv(&t.usdc).get_ptoken_balance(&liq);
    assert!(seized > 0);
    // (3) while paused, exits are limited to receipt cash plus the adapter's free idle cash.
    let idle_cap = t.bal(&t.usdc.asset, &t.usdc.rv) as u128 + t.strat(&t.usdc).get_state().idle;
    let rate = t.rv(&t.usdc).get_exchange_rate();
    let seized_underlying = seized * rate / 1_000_000;
    std::println!("paused: seized {seized_underlying} raw USDC vs idle capacity {idle_cap}");
    assert!(
        seized_underlying > idle_cap,
        "fixture must seize more than the idle capacity"
    );
    assert!(
        t.rv(&t.usdc).try_withdraw(&liq, &seized).is_err(),
        "full exit must wait for Sentora"
    );
    let small = (idle_cap / 2) * 1_000_000 / rate;
    t.rv(&t.usdc).withdraw(&liq, &small);
    // Unpause: the remaining seized pTokens exit through Sentora.
    let _: () = e.invoke_contract(
        &sentora,
        &Symbol::new(e, "set_status"),
        (false,).into_val(e),
    );
    let rest = t.rv(&t.usdc).get_ptoken_balance(&liq);
    t.rv(&t.usdc).withdraw(&liq, &rest);
    assert_eq!(t.rv(&t.usdc).get_ptoken_balance(&liq), 0);
}

// ------------------------------------------------------------------ phase 2: cap, kill path, reward fairness

#[test]
fn absolute_invest_cap_bounds_what_is_parked_in_sentora() {
    let t = setup();
    let s = t.strat(&t.usdc);
    s.set_invest_cap(&t.admin, &(500 * E7 as u128));
    let user = Address::generate(&t.e);
    t.mint(&t.usdc.asset, &user, 2_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(2_000 * E7 as u128));
    let st = s.get_state();
    assert_eq!(
        st.principal,
        500 * E7 as u128,
        "only the cap reaches Sentora"
    );
    assert_eq!(sentora_bal(&t, &t.usdc), 500 * E7);
    assert!(
        s.try_invest_idle(&t.admin, &(10 * E7 as u128)).is_err(),
        "cap reached"
    );
    // Exits keep working from idle cash and the capped Sentora principal.
    t.rv(&t.usdc).withdraw(&user, &(1_900 * E7 as u128));
    assert_eq!(t.bal(&t.usdc.asset, &user), 1_900 * E7);
}

#[test]
fn guardian_kill_pulls_everything_out_and_stops_investing() {
    let t = setup();
    let e = &t.e;
    let s = t.strat(&t.usdc);
    let guardian = Address::generate(e);
    let rando = Address::generate(e);
    s.set_guardian(&t.admin, &Some(guardian.clone()));
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    let nav0 = s.get_state().total_assets;
    assert!(
        s.try_emergency_exit(&rando).is_err(),
        "only guardian or admin"
    );
    // Strict auth: only the guardian signs.
    e.mock_auths(&[MockAuth {
        address: &guardian,
        invoke: &MockAuthInvoke {
            contract: t.usdc.strat.as_ref().unwrap(),
            fn_name: "emergency_exit",
            args: (guardian.clone(),).into_val(e),
            sub_invokes: &[],
        },
    }]);
    s.emergency_exit(&guardian);
    e.mock_all_auths_allowing_non_root_auth();
    let st = s.get_state();
    assert!(st.killed && !st.auto_invest);
    assert_eq!(st.principal, 0);
    assert_eq!(st.total_assets, nav0, "NAV unchanged by the pull-out");
    assert_eq!(sentora_bal(&t, &t.usdc), 0);
    // New deposits stay idle; admin cannot re-invest or re-enable auto-invest until resume.
    let late = Address::generate(e);
    t.mint(&t.usdc.asset, &late, 500 * E7);
    t.rv(&t.usdc).deposit(&late, &(500 * E7 as u128));
    assert_eq!(s.get_state().principal, 0);
    assert!(s.try_invest_idle(&t.admin, &(100 * E7 as u128)).is_err());
    assert!(s.try_set_auto_invest(&t.admin, &true).is_err());
    t.refresh();
    t.rv(&t.usdc)
        .withdraw(&user, &t.rv(&t.usdc).get_ptoken_balance(&user));
    assert!(close(t.bal(&t.usdc.asset, &user), 1_000 * E7));
    s.resume(&t.admin);
    s.invest_idle(&t.admin, &(100 * E7 as u128));
    assert_eq!(s.get_state().principal, 100 * E7 as u128);
}

#[test]
fn kill_while_sentora_paused_stops_investing_and_retry_completes_after_unpause() {
    let t = setup();
    let e = &t.e;
    let s = t.strat(&t.usdc);
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    let sentora = t.usdc.sentora.clone().unwrap();
    let _: () = e.invoke_contract(&sentora, &Symbol::new(e, "set_status"), (true,).into_val(e));
    s.emergency_exit(&t.admin);
    let st = s.get_state();
    assert!(
        st.killed && st.principal > 0,
        "kill sticks even though Sentora refused the withdrawal"
    );
    let _: () = e.invoke_contract(
        &sentora,
        &Symbol::new(e, "set_status"),
        (false,).into_val(e),
    );
    s.emergency_exit(&t.admin);
    assert_eq!(s.get_state().principal, 0);
}

#[test]
fn permissionless_exit_only_on_a_detected_drain() {
    let t = setup();
    let e = &t.e;
    let s = t.strat(&t.usdc);
    let anyone = Address::generate(e);
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    assert!(
        s.try_exit_if_drained(&anyone).is_err(),
        "no drain, no permissionless kill"
    );
    let sentora = t.usdc.sentora.clone().unwrap();
    let sink = Address::generate(e);
    token::TokenClient::new(e, &t.usdc.asset).transfer(&sentora, &sink, &(300 * E7));
    s.exit_if_drained(&anyone);
    let st = s.get_state();
    assert!(st.killed);
    // Everything recoverable (Sentora's remaining 500) came back; 300 stays booked as the marked-down loss.
    assert_eq!(st.principal, 300 * E7 as u128);
    assert_eq!(st.principal_marked, 0);
    assert_eq!(sentora_bal(&t, &t.usdc), 0);
}

/// Rewards land as market cash backing (adapter idle) and lift the supplier exchange rate linearly over the vest.
/// A just-in-time depositor right after the claim cannot take a step jump; a holder through the vest earns it.
#[test]
fn rewards_raise_supplier_rate_and_cannot_be_captured_just_in_time() {
    let t = setup();
    let e = &t.e;
    let a = Address::generate(e);
    let b = Address::generate(e);
    t.mint(&t.usdc.asset, &a, 1_000 * E7);
    t.mint(&t.usdc.asset, &b, 1_000 * E7);
    t.rv(&t.usdc).deposit(&a, &(1_000 * E7 as u128));
    let r0 = t.rv(&t.usdc).get_exchange_rate();
    let claim = publish_epoch(
        &t,
        &t.usdc,
        t.usdc.strat.as_ref().unwrap(),
        1,
        70 * E7 as u128,
        1,
    );
    t.strat(&t.usdc).claim_rewards(&1, &claim);
    t.refresh();
    assert_eq!(
        t.rv(&t.usdc).get_exchange_rate(),
        r0,
        "no step jump at claim"
    );
    // JIT: B deposits right after the claim and leaves one block later with no gain.
    t.rv(&t.usdc).deposit(&b, &(1_000 * E7 as u128));
    t.advance(5);
    t.refresh();
    t.rv(&t.usdc)
        .withdraw(&b, &t.rv(&t.usdc).get_ptoken_balance(&b));
    let b_gain = t.bal(&t.usdc.asset, &b) - 1_000 * E7;
    assert!(
        b_gain <= 2 * E7 / 100,
        "JIT gain {b_gain} (<= 5 s of vesting)"
    );
    // Holder A: rate rises linearly; full reward after the vest.
    t.advance(WEEK / 2);
    t.refresh();
    let r_half = t.rv(&t.usdc).get_exchange_rate();
    assert!(r_half > r0 && r_half < r0 + 70_000, "half-way {r_half}");
    t.advance(WEEK);
    t.refresh();
    let r_end = t.rv(&t.usdc).get_exchange_rate();
    assert!((r0 + 69_900..=r0 + 70_000).contains(&r_end), "end {r_end}");
    t.rv(&t.usdc)
        .withdraw(&a, &t.rv(&t.usdc).get_ptoken_balance(&a));
    let a_gain = t.bal(&t.usdc.asset, &a) - 1_000 * E7;
    assert!(
        a_gain > 69 * E7 && a_gain <= 70 * E7,
        "holder gain {a_gain}"
    );
}

#[test]
fn sentora_upgrade_fires_permissionless_kill_and_blocks_investing() {
    let t = setup();
    let e = &t.e;
    let s = t.strat(&t.usdc);
    let anyone = Address::generate(e);
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 1_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    assert!(
        s.try_exit_if_upgraded(&anyone).is_err(),
        "pinned code: no permissionless kill"
    );
    // Sentora's admin upgrades the vault (instant, no timelock) to other code (here: the escrow bytes).
    let other = e
        .deployer()
        .upload_contract_wasm(deployed_wasm(ESCROW_SHA).as_slice());
    let sentora = t.usdc.sentora.clone().unwrap();
    let _: () = e.invoke_contract(&sentora, &Symbol::new(e, "upgrade"), (other,).into_val(e));
    s.exit_if_upgraded(&anyone);
    let st = s.get_state();
    assert!(st.killed, "kill fired");
    // The new code has no compatible withdraw: the kill sticks, principal stays booked for a later retry.
    assert!(st.principal > 0);
    assert!(
        s.try_resume(&t.admin).is_err(),
        "cannot resume against unreviewed code"
    );
    let late = Address::generate(e);
    t.mint(&t.usdc.asset, &late, 100 * E7);
    t.rv(&t.usdc).deposit(&late, &(100 * E7 as u128));
    assert_eq!(s.get_state().principal, st.principal, "no new investing");
}

#[test]
fn constructor_rejects_wrong_pins() {
    let t = setup();
    let e = &t.e;
    let (v, esc) = pins(e);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.register(
            SentoraStrategy,
            (
                t.admin.clone(),
                t.usdc.rv.clone(),
                t.usdc.asset.clone(),
                t.usdc.sentora.clone().unwrap(),
                t.escrow.clone(),
                WEEK,
                CAP,
                (esc.clone(), v.clone()),
            ),
        );
    }));
    assert!(r.is_err());
}

#[test]
fn upgraded_vault_found_on_deposit_sets_kill_without_calling_it_and_deposit_succeeds() {
    let t = setup();
    let e = &t.e;
    let s = t.strat(&t.usdc);
    let user = Address::generate(e);
    t.mint(&t.usdc.asset, &user, 2_000 * E7);
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    let p0 = s.get_state().principal;
    let other = e
        .deployer()
        .upload_contract_wasm(deployed_wasm(ESCROW_SHA).as_slice());
    let sentora = t.usdc.sentora.clone().unwrap();
    let _: () = e.invoke_contract(&sentora, &Symbol::new(e, "upgrade"), (other,).into_val(e));
    // No exit call first: the next user deposit detects the new code hash.
    t.rv(&t.usdc).deposit(&user, &(1_000 * E7 as u128));
    let st = s.get_state();
    assert!(
        st.killed && !st.auto_invest,
        "kill flag set by the deposit path"
    );
    assert_eq!(
        st.principal, p0,
        "nothing deposited into the unreviewed code"
    );
}
