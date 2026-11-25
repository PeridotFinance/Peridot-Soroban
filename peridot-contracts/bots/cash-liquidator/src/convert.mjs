// Conversion of SEIZED margin collateral back to the debt asset (margin pass only; see README for why the cash pass keeps
// its plain redeem). Each successful margin liquidation records a "lot" = exactly the pTokens it seized. A lot is
// (1) redeemed on its receipt (withdraw, never more than the lot or the current pToken balance), then
// (2) swapped on the pinned Aquarius XLM/USDC pool, only at >= the Reflector-oracle value less maxSlippageBps (1.5 %) as
//     the pool's out_min, never more than the lot's redeemed amount. Below the floor (or any failure) the lot is held and
//     retried with exponential backoff; conversionFailed alerts after alertAfterAttempts or alertAfterSecs.
// Long liquidations seize XLM -> sell for USDC. Short liquidations seize USDC -> redeem; buy XLM only while the XLM
// inventory is below its funded minimum (inventory policy). Simulate first, journal, per-tx fee cap + daily budget.
import {budgetFor} from './plan.mjs';

const usd = (a, p) => (a * p[0]) / p[1];
export const CONVERSION_FAILED_MSG = 'conversionFailed: seized collateral held unconverted (pool below the oracle floor or redeem failing)';

/** Oracle floor for selling amountIn of the input asset: oracle value less slippageBps, in output raw units (both 7 dp). */
export function oracleMinOut(amountIn, pIn, pOut, slippageBps) {
  const v = (usd(BigInt(amountIn), pIn) * BigInt(10_000 - slippageBps)) / 10_000n;
  return (v * pOut[1]) / pOut[0];
}
/** Executed-vs-oracle check (bps below the oracle value; negative = better than the oracle). */
export function belowOracleBps(amountIn, out, pIn, pOut) {
  const vin = usd(BigInt(amountIn), pIn), vout = usd(BigInt(out), pOut);
  return vin === 0n ? 0 : Number(((vin - vout) * 10_000n) / vin);
}
export const backoffMs = (attempts, baseMs, maxMs) => Math.min(maxMs, baseMs * 2 ** Math.max(0, attempts - 1));
export function conversionAlertDue(lot, nowMs, cv) {
  return !lot.alerted && (lot.attempts >= cv.alertAfterAttempts || nowMs - lot.firstAt >= cv.alertAfterMs);
}
/** Target for a seized asset: XLM -> USDC always; USDC -> XLM only while XLM inventory is below its funded minimum. */
export function swapTarget(seized, inventory, minInventoryRaw = {}) {
  if (seized === 'XLM') return 'USDC';
  const need = minInventoryRaw.XLM != null ? BigInt(minInventoryRaw.XLM) : 0n;
  return (inventory.XLM ?? 0n) < need ? 'XLM' : null;
}
export function newLot({positionId, side, seizePt, ledger, nowMs, cfg, inventory}) {
  const seized = side === 'long' ? 'XLM' : 'USDC';
  return {lotId: `${positionId}-${ledger}`, positionId: String(positionId), seized, receipt: cfg.pins.markets[seized].receipt,
    ptokens: String(seizePt), stage: 'redeem', underlying: null, target: swapTarget(seized, inventory, cfg.pins.minInventoryRaw),
    attempts: 0, firstAt: nowMs, nextAt: nowMs, alerted: false, lastErr: null};
}
/** pTokens reserved by open margin lots on a receipt (the cash pass's seized-collateral withdraw must leave them). */
export function reservedPt(st, receipt) {
  return (st.convert?.lots ?? []).filter(l => l.stage === 'redeem' && l.receipt === receipt).reduce((s, l) => s + BigInt(l.ptokens), 0n);
}

