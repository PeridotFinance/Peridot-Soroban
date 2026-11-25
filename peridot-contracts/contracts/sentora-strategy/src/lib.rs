#![no_std]
//! RESEARCH PROTOTYPE, not reviewed for deployment.
//!
//! Boosted-vault adapter that lets exactly one Peridot ReceiptVault park idle
//! cash in a Sentora vault (IntoTheBlock `StellarVaults`, wasm ca6b85a1...).
//!
//! Sentora facts this adapter is built around (verified on Mainnet 2026-11-25):
//! * `deposit(from, amount, referral)` pulls `amount` of the single deposit asset
//!   into the Sentora vault contract itself and adds it to one per-address
//!   principal record. `withdraw(from, amount)` pays principal back 1:1. There is
//!   no share token, no NAV, no view function and no on-chain yield.
//! * Yield is paid separately by an escrow contract as admin-published Merkle
//!   epochs, one leaf per `(vault, epoch, deposit_id, owner, amount)`. Claims need
//!   `owner.require_auth()`; this adapter is the owner of its leaves.
//! * Admin (a 4-of-7 classic multisig) can pause deposits and withdrawals and
//!   upgrade both contracts instantly.
//!
//! So the adapter's NAV comes only from its own books: principal it put into
//! Sentora, marked down to Sentora's live token balance if that is lower, plus
//! idle cash it accounts for, minus claimed rewards that have not vested yet.
//! Unsolicited token transfers to the adapter are never counted.
//!
//! The ABI is the subset ReceiptVault calls on a boosted vault: `deposit`,
//! `withdraw`, `get_asset_amounts_per_shares`, `balance`, `total_supply`.

use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, token,
    Address, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};

const DAY_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP_THRESHOLD: u32 = 30 * DAY_LEDGERS;
const INSTANCE_BUMP_TO: u32 = 120 * DAY_LEDGERS;
const MAX_VEST_SECS: u64 = 30 * 24 * 3600;
/// ReceiptVault's MIN_BOOSTED_DEPLOY_AMOUNT; smaller deposits stay idle.
const MIN_INVEST: u128 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotReceipt = 1,
    NotAdmin = 2,
    BadAmount = 3,
    AssetMismatch = 4,
    BelowMinimum = 5,
    SentoraFailed = 6,
    BalanceDelta = 7,
    BadConfig = 8,
    NoReward = 9,
    Insolvent = 10,
    Killed = 11,
    NotGuardian = 12,
    NotDrained = 13,
    NotUpgraded = 14,
}

#[contracttype]
#[derive(Clone)]
enum Key {
    Admin,
    Receipt,
    Asset,
    Sentora,
    Escrow,
    Shares,
    Principal,
    Idle,
    Vest,
    VestSecs,
    AutoInvest,
    /// Absolute cap on principal booked in Sentora (raw units).
    InvestCap,
    /// Keeper role that may only trigger `emergency_exit`.
    Guardian,
    /// Set by `emergency_exit`/`exit_if_drained`/`exit_if_upgraded`; cleared only by the admin's `resume`.
    Killed,
    /// Pinned wasm hashes of the Sentora vault and escrow (checked on chain via `Address::executable`).
    VaultWasm,
    EscrowWasm,
}

/// Claimed-but-unvested reward: `amount` unlocks linearly from `start` to `end`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vest {
    pub amount: u128,
    pub start: u64,
    pub end: u64,
}

/// Same layout as the Sentora escrow's `ClaimParams` (field names define the XDR map).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimParams {
    pub amount: u128,
    pub deposit_id: u64,
    pub leaf_index: u32,
    pub proof: Vec<BytesN<32>>,
}

/// Same layout as the Sentora escrow's `RewardEpoch`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardEpoch {
    pub asset: Address,
    pub leaf_count: u32,
    pub program_end_ts: u64,
    pub root: BytesN<32>,
    pub total_rewards: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub principal: u128,
    pub principal_marked: u128,
    pub idle: u128,
    pub unvested: u128,
    pub total_assets: u128,
    pub shares: u128,
    pub auto_invest: bool,
    pub invest_cap: u128,
    pub killed: bool,
}

