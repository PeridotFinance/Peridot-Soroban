use soroban_sdk::{Address, Env, IntoVal, Symbol};

use crate::events::{ExternalCallFailed, InterestOverflow, InvalidSeizeAttempt};

pub fn abort_seize(
    env: &Env,
    borrower: &Address,
    liquidator: &Address,
    amount: u128,
    reason: &str,
) -> ! {
    InvalidSeizeAttempt {
        borrower: borrower.clone(),
        liquidator: liquidator.clone(),
        requested: amount,
        reason: Symbol::new(env, reason),
    }
    .publish(env);
    panic!("{}", reason);
}

pub fn ensure_user_auth(_env: &Env, user: &Address) {
    user.require_auth();
}

pub fn checked_interest_product(
    env: &Env,
    amount: u128,
    yearly_rate_scaled: u128,
    elapsed: u128,
) -> u128 {
    amount
        .checked_mul(yearly_rate_scaled)
        .and_then(|v| v.checked_mul(elapsed))
        .unwrap_or_else(|| {
            InterestOverflow {
                amount,
                yearly_rate_scaled,
                elapsed,
            }
            .publish(env);
            panic!("interest overflow");
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallErrorKind {
    ContractRevert,
    HostError,
}

impl CallErrorKind {
    pub fn as_code(&self) -> u32 {
        match self {
            CallErrorKind::ContractRevert => 0,
            CallErrorKind::HostError => 1,
        }
    }
}

pub(crate) struct CallError {
    pub function: Symbol,
    pub kind: CallErrorKind,
}

pub(crate) fn emit_external_call_failure(
    env: &Env,
    contract: &Address,
    error: &CallError,
    recoverable: bool,
) {
    ExternalCallFailed {
        contract: contract.clone(),
        function: error.function.clone(),
        recoverable,
        failure_kind: error.kind.as_code(),
    }
    .publish(env);
}

pub(crate) fn try_call_contract<T, A>(
    env: &Env,
    contract: &Address,
    func: &str,
    args: A,
) -> Result<T, CallError>
where
    T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    A: IntoVal<Env, soroban_sdk::Vec<soroban_sdk::Val>>,
{
    use soroban_sdk::{InvokeError, Symbol, Val, Vec};
    let symbol = Symbol::new(env, func);
    let args_val: Vec<Val> = args.into_val(env);
    match env.try_invoke_contract::<T, InvokeError>(contract, &symbol, args_val) {
        Ok(Ok(val)) => Ok(val),
        Ok(Err(_)) => Err(CallError {
            function: symbol,
            kind: CallErrorKind::ContractRevert,
        }),
        Err(Ok(_)) | Err(Err(_)) => Err(CallError {
            function: symbol,
            kind: CallErrorKind::HostError,
        }),
    }
}

pub fn call_contract_or_panic<T, A>(env: &Env, contract: &Address, func: &str, args: A) -> T
where
    T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    A: IntoVal<Env, soroban_sdk::Vec<soroban_sdk::Val>>,
{
    match try_call_contract(env, contract, func, args) {
        Ok(val) => val,
        Err(err) => {
            emit_external_call_failure(env, contract, &err, false);
            panic!("{} call failed", func);
        }
    }
}
