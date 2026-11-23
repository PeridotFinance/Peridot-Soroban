// Pure liquidation planning. No I/O. Mirrors the deployed controller's arithmetic (simple-peridottroller 4c98bdd3,
// liquidate_internal 2657–2950): USD = raw * price / scale; repay <= debt * closeFactor / 1e6; seize = repay_usd * LI.

const SCALE = 1_000_000n;

/**
 * positions: [{market, receipt, pbal, debt, rate, price: bigint|null, scale: bigint|null, cf}]
 * inventory: {market: rawBalanceOfThatMarketsUnderlying}
 * params: {closeFactor, li, minProfitBps, neverTouch: Set<receipt>}
 * Returns {action:'liquidate', repayMarket, collateralMarket, repay, expectedSeizeUsd, repayUsd} or {action:'skip', reason}.
 */
export function planLiquidation(positions, inventory, params) {
  const live = positions.filter(p => !params.neverTouch.has(p.receipt));
  const usd = (raw, p) => (raw * p.price) / p.scale;
  if (positions.some(p => p.debt > 0n && p.price == null)) return {action: 'skip', reason: 'debt in an unpriced market (health indeterminate)'};
  // Seizable collateral: entered, holds pTokens, live price. (The controller does not check CF on the seized market,
  // but a CF-0 market is never chosen here so liquidations only take collateral that backed the debt.)
  const collaterals = live.filter(p => p.pbal > 0n && p.price != null && p.cf > 0n)
    .map(p => ({...p, underlying: (p.pbal * p.rate) / SCALE})).map(p => ({...p, usd: usd(p.underlying, p)}))
    .sort((a, b) => (b.usd > a.usd ? 1 : b.usd < a.usd ? -1 : 0));
  const debts = live.filter(p => p.debt > 0n && p.price != null).map(p => ({...p, usd: usd(p.debt, p)}))
    .sort((a, b) => (b.usd > a.usd ? 1 : b.usd < a.usd ? -1 : 0));
  if (!debts.length) return {action: 'skip', reason: 'no priced debt'};
  if (!collaterals.length) return {action: 'skip', reason: 'no seizable priced collateral (bad debt?)'};
  let best = null;
  for (const d of debts) for (const c of collaterals) {
    if (d.market === c.market) continue;                       // controller rejects repay == collateral market
    const inv = inventory[d.market] ?? 0n;
    if (inv <= 0n) continue;
    let repay = (d.debt * params.closeFactor) / SCALE;
    if (repay > inv) repay = inv;
    // Do not repay more than the collateral can pay for at the incentive (the contract would scale it down anyway).
    const maxRepayUsd = (c.usd * SCALE) / params.li;
    const maxRepay = (maxRepayUsd * d.scale) / d.price;
    if (repay > maxRepay) repay = maxRepay;
    if (repay <= 0n) continue;
    const repayUsd = usd(repay, d);
    if (!best || repayUsd > best.repayUsd) best = {action: 'liquidate', repayMarket: d.market, collateralMarket: c.market, repay, repayUsd,
      expectedSeizeUsd: (repayUsd * params.li) / SCALE};
  }
  if (!best) {
    const sameOnly = debts.every(d => collaterals.every(c => c.market === d.market));
    return {action: 'skip', reason: sameOnly ? 'same-market loop only (repay == collateral is rejected): alert' : 'no inventory for any debt asset'};
  }
  return best;
}

/** Accept the simulated seize only if its USD value covers repay * (1 + minProfitBps). */
export function seizeIsProfitable(seizePtokens, collateral, repayUsd, minProfitBps) {
  const seizeUsd = (((seizePtokens * collateral.rate) / SCALE) * collateral.price) / collateral.scale;
  return {ok: seizeUsd * 10_000n >= repayUsd * BigInt(10_000 + minProfitBps), seizeUsd};
}

/** Depeg watcher (alert only). prices are 14-decimal bigints from the USD-based feeds. */
export function depegAlerts({usdc, eurc, eurFiat}, {usdcBps = 50, eurcBps = 100} = {}) {
  const out = [];
  const one = 10n ** 14n;
  if (usdc != null) { const d = Number(((usdc - one) * 10_000n) / one); if (Math.abs(d) > usdcBps) out.push({asset: 'USDC', deviationBps: d}); }
  if (eurc != null && eurFiat != null && eurFiat > 0n) { const d = Number(((eurc - eurFiat) * 10_000n) / eurFiat); if (Math.abs(d) > eurcBps) out.push({asset: 'EURC', vs: 'EUR', deviationBps: d}); }
  return out;
}

/** Track borrowers from decoded borrow_event topics; returns a new sorted unique list. */
export function mergeBorrowers(known, events) {
  const s = new Set(known);
  for (const e of events) if (e.borrower) s.add(e.borrower);
  return [...s].sort();
}

