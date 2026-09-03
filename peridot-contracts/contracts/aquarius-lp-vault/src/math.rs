use soroban_sdk::{Env, U256};

/// Splits `a * b / denom` into `(quotient, has_remainder)` without
/// overflowing the intermediate product.
///
/// A 256-bit product is exact for two `u128` factors. Returning `None` means
/// either division by zero or that the final quotient itself cannot fit in a
/// `u128`; intermediate overflow is never represented by a sentinel value.
fn try_mul_div_parts(env: &Env, a: u128, b: u128, denom: u128) -> Option<(u128, bool)> {
    if denom == 0 {
        return None;
    }
    if a == 0 || b == 0 {
        return Some((0, false));
    }

    let product = U256::from_u128(env, a).mul(&U256::from_u128(env, b));
    let divisor = U256::from_u128(env, denom);
    let quotient = product.div(&divisor).to_u128()?;
    let has_remainder = product.rem_euclid(&divisor).to_u128()? != 0;
    Some((quotient, has_remainder))
}

/// `floor(a * b / denom)`, or `None` if the denominator is zero or the exact
/// quotient does not fit in `u128`. Oracle paths use this form so hostile
/// external values degrade like a missing observation rather than trapping.
pub fn try_mul_div(env: &Env, a: u128, b: u128, denom: u128) -> Option<u128> {
    try_mul_div_parts(env, a, b, denom).map(|(q, _)| q)
}

/// `floor(a * b / denom)`.
pub fn mul_div(env: &Env, a: u128, b: u128, denom: u128) -> u128 {
    if denom == 0 {
        panic!("division by zero");
    }
    try_mul_div(env, a, b, denom).expect("mul-div result exceeds u128")
}

/// `ceil(a * b / denom)`. Used where rounding against the vault is the safe
/// direction (shares to burn, liquidity to redeem).
pub fn mul_div_ceil(env: &Env, a: u128, b: u128, denom: u128) -> u128 {
    if denom == 0 {
        panic!("division by zero");
    }
    match try_mul_div_parts(env, a, b, denom) {
        Some((q, true)) => q.checked_add(1).expect("mul-div result exceeds u128"),
        Some((q, false)) => q,
        None => panic!("mul-div result exceeds u128"),
    }
}

/// Integer square root via Newton's method.
///
/// Returns `floor(sqrt(n))`. Converges in at most ~7 iterations for u128
/// inputs, so the CPU cost is bounded and predictable.
pub fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    // Seed with a power of two above the true root so Newton converges from
    // above and is monotonically decreasing.
    let mut x = {
        let bits = 128 - n.leading_zeros();
        1u128 << bits.div_ceil(2)
    };
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            break;
        }
        x = y;
    }
    x
}

/// Applies a basis-point haircut: `amount * (10_000 - bps) / 10_000`.
pub fn apply_slippage_floor(env: &Env, amount: u128, bps: u32) -> u128 {
    let bps = bps as u128;
    if bps >= 10_000 {
        return 0;
    }
    mul_div(env, amount, 10_000u128 - bps, 10_000u128)
}

/// Saturating `u128 -> i128` conversion for cross-contract boundaries.
pub fn to_i128(value: u128) -> i128 {
    if value > i128::MAX as u128 {
        panic!("amount exceeds i128");
    }
    value as i128
}

/// Clamps a negative or overflowing `i128` to `0u128`.
pub fn to_u128(value: i128) -> u128 {
    if value <= 0 {
        0u128
    } else {
        value as u128
    }
}

/// Widest tick-spacing-aligned bound inside the protocol tick limit.
pub fn full_range_bounds(tick_spacing: i32, max_tick_abs: i32) -> (i32, i32) {
    if tick_spacing <= 0 {
        panic!("invalid tick spacing");
    }
    let aligned = (max_tick_abs / tick_spacing) * tick_spacing;
    if aligned <= 0 {
        panic!("invalid tick spacing");
    }
    (-aligned, aligned)
}

/// Greatest tick-spacing multiple less than or equal to `tick`.
pub fn align_tick_down(tick: i32, tick_spacing: i32) -> i32 {
    if tick_spacing <= 0 {
        panic!("invalid tick spacing");
    }
    let quotient = tick / tick_spacing;
    let remainder = tick % tick_spacing;
    if remainder < 0 {
        (quotient - 1) * tick_spacing
    } else {
        quotient * tick_spacing
    }
}

/// Builds a spacing-aligned range centered on the current pool tick.
///
/// The requested half-width is rounded outward. Near the protocol boundary,
/// the whole range is shifted back inside the usable domain without changing
/// its width.
pub fn centered_range(
    tick: i32,
    tick_spacing: i32,
    half_width_ticks: u32,
    max_tick_abs: i32,
) -> (i32, i32) {
    if tick_spacing <= 0 || half_width_ticks == 0 {
        panic!("invalid range parameters");
    }
    let half_requested = i32::try_from(half_width_ticks).expect("range too wide");
    let half = ((half_requested + tick_spacing - 1) / tick_spacing)
        .checked_mul(tick_spacing)
        .expect("range overflow");
    let (min_tick, max_tick) = full_range_bounds(tick_spacing, max_tick_abs);
    let width = half.checked_mul(2).expect("range overflow");
    if width >= max_tick.saturating_sub(min_tick) {
        panic!("range is not concentrated");
    }

    let center = align_tick_down(tick, tick_spacing);
    let mut lower = center.saturating_sub(half);
    let mut upper = center.saturating_add(half);
    if lower < min_tick {
        lower = min_tick;
        upper = min_tick.checked_add(width).expect("range overflow");
    }
    if upper > max_tick {
        upper = max_tick;
        lower = max_tick.checked_sub(width).expect("range overflow");
    }
    if lower >= upper || tick < lower || tick >= upper {
        panic!("invalid centered range");
    }
    (lower, upper)
}
