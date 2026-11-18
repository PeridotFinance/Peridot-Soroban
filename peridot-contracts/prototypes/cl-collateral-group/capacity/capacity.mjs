#!/usr/bin/env node
// Liquidation capacity for the CL PYUSD/USDC collateral group (COLLATERAL-BORROWING-SPEC.md item 4, design 1-2;
// DESIGN.md §2-3). Offline arithmetic only: no network, no SDK. Amounts are HUMAN units (both tokens 7 decimals, so a
// raw sqrt price equals the human one and amount = L_raw / 1e7 x sqrt-price difference). token0 = PYUSD, token1 = USDC,
// price = USDC per PYUSD.
//   node capacity.mjs input.json
import fs from 'node:fs';

export const sqrtAtTick = t => Math.pow(1.0001, t / 2);

/** Position amounts at sqrt price s over [tickLower, tickUpper) (Uniswap-v3 / Aquarius CL formulas). */
export function amounts(L, s, tickLower, tickUpper) {
  const sl = sqrtAtTick(tickLower), su = sqrtAtTick(tickUpper), x = Math.min(Math.max(s, sl), su);
  return {pyusd: L * (1 / x - 1 / su), usdc: L * (x - sl)};
}

/**
 * (a) In-kind payout when `units` seized units are closed with stage_withdraw_kind(min_swap = 0): the pro-rata share
 * units / totalUnits of the position principal (liquidity burn = L x units / supply) plus idle balances (incl. fees).
 * `worst` is the same close with the pool at/below the lower edge (all PYUSD), the case a depeg produces.
 */
export function inKindPayout(pos, units, price) {
  const L = Number(pos.liquidityRaw) / 10 ** (pos.decimals ?? 7), f = units / pos.totalUnits;
  const s = pos.sqrtPrice ?? sqrtAtTick(pos.tick ?? 0);
  const at = sp => { const a = amounts(L, sp, pos.tickLower, pos.tickUpper); return {pyusd: f * (a.pyusd + (pos.idlePyusd ?? 0)), usdc: f * (a.usdc + (pos.idleUsdc ?? 0))}; };
  const now = at(s), worst = at(sqrtAtTick(pos.tickLower));
  const share = m => (m.pyusd * price) / (m.pyusd * price + m.usdc || 1);
  return {now: {...now, pyusdValueShare: share(now)}, worst: {...worst, pyusdValueShare: share(worst)}};
}

const slip = (q, price) => 1 - q.out / (q.in * price);

/** (b) PYUSD sellable for USDC within `budget` (fraction) of the oracle price, from simulated quotes (sorted by in). */
export function capacityFromQuotes(quotes, price, budget) {
  const qs = [...quotes].sort((a, b) => a.in - b.in);
  let prev = {in: 0, s: 0};
  for (const q of qs) {
    const s = slip(q, price);
    if (s > budget) return {pyusd: prev.in + ((budget - prev.s) / (s - prev.s)) * (q.in - prev.in), bound: 'interpolated'};
    prev = {in: q.in, s};
  }
  return {pyusd: prev.in, bound: 'lower-bound: largest quote still within budget'};
}

/**
 * (b) Single-range constant-liquidity pool model (approximation when no quote ladder is available). Effective
 * liquidity = (pool active L - our L) x (1 - thirdPartyHaircut): the spec's "our liquidity and 50% of third-party
 * liquidity leave the pool". Selling dx PYUSD: 1/s' = 1/s + dx(1-fee)/L, out = L(s - s'), stops at the range edge.
 */
