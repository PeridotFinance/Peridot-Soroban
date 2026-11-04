//! Start of the funded in-kind route. No flash-loan or swap dependency.
//! This calculation does not authorize seizure or mutate debt.
use crate::math::mul_div;
use soroban_sdk::Env;
pub(crate) fn seize_shares(
    e: &Env,
    repay_value: u128,
    bonus_bps: u32,
    share_price: u128,
    price_scale: u128,
    available: u128,
) -> u128 {
    assert!(
        repay_value > 0 && share_price > 0 && price_scale > 0,
        "trusted positive valuation required"
    );
    assert!(bonus_bps <= 2000, "bonus too large");
    let base = mul_div(e, repay_value, price_scale, share_price);
    let seize = base
        .checked_add(mul_div(e, base, bonus_bps as u128, 10_000))
        .unwrap();
    assert!(
        seize > 0 && seize <= available,
        "insufficient collateral for funded repayment"
    );
    seize
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn funded_bonus_uses_value_not_cf() {
        let e = Env::default();
        assert_eq!(seize_shares(&e, 100, 800, 2, 1, 100), 54);
    }
    #[test]
    #[should_panic]
    fn no_zero_price_seizure() {
        seize_shares(&Env::default(), 100, 800, 0, 1, 100);
    }
}