/**
 * Drop tracked borrowers whose debt is zero in every market read. `debts` maps user -> array of BigInt debts, with
 * null for a failed read. Fail-safe: a user with no entry, any failed read or any nonzero debt is kept. Safe because a
 * pruned account that borrows again emits a new borrow_event after the event cursor and is re-added by discovery.
 */
export function pruneZeroDebt(borrowers, debts) {
  const kept = [], pruned = [];
  for (const u of borrowers) {
    const d = debts.get(u);
    if (!d || d.length === 0 || d.some(x => x == null) || d.some(x => x > 0n)) kept.push(u); else pruned.push(u);
  }
  return {kept, pruned};
}

/** XLM is both repay inventory and the fee/reserve balance: never plan a repay out of the fee floor or account reserves. */
export function netInventory(inv, {feeFloorXlmRaw = 0n, accountReserveXlmRaw = 0n} = {}) {
  const out = {...inv};
  if (out.XLM != null) { const x = out.XLM - feeFloorXlmRaw - accountReserveXlmRaw; out.XLM = x > 0n ? x : 0n; }
  return out;
}

/**
 * Price-cache refresher (every Reflector round). Refresh a token when the feed has a round newer than the controller's
 * cached observation, the controller is wired to that feed, and today's fee budget still covers one max-fee refresh.
 * live/cached: {timestamp} | null. Budget accounting is conservative: each submitted refresh is charged its max fee.
 */
export function refreshDecision({oracleMatches, live, cached, spentTodayStroops, dailyBudgetStroops, maxFeeStroops}) {
  if (!oracleMatches) return {refresh: false, reason: 'controller oracle not set to the pinned feed'};
  if (!live) return {refresh: false, reason: 'no live feed price'};
  if (cached && BigInt(cached.timestamp) >= BigInt(live.timestamp)) return {refresh: false, reason: 'cache already holds the latest round'};
  if (BigInt(spentTodayStroops) + BigInt(maxFeeStroops) > BigInt(dailyBudgetStroops)) return {refresh: false, reason: 'daily refresh fee budget exhausted', alert: true};
  return {refresh: true};
}

/** Rolls the per-UTC-day refresh spend; returns the (possibly reset) budget record. */
export function budgetFor(record, nowMs) {
  const day = new Date(nowMs).toISOString().slice(0, 10);
  return record && record.day === day ? record : {day, spentStroops: '0'};
}

/**
 * Staleness signal for the pause watcher (ALERT ONLY, never a pause). A feed observation older than staleAfterSeconds
 * (or missing) is an alert. A failed READ is not staleness (our RPC may be down) and is reported separately by the caller.
 */
export function stalenessAlerts(prices, nowSec, staleAfterSeconds) {
  const out = [];
  for (const [asset, p] of Object.entries(prices)) {
    if (p == null) { out.push({asset, kind: 'stale', reason: 'no price'}); continue; }
    const age = nowSec - Number(p.timestamp);
    if (age > staleAfterSeconds) out.push({asset, kind: 'stale', ageSeconds: age});
    else if (age < -60) out.push({asset, kind: 'stale', reason: 'future timestamp', ageSeconds: age});
  }
  return out;
}

/** Kinds that can open a pause incident. Everything else the watcher reports ('stale', 'usdc-deviation') is alert only. */
export const PAUSE_KINDS = new Set(['band', 'depeg']);

/**
 * Pause-watcher signals (pure). Policy (user rule, 2026-11-21): a stale or missing price NEVER pauses; it is an alert.
 * Only two conditions pause, and each is evaluated only on that asset's own FRESH observation (age <= staleAfterSeconds
 * and not > 60 s in the future), so a frozen old value can never trigger or confirm a pause:
 *  - 'band':  USDC outside the controller's price band. Same test as the contract's price_in_band (inclusive, exact
 *             BigInt): in band iff lo * 10^decimals <= price * 1e6 <= hi * 10^decimals, band in PriceBand 1e6 scale.
 *  - 'depeg': EURC deviating from the fresh fiat EUR reference by more than eurcBps (exact: |eurc - eur| * 1e4 > bps * eur).
 * Inside the band, a USDC deviation above usdcAlertBps from $1 is an 'usdc-deviation' alert only.
 * prices: {USDC, XLM, EURC: {price, timestamp} | null}; eurFiat: {price, timestamp} | null; prices are 10^decimals-scaled.
 * Alert records carry prices as decimal strings (they are journaled with plain JSON.stringify).
 */