export function poolSwap(pool, dx) {
  const Leff = Math.max(0, (pool.activeLiquidity - (pool.ourLiquidity ?? 0)) * (1 - (pool.thirdPartyHaircut ?? 0.5)));
  const s = pool.sqrtPrice ?? sqrtAtTick(pool.tick ?? 0), sl = sqrtAtTick(pool.tickLower);
  const dxe = dx * (1 - (pool.feeBps ?? 0) / 10_000), max = Leff * (1 / sl - 1 / s);
  const used = Math.min(dxe, max), s2 = Leff > 0 ? 1 / (1 / s + used / Leff) : s;
  return {out: Leff * (s - s2), exhausted: dxe > max, Leff};
}
export function capacityFromPool(pool, price, budget) {
  const ok = dx => { const r = poolSwap(pool, dx); return !r.exhausted && slip({in: dx, out: r.out}, price) <= budget; };
  let lo = 0, hi = 1;
  while (ok(hi) && hi < 1e15) { lo = hi; hi *= 2; }
  for (let i = 0; i < 100; i++) { const m = (lo + hi) / 2; if (ok(m)) lo = m; else hi = m; }
  return {pyusd: lo, bound: poolSwap(pool, hi).exhausted ? 'range edge' : 'slippage'};
}

/**
 * (c) Caps. A borrower at the liquidation threshold with debt D: one close-factor round seizes (1 + LI) x CF_close x D
 * of collateral value (0.54 D); the whole debt over several rounds seizes (1 + LI) x D (1.08 D). Only the PYUSD
 * share `f` of the seized in-kind payout must be sold. Correlated liquidations (a depeg hits every borrower) share
 * one pool, so the bound is on TOTAL debt: f x seized(D) <= conversion capacity (USDC) / safety factor.
 */
export function caps(capUsd, {closeFactor = 0.5, incentive = 0.08, safetyFactor = 2, collateralFactor, pyusdShare = 1}) {
  const k = x => capUsd / (safetyFactor * pyusdShare * x);
  const oneRound = k((1 + incentive) * closeFactor), fullDebt = k(1 + incentive);
  return {borrowCapOneRound: oneRound, borrowCapFullDebt: fullDebt, borrowCap: fullDebt,
    backingCap: collateralFactor ? fullDebt / collateralFactor : null};
}

export function run(input) {
  const price = input.oraclePrice ?? 1, P = input.params ?? {}, out = {assumptions: ASSUMPTIONS, price, payouts: [], budgets: []};
  for (const u of input.closeUnits ?? []) out.payouts.push({units: u, ...inKindPayout(input.position, u, price)});
  const f = P.pyusdShare ?? 1;   // worst case (all PYUSD) unless overridden
  for (const bps of P.slippageBudgetsBps ?? [100, 200]) {
    const b = bps / 10_000;
    const c = input.quotes?.length ? capacityFromQuotes(input.quotes, price, b) : capacityFromPool(input.pool, price, b);
    out.budgets.push({budgetBps: bps, convertiblePyusd: c.pyusd, bound: c.bound, convertibleUsd: c.pyusd * price * (1 - b),
      ...caps(c.pyusd * price * (1 - b), {...P, pyusdShare: f})});
  }
  return out;
}

export const ASSUMPTIONS = [
  'token0 = PYUSD, token1 = USDC, 7 decimals each; price is USDC per PYUSD (Reflector CALI convention).',
  'Quotes / pool liquidity must be taken AFTER removing our position and 50% of third-party liquidity (spec I4-a); the pool model applies that removal itself, quotes are used as given.',
  'Quotes are PYUSD->USDC exact-input; slippage is measured against the oracle price and assumed monotone between quotes.',
  'Pool model is a single constant-L range (no tick crossings into other liquidity); SDEX and other pools are ignored unless included as quotes.',
  'In-kind payout = pro-rata principal of the liquidity burn plus pro-rata idle (fees included in idle); swap skipped (min_swap = 0).',
  'Caps use the worst-case PYUSD share 1.0 (position at/below the lower edge, a depeg) unless params.pyusdShare is set.',
  'Borrow cap bounds TOTAL group debt (correlated liquidations share one pool); the recommended cap is the full-debt case (1.08 D), the one-round case (0.54 D) is reported for comparison. Safety factor default 2 (spec I4-a).',
  'backingCap = borrowCap / collateralFactor: the collateral that, fully used, carries the borrow cap.',
  'Convertible USD is discounted by the full budget (conservative); gas, rewards and the L5 exit delay are not modelled.',
];

if (import.meta.url === `file://${process.argv[1]}`) {
  if (!process.argv[2]) { console.error('usage: node capacity.mjs input.json'); process.exit(2); }
  console.log(JSON.stringify(run(JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))), null, 1));
}