#[contractevent(topics = ["sentora_invest"])]
pub struct Invested {
    pub amount: u128,
    pub deposit_id: u64,
}

#[contractevent(topics = ["sentora_invest_skipped"])]
pub struct InvestSkipped {
    pub amount: u128,
}

#[contractevent(topics = ["sentora_divest"])]
pub struct Divested {
    pub amount: u128,
}

/// Kill path fired: new investing stops; `withdrawn` principal came back to idle
/// cash (0 if Sentora refused, e.g. paused; the keeper retries `emergency_exit`).
#[contractevent(topics = ["sentora_kill"])]
pub struct KillFired {
    pub by: Address,
    pub drained: bool,
    pub withdrawn: u128,
    pub remaining_principal: u128,
}

#[contractevent(topics = ["sentora_resume"])]
pub struct Resumed {}

#[contractevent(topics = ["sentora_claim"])]
pub struct Claimed {
    pub epoch: u32,
    pub deposit_id: u64,
    pub amount: u128,
}

fn get<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, k: &Key) -> T {
    e.storage()
        .instance()
        .get(k)
        .unwrap_or_else(|| panic_with_error!(e, Error::BadConfig))
}
fn get_u(e: &Env, k: &Key) -> u128 {
    e.storage().instance().get(k).unwrap_or(0)
}
fn put_u(e: &Env, k: &Key, v: u128) {
    e.storage().instance().set(k, &v);
}
fn bump(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TO);
}
fn i(e: &Env, v: u128) -> i128 {
    i128::try_from(v).unwrap_or_else(|_| panic_with_error!(e, Error::BadAmount))
}
fn mul_div(e: &Env, a: u128, b: u128, d: u128) -> u128 {
    soroban_sdk::U256::from_u128(e, a)
        .mul(&soroban_sdk::U256::from_u128(e, b))
        .div(&soroban_sdk::U256::from_u128(e, d))
        .to_u128()
        .unwrap_or_else(|| panic_with_error!(e, Error::BadAmount))
}
fn bal(e: &Env, asset: &Address, who: &Address) -> u128 {
    let b = token::TokenClient::new(e, asset).balance(who);
    if b < 0 {
        0
    } else {
        b as u128
    }
}

fn unvested(e: &Env) -> u128 {
    let Some(v) = e.storage().instance().get::<_, Vest>(&Key::Vest) else {
        return 0;
    };
    let now = e.ledger().timestamp();
    if now >= v.end || v.end <= v.start {
        return 0;
    }
    mul_div(
        e,
        v.amount,
        (v.end - now) as u128,
        (v.end - v.start) as u128,
    )
}

/// Principal booked in Sentora, capped by Sentora's live balance of the asset: a
/// drained or rugged vault marks the market down instead of hiding the loss.
fn principal_marked(e: &Env) -> (u128, u128) {
    let p = get_u(e, &Key::Principal);
    if p == 0 {
        return (0, 0);
    }
    let live = bal(e, &get(e, &Key::Asset), &get(e, &Key::Sentora));
    (p, p.min(live))
}

fn total_assets(e: &Env) -> u128 {
    let (_, marked) = principal_marked(e);
    marked
        .saturating_add(get_u(e, &Key::Idle))
        .saturating_sub(unvested(e))
}

fn require_receipt(e: &Env, from: &Address) {
    let r: Address = get(e, &Key::Receipt);
    if *from != r {
        panic_with_error!(e, Error::NotReceipt);
    }
    from.require_auth();
}

fn require_admin(e: &Env, admin: &Address) {
    let a: Address = get(e, &Key::Admin);
    if *admin != a {
        panic_with_error!(e, Error::NotAdmin);
    }
    admin.require_auth();
}

/// Idle cash that is neither unvested reward nor needed for anything else.
fn free_idle(e: &Env) -> u128 {
    get_u(e, &Key::Idle).saturating_sub(unvested(e))
}

fn killed(e: &Env) -> bool {
    e.storage().instance().get(&Key::Killed).unwrap_or(false)
}

