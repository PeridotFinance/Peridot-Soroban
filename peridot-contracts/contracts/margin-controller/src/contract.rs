//! Constructor, admin configuration and positions (WP2 owns the position flows).
use crate::common::*;
use crate::types::*;
use soroban_sdk::{contract, contractimpl, panic_with_error, Address, BytesN, Env, Vec};

#[contract]
pub struct MarginController;

#[contractimpl]
impl MarginController {
    /// Runs atomically with creation (no front-runnable initialize). Opens start paused.
    pub fn __constructor(env: Env, admin: Address, peridottroller: Address, guardian: Address, usdc: Address, xlm: Address) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        let s = env.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Guardian, &guardian);
        s.set(&Key::Ctrl, &peridottroller);
        s.set(&Key::Usdc, &usdc);
        s.set(&Key::Xlm, &xlm);
        s.set(&Key::OpensPaused, &true);
        s.set(&Key::MaxSlippage, &10_000u128); // 1 %
        s.set(&Key::Fees, &Fees { open_fee: 1_000, close_fee: 1_000, funding_apr: 20_000, bonus_liquidator: 30_000, bonus_backstop: 10_000 });
        s.set(&Key::BsParams, &BackstopParams { cooldown_s: 259_200, window_s: 172_800, min_deposit_pt: 1_000_000 });
        s.set(&Key::Headroom, &Headroom { hf_target: 1_250_000, hf_stress: 1_100_000, stress: 200_000, close_reserve_usd: 0, coef_long: 3_000_000, coef_short: 2_100_000, coef_largest: 4_600_000 });
        s.set(&Key::Counter, &0u64);
        bump(&env);
    }

    // ---- admin configuration (immediate; affects only this contract)
    pub fn set_market(env: Env, admin: Address, asset: Address, receipt_addr: Address) {
        require_admin(&env, &admin);
        if ReceiptClient::new(&env, &receipt_addr).get_underlying_token() != asset {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage().instance().set(&Key::Market(asset), &receipt_addr);
        bump(&env);
    }

    /// Enters the configured receipts as the margin account (live entered-market cap is 2).
    pub fn enter_markets(env: Env, admin: Address, assets: Vec<Address>) {
        require_admin(&env, &admin);
        for a in assets.iter() {
            ctrl(&env).enter_market(&me(&env), &receipt(&env, &a));
        }
        bump(&env);
    }

    pub fn set_pool(env: Env, admin: Address, pool: Address) {
        require_admin(&env, &admin);
        let toks = PoolClient::new(&env, &pool).get_tokens();
        let s = env.storage().instance();
        s.set(&Key::Pool, &pool);
        s.set(&Key::PoolTokens, &toks);
        bump(&env);
    }

    pub fn set_guardian(env: Env, admin: Address, guardian: Address) {
        require_admin(&env, &admin);
        env.storage().instance().set(&Key::Guardian, &guardian);
    }

    /// Kill switch for NEW positions: admin or guardian (guardian may only pause),
    /// immediate. Close, close_with_repay and liquidate always stay available.
    pub fn set_opens_paused(env: Env, caller: Address, paused: bool) {
        let admin: Address = need(&env, &Key::Admin);
        let guardian: Address = need(&env, &Key::Guardian);
        if caller != admin && caller != guardian {
            panic_with_error!(&env, Error::NotGuardian);
        }
        if caller != admin && !paused {
            panic_with_error!(&env, Error::NotAdmin);
        }
        caller.require_auth();
        env.storage().instance().set(&Key::OpensPaused, &paused);
        bump(&env);
    }

    pub fn propose_upgrade(env: Env, admin: Address, hash: BytesN<32>) {
        require_admin(&env, &admin);
        let eta = env.ledger().timestamp() + UPGRADE_TIMELOCK_SECS;
        env.storage()
            .instance()
            .set(&Key::PendingUpgrade, &PendingUpgrade { hash, eta });
    }

    pub fn upgrade(env: Env, admin: Address, hash: BytesN<32>) {
        require_admin(&env, &admin);
        let p: PendingUpgrade = need(&env, &Key::PendingUpgrade);
        if p.hash != hash {
            panic_with_error!(&env, Error::UpgradeMismatch);
        }
        if env.ledger().timestamp() < p.eta {
            panic_with_error!(&env, Error::Timelocked);
        }
        env.storage().instance().remove(&Key::PendingUpgrade);
        env.deployer().update_current_contract_wasm(hash);
    }

    // ---- WP2: risk, caps, fees (signatures frozen)
    /// 1e6 < max_leverage <= MAX_LEVERAGE_CAP; solvency: liq_ltv_max * (1e6 + bonuses) * (1e6 + slippage) < 1e18,
    /// with liq_ltv_max = 1e6 - 1e12 / (2 * max_leverage).
    pub fn set_risk(_env: Env, _admin: Address, _held_asset: Address, _max_leverage: u128) { unimplemented!("WP2") }
    pub fn set_caps(_env: Env, _admin: Address, _held_asset: Address, _caps: Caps) { unimplemented!("WP2") }
    /// Bounds: fees <= MAX_FEE, funding <= MAX_FUNDING_APR, bonuses sum <= MAX_TOTAL_BONUS; re-checks set_risk solvency for both sides.
    pub fn set_fees(_env: Env, _admin: Address, _fees: Fees) { unimplemented!("WP2") }
    pub fn set_max_slippage(_env: Env, _admin: Address, _max_slippage: u128) { unimplemented!("WP2") }

    // ---- WP2: user flows (signatures frozen)
    /// Long XLM: held XLM, debt USDC. Short XLM: held USDC, debt XLM. `collateral` raw held units
    /// from the user's wallet; `max_fee` = most USDC (raw) the user accepts for the open fee.
    pub fn open(_env: Env, _user: Address, _held_asset: Address, _debt_asset: Address, _collateral: u128, _borrow: u128, _min_out: u128, _max_fee: u128) -> u64 { unimplemented!("WP2") }
    pub fn close(_env: Env, _user: Address, _id: u64, _min_out: u128) { unimplemented!("WP2") }
    /// Oracle-free for USDC debt (long); short needs the price only for fees.
    pub fn close_with_repay(_env: Env, _user: Address, _id: u64) { unimplemented!("WP2") }
    /// At LTV >= position.liq_ltv. Liquidator pays at most `max_pay` (raw debt units) in the debt asset.
    pub fn liquidate(_env: Env, _liquidator: Address, _id: u64, _max_pay: u128) { unimplemented!("WP2") }
    /// Owner remainder after liquidation, kept internally until claimed. Returns pTokens sent.
    pub fn claim(_env: Env, _owner: Address, _asset: Address) -> u128 { unimplemented!("WP2") }

    // ---- WP2 views (signatures frozen)
    pub fn get_position(_env: Env, _id: u64) -> Option<Position> { unimplemented!("WP2") }
    pub fn get_position_counter(_env: Env) -> u64 { unimplemented!("WP2") }
    /// Debt incl. interest, raw debt units (rounded up).
    pub fn get_position_debt(_env: Env, _id: u64) -> u128 { unimplemented!("WP2") }
    /// Funding owed so far, raw USDC.
    pub fn get_position_funding(_env: Env, _id: u64) -> u128 { unimplemented!("WP2") }
    /// (debt_usd + funding_usd) / held_usd, 1e6, rounded up.
    pub fn get_position_ltv(_env: Env, _id: u64) -> u128 { unimplemented!("WP2") }
    pub fn is_liquidatable(_env: Env, _id: u64) -> bool { unimplemented!("WP2") }
    pub fn get_claim(_env: Env, _owner: Address, _asset: Address) -> u128 { unimplemented!("WP2") }
    pub fn get_side(_env: Env, _held_asset: Address) -> SideTotals { unimplemented!("WP2") }
    pub fn opens_paused(env: Env) -> bool {
        inst_get(&env, &Key::OpensPaused).unwrap_or(true)
    }
    pub fn bump_ttl(env: Env, ids: Vec<u64>) {
        bump(&env);
        for id in ids.iter() {
            let k = Key::Position(id);
            if env.storage().persistent().has(&k) {
                env.storage().persistent().extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
            }
        }
    }
}
