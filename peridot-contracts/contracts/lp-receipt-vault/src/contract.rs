//! Lean principal engine. Deliberately NOT a #[contractimpl]: exposing these
//! primitives directly would bypass the still-in-development reward coordinator.
//! No borrow/repay, JRM, flash loans, collateral, liquidation, margin or lending
//! incentives. Aquarius remains responsible for pool/oracle/swap/range guards.

use crate::{storage::*, SCALE_1E6};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    token, vec, Address, Env, IntoVal, MuxedAddress, Symbol, Val, Vec, U256,
};
use stellar_tokens::fungible::{burnable::emit_burn, Base as TokenBase};

pub struct LpReceiptVault;

fn ratio(env: &Env, a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0, "zero denominator");
    U256::from_u128(env, a)
        .mul(&U256::from_u128(env, b))
        .div(&U256::from_u128(env, d))
        .to_u128()
        .expect("ratio overflow")
}
fn cash(env: &Env, asset: &Address) -> u128 {
    token::Client::new(env, asset)
        .balance(&env.current_contract_address())
        .try_into()
        .expect("negative cash")
}
fn managed(env: &Env) -> u128 {
    env.storage()
        .persistent()
        .get(&DataKey::ManagedCash)
        .expect("managed cash missing")
}
fn check_admin(env: &Env, admin: &Address) {
    ensure_initialized(env);
    assert_eq!(
        env.storage()
            .persistent()
            .get::<_, Address>(&DataKey::Admin),
        Some(admin.clone())
    );
    admin.require_auth();
}
fn strategy(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&DataKey::BoostedVault)
}
fn strategy_value(env: &Env) -> (u128, u128) {
    let Some(s) = strategy(env) else {
        return (0, 0);
    };
    let shares = token::Client::new(env, &s).balance(&env.current_contract_address());
    assert!(shares >= 0, "negative strategy shares");
    let nav = if shares == 0 {
        0
    } else {
        let quote: Vec<i128> = env.invoke_contract(
            &s,
            &Symbol::new(env, "get_asset_amounts_per_shares"),
            (shares,).into_val(env),
        );
        assert_eq!(quote.len(), 1, "LP receipt requires one settlement asset");
        let value = quote.get(0).unwrap();
        assert!(value > 0, "invalid strategy quote");
        value as u128
    };
    env.storage()
        .persistent()
        .set(&DataKey::BoostedUnderlyingCached, &nav);
    (shares as u128, nav)
}

impl LpReceiptVault {
    /// Native fixture initializer. A release needs atomic constructor/admin
    /// policy plus an independently tested legacy-state activation path.
    pub fn initialize(
        env: Env,
        asset: Address,
        supply_rate: u128,
        borrow_rate: u128,
        admin: Address,
    ) {
        assert_eq!(
            (supply_rate, borrow_rate),
            (0, 0),
            "LP receipts have no interest model"
        );
        let p = env.storage().persistent();
        assert!(
            !p.has(&DataKey::Initialized)
                && !p.has(&DataKey::Admin)
                && !p.has(&DataKey::UnderlyingToken)
                && TokenBase::total_supply(&env) == 0,
            "already initialized or legacy receipt"
        );
        assert!(!env.storage().instance().has(&LpKey::LpReceiptVersion));
        admin.require_auth();
        env.storage()
            .instance()
            .set(&LpKey::LpReceiptVersion, &1u32);
        env.storage()
            .instance()
            .set(&LpKey::LpDepositPaused, &false);
        p.set(&DataKey::Initialized, &true);
        p.set(&DataKey::Admin, &admin);
        p.set(&DataKey::UnderlyingToken, &asset);
        p.set(&DataKey::ManagedCash, &0u128);
        p.set(&DataKey::TotalDeposited, &0u128);
        p.set(&DataKey::InitialExchangeRate, &SCALE_1E6);
        p.set(&DataKey::SupplyCap, &0u128);
        p.set(&DataKey::IdleCashBufferBps, &0u32);
        let metadata = env.current_contract_address().to_string();
        TokenBase::set_metadata(
            &env,
            receipt_vault::PTOKEN_DECIMALS,
            metadata.clone(),
            metadata,
        );
        ensure_initialized(&env);
    }