/// Room left under the absolute Sentora cap.
fn cap_room(e: &Env) -> u128 {
    let cap: u128 = e.storage().instance().get(&Key::InvestCap).unwrap_or(0);
    cap.saturating_sub(get_u(e, &Key::Principal))
}

/// True when the Sentora vault or escrow no longer runs the pinned code (instant admin upgrade).
fn upgraded(e: &Env) -> bool {
    let pin = |k: &Key, who: &Key| {
        let want: BytesN<32> = get(e, k);
        let a: Address = get(e, who);
        a.executable() != Some(soroban_sdk::Executable::Wasm(want))
    };
    pin(&Key::VaultWasm, &Key::Sentora) || pin(&Key::EscrowWasm, &Key::Escrow)
}

fn invest(e: &Env, amount: u128, best_effort: bool) -> bool {
    if !killed(e) && upgraded(e) {
        // Never deposit into code we did not review. Set the kill flag only: calling the
        // unreviewed code from inside a user's deposit could revert that deposit (and the
        // flag with it). The pull-out is `exit_if_upgraded` / `emergency_exit`.
        e.storage().instance().set(&Key::Killed, &true);
        e.storage().instance().set(&Key::AutoInvest, &false);
        KillFired {
            by: e.current_contract_address(),
            drained: false,
            withdrawn: 0,
            remaining_principal: get_u(e, &Key::Principal),
        }
        .publish(e);
    }
    if killed(e) {
        if best_effort {
            InvestSkipped { amount }.publish(e);
            return false;
        }
        panic_with_error!(e, Error::Killed);
    }
    let amount = amount.min(cap_room(e));
    if amount < MIN_INVEST {
        if best_effort {
            return false;
        }
        panic_with_error!(e, Error::BadAmount);
    }
    let asset: Address = get(e, &Key::Asset);
    let sentora: Address = get(e, &Key::Sentora);
    let me = e.current_contract_address();
    // Read the balance BEFORE authorizing: an invoker-contract auth entry lives
    // only until this contract's next sub-contract call (soroban-env-host
    // auth.rs pop_frame), so any call in between would consume it.
    let before = bal(e, &asset, &me);
    // Sentora calls asset.transfer(from = this adapter, to = Sentora): that nested
    // call is not invoked by us directly, so authorize exactly that transfer.
    e.authorize_as_current_contract(soroban_sdk::vec![
        e,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: asset.clone(),
                fn_name: Symbol::new(e, "transfer"),
                args: (me.clone(), sentora.clone(), i(e, amount)).into_val(e),
            },
            sub_invocations: Vec::new(e),
        })
    ]);
    let res = e.try_invoke_contract::<u64, soroban_sdk::Error>(
        &sentora,
        &Symbol::new(e, "deposit"),
        (me.clone(), amount, Option::<String>::None).into_val(e),
    );
    let deposit_id = match res {
        Ok(Ok(id)) => id,
        _ => {
            // Sentora paused, upgraded to an incompatible ABI, or otherwise
            // failing: the sub-call rolled back. A best-effort auto-invest
            // keeps the cash idle so ReceiptVault deposits are never bricked;
            // the admin path (`invest_idle`) treats this as an error.
            if best_effort {
                InvestSkipped { amount }.publish(e);
                return false;
            }
            panic_with_error!(e, Error::SentoraFailed);
        }
    };
    if before.saturating_sub(bal(e, &asset, &me)) != amount {
        panic_with_error!(e, Error::BalanceDelta);
    }
    put_u(e, &Key::Principal, get_u(e, &Key::Principal) + amount);
    put_u(e, &Key::Idle, get_u(e, &Key::Idle) - amount);
    Invested { amount, deposit_id }.publish(e);
    true
}

fn divest(e: &Env, amount: u128) {
    let asset: Address = get(e, &Key::Asset);
    let sentora: Address = get(e, &Key::Sentora);
    let me = e.current_contract_address();
    let principal = get_u(e, &Key::Principal);
    if amount == 0 || amount > principal {
        panic_with_error!(e, Error::BadAmount);
    }
    let before = bal(e, &asset, &me);
    let res = e.try_invoke_contract::<Val, soroban_sdk::Error>(
        &sentora,
        &Symbol::new(e, "withdraw"),
        (me.clone(), amount).into_val(e),
    );
    if res.is_err() {
        panic_with_error!(e, Error::SentoraFailed);
    }
    if bal(e, &asset, &me).saturating_sub(before) != amount {
        panic_with_error!(e, Error::BalanceDelta);
    }
    put_u(e, &Key::Principal, principal - amount);
    put_u(e, &Key::Idle, get_u(e, &Key::Idle) + amount);
    Divested { amount }.publish(e);
}

