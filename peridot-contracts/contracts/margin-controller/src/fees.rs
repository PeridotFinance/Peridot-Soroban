use soroban_sdk::{Address, Env};

use crate::constants::*;
use crate::contract::MarginController;
use crate::events::{CloseFeeReserved, MarginFeesDistributed, OpenFeeReserved};
use crate::helpers::collect_margin_fee;
use crate::storage::*;

pub(crate) fn fee_for_amount(amount: u128, bps: u128) -> u128 {
    if bps > MAX_BASIS_FEE_BPS {
        panic!("fee too high");
    }
    // Split before multiplying so any representable amount remains valid.
    (amount / BPS_SCALE)
        .checked_mul(bps)
        .and_then(|whole| whole.checked_add((amount % BPS_SCALE) * bps / BPS_SCALE))
        .expect("fee overflow")
}

pub(crate) fn preview_open_fees(env: &Env, margin: u128, leverage: u128) -> PerpsOpenFeeQuote {
    if margin == 0 || !(1..=MAX_LEVERAGE_CAP).contains(&leverage) {
        panic!("invalid fee quote");
    }
    let persistent = env.storage().persistent();
    let open_bps = persistent.get(&DataKey::OpenFeeBps).unwrap_or(0u128);
    let close_bps = persistent.get(&DataKey::CloseFeeBps).unwrap_or(0u128);
    if close_bps > MAX_BASIS_FEE_BPS {
        panic!("fee too high");
    }
    let fee = if open_bps == 0 {
        0
    } else {
        fee_for_amount(
            margin.checked_mul(leverage).expect("fee overflow"),
            open_bps,
        )
    };
    PerpsOpenFeeQuote {
        open_fee_ptokens: fee,
        total_required_ptokens: margin.checked_add(fee).expect("fee overflow"),
        close_fee_bps: close_bps,
    }
}

pub(crate) fn bump_fee_terms_ttl(env: &Env, id: u64) {
    let key = DataKey::PerpsFeeTerms(id);
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub(crate) fn get_fee_terms(env: &Env, id: u64) -> Option<PerpsFeeTerms> {
    bump_fee_terms_ttl(env, id);
    env.storage().persistent().get(&DataKey::PerpsFeeTerms(id))
}

pub(crate) fn set_fee_terms(env: &Env, id: u64, terms: &PerpsFeeTerms) {
    env.storage()
        .persistent()
        .set(&DataKey::PerpsFeeTerms(id), terms);
    bump_fee_terms_ttl(env, id);
}

pub(crate) fn initialize_fee_terms(env: &Env, id: u64, quote: &PerpsOpenFeeQuote) {
    // Absent terms always mean fee-free, including positions created by old WASM.
    if quote.open_fee_ptokens > 0 || quote.close_fee_bps > 0 {
        set_fee_terms(
            env,
            id,
            &PerpsFeeTerms {
                open_fee_ptokens: quote.open_fee_ptokens,
                close_fee_bps: quote.close_fee_bps,
                close_fee_underlying: 0,
            },
        );
    }
}

pub(crate) fn collect_open_fee(env: &Env, id: u64, vault: &Address) {
    if let Some(mut terms) = get_fee_terms(env, id) {
        let fee = terms.open_fee_ptokens;
        if fee > 0 {
            terms.open_fee_ptokens = 0;
            set_fee_terms(env, id, &terms);
            let mut pending = pending_margin_fees(env, vault);
            pending.ptokens = pending.ptokens.checked_add(fee).expect("fee overflow");
            set_pending_margin_fees(env, vault, &pending);
            OpenFeeReserved {
                position_id: id,
                vault: vault.clone(),
                fee_ptokens: fee,
            }
            .publish(env);
        }
    }
}

pub(crate) fn set_close_execution_fee(env: &Env, id: u64, notional_underlying: u128) {
    if let Some(mut terms) = get_fee_terms(env, id) {
        terms.close_fee_underlying = fee_for_amount(notional_underlying, terms.close_fee_bps);
        set_fee_terms(env, id, &terms);
    }
}

pub(crate) fn pending_margin_fees(env: &Env, vault: &Address) -> PendingMarginFees {
    let key = DataKey::PendingMarginFees(vault.clone());
    let persistent = env.storage().persistent();
    let fees = persistent.get(&key).unwrap_or(PendingMarginFees {
        underlying: 0,
        ptokens: 0,
    });
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    fees
}

fn set_pending_margin_fees(env: &Env, vault: &Address, fees: &PendingMarginFees) {
    let key = DataKey::PendingMarginFees(vault.clone());
    env.storage().persistent().set(&key, fees);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Only call after debt is zero. The fee cannot consume another position's backing,
/// require a wallet top-up, or turn a solvent settlement into bad debt.
pub(crate) fn reserve_close_fee(env: &Env, id: u64, vault: &Address, surplus: u128) -> u128 {
    let Some(mut terms) = get_fee_terms(env, id) else {
        return 0;
    };
    let fee = terms.close_fee_underlying.min(surplus);
    if fee > 0 {
        let mut pending = pending_margin_fees(env, vault);
        pending.underlying = pending.underlying.checked_add(fee).expect("fee overflow");
        set_pending_margin_fees(env, vault, &pending);
        terms.close_fee_underlying = 0;
        set_fee_terms(env, id, &terms);
        CloseFeeReserved {
            position_id: id,
            vault: vault.clone(),
            fee_underlying: fee,
        }
        .publish(env);
    }
    fee
}

impl MarginController {
    pub(crate) fn distribute_margin_fees_impl(env: &Env, vault: &Address) -> u128 {
        let pending = pending_margin_fees(env, vault);
        let mut amount = pending.underlying;
        if amount == 0 && pending.ptokens == 0 {
            return 0;
        }
        let controller = env.current_contract_address();
        let client = ReceiptVaultClient::new(env, vault);
        let before = client.get_ptoken_balance(&controller);
        if before < pending.ptokens {
            panic!("fee backing missing");
        }
        env.storage()
            .persistent()
            .remove(&DataKey::PendingMarginFees(vault.clone()));
        let mut minted = 0;
        if amount > 0 {
            client.update_interest();
            let rate = client.get_exchange_rate();
            if rate == 0 {
                panic!("invalid exchange rate");
            }
            let minimum = rate.div_ceil(SCALE_1E6);
            if amount < minimum {
                // Underlying dust must not hold an otherwise distributable
                // pToken batch hostage. Preserve it for a later conversion.
                set_pending_margin_fees(
                    env,
                    vault,
                    &PendingMarginFees {
                        underlying: amount,
                        ptokens: 0,
                    },
                );
                amount = 0;
            }
        }
        if amount > 0 {
            let underlying = client.get_underlying_token();
            Self::authorize_controller_vault_deposit(env, vault, &underlying, &controller, amount);
            client.deposit(&controller, &amount);
            minted = client
                .get_ptoken_balance(&controller)
                .checked_sub(before)
                .expect("fee mint failed");
            if minted == 0 {
                panic!("fee too small to distribute");
            }
        }
        // Only actual newly minted pTokens enter the index. If depositing dust or
        // into a paused vault fails, rollback preserves the entire reserved balance.
        let distributed = minted.checked_add(pending.ptokens).expect("fee overflow");
        collect_margin_fee(env, vault, distributed);
        MarginFeesDistributed {
            vault: vault.clone(),
            underlying: amount,
            fee_ptokens: distributed,
        }
        .publish(env);
        distributed
    }
}