export function pauseSignals({prices, eurFiat, usdcBand, decimals = 14}, nowSec, {staleAfterSeconds, usdcAlertBps = 50, eurcBps = 100}) {
  const notices = stalenessAlerts({...prices, ...(eurFiat !== undefined ? {EUR: eurFiat} : {})}, nowSec, staleAfterSeconds);
  const fresh = p => { if (p == null) return false; const age = nowSec - Number(p.timestamp); return age <= staleAfterSeconds && age >= -60; };
  const one = 10n ** BigInt(decimals), pause = [];
  const usdc = prices.USDC;
  if (fresh(usdc)) {
    const p = BigInt(usdc.price), lo = BigInt(usdcBand[0]), hi = BigInt(usdcBand[1]);
    const inBand = p * 1_000_000n >= lo * one && p * 1_000_000n <= hi * one;
    const deviationBps = Number(((p - one) * 10_000n) / one);
    if (!inBand) pause.push({kind: 'band', asset: 'USDC', price: String(p), band: [String(lo), String(hi)], deviationBps, observedAt: Number(usdc.timestamp)});
    else if (Math.abs(deviationBps) > usdcAlertBps) notices.push({kind: 'usdc-deviation', asset: 'USDC', price: String(p), deviationBps, alertOnly: true});
  }
  const eurc = prices.EURC;
  if (fresh(eurc) && fresh(eurFiat) && BigInt(eurFiat.price) > 0n) {
    const e = BigInt(eurc.price), f = BigInt(eurFiat.price), diff = e > f ? e - f : f - e;
    if (diff * 10_000n > BigInt(eurcBps) * f) pause.push({kind: 'depeg', asset: 'EURC', vs: 'EUR', deviationBps: Number(((e - f) * 10_000n) / f), observedAt: Number(eurc.timestamp), referenceAt: Number(eurFiat.timestamp)});
  }
  return {pause, notices};
}

/** Confirmation key: changes only when a TRIGGERING asset publishes a new observation (not when another asset ticks). */
export function pauseRoundKey(pause) {
  return pause.length ? pause.map(a => `${a.asset}:${a.observedAt}${a.referenceAt != null ? '/' + a.referenceAt : ''}`).sort().join(',') : null;
}

/**
 * Pause watcher state machine (pure). Only PAUSE_KINDS alerts are considered; anything else (stale, alert-only) is
 * ignored here and counts as a clean cycle. An incident opens after `confirmations` observations, each counted once per
 * NEW roundKey (a new observation of the triggering asset). While the incident is open and some target market is not yet
 * done (submitted or already paused), the action is 'pause'; once all are done it is 'latched', so a Ledger unpause is
 * not overridden and the 72 h is not extended. `clearCycles` consecutive clean cycles close the incident.
 */
export function pauseStep(state, alerts, {confirmations = 2, clearCycles = 20} = {}, {roundKey = null, allDone = false} = {}) {
  const s = {streak: 0, clean: 0, incident: false, done: {}, lastRound: null, ...state};
  const triggering = alerts.filter(a => PAUSE_KINDS.has(a.kind));
  if (triggering.length) {
    const counts = roundKey == null || String(roundKey) !== String(s.lastRound);
    if (counts) s.streak += 1;
    s.lastRound = roundKey == null ? s.lastRound : String(roundKey); s.clean = 0;
    if (!s.incident && s.streak >= confirmations) { s.incident = true; s.incidentAt = new Date().toISOString(); }
    if (!s.incident) return {state: s, action: 'confirming'};
    return {state: s, action: allDone ? 'latched' : 'pause'};
  }
  s.streak = 0; s.clean += 1;
  if (s.incident && s.clean >= clearCycles) { s.incident = false; s.done = {}; delete s.incidentAt; return {state: s, action: 'rearmed'}; }
  return {state: s, action: 'none'};
}

/** Per-asset shortfall of (net) inventory against the funded minimums. */
export function inventoryShortfalls(inv, minRaw = {}) {
  const out = [];
  for (const [n, m] of Object.entries(minRaw)) { const have = inv[n] ?? 0n, need = BigInt(m); if (have < need) out.push({asset: n, haveRaw: have, minRaw: need}); }
  return out;
}

// get_price_usd returns None for a banded token outside its band (cash-borrow upgrade f41e5260), but the contract's
// health check and liquidation keep using the unbanded cached price. Mirror the contract's cache freshness rule:
// usable iff price > 0, scale > 0, cached.timestamp + k * resolution >= now and now - timestamp <= maxAgeSecs.
export function cachedPriceForLiquidation(cached, nowSec, {k = 2n, maxAgeSecs = 900n} = {}) {
  if (!cached) return null;
  const price = BigInt(cached.price ?? 0), scale = BigInt(cached.scale ?? 0), ts = BigInt(cached.timestamp ?? 0), res = BigInt(cached.resolution ?? 0);
  const now = BigInt(nowSec);
  if (price <= 0n || scale <= 0n || res <= 0n || ts > now) return null;
  if (ts + BigInt(k) * res < now || now - ts > BigInt(maxAgeSecs)) return null;
  return [price, scale];
}