/// Kill path: mark killed, then try to pull ALL recoverable principal (capped by
/// Sentora's live balance) back to idle cash. A Sentora failure (paused, broken
/// ABI) does not revert the kill: investing stays stopped and the call can be
/// retried. Shares and NAV accounting are unchanged by a successful pull-out.
fn kill(e: &Env, by: Address, drained: bool) {
    e.storage().instance().set(&Key::Killed, &true);
    e.storage().instance().set(&Key::AutoInvest, &false);
    let (booked, marked) = principal_marked(e);
    let want = booked.min(marked);
    let mut withdrawn = 0u128;
    if want > 0 {
        let asset: Address = get(e, &Key::Asset);
        let sentora: Address = get(e, &Key::Sentora);
        let me = e.current_contract_address();
        let before = bal(e, &asset, &me);
        let res = e.try_invoke_contract::<Val, soroban_sdk::Error>(
            &sentora,
            &Symbol::new(e, "withdraw"),
            (me.clone(), want).into_val(e),
        );
        if matches!(res, Ok(Ok(_))) {
            // Book what actually arrived (at most `want`). A short-paying (e.g. upgraded)
            // vault must not revert the kill: the shortfall stays booked as principal,
            // marked down by the live-balance rule, and the call can be retried.
            let got = bal(e, &asset, &me).saturating_sub(before).min(want);
            withdrawn = got;
            put_u(e, &Key::Principal, booked - got);
            put_u(e, &Key::Idle, get_u(e, &Key::Idle) + got);
        }
    }
    KillFired {
        by,
        drained,
        withdrawn,
        remaining_principal: get_u(e, &Key::Principal),
    }
    .publish(e);
}

#[contract]
pub struct SentoraStrategy;