async function balance(c, token, who) {
  try { return BigInt(await c.view(token, 'balance', [c.addr(who)])); } catch (e) { if (/Error\(Contract, #13\)/.test(e.message)) return 0n; throw e; }
}

function hold(lot, err, nowMs, cv, deps, extra = {}) {
  lot.attempts += 1; lot.lastErr = String(err).slice(0, 200); lot.nextAt = nowMs + backoffMs(lot.attempts, cv.backoffBaseMs, cv.backoffMaxMs);
  deps.log('warn', 'seized collateral conversion held; retry with backoff', {lotId: lot.lotId, stage: lot.stage, attempts: lot.attempts, nextInSecs: Math.round((lot.nextAt - nowMs) / 1000), err: lot.lastErr, ...extra});
  if (conversionAlertDue(lot, nowMs, cv)) {
    lot.alerted = true;
    deps.log('alert', CONVERSION_FAILED_MSG, {type: 'conversionFailed', key: `conversion:${lot.lotId}`,
      lotId: lot.lotId, seized: lot.seized, target: lot.target, stage: lot.stage, ptokens: lot.ptokens, underlying: lot.underlying, attempts: lot.attempts, heldMinutes: Math.floor((nowMs - lot.firstAt) / 60_000), err: lot.lastErr});
  }
}

function budgetOk(st, fee, cv, nowMs) {
  st.convert.budget = budgetFor(st.convert.budget, nowMs);
  return BigInt(st.convert.budget.spentStroops) + BigInt(fee) <= BigInt(cv.dailyBudgetStroops);
}
function charge(st, res, fee) { st.convert.budget = {...st.convert.budget, spentStroops: String(BigInt(st.convert.budget.spentStroops) + BigInt(res.feeCharged ?? fee))}; }

async function sendStep(cfg, c, st, sim, kind, rec, deps) {
  const signed = c.sign(sim.prepared), hash = signed.hash().toString('hex');
  deps.journal(cfg.journalFile, {kind, intent: hash, ...rec});
  const res = await c.submit(signed);
  deps.journal(cfg.journalFile, {kind, result: res.status, hash, ledger: res.ledger});
  charge(st, res, sim.metrics.fee);
  deps.log(res.status === 'SUCCESS' ? 'info' : 'alert', `${kind} submitted`, {...rec, ...res, metrics: sim.metrics});
  return res;
}

// An unknown outcome (UNKNOWN / send error) is never retried: retrying against the live balance could sell inventory that is
// not part of the lot. The lot is parked for operator review and alerted.
function review(lot, status, deps) {
  lot.stage = 'review'; lot.alerted = true;
  deps.log('alert', 'conversionFailed: conversion outcome unknown; lot parked for operator review (not retried)', {type: 'conversionFailed', key: `conversion:${lot.lotId}`, lotId: lot.lotId, status, seized: lot.seized, target: lot.target, ptokens: lot.ptokens, underlying: lot.underlying});
}

function done(lot, deps, note) {
  lot.stage = 'done';
  deps.log('info', 'seized collateral conversion done', {lotId: lot.lotId, note, underlying: lot.underlying, target: lot.target});
  if (lot.alerted) deps.log('info', 'alert resolved', {key: `conversion:${lot.lotId}`, resolved: true, type: 'conversionFailed'});
}

/** env: {dry, prices:{USDC, XLM}, nowMs}. Returns {out, stop}. Never touches balances beyond a lot's own amounts. */
export async function convertPass(cfg, c, st, env, deps) {
  const m = cfg.margin, cv = m.convert, M = cfg.pins.markets, out = [];
  st.convert = {lots: [], budget: null, ...(st.convert || {})};
  if (!cv?.enabled) return {out};
  const opts = {cpuStop: m.cpuStop, maxFeeStroops: m.maxFeeStroops};
  for (const lot of st.convert.lots) {
    if (lot.stage === 'done' || lot.stage === 'review' || env.nowMs < lot.nextAt) continue;
    const rec = {lotId: lot.lotId, seized: lot.seized, target: lot.target};
    try {
      if (lot.stage === 'redeem') {
        const bal = BigInt(await c.view(lot.receipt, 'get_ptoken_balance', [c.addr(cfg.publicKey)]));
        const pt = BigInt(lot.ptokens) < bal ? BigInt(lot.ptokens) : bal;   // never more than seized
        if (pt === 0n) { done(lot, deps, 'no seized pTokens left to redeem'); continue; }
        const sim = await c.prepare(lot.receipt, 'withdraw', [c.addr(cfg.publicKey), c.u128(pt)], opts);
        if (!sim.ok) { hold(lot, sim.error, env.nowMs, cv, deps, rec); out.push({...rec, held: sim.error}); continue; }
        if (env.dry) { deps.log('info', 'DRY_RUN would redeem seized pTokens', {...rec, ptokens: pt, metrics: sim.metrics}); out.push({...rec, dryRun: 'redeem'}); continue; }
        if (!budgetOk(st, sim.metrics.fee, cv, env.nowMs)) { deps.log('warn', 'conversion daily fee budget exhausted; holding', {...rec, budget: st.convert.budget}); continue; }
        const tok = M[lot.seized].underlying, before = await balance(c, tok, cfg.publicKey);
        const res = await sendStep(cfg, c, st, sim, 'margin_redeem', {...rec, ptokens: pt}, deps);
        out.push({...rec, stage: 'redeem', ...res});
        if (res.status === 'FAILED') { hold(lot, 'redeem FAILED', env.nowMs, cv, deps, rec); continue; }
        if (res.status !== 'SUCCESS') { review(lot, res.status, deps); return {out, stop: true}; }
        const after = await balance(c, tok, cfg.publicKey);
        lot.underlying = String(after > before ? after - before : 0n);
        lot.ptokens = String(BigInt(lot.ptokens) - pt);
        if (lot.target && BigInt(lot.underlying) > 0n) { lot.stage = 'swap'; lot.attempts = 0; } else { done(lot, deps, 'redeemed; no swap needed by inventory policy'); continue; }
      }   // falls through: a freshly redeemed lot is offered to the pool in the same cycle
      if (lot.stage === 'swap') {
        const tin = M[lot.seized].underlying, tout = M[lot.target].underlying;
        const have = await balance(c, tin, cfg.publicKey);
        const amountIn = BigInt(lot.underlying) < have ? BigInt(lot.underlying) : have;   // never more than the lot
        if (amountIn === 0n) { done(lot, deps, 'nothing left to swap'); continue; }
        const pIn = env.prices[lot.seized], pOut = env.prices[lot.target];
        if (!pIn || !pOut) { hold(lot, 'oracle price unavailable', env.nowMs, cv, deps, rec); continue; }
        const minOut = oracleMinOut(amountIn, pIn, pOut, cv.maxSlippageBps);
        if (minOut === 0n) { hold(lot, 'amount too small for a nonzero floor', env.nowMs, cv, deps, rec); continue; }
        const tokens = await c.view(m.pool, 'get_tokens');
        const ii = tokens.indexOf(tin), oi = tokens.indexOf(tout);
        if (ii < 0 || oi < 0) throw Error('pinned pool does not hold the pair');
        const sim = await c.prepare(m.pool, 'swap', [c.addr(cfg.publicKey), c.u32(ii), c.u32(oi), c.u128(amountIn), c.u128(minOut)], opts);
        const quoted = sim.ok && sim.retval != null ? BigInt(sim.retval) : null;
        const r2 = {...rec, amountIn, minOut, quotedOut: quoted, belowOracleBps: quoted != null ? belowOracleBps(amountIn, quoted, pIn, pOut) : null, metrics: sim.metrics};
        if (!sim.ok || quoted == null || quoted < minOut) { hold(lot, sim.ok ? 'quote below the oracle floor' : sim.error, env.nowMs, cv, deps, r2); out.push({...r2, held: true}); continue; }
        if (env.dry) { deps.log('info', 'DRY_RUN would swap seized collateral', r2); out.push({...r2, dryRun: 'swap'}); continue; }
        if (!budgetOk(st, sim.metrics.fee, cv, env.nowMs)) { deps.log('warn', 'conversion daily fee budget exhausted; holding', {...r2, budget: st.convert.budget}); continue; }
        const res = await sendStep(cfg, c, st, sim, 'margin_swap', r2, deps);
        out.push({...r2, ...res});
        if (res.status === 'FAILED') { hold(lot, 'swap FAILED (pool moved below the floor at execution?)', env.nowMs, cv, deps, rec); continue; }
        if (res.status !== 'SUCCESS') { review(lot, res.status, deps); return {out, stop: true}; }
        lot.underlying = String(BigInt(lot.underlying) - amountIn);
        done(lot, deps, 'swapped');
      }
    } catch (e) { hold(lot, e.message, env.nowMs, cv, deps, rec); }
  }
  st.convert.lots = st.convert.lots.filter(l => l.stage !== 'done');
  return {out};
}
