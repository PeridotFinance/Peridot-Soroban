//! Native-only reward conversion/recycling/payout shared by both receipt engines.
//! Complete fresh claims precede unit issuance/redemption. Outstanding raw IOUs
//! cannot fund payouts. Production ABI and legacy-harvest exclusion remain gated.
use crate::reward_claims::{
    cash, check, claim, owed, put_recycled, recycled, strategy, sync_pending, weight,
};
use crate::{reward_backing as backing, reward_ledger as ledger, ReceiptVault};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contracttype, token, Address, Env, IntoVal, Symbol, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Nothing,
    Deferred,
    Completed(u128),
}

fn swap(env: &Env, reward: &Address, raw: u128, minimum: u128) -> Option<u128> {
    let strategy = strategy(env);
    let amount: i128 = raw.try_into().expect("reward amount exceeds i128");
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: reward.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: (env.current_contract_address(), strategy.clone(), amount).into_val(env),
            },
            sub_invocations: Vec::new(env),
        })
    ]);
    match env.try_invoke_contract::<u128, soroban_sdk::InvokeError>(
        &strategy,
        &Symbol::new(env, "hybrid_swap"),
        (reward.clone(), raw, minimum).into_val(env),
    ) {
        Ok(Ok(output)) => Some(output),
        _ => None,
    }
}

/// Called only after this invocation's fresh all-stream checkpoint. Successful
/// earlier streams may remain settled when a later route defers. Units do not
/// change until EVERY stream's pending inventory is zero; retries cannot replay.
fn settle_recycled(env: &Env) -> bool {
    for asset in ledger::assets(env).iter() {
        if settle_one_recycled(env, &asset) == Outcome::Deferred {
            return false;
        }
    }
    check(env);
    true
}

fn settle_one_recycled(env: &Env, asset: &Address) -> Outcome {
    let raw = recycled(env, asset);
    if raw == 0 {
        return Outcome::Nothing;
    }
    if owed(env, asset) > 0 {
        return Outcome::Deferred;
    }
    let before = cash(env, asset);
    let snapshot = backing::begin_recycled(env);
    let Some(output) = swap(env, asset, raw, 1) else {
        return Outcome::Deferred;
    };
    assert_eq!(
        before.checked_sub(cash(env, asset)),
        Some(raw),
        "recycled swap mismatch"
    );
    assert_eq!(backing::finish_idle(env, snapshot, output, output, 0), 0);
    put_recycled(env, asset, 0);
    sync_pending(env);
    Outcome::Completed(output)
}

/// Permissionless preparation can settle one stream without issuing/redeeming
/// units. Later calls MUST still checkpoint new emissions; this is not permission
/// to skip them or a guarantee that continuously accruing multi-route calls fit.
pub fn recycle(env: &Env, asset: &Address) -> Outcome {
    claim(env);
    let result = settle_one_recycled(env, asset);
    check(env);
    result
}

pub fn compound(env: &Env, asset: &Address) -> Outcome {
    claim(env);
    if !settle_recycled(env) {
        return Outcome::Deferred;
    }
    if ledger::stream(env, asset).raw == 0 {
        return Outcome::Nothing;
    }
    if owed(env, asset) > 0 {
        return Outcome::Deferred;
    }
    let conversion = ledger::begin_conversion(env, asset);
    let snapshot = backing::begin(env);
    let Some(output) = swap(env, asset, conversion.raw_amount(), 1) else {
        return Outcome::Deferred;
    };
    let units = backing::finish_idle(env, snapshot, output, 0, 1);
    ledger::finish_conversion(env, conversion, units);
    check(env);
    Outcome::Completed(units)
}

pub fn redeem(env: &Env, owner: &Address, minimum: u128) -> Outcome {
    redeem_with(env, owner, minimum, ReceiptVault::withdraw)
}

/// Internal native engine selection, never a user-supplied callback or selector.
pub fn redeem_with(
    env: &Env,
    owner: &Address,
    minimum: u128,
    withdraw: fn(Env, Address, u128),
) -> Outcome {
    claim(env);
    if !settle_recycled(env) {
        return Outcome::Deferred;
    }
    for asset in ledger::assets(env).iter() {
        ledger::allocate(env, &asset, owner, weight(env, owner));
    }
    let units = backing::owner_units(env, owner);
    if units == 0 {
        return Outcome::Nothing;
    }
    // The backing withdrawal authenticates owner once in this same call frame.
    let paid = backing::redeem_with(env, owner, units, minimum, withdraw);
    check(env);
    Outcome::Completed(paid)
}

pub fn settle_reserved(
    env: &Env,
    asset: &Address,
    owner: &Address,
    raw: u128,
    minimum: u128,
) -> Outcome {
    owner.require_auth();
    check(env);
    if owed(env, asset) > 0 {
        claim(env);
        if owed(env, asset) > 0 {
            return Outcome::Deferred;
        }
    }
    let snapshot = ledger::begin_reserved(env, asset, owner, weight(env, owner), raw);
    let Some(output) = swap(env, asset, raw, minimum) else {
        return Outcome::Deferred;
    };
    let underlying = ReceiptVault::get_underlying_token(env.clone());
    let token = token::Client::new(env, &underlying);
    let before = token.balance(owner);
    let amount: i128 = output.try_into().expect("settlement exceeds i128");
    token.transfer(&env.current_contract_address(), owner, &amount);
    assert_eq!(
        token.balance(owner).checked_sub(before),
        Some(amount),
        "settlement payout mismatch"
    );
    assert!(output >= minimum, "settlement below minimum");
    ledger::finish_reserved(env, snapshot);
    check(env);
    Outcome::Completed(output)
}