#[contractimpl]
impl SentoraStrategy {
    /// `receipt_vault` must already be initialised with `asset` as its underlying.
    /// Sentora exposes no getters, so `sentora_vault`'s deposit asset and `escrow`
    /// cannot be verified on-chain; the deployment script must pin them.
    pub fn __constructor(
        e: Env,
        admin: Address,
        receipt_vault: Address,
        asset: Address,
        sentora_vault: Address,
        escrow: Address,
        vest_secs: u64,
        invest_cap: u128,
        pinned_wasm: (BytesN<32>, BytesN<32>),
    ) {
        if vest_secs == 0 || vest_secs > MAX_VEST_SECS {
            panic_with_error!(&e, Error::BadConfig);
        }
        let underlying: Address = e.invoke_contract(
            &receipt_vault,
            &Symbol::new(&e, "get_underlying_token"),
            Vec::new(&e),
        );
        if underlying != asset {
            panic_with_error!(&e, Error::AssetMismatch);
        }
        let s = e.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Receipt, &receipt_vault);
        s.set(&Key::Asset, &asset);
        s.set(&Key::Sentora, &sentora_vault);
        s.set(&Key::Escrow, &escrow);
        s.set(&Key::VestSecs, &vest_secs);
        s.set(&Key::AutoInvest, &true);
        s.set(&Key::InvestCap, &invest_cap);
        s.set(&Key::VaultWasm, &pinned_wasm.0);
        s.set(&Key::EscrowWasm, &pinned_wasm.1);
        if upgraded(&e) {
            // The deployment must pin the code that was reviewed (ca6b85a1.., f22e4a52..).
            panic_with_error!(&e, Error::BadConfig);
        }
        bump(&e);
    }

    // ------------------------------------------------ boosted-vault ABI (ReceiptVault only)

    pub fn deposit(
        e: Env,
        amounts_desired: Vec<i128>,
        amounts_min: Vec<i128>,
        from: Address,
        invest_now: bool,
    ) -> i128 {
        bump(&e);
        require_receipt(&e, &from);
        if amounts_desired.len() != 1 || amounts_min.len() != 1 {
            panic_with_error!(&e, Error::BadAmount);
        }
        let want = amounts_desired.get_unchecked(0);
        if want <= 0 {
            panic_with_error!(&e, Error::BadAmount);
        }
        let asset: Address = get(&e, &Key::Asset);
        let me = e.current_contract_address();
        let nav_before = total_assets(&e);
        let supply = get_u(&e, &Key::Shares);
        let before = bal(&e, &asset, &me);
        token::TokenClient::new(&e, &asset).transfer(&from, &me, &want);
        let received = bal(&e, &asset, &me).saturating_sub(before);
        if received != want as u128 || (received as i128) < amounts_min.get_unchecked(0) {
            panic_with_error!(&e, Error::BalanceDelta);
        }
        let shares = if supply == 0 {
            if nav_before > 0 {
                // Never let a new first depositor inherit ownerless value.
                panic_with_error!(&e, Error::BadConfig);
            }
            received
        } else {
            if nav_before == 0 {
                panic_with_error!(&e, Error::Insolvent);
            }
            mul_div(&e, received, supply, nav_before)
        };
        if shares == 0 {
            panic_with_error!(&e, Error::BelowMinimum);
        }
        put_u(&e, &Key::Shares, supply + shares);
        put_u(&e, &Key::Idle, get_u(&e, &Key::Idle) + received);
        let auto: bool = e.storage().instance().get(&Key::AutoInvest).unwrap_or(true);
        if invest_now && auto && received >= MIN_INVEST {
            invest(&e, received, true);
        }
        i(&e, shares)
    }

    pub fn withdraw(
        e: Env,
        withdraw_shares: i128,
        min_amounts_out: Vec<i128>,
        from: Address,
    ) -> Vec<i128> {
        bump(&e);
        require_receipt(&e, &from);
        let supply = get_u(&e, &Key::Shares);
        if withdraw_shares <= 0 || withdraw_shares as u128 > supply {
            panic_with_error!(&e, Error::BadAmount);
        }
        let shares = withdraw_shares as u128;
        let final_exit = shares == supply;
        let (booked, marked) = principal_marked(&e);
        let amount = if final_exit {
            // The last holder takes everything, vesting rewards included, so no
            // value is left for a future first depositor to capture.
            e.storage().instance().remove(&Key::Vest);
            marked + get_u(&e, &Key::Idle)
        } else {
            mul_div(&e, shares, total_assets(&e), supply)
        };
        let min = min_amounts_out.get(0).unwrap_or(0);
        if (amount as i128) < min || amount == 0 {
            panic_with_error!(&e, Error::BelowMinimum);
        }
        let free = if final_exit {
            get_u(&e, &Key::Idle)
        } else {
            free_idle(&e)
        };
        if amount > free {
            let need = amount - free;
            if need > booked.min(marked) {
                panic_with_error!(&e, Error::Insolvent);
            }
            divest(&e, need);
        }
        if final_exit && booked > marked {
            // Write off the unrecoverable part of a marked-down principal.
            put_u(&e, &Key::Principal, 0);
        }
        let asset: Address = get(&e, &Key::Asset);
        token::TokenClient::new(&e, &asset).transfer(
            &e.current_contract_address(),
            &from,
            &i(&e, amount),
        );
        put_u(&e, &Key::Idle, get_u(&e, &Key::Idle) - amount);
        put_u(&e, &Key::Shares, supply - shares);
        soroban_sdk::vec![&e, i(&e, amount)]
    }

    pub fn get_asset_amounts_per_shares(e: Env, vault_shares: i128) -> Vec<i128> {
        let supply = get_u(&e, &Key::Shares);
        let amount = if vault_shares <= 0 || supply == 0 {
            0
        } else {
            mul_div(&e, vault_shares as u128, total_assets(&e), supply)
        };
        soroban_sdk::vec![&e, i(&e, amount)]
    }

    pub fn balance(e: Env, id: Address) -> i128 {
        let r: Address = get(&e, &Key::Receipt);
        if id == r {
            i(&e, get_u(&e, &Key::Shares))
        } else {
            0
        }
    }

    pub fn total_supply(e: Env) -> i128 {
        i(&e, get_u(&e, &Key::Shares))
    }

    pub fn decimals(_e: Env) -> u32 {
        7
    }

    // ------------------------------------------------ rewards (permissionless)

    /// Claims one of this adapter's Sentora reward leaves. Anyone may submit the
    /// proof (from Sentora's off-chain API); the escrow verifies it and pays the
    /// adapter only. Only the measured underlying balance increase is credited,
    /// and it vests linearly over `vest_secs` so a just-in-time ReceiptVault
    /// depositor cannot capture a step increase in NAV.
    pub fn claim_rewards(e: Env, epoch: u32, claim: ClaimParams) -> u128 {
        bump(&e);
        let asset: Address = get(&e, &Key::Asset);
        let sentora: Address = get(&e, &Key::Sentora);
        let escrow: Address = get(&e, &Key::Escrow);
        let me = e.current_contract_address();
        let ep: Option<RewardEpoch> = e.invoke_contract(
            &escrow,
            &Symbol::new(&e, "get_reward_epoch"),
            (sentora.clone(), epoch).into_val(&e),
        );
        match ep {
            Some(ep) if ep.asset == asset => {}
            _ => panic_with_error!(&e, Error::AssetMismatch),
        }
        let deposit_id = claim.deposit_id;
        let before = bal(&e, &asset, &me);
        let res = e.try_invoke_contract::<Val, soroban_sdk::Error>(
            &escrow,
            &Symbol::new(&e, "claim"),
            (me.clone(), sentora, epoch, claim).into_val(&e),
        );
        if res.is_err() {
            panic_with_error!(&e, Error::NoReward);
        }
        let got = bal(&e, &asset, &me).saturating_sub(before);
        if got == 0 {
            panic_with_error!(&e, Error::NoReward);
        }
        let now = e.ledger().timestamp();
        let secs: u64 = get(&e, &Key::VestSecs);
        let carry = unvested(&e);
        e.storage().instance().set(
            &Key::Vest,
            &Vest {
                amount: carry + got,
                start: now,
                end: now + secs,
            },
        );
        put_u(&e, &Key::Idle, get_u(&e, &Key::Idle) + got);
        Claimed {
            epoch,
            deposit_id,
            amount: got,
        }
        .publish(&e);
        got
    }

    // ------------------------------------------------ admin

    /// Moves vested idle cash (claimed rewards, small deposits) into Sentora as
    /// one deposit, so reward leaves are not fragmented per user deposit.
    pub fn invest_idle(e: Env, admin: Address, amount: u128) {
        bump(&e);
        require_admin(&e, &admin);
        if amount < MIN_INVEST || amount > free_idle(&e) || amount > cap_room(&e) {
            panic_with_error!(&e, Error::BadAmount);
        }
        invest(&e, amount, false);
    }

    /// Absolute cap on principal parked in Sentora. Lowering it does not divest by
    /// itself (use `divest` or `emergency_exit`); it only stops further investing.
    pub fn set_invest_cap(e: Env, admin: Address, cap: u128) {
        require_admin(&e, &admin);
        e.storage().instance().set(&Key::InvestCap, &cap);
        bump(&e);
    }

    /// The keeper role that may trigger `emergency_exit` (and nothing else).
    pub fn set_guardian(e: Env, admin: Address, guardian: Option<Address>) {
        require_admin(&e, &admin);
        match guardian {
            Some(g) => e.storage().instance().set(&Key::Guardian, &g),
            None => e.storage().instance().remove(&Key::Guardian),
        }
        bump(&e);
    }

    /// Kill path for the off-chain Sentora watcher (admin/signer change, pause,
    /// TotalPrincipal/balance mismatch, anything else a contract cannot see):
    /// stops all new investing and pulls every recoverable unit of principal back
    /// into idle cash. Callable by the guardian or the admin; repeatable (retries
    /// a pull-out that Sentora refused). Funds can only move back to this adapter.
    pub fn emergency_exit(e: Env, caller: Address) {
        bump(&e);
        let admin: Address = get(&e, &Key::Admin);
        let guardian: Option<Address> = e.storage().instance().get(&Key::Guardian);
        if caller != admin && Some(caller.clone()) != guardian {
            panic_with_error!(&e, Error::NotGuardian);
        }
        caller.require_auth();
        kill(&e, caller, false);
    }

    /// Permissionless kill path: Sentora's live balance of the asset is below the
    /// principal this adapter booked there (a drain or rug). (Code changes are
    /// covered by `exit_if_upgraded`; Sentora's admin/signers and pause state are
    /// not readable on chain and stay with the guardian.)
    pub fn exit_if_drained(e: Env, caller: Address) {
        bump(&e);
        caller.require_auth();
        let (booked, marked) = principal_marked(&e);
        if booked == 0 || marked >= booked {
            panic_with_error!(&e, Error::NotDrained);
        }
        kill(&e, caller, true);
    }

    /// Permissionless kill path: the Sentora vault or escrow was upgraded away from the
    /// pinned wasm hash. Contracts can read another contract's executable
    /// (`Address::executable`), so this trigger needs no trusted watcher.
    pub fn exit_if_upgraded(e: Env, caller: Address) {
        bump(&e);
        caller.require_auth();
        if !upgraded(&e) {
            panic_with_error!(&e, Error::NotUpgraded);
        }
        kill(&e, caller, false);
    }

    /// Admin only: allow investing again after a kill (auto-invest stays off until
    /// re-enabled with `set_auto_invest`).
    pub fn resume(e: Env, admin: Address) {
        require_admin(&e, &admin);
        if upgraded(&e) {
            // Resuming against unreviewed Sentora code needs a new adapter (new pins), not a flag flip.
            panic_with_error!(&e, Error::NotUpgraded);
        }
        e.storage().instance().set(&Key::Killed, &false);
        Resumed {}.publish(&e);
        bump(&e);
    }

    /// Pulls principal back into idle cash held here (e.g. ahead of an announced
    /// Sentora pause or program end). Value and shares are unchanged.
    pub fn divest(e: Env, admin: Address, amount: u128) {
        bump(&e);
        require_admin(&e, &admin);
        divest(&e, amount);
    }

    pub fn set_auto_invest(e: Env, admin: Address, enabled: bool) {
        require_admin(&e, &admin);
        if enabled && (killed(&e) || upgraded(&e)) {
            panic_with_error!(&e, Error::Killed);
        }
        e.storage().instance().set(&Key::AutoInvest, &enabled);
        bump(&e);
    }

    pub fn set_vest_secs(e: Env, admin: Address, secs: u64) {
        require_admin(&e, &admin);
        if secs == 0 || secs > MAX_VEST_SECS {
            panic_with_error!(&e, Error::BadConfig);
        }
        e.storage().instance().set(&Key::VestSecs, &secs);
        bump(&e);
    }

    // ------------------------------------------------ views / keepalive

    pub fn get_state(e: Env) -> State {
        let (principal, principal_marked) = principal_marked(&e);
        State {
            principal,
            principal_marked,
            idle: get_u(&e, &Key::Idle),
            unvested: unvested(&e),
            total_assets: total_assets(&e),
            shares: get_u(&e, &Key::Shares),
            auto_invest: e.storage().instance().get(&Key::AutoInvest).unwrap_or(true),
            invest_cap: e.storage().instance().get(&Key::InvestCap).unwrap_or(0),
            killed: killed(&e),
        }
    }

    pub fn get_config(e: Env) -> (Address, Address, Address, Address, Address, u64) {
        (
            get(&e, &Key::Admin),
            get(&e, &Key::Receipt),
            get(&e, &Key::Asset),
            get(&e, &Key::Sentora),
            get(&e, &Key::Escrow),
            get(&e, &Key::VestSecs),
        )
    }

    /// Permissionless instance keepalive. Sentora's own `Deposit(adapter)` record
    /// (30-day max TTL) must be extended separately with ExtendFootprintTTL.
    pub fn bump_ttl(e: Env) {
        bump(&e);
    }
}

#[cfg(test)]
mod test;
