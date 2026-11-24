'use strict';
// Pure sizing / health / slippage / sequencing logic for loop.cjs (v0 leverage loop). No I/O; unit-tested offline.
// Units: every token amount is a raw 7-decimal integer (BigInt). A price is {p, s}: USD per raw unit = p / s, as returned
// by controller.get_price_usd(token) -> (price, scale). USD values are raw 7-decimal USD (BigInt). CF is 1e6-scaled.
//
// Health factor (HF) = sum(collateral_usd * CF) / debt_usd. The controller reports shortfall > 0 (liquidatable) exactly
// when HF < 1. A loop is NOT atomic (the live liquidator can act between any two transactions), so every single step,
// including a borrow that is not yet redeposited, must keep HF >= hfMin.
const SCALE_CF = 1_000_000n;
const SIDES = Object.freeze({
  long: {coll: 'XLM', debt: 'USDC', label: 'long XLM (XLM collateral, USDC debt)'},
  short: {coll: 'USDC', debt: 'XLM', label: 'short XLM (USDC collateral, XLM debt)'},
});
// The loop tool's own caps (raw). Independent of team.cjs caps; deliberately small: this is a ~$10 test, not a product.
const CAPS = Object.freeze({
  deposit: {XLM: 1_500_000_000n, USDC: 250_000_000n},   // 150 XLM / 25 USDC per transaction
  borrow: {XLM: 900_000_000n, USDC: 120_000_000n},      // 90 XLM / 12 USDC per transaction
  swapIn: {XLM: 1_500_000_000n, USDC: 250_000_000n},
});
const usd = (raw, px) => BigInt(raw) * BigInt(px.p) / BigInt(px.s);
const fromUsd = (v, px) => BigInt(v) * BigInt(px.s) / BigInt(px.p);
const pct = (bps) => Number(bps) / 100;

