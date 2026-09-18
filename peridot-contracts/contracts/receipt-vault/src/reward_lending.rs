//! Native-only Aquarius/lending integration. No production entrypoint calls this.
//! Uses the lending engine's debt-inclusive NAV and collateral/liquidity checks,
//! never the lean engine's cash/strategy-only proportional exit. All operations
//! must execute atomically; callers cannot catch a failed local finish and commit.
//! Authentication is performed exactly once by Core in the same call frame;
//! adding a second require_auth here conflicts with Core's authorization args.
use crate::{
    reward_backing as backing, reward_claims as claims, reward_ledger as ledger,
    reward_share_hooks as hooks, ReceiptVault as Core, SeizeContext,
};
use soroban_sdk::{token, vec, Address, Env, Map};

fn ordinary_owner(env: &Env, owner: &Address) {
    assert_ne!(
        *owner,
        env.current_contract_address(),
        "escrow is not ordinary principal"
    );
}

pub fn deposit(env: &Env, owner: &Address, amount: u128) {
    ordinary_owner(env, owner);
    Core::validate_managed_cash(env);
    claims::claim(env);
    let before = claims::weight(env, owner);
    let supply = Core::get_total_ptokens(env.clone());
    let escrow = backing::state(env);
    for asset in ledger::assets(env).iter() {
        ledger::checkpoint(env, &asset, owner, before);
    }
    Core::deposit(env.clone(), owner.clone(), amount);
    let after = claims::weight(env, owner);
    assert_eq!(
        Core::get_total_ptokens(env.clone()).checked_sub(supply),
        Some(after.checked_sub(before).expect("deposit burned shares")),
        "unattributed minted shares"
    );
    assert_eq!(backing::state(env), escrow);
    for asset in ledger::assets(env).iter() {
        ledger::change_weight(env, &asset, owner, before, after);
    }
    claims::check(env);
}

/// Reserve historical raw rewards, but pay only principal here. Actual reward
/// conversion is separate. A minimum failure reverts the loan-aware exit too.
pub fn withdraw(
    env: &Env,
    owner: &Address,
    shares: u128,
    minimum: u128,
) -> (Map<Address, u128>, u128) {
    ordinary_owner(env, owner);
    assert!(
        shares > 0 && minimum > 0,
        "positive exit and minimum required"
    );
    claims::claim(env);
    let before = claims::weight(env, owner);
    let supply = Core::get_total_ptokens(env.clone());
    let escrow = backing::state(env);
    let mut reserved = Map::new(env);
    for asset in ledger::assets(env).iter() {
        reserved.set(
            asset.clone(),
            ledger::reserve(env, &asset, owner, before, shares),
        );
    }
    let underlying = Core::get_underlying_token(env.clone());
    let token = token::Client::new(env, &underlying);
    let cash = token.balance(owner);
    Core::withdraw_managed(env.clone(), owner.clone(), shares);
    let after = claims::weight(env, owner);
    assert_eq!(
        before.checked_sub(after),
        Some(shares),
        "exit burn mismatch"
    );
    assert_eq!(
        supply.checked_sub(Core::get_total_ptokens(env.clone())),
        Some(shares)
    );
    assert_eq!(backing::state(env), escrow);
    let paid: u128 = token
        .balance(owner)
        .checked_sub(cash)
        .expect("cash underflow")
        .try_into()
        .expect("negative payout");
    assert!(paid >= minimum, "exit below user minimum");
    for asset in ledger::assets(env).iter() {
        ledger::change_weight(env, &asset, owner, before, after);
    }
    claims::check(env);
    (reserved, paid)
}

pub fn borrow(env: &Env, owner: &Address, amount: u128) {
    ordinary_owner(env, owner);
    claims::claim(env);
    let supply = Core::get_total_ptokens(env.clone());
    let escrow = backing::state(env);
    Core::borrow_managed(env.clone(), owner.clone(), amount);
    assert_eq!(Core::get_total_ptokens(env.clone()), supply);
    assert_eq!(backing::state(env), escrow);
    claims::check(env);
}

// Repayment deliberately uses Core::repay directly: it only exchanges real
// settlement cash for debt, with no share mutation or strategy redeployment.
// Do not make risk-reducing repayment depend on reward observation availability.

pub fn reinvest(env: &Env, admin: &Address) {
    // Custody-only: no weight change, claim/conversion or donation adoption.
    Core::rebalance_managed(env.clone(), admin.clone());
}

pub fn redeem_rewards(
    env: &Env,
    owner: &Address,
    minimum: u128,
) -> crate::reward_settlement::Outcome {
    Core::validate_managed_cash(env);
    crate::reward_settlement::redeem_with(env, owner, minimum, Core::withdraw_managed)
}

pub fn transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
    claims::claim(env);
    let snapshot = hooks::begin(env, vec![env, from.clone(), to.clone()]);
    Core::transfer(env.clone(), from.clone(), to.clone().into(), amount);
    hooks::finish(env, snapshot);
    claims::check(env);
}

pub fn transfer_from(env: &Env, spender: &Address, from: &Address, to: &Address, amount: i128) {
    claims::claim(env);
    let snapshot = hooks::begin(env, vec![env, from.clone(), to.clone()]);
    Core::transfer_from(
        env.clone(),
        spender.clone(),
        from.clone(),
        to.clone(),
        amount,
    );
    hooks::finish(env, snapshot);
    claims::check(env);
}

pub fn seize(
    env: &Env,
    borrower: &Address,
    liquidator: &Address,
    amount: u128,
    ctx: Option<SeizeContext>,
) {
    claims::claim(env);
    let mut owners = vec![env, borrower.clone(), liquidator.clone()];
    if let Some(ref c) = ctx {
        if let Some(ref recipient) = c.fee_recipient {
            owners.push_back(recipient.clone());
        }
    }
    let snapshot = hooks::begin(env, owners);
    Core::seize(
        env.clone(),
        borrower.clone(),
        liquidator.clone(),
        amount,
        ctx,
    );
    hooks::finish(env, snapshot);
    claims::check(env);
}