    // Internal compatibility adapters for the shared native backing harness.
    // These do not enable a lending mode or maintain any borrowing/rate keys.
    pub fn enable_static_rates(env: Env, admin: Address) {
        check_admin(&env, &admin);
    }
    pub fn update_interest(env: Env) {
        ensure_initialized(&env);
    }
    pub fn get_underlying_token(env: Env) -> Address {
        ensure_initialized(&env)
    }
    pub fn get_boosted_vault(env: Env) -> Option<Address> {
        ensure_initialized(&env);
        strategy(&env)
    }
    pub fn get_total_ptokens(env: Env) -> u128 {
        ensure_initialized(&env);
        total_ptokens_supply(&env)
    }
    pub fn balance(env: Env, owner: Address) -> i128 {
        ensure_initialized(&env);
        TokenBase::balance(&env, &owner)
    }
    pub fn get_total_underlying(env: Env) -> u128 {
        let asset = ensure_initialized(&env);
        let idle = managed(&env);
        assert!(cash(&env, &asset) >= idle, "managed cash missing");
        idle.checked_add(strategy_value(&env).1)
            .expect("NAV overflow")
    }
    pub fn set_boosted_vault(env: Env, admin: Address, boosted: Address) {
        check_admin(&env, &admin);
        assert!(strategy(&env).is_none(), "strategy already bound");
        assert_eq!(total_ptokens_supply(&env), 0, "bind before funding");
        assert_eq!(managed(&env), 0, "bind before funding");
        let asset: Address = env.invoke_contract(
            &boosted,
            &Symbol::new(&env, "get_underlying"),
            Vec::new(&env),
        );
        assert_eq!(asset, ensure_initialized(&env), "strategy asset mismatch");
        let shape: Vec<i128> = env.invoke_contract(
            &boosted,
            &Symbol::new(&env, "get_asset_amounts_per_shares"),
            (0i128,).into_val(&env),
        );
        assert_eq!(shape, vec![&env, 0i128], "invalid LP strategy shape");
        env.storage()
            .persistent()
            .set(&DataKey::BoostedVault, &boosted);
        env.storage()
            .persistent()
            .set(&DataKey::BoostedUnderlyingCached, &0u128);
    }
    pub fn set_idle_cash_buffer_bps(env: Env, admin: Address, bps: u32) {
        check_admin(&env, &admin);
        assert!(bps <= 10_000, "invalid buffer");
        env.storage()
            .persistent()
            .set(&DataKey::IdleCashBufferBps, &bps);
    }
    pub fn set_supply_cap(env: Env, admin: Address, cap: u128) {
        check_admin(&env, &admin);
        env.storage().persistent().set(&DataKey::SupplyCap, &cap);
    }
    pub fn set_deposit_paused(env: Env, admin: Address, paused: bool) {
        check_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&LpKey::LpDepositPaused, &paused);
    }

    fn deploy(env: &Env, asset: &Address, amount: u128) {
        if amount < 10_000 {
            return;
        }
        let Some(boosted) = strategy(env) else {
            return;
        };
        assert!(
            amount <= managed(env),
            "cannot invest reserved or donated cash"
        );
        let before = cash(env, asset);
        let shares_before =
            token::Client::new(env, &boosted).balance(&env.current_contract_address());
        let amount_i = to_i128(amount);
        env.authorize_as_current_contract(vec![
            env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: asset.clone(),
                    fn_name: Symbol::new(env, "transfer"),
                    args: (env.current_contract_address(), boosted.clone(), amount_i).into_val(env),
                },
                sub_invocations: Vec::new(env),
            }),
        ]);
        let _: Val = env.invoke_contract(
            &boosted,
            &Symbol::new(env, "deposit"),
            (
                vec![env, amount_i],
                vec![env, amount_i],
                env.current_contract_address(),
                true,
            )
                .into_val(env),
        );
        assert_eq!(
            before.checked_sub(cash(env, asset)),
            Some(amount),
            "strategy deposit spend mismatch"
        );
        assert!(
            token::Client::new(env, &boosted).balance(&env.current_contract_address())
                > shares_before,
            "strategy deposit minted no shares"
        );
        env.storage()
            .persistent()
            .set(&DataKey::ManagedCash, &(managed(env) - amount));
    }

    pub fn deposit_excess_idle_cash(env: &Env, asset: &Address, available: u128) {
        assert_eq!(*asset, ensure_initialized(env));
        assert!(
            !env.storage()
                .instance()
                .get::<_, bool>(&LpKey::LpDepositPaused)
                .expect("pause state missing"),
            "deposit paused"
        );
        let nav = Self::get_total_underlying(env.clone());
        let bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::IdleCashBufferBps)
            .expect("buffer missing");
        let desired = ratio(env, nav, bps as u128, 10_000);
        Self::deploy(
            env,
            asset,
            available.min(managed(env)).saturating_sub(desired),
        );
    }
    pub fn rebalance_idle_cash(env: Env, admin: Address) {
        check_admin(&env, &admin);
        let asset = ensure_initialized(&env);
        Self::deposit_excess_idle_cash(&env, &asset, managed(&env));
    }

    /// Internal coordinator primitive: stop future emissions before rotating the
    /// reward denomination. Changes custody only, never receipt/escrow ownership.
    pub(crate) fn unwind_for_rotation(env: &Env, minimum: u128) -> u128 {
        assert!(minimum > 0, "rotation minimum required");
        let asset = ensure_initialized(env);
        let boosted = strategy(env).expect("strategy missing");
        let owned = token::Client::new(env, &boosted).balance(&env.current_contract_address());
        assert!(owned >= 0, "negative strategy shares");
        let before = cash(env, &asset);
        assert!(before >= managed(env), "managed cash missing");
        if owned > 0 {
            let result: Vec<i128> = env.invoke_contract(
                &boosted,
                &Symbol::new(env, "withdraw"),
                (owned, vec![env, 1i128], env.current_contract_address()).into_val(env),
            );
            let received = cash(env, &asset)
                .checked_sub(before)
                .expect("negative unwind");
            assert_eq!(
                result,
                vec![env, to_i128(received)],
                "unwind payout mismatch"
            );
            assert_eq!(
                token::Client::new(env, &boosted).balance(&env.current_contract_address()),
                0
            );
            env.storage().persistent().set(
                &DataKey::ManagedCash,
                &managed(env).checked_add(received).expect("cash overflow"),
            );
        }
        // Floor protects total managed settlement cash, including pre-existing idle.
        assert!(managed(env) >= minimum, "rotation below minimum");
        managed(env)
    }
    pub fn deposit(env: Env, owner: Address, amount: u128) {
        let asset = ensure_initialized(&env);
        owner.require_auth();
        assert_ne!(
            owner,
            env.current_contract_address(),
            "cannot deposit into reward escrow"
        );
        assert!(amount > 0, "zero deposit");
        assert!(
            !env.storage()
                .instance()
                .get::<_, bool>(&LpKey::LpDepositPaused)
                .expect("pause state missing"),
            "deposit paused"
        );
        let supply = total_ptokens_supply(&env);
        let nav = Self::get_total_underlying(env.clone());
        assert!(
            (supply == 0 && nav == 0) || (supply > 0 && nav > 0),
            "invalid receipt valuation"
        );
        let cap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::SupplyCap)
            .expect("cap missing");
        let after = nav.checked_add(amount).expect("NAV overflow");
        assert!(cap == 0 || after <= cap, "supply cap exceeded");
        // Round newly issued shares DOWN using the exact NAV ratio. Rounding an
        // intermediate exchange rate down first would overmint at some ratios.
        let shares = if supply == 0 {
            let rate = env
                .storage()
                .persistent()
                .get(&DataKey::InitialExchangeRate)
                .expect("initial rate missing");
            ratio(&env, amount, SCALE_1E6, rate)
        } else {
            ratio(&env, amount, supply, nav)
        };
        assert!(shares > 0, "deposit below share dust");
        let before = cash(&env, &asset);
        token::Client::new(&env, &asset).transfer(
            &owner,
            env.current_contract_address(),
            &to_i128(amount),
        );
        assert_eq!(
            cash(&env, &asset).checked_sub(before),
            Some(amount),
            "deposit transfer mismatch"
        );
        env.storage().persistent().set(
            &DataKey::ManagedCash,
            &managed(&env).checked_add(amount).expect("cash overflow"),
        );
        Self::deposit_excess_idle_cash(&env, &asset, managed(&env));
        TokenBase::mint(&env, &owner, to_i128(shares));
        let deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .expect("deposits missing");
        env.storage().persistent().set(
            &DataKey::TotalDeposited,
            &deposited.checked_add(amount).expect("deposits overflow"),
        );
        receipt_vault::Mint {
            minter: owner,
            mint_amount: amount,
            mint_tokens: shares,
        }
        .publish(&env);
    }

    pub fn withdraw(env: Env, owner: Address, shares: u128) {
        let asset = ensure_initialized(&env);
        owner.require_auth();
        assert_ne!(
            owner,
            env.current_contract_address(),
            "cannot withdraw reward escrow directly"
        );
        assert!(
            shares > 0 && to_i128(shares) <= TokenBase::balance(&env, &owner),
            "invalid withdrawal"
        );
        let supply = total_ptokens_supply(&env);
        let (owned_strategy_shares, strategy_nav) = strategy_value(&env);
        let nav = managed(&env)
            .checked_add(strategy_nav)
            .expect("NAV overflow");
        let target = ratio(&env, shares, nav, supply);
        let final_exit = shares == supply;
        let needed = target.saturating_sub(managed(&env));
        if owned_strategy_shares > 0 && (needed > 0 || final_exit) {
            let boosted = strategy(&env).expect("strategy missing");
            let burn = if final_exit {
                owned_strategy_shares
            } else {
                let floor = ratio(&env, needed, owned_strategy_shares, strategy_nav);
                // A one-share ceiling buffer; no caller-supplied pool quote.
                floor
                    .checked_add(1)
                    .expect("share overflow")
                    .min(owned_strategy_shares)
            };
            let before = cash(&env, &asset);
            let result: Vec<i128> = env.invoke_contract(
                &boosted,
                &Symbol::new(&env, "withdraw"),
                (
                    to_i128(burn),
                    vec![&env, to_i128(needed)],
                    env.current_contract_address(),
                )
                    .into_val(&env),
            );
            let received = cash(&env, &asset)
                .checked_sub(before)
                .expect("negative redemption");
            assert_eq!(
                result,
                vec![&env, to_i128(received)],
                "strategy payout mismatch"
            );
            assert!(received >= needed, "withdraw liquidity shortfall");
            let remaining =
                token::Client::new(&env, &boosted).balance(&env.current_contract_address());
            assert_eq!(
                remaining,
                to_i128(owned_strategy_shares - burn),
                "strategy share burn mismatch"
            );
            env.storage().persistent().set(
                &DataKey::ManagedCash,
                &managed(&env).checked_add(received).expect("cash overflow"),
            );
        }
        let payout = if final_exit { managed(&env) } else { target };
        assert!(
            payout > 0 && payout >= target && managed(&env) >= payout,
            "withdraw liquidity shortfall"
        );
        Self::pay_withdraw(&env, &asset, &owner, shares, payout, final_exit);
    }

    /// User-minimum protected, proportional exit. No receipt NAV quote is needed:
    /// redeem only this holder's fraction of actual strategy shares plus managed
    /// idle cash. User input never raises the strategy's floor above one raw unit,
    /// since that could request other holders' idle strategy cash. The complete
    /// user minimum is enforced AFTER measuring the actual receipt-side payout.
    pub fn withdraw_proportional(env: Env, owner: Address, shares: u128, minimum: u128) -> u128 {
        let asset = ensure_initialized(&env);
        owner.require_auth();
        assert_ne!(
            owner,
            env.current_contract_address(),
            "cannot withdraw reward escrow directly"
        );
        assert!(minimum > 0, "exit minimum required");
        assert!(
            shares > 0 && to_i128(shares) <= TokenBase::balance(&env, &owner),
            "invalid withdrawal"
        );
        let supply = total_ptokens_supply(&env);
        let final_exit = shares == supply;
        let idle = ratio(&env, managed(&env), shares, supply);
        let mut received = 0;
        if let Some(boosted) = strategy(&env) {
            let owned: u128 = token::Client::new(&env, &boosted)
                .balance(&env.current_contract_address())
                .try_into()
                .expect("negative strategy shares");
            if owned > 0 {
                let burn = ratio(&env, owned, shares, supply);
                assert!(burn > 0, "exit below strategy share dust");
                let before = cash(&env, &asset);
                let result: Vec<i128> = env.invoke_contract(
                    &boosted,
                    &Symbol::new(&env, "withdraw"),
                    (
                        to_i128(burn),
                        vec![&env, 1i128],
                        env.current_contract_address(),
                    )
                        .into_val(&env),
                );
                received = cash(&env, &asset)
                    .checked_sub(before)
                    .expect("negative redemption");
                assert_eq!(
                    result,
                    vec![&env, to_i128(received)],
                    "strategy payout mismatch"
                );
                assert!(received > 0, "strategy exit returned no funds");
                assert_eq!(
                    token::Client::new(&env, &boosted).balance(&env.current_contract_address()),
                    to_i128(owned - burn),
                    "strategy share burn mismatch"
                );
                env.storage().persistent().set(
                    &DataKey::ManagedCash,
                    &managed(&env).checked_add(received).expect("cash overflow"),
                );
            }
        }
        let payout = idle.checked_add(received).expect("payout overflow");
        assert!(
            payout >= minimum && managed(&env) >= payout,
            "exit below user minimum"
        );
        Self::pay_withdraw(&env, &asset, &owner, shares, payout, final_exit);
        payout
    }

    fn pay_withdraw(
        env: &Env,
        asset: &Address,
        owner: &Address,
        shares: u128,
        payout: u128,
        final_exit: bool,
    ) {
        TokenBase::update(env, Some(owner), None, to_i128(shares));
        emit_burn(env, owner, to_i128(shares));
        let before = cash(env, asset);
        assert!(before >= managed(env), "managed cash missing");
        let client = token::Client::new(env, asset);
        let owner_before = client.balance(owner);
        client.transfer(&env.current_contract_address(), owner, &to_i128(payout));
        assert_eq!(
            before.checked_sub(cash(env, asset)),
            Some(payout),
            "withdraw spend mismatch"
        );
        assert_eq!(
            client.balance(owner).checked_sub(owner_before),
            Some(to_i128(payout)),
            "withdraw receipt mismatch"
        );
        env.storage()
            .persistent()
            .set(&DataKey::ManagedCash, &(managed(env) - payout));
        let deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .expect("deposits missing");
        env.storage().persistent().set(
            &DataKey::TotalDeposited,
            &if final_exit {
                0
            } else {
                deposited.saturating_sub(payout)
            },
        );
        receipt_vault::Redeem {
            redeemer: owner.clone(),
            redeem_amount: payout,
            redeem_tokens: shares,
        }
        .publish(env);
    }
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        ensure_initialized(&env);
        let recipient = to.address();
        assert_ne!(
            from,
            env.current_contract_address(),
            "cannot transfer from reward escrow"
        );
        assert_ne!(
            recipient,
            env.current_contract_address(),
            "cannot transfer into reward escrow"
        );
        assert!(amount >= 0, "negative transfer");
        TokenBase::transfer(&env, &from, &to, amount);
    }
}
