//! Property-style sweeps over the jump-rate curve (deterministic, dependency-free).
//! These check invariants that must hold for ANY admissible parameters, not only the
//! hand-picked values in `test`.
use super::*;

const ONE: u128 = 1_000_000;
const MAX_PARAM: u128 = 10_000_000;

/// Small deterministic generator so failures reproduce exactly.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    fn below(&mut self, n: u128) -> u128 {
        (((self.next() as u128) << 64) | self.next() as u128) % n
    }
}

fn client(env: &Env, base: u128, mult: u128, jump: u128, kink: u128) -> JumpRateModelClient<'_> {
    env.mock_all_auths();
    let admin = Address::from_string(&String::from_str(env, DEFAULT_INIT_ADMIN));
    let id = env.register(JumpRateModel, ());
    let c = JumpRateModelClient::new(env, &id);
    c.initialize(&base, &mult, &jump, &kink, &admin);
    c
}

/// Rate at an exact utilization (parts per 1e6): cash 1e9 - util_cash, borrows util_borrows.
fn rate_at(c: &JumpRateModelClient, util: u128) -> u128 {
    // total assets = 1e12; borrows = util * 1e6 => utilization is exactly `util` (no reserves).
    let total = 1_000_000_000_000u128;
    let borrows = total / ONE * util;
    c.get_borrow_rate(&(total - borrows), &borrows, &0)
}

fn params(rng: &mut Lcg) -> (u128, u128, u128, u128) {
    (
        rng.below(MAX_PARAM + 1),
        rng.below(MAX_PARAM + 1),
        rng.below(MAX_PARAM + 1),
        rng.below(ONE + 1),
    )
}

#[test]
fn borrow_rate_never_decreases_as_utilization_rises() {
    let mut rng = Lcg(7);
    for _ in 0..40 {
        let env = Env::default();
        let (b, m, j, k) = params(&mut rng);
        let c = client(&env, b, m, j, k);
        let mut prev = 0u128;
        for step in 0..=100u128 {
            let r = rate_at(&c, step * ONE / 100);
            assert!(r >= prev, "rate fell at util step {step} for params {b},{m},{j},{k}");
            prev = r;
        }
    }
}

#[test]
fn rate_at_zero_borrows_is_exactly_the_base_rate() {
    let mut rng = Lcg(11);
    for _ in 0..40 {
        let env = Env::default();
        let (b, m, j, k) = params(&mut rng);
        let c = client(&env, b, m, j, k);
        let cash = rng.below(1_000_000_000_000_000);
        assert_eq!(c.get_borrow_rate(&cash, &0, &0), b);
    }
}

#[test]
fn rate_is_continuous_at_the_kink() {
    let mut rng = Lcg(13);
    for _ in 0..40 {
        let env = Env::default();
        let (b, m, j, mut k) = params(&mut rng);
        k = k.clamp(1, ONE - 1);
        let c = client(&env, b, m, j, k);
        let at = rate_at(&c, k);
        let after = rate_at(&c, k + 1);
        assert_eq!(at, b + k * m / ONE, "rate at the kink is the normal-slope rate");
        // One more unit of utilization may add at most one unit of the jump slope (plus rounding).
        assert!(after - at <= j / ONE + 1, "discontinuity at kink {k}: {at} -> {after}");
    }
}

#[test]
fn full_utilization_rate_matches_closed_form() {
    let mut rng = Lcg(17);
    for _ in 0..40 {
        let env = Env::default();
        let (b, m, j, k) = params(&mut rng);
        let c = client(&env, b, m, j, k);
        // No cash at all: utilization saturates at exactly 100%.
        let expected = b + k * m / ONE + (ONE - k) * j / ONE;
        assert_eq!(c.get_borrow_rate(&0, &1_000_000_000, &0), expected);
    }
}

#[test]
fn supply_rate_never_exceeds_borrow_rate_and_falls_as_reserve_factor_rises() {
    let mut rng = Lcg(19);
    for _ in 0..30 {
        let env = Env::default();
        let (b, m, j, k) = params(&mut rng);
        let c = client(&env, b, m, j, k);
        let cash = rng.below(1_000_000_000_000) + 1;
        let borrows = rng.below(1_000_000_000_000) + 1;
        let borrow = c.get_borrow_rate(&cash, &borrows, &0);
        let mut prev = u128::MAX;
        for rf in [0, ONE / 10, ONE / 2, ONE] {
            let supply = c.get_supply_rate(&cash, &borrows, &0, &rf);
            assert!(supply <= borrow, "supply {supply} exceeds borrow {borrow} at rf {rf}");
            assert!(supply <= prev, "supply rose with a higher reserve factor");
            prev = supply;
        }
        assert_eq!(c.get_supply_rate(&cash, &borrows, &0, &ONE), 0, "100% reserve factor");
    }
}

#[test]
fn reserves_reduce_the_denominator_and_raise_utilization() {
    let env = Env::default();
    let c = client(&env, 20_000, 180_000, 4_000_000, 800_000);
    let without = c.get_borrow_rate(&1_000_000, &1_000_000, &0);
    let with = c.get_borrow_rate(&1_000_000, &1_000_000, &500_000);
    assert!(with > without);
}

#[test]
#[should_panic(expected = "reserves exceed total assets")]
fn reserves_larger_than_assets_are_rejected() {
    let env = Env::default();
    let c = client(&env, 20_000, 180_000, 4_000_000, 800_000);
    c.get_borrow_rate(&100, &100, &201);
}

#[test]
fn extreme_inputs_do_not_panic_and_stay_bounded() {
    let env = Env::default();
    let c = client(&env, MAX_PARAM, MAX_PARAM, MAX_PARAM, ONE);
    let cap = MAX_PARAM + MAX_PARAM; // base + full normal slope; kink == 100% so no jump segment
    for (cash, borrows) in [(u128::MAX, u128::MAX), (0, u128::MAX), (u128::MAX, 1), (1, u128::MAX)] {
        let r = c.get_borrow_rate(&cash, &borrows, &0);
        assert!(r <= cap, "rate {r} above the parameter cap for {cash},{borrows}");
    }
}

/// PIN (latent limitation, unrealistic sizes): utilization is `borrows * 1e6 / denom` with a
/// saturating multiply. Once `borrows * 1e6` exceeds u128::MAX (about 3.4e32 raw units) the
/// product saturates and utilization is UNDERSTATED, so a fully borrowed market at that size
/// is priced as if it were far less utilized. Harmless at 7-decimal token scales in practice.
#[test]
fn utilization_is_understated_once_borrows_times_scale_saturates() {
    let env = Env::default();
    let c = client(&env, 0, ONE, 0, ONE); // rate == utilization (parts per 1e6)
    let safe = u128::MAX / ONE; // largest borrows whose product still fits
    assert!(c.get_borrow_rate(&0, &safe, &0) >= ONE - 1, "exact below the limit");
    let over = (u128::MAX / ONE) * 2; // 100% utilized in truth
    assert_eq!(c.get_borrow_rate(&0, &over, &0), ONE / 2, "saturated: reported as ~50%");
}