// pos = {XLM: {coll, debt, cf, px}, USDC: {...}} (coll = underlying raw, i.e. pTokens * exchangeRate / 1e6)
function health(pos) {
  let collUsd = 0n, weightedUsd = 0n, debtUsd = 0n;
  for (const m of Object.values(pos)) {
    const c = usd(m.coll, m.px);
    collUsd += c; weightedUsd += c * BigInt(m.cf) / SCALE_CF; debtUsd += usd(m.debt, m.px);
  }
  const equityUsd = collUsd - debtUsd;
  return {collUsd, weightedUsd, debtUsd, equityUsd, hf: debtUsd === 0n ? Infinity : Number(weightedUsd * 1_000_000n / debtUsd) / 1e6,
    leverage: equityUsd > 0n ? Number(collUsd * 1_000_000n / equityUsd) / 1e6 : Infinity,
    liquidityUsd: weightedUsd > debtUsd ? weightedUsd - debtUsd : 0n, shortfallUsd: debtUsd > weightedUsd ? debtUsd - weightedUsd : 0n};
}
const hfStr = h => h === Infinity ? 'inf' : h.toFixed(3);
// Extra USD debt that keeps HF >= hfMin: weighted / hfMin - debt (floored at 0). hfMin as Number, converted to 1e6.
function maxExtraDebtUsd(pos, hfMin) {
  const h = health(pos), k = BigInt(Math.round(hfMin * 1e6));
  const cap = h.weightedUsd * 1_000_000n / k;
  return cap > h.debtUsd ? cap - h.debtUsd : 0n;
}
// Asymptotic leverage limit when every step must keep HF >= hfMin: L* = hfMin / (hfMin - CF).
function maxSafeLeverage(cf, hfMin) { const c = Number(cf) / 1e6; return hfMin > c ? hfMin / (hfMin - c) : Infinity; }
const clone = pos => Object.fromEntries(Object.entries(pos).map(([k, v]) => [k, {...v}]));
function apply(pos, ev) {
  const p = clone(pos), m = p[ev.market];
  if (ev.action === 'deposit') m.coll = BigInt(m.coll) + BigInt(ev.amount);
  else if (ev.action === 'borrow') m.debt = BigInt(m.debt) + BigInt(ev.amount);
  else if (ev.action === 'repay') m.debt = BigInt(m.debt) > BigInt(ev.amount) ? BigInt(m.debt) - BigInt(ev.amount) : 0n;
  else if (ev.action === 'withdraw') m.coll = BigInt(m.coll) > BigInt(ev.amount) ? BigInt(m.coll) - BigInt(ev.amount) : 0n;
  else throw Error('apply: ' + ev.action);
  return p;
}
// Borrow amount for the next round: the remaining gap to target leverage, clamped so HF stays >= hfMin right after
// the borrow (before the swapped proceeds are redeposited). Target debt = (L - 1) * equity.
function nextBorrow(pos, side, lev, hfMin) {
  const d = SIDES[side].debt, h = health(pos), px = pos[d].px;
  const targetDebtUsd = h.equityUsd * BigInt(Math.round((lev - 1) * 1e6)) / 1_000_000n;
  const gapUsd = targetDebtUsd > h.debtUsd ? targetDebtUsd - h.debtUsd : 0n;
  const safeUsd = maxExtraDebtUsd(pos, hfMin);
  const limited = gapUsd > safeUsd;
  let amount = fromUsd(limited ? safeUsd : gapUsd, px);
  const capped = amount > CAPS.borrow[d]; if (capped) amount = CAPS.borrow[d];
  return {amount, market: d, limitedByHf: limited, capped, targetDebtUsd, gapUsd};
}
// Swap guard: quote vs the controller's oracle. ratio = USD out / USD in at oracle prices (1.0 = oracle-fair, before
// fees). Refuse when |1 - ratio| > maxDevBps. outMin = quote * (1 - slippageBps).
function swapGuard({amountIn, quoteOut, pxIn, pxOut, slippageBps = 100, maxDevBps = 100}) {
  const vin = usd(amountIn, pxIn), vout = usd(quoteOut, pxOut);
  if (vin <= 0n || BigInt(quoteOut) <= 0n) throw Error('swap: zero amount or zero quote');
  const devBps = Number((vin - vout) * 10000n / vin);   // positive = worse than oracle
  if (Math.abs(devBps) > maxDevBps) throw Error(`swap refused: pool quote deviates ${devBps} bps from the oracle (max ${maxDevBps})`);
  if (slippageBps < 1 || slippageBps > 300) throw Error('slippage-bps must be 1..300');
  const outMin = BigInt(quoteOut) * BigInt(10000 - slippageBps) / 10000n;
  return {outMin, devBps, oracleOut: fromUsd(vin, pxOut)};
}
// Pick the pool with the largest exact-amount quote (quotes = [{index, pool, out}]).
function bestPool(quotes) {
  const ok = quotes.filter(q => BigInt(q.out) > 0n);
  if (!ok.length) throw Error('no pool quotes a positive output');
  return ok.reduce((a, b) => BigInt(b.out) > BigInt(a.out) ? b : a);
}
// Close: pTokens-equivalent underlying to withdraw from the collateral market next. Covers the remaining debt plus
// buffer (swap fees/slippage + dust), but never more than keeps HF >= hfMin. 0 debt -> everything.
function nextWithdraw(pos, side, hfMin, bufferBps = 250) {
  const {coll: c, debt: d} = SIDES[side], h = health(pos), m = pos[c];
  if (h.debtUsd === 0n) return {amount: BigInt(m.coll), all: true};
  const need = fromUsd(usd(pos[d].debt, pos[d].px) * BigInt(10000 + bufferBps) / 10000n, m.px);
  // Safe: CF_c * (coll_c - w) * px + otherWeighted >= hfMin * debt  ->  w <= coll_c - (hfMin*debt - otherWeighted)/(CF*px)
  const k = BigInt(Math.round(hfMin * 1e6)), reqWeighted = h.debtUsd * k / 1_000_000n;
  const otherWeighted = h.weightedUsd - usd(m.coll, m.px) * BigInt(m.cf) / SCALE_CF;
  const ceil = (a, b) => (a + b - 1n) / b;
  const reqLocalUsd = reqWeighted > otherWeighted ? ceil((reqWeighted - otherWeighted + 1n) * SCALE_CF, BigInt(m.cf)) : 0n;
  const reqLocal = ceil(reqLocalUsd * BigInt(m.px.s), BigInt(m.px.p)) + 100n;   // round against the user (+100 raw margin)
  const safe = BigInt(m.coll) > reqLocal ? BigInt(m.coll) - reqLocal : 0n;
  const amount = need < safe ? need : safe;
  return {amount, all: false, limitedByHf: safe < need, need, safe};
}
// Offline projection of the whole open/close sequence (used by `plan` and the tests). feeBps = swap loss vs oracle.
function projectOpen({side, lev, hfMin = 1.25, equity, pos0, feeBps = 30, maxRounds = 8}) {
  const {coll: c, debt: d} = SIDES[side];
  let pos = clone(pos0); const steps = [];
  const push = (ev, note) => { const before = health(pos); pos = apply(pos, ev); const after = health(pos);
    steps.push({...ev, hfBefore: before.hf, hfAfter: after.hf, levAfter: after.leverage, note}); };
  push({action: 'deposit', market: c, amount: BigInt(equity)}, 'equity');
  for (let r = 1; r <= maxRounds; r++) {
    const b = nextBorrow(pos, side, lev, hfMin);
    if (b.amount <= 0n || b.gapUsd * 1000n < health(pos).equityUsd) break;     // within 0.1% of equity: done
    push({action: 'borrow', market: d, amount: b.amount}, `round ${r}${b.limitedByHf ? ' (HF-limited)' : ''}`);
    const out = fromUsd(usd(b.amount, pos[d].px) * BigInt(10000 - feeBps) / 10000n, pos[c].px);
    steps.push({action: 'swap', from: d, to: c, amount: b.amount, estOut: out, hfBefore: health(pos).hf, hfAfter: health(pos).hf, levAfter: health(pos).leverage});
    push({action: 'deposit', market: c, amount: out}, `round ${r}`);
  }
  if (steps.filter(s => s.action === 'borrow').length === maxRounds) throw Error('did not converge');
  return {steps, final: health(pos), pos, borrows: steps.filter(s => s.action === 'borrow').length};
}
function projectClose({side, hfMin = 1.25, pos0, feeBps = 30, maxRounds = 10}) {
  const {coll: c, debt: d} = SIDES[side];
  let pos = clone(pos0); const steps = [];
  const push = (ev, note) => { const before = health(pos); pos = apply(pos, ev); const after = health(pos);
    steps.push({...ev, hfBefore: before.hf, hfAfter: after.hf, note}); };
  for (let r = 1; r <= maxRounds && BigInt(pos[d].debt) > 0n; r++) {
    const w = nextWithdraw(pos, side, hfMin);
    if (w.amount <= 0n) throw Error(`close stuck: HF ${hfStr(health(pos).hf)} leaves no safe withdrawal; repay from wallet first`);
    push({action: 'withdraw', market: c, amount: w.amount}, `round ${r}${w.limitedByHf ? ' (HF-limited)' : ''}`);
    const out = fromUsd(usd(w.amount, pos[c].px) * BigInt(10000 - feeBps) / 10000n, pos[d].px);
    steps.push({action: 'swap', from: c, to: d, amount: w.amount, estOut: out, hfBefore: health(pos).hf, hfAfter: health(pos).hf});
    const rep = out < BigInt(pos[d].debt) ? out : BigInt(pos[d].debt);
    push({action: 'repay', market: d, amount: rep}, `round ${r}`);
  }
  if (BigInt(pos[d].debt) > 0n) throw Error('close did not converge');
  push({action: 'withdraw', market: c, amount: BigInt(pos[c].coll)}, 'remainder (debt 0)');
  return {steps, final: health(pos), pos};
}
module.exports = {SIDES, CAPS, SCALE_CF, usd, fromUsd, pct, health, hfStr, maxExtraDebtUsd, maxSafeLeverage, apply, nextBorrow, swapGuard,
  bestPool, nextWithdraw, projectOpen, projectClose};
