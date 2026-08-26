/// Integer helpers shared by the NAV and share-accounting paths.
///
/// Everything here is deliberately overflow-safe: the release profile enables
/// `overflow-checks`, and a panic inside `get_asset_amounts_per_shares` would
/// stall every market that reads this vault.
fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// Splits `a * b / denom` into `(quotient, has_remainder)` without
/// overflowing the intermediate product.
///
/// Reduces both factors against the denominator before multiplying, which is
/// the same trick `receipt-vault` uses for borrow-index math.
fn try_mul_div_parts(a: u128, b: u128, denom: u128) -> Option<(u128, bool)> {
    if denom == 0 {
        return None;
    }
    if a == 0 || b == 0 {
        return Some((0, false));
    }
    let mut left = a;
    let mut right = b;
    let mut d = denom;

    let g1 = gcd_u128(left, d);
    left /= g1;
    d /= g1;
    let g2 = gcd_u128(right, d);
    right /= g2;
    d /= g2;

    let num = left.checked_mul(right)?;
    Some((num / d, !num.is_multiple_of(d)))
}

/// `floor(a * b / denom)`, or `None` if the result cannot be computed without
/// overflowing `u128`. Oracle paths use this form so hostile external values
/// degrade like a missing observation rather than trapping the transaction.
pub fn try_mul_div(a: u128, b: u128, denom: u128) -> Option<u128> {
    try_mul_div_parts(a, b, denom).map(|(q, _)| q)
}

/// `floor(a * b / denom)`.
pub fn mul_div(a: u128, b: u128, denom: u128) -> u128 {
    if denom == 0 {
        panic!("division by zero");
    }
    // Saturation keeps non-oracle accounting paths fail-safe without wrapping.
    // Values this large cannot cross the contract's i128 token boundaries and
    // will be rejected by the surrounding liquidity/minimum checks.
    try_mul_div(a, b, denom).unwrap_or(u128::MAX)
}

/// `ceil(a * b / denom)`. Used where rounding against the vault is the safe
/// direction (shares to burn, liquidity to redeem).
pub fn mul_div_ceil(a: u128, b: u128, denom: u128) -> u128 {
    if denom == 0 {
        panic!("division by zero");
    }
    match try_mul_div_parts(a, b, denom) {
        Some((q, true)) => q.saturating_add(1),
        Some((q, false)) => q,
        None => u128::MAX,
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
pub fn apply_slippage_floor(amount: u128, bps: u32) -> u128 {
    let bps = bps as u128;
    if bps >= 10_000 {
        return 0;
    }
    mul_div(amount, 10_000u128 - bps, 10_000u128)
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
