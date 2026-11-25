// Pure margin-pass logic (pooled margin v2, contracts/margin-controller, build b1cb1cb2). No I/O.
// Mirrors the contract arithmetic of `liquidate` (contract.rs 731-840) and the harness's liq_pay replica
// (harness/margin-fork/tests/margin.rs 280-300). Units: raw 7-decimal amounts, 1e6 fractions/rates, usd7 values,
// prices as [price, scale] bigints from the cash controller's get_price_usd.

export const E6 = 1_000_000n;
export const U128_MAX = (1n << 128n) - 1n;

const usd = (amount, p) => (amount * p[0]) / p[1];
const mulDivUp = (a, b, d) => { const n = a * b; return n / d + (n % d === 0n ? 0n : 1n); };
const fracUp = (x, f) => mulDivUp(x, f, E6);
const fracDown = (x, f) => (x * f) / E6;
const usdToAmtDown = (v, p) => (v * p[1]) / p[0];
const amtToPtDown = (a, rate) => (a * E6) / rate;
const ptToAmt = (pt, rate) => (pt * rate) / E6;
const min = (a, b) => (a < b ? a : b);

/**
 * Expected outcome of liquidate(keeper, id, max_pay) under the contract math, at the given snapshot.
 * pos: {side:'long'|'short', debt, funding, heldPt}; ctx: {rateHeld, priceHeld, priceDebt, fees:{bonus_liquidator, bonus_backstop}}
 * Long = held XLM, debt USDC (pu = debt price); short = held USDC, debt XLM (pu = held price).
 * Returns {underwater, pay, seizePt}: pay in raw debt units, seizePt in held pTokens.
 */
export function expectedLiquidation(pos, ctx) {
  const {debt: d, funding: f, heldPt: held} = pos;
  const {rateHeld: rate, priceHeld: ph, priceDebt: pd} = ctx;
  const bl = BigInt(ctx.fees.bonus_liquidator), bb = BigInt(ctx.fees.bonus_backstop);
  const long = pos.side === 'long';
  const pu = long ? pd : ph;
  const hUsd = usd(ptToAmt(held, rate), ph);
  const dUsd = usd(d, pd);
  const seizeReq = fracDown(dUsd, E6 + bl);
  if (hUsd < seizeReq) {
    const r = min(usdToAmtDown((hUsd * E6) / (E6 + bl), pd), d);
    return {underwater: true, pay: r, seizePt: held};
  }
  if (long) {
    const b = fracUp(d, bb);
    const fEff = min(f, usdToAmtDown(hUsd - seizeReq, pu));
    const seizeUsd = fracDown(usd(d, pd), E6 + bl) + usd(fEff, pu);
    return {underwater: false, pay: d + b + fEff, seizePt: min(amtToPtDown(usdToAmtDown(seizeUsd, ph), rate), held)};
  }
  const seizeUsd = fracDown(usd(d, pd), E6 + bl);
  return {underwater: false, pay: d, seizePt: min(amtToPtDown(usdToAmtDown(seizeUsd, ph), rate), held)};
}

/**
 * Upper bound for max_pay: the worst-case payment (long: D + D*bonus_backstop + funding; short/underwater: D) plus a
 * buffer for interest/funding accrued between simulation and inclusion. Never above the available inventory.
 */
export function maxPayFor(pos, fees, {bufferBps = 50, inventory = null} = {}) {
  const d = pos.debt, worst = pos.side === 'long' ? d + fracUp(d, BigInt(fees.bonus_backstop)) + pos.funding : d;
  const buffered = worst + mulDivUp(worst, BigInt(bufferBps), 10_000n) + 1n;
  return {worst, maxPay: inventory == null ? buffered : min(buffered, inventory)};
}

/**
 * Profit check: bonus minus fees. seize value (held pTokens at the held rate and price) minus pay value (debt asset)
 * minus the transaction fee (stroops = raw XLM, at the XLM price). ok iff profit > 0 and profit >= pay * minProfitBps.
 */
export function marginProfit({seizePt, rateHeld, priceHeld, pay, priceDebt, feeStroops, priceXlm, minProfitBps = 0}) {
  const seizeUsd = usd(ptToAmt(BigInt(seizePt), rateHeld), priceHeld);
  const payUsd = usd(BigInt(pay), priceDebt);
  const feeUsd = usd(BigInt(feeStroops), priceXlm);
  const profitUsd = seizeUsd - payUsd - feeUsd;
  const ok = profitUsd > 0n && profitUsd * 10_000n >= payUsd * BigInt(minProfitBps);
  return {ok, seizeUsd, payUsd, feeUsd, profitUsd, bps: payUsd > 0n ? Number((profitUsd * 10_000n) / payUsd) : null};
}

/** Liquidatable = LTV >= its own stored liq_ltv. Worst first: highest LTV / liq_ltv ratio, then the larger raw debt. */
export function rankLiquidatable(positions) {
  return positions.filter(p => p.ltv != null && p.ltv >= p.liqLtv)
    .map(p => ({...p, ratio: (p.ltv * E6) / p.liqLtv}))
    .sort((a, b) => (b.ratio !== a.ratio ? (b.ratio > a.ratio ? 1 : -1) : (b.debt ?? 0n) > (a.debt ?? 0n) ? 1 : (b.debt ?? 0n) < (a.debt ?? 0n) ? -1 : 0));
}

/**
 * Position/backstop discovery from margin events (single-symbol topics; the id or backer is data[0]).
 * events: [{ledger, name, data}] in ledger order. Returns new {active:Set<string>, backers:{addr: expiresAt}, impairedSeen}.
 */
export function applyMarginEvents({active, backers}, events) {
  const a = new Set([...active].map(String)), b = {...backers};
  let impairedSeen = false;
  for (const e of events) {
    const d = Array.isArray(e.data) ? e.data : [];
    switch (e.name) {
      case 'margin_open': a.add(String(d[0])); break;
      case 'margin_close': case 'margin_liquidate': a.delete(String(d[0])); break;
      case 'margin_impaired': impairedSeen = true; break;
      case 'bs_request': b[String(d[0])] = String(d[3]); break;
      case 'bs_cancel': case 'bs_withdraw': delete b[String(d[0])]; break;
      default: break;
    }
  }
  return {active: a, backers: b, impairedSeen};
}

/**
 * CAP-88: ledger timestamps are whole seconds and repeat across sub-second ledgers, so observations are keyed on the
 * ledger SEQUENCE and durations use the keeper's own clock (never a ledger-timestamp delta).
 * - A new sequence counts once, even with the same timestamp as the previous ledger.
 * - The same sequence seen twice is not a new observation (no double count).
 * - A position observed healthy or closed is dropped (a later episode starts a new timer); one whose views failed is kept.
 * since: {id: {firstLedger, firstSeenMs, lastLedger, observations, alerted}}. Returns a new object.
 */
export function trackLiquidatable(since, ids, ledgerSeq, nowMs, unknown = []) {
  const out = {};
  // A position whose views failed this cycle (price #11, RPC error) was not observed healthy: keep its timer running, so an
  // oracle outage cannot silence the > 10 min alert.
  for (const id of unknown.map(String)) if (since?.[id]) out[id] = since[id];
  for (const id of ids.map(String)) {
    const prev = since?.[id];
    if (!prev) { out[id] = {firstLedger: ledgerSeq, firstSeenMs: nowMs, lastLedger: ledgerSeq, observations: 1}; continue; }
    const fresh = ledgerSeq > prev.lastLedger;
    out[id] = {...prev, lastLedger: fresh ? ledgerSeq : prev.lastLedger, observations: prev.observations + (fresh ? 1 : 0)};
  }
  return out;
}

/** Cycle key: run the margin pass once per ledger sequence. A repeated timestamp with a new sequence still runs. */
export function isNewLedger(lastSeq, seq) { return lastSeq == null || seq > lastSeq; }

/**
 * Alerts from one cycle's snapshot (pure).
 * snap: {positions:[{id, side, worstPay, ...}], liqSince, health:{hf, hfDown}|null, headroom:{hf_target}|null,
 *        impaired, impairedSeen, bsShares:{USDC, XLM}, inventory:{USDC, XLM}, unavailable:[ids]}
 * th: {liquidatableAlertMs, hfDownMin}
 */
export function marginAlerts(snap, th, nowMs) {
  const out = [];
  for (const [id, s] of Object.entries(snap.liqSince ?? {})) {
    const ms = nowMs - s.firstSeenMs;
    if (ms > th.liquidatableAlertMs) out.push({key: `liquidatable:${id}`, msg: 'margin position liquidatable for more than 10 min (owner rule)', id, minutes: Math.floor(ms / 60_000), firstLedger: s.firstLedger, observations: s.observations});
  }
  if (snap.health) {
    const hfT = snap.headroom ? BigInt(snap.headroom.hf_target) : null;
    if ((hfT != null && snap.health.hf < hfT) || snap.health.hfDown < BigInt(th.hfDownMin)) {
      out.push({key: 'pooled_health', msg: 'margin pooled_health below target: hf < hf_target or hf_down < 1.10', hf: snap.health.hf, hfDown: snap.health.hfDown, hfTarget: hfT, hfDownMin: th.hfDownMin});
    }
  }
  if (snap.impaired || snap.impairedSeen) out.push({key: 'impaired', msg: 'margin_impaired: margin is Impaired, opens paused (admin_recapitalize / admin_resolve_impairment)', impaired: !!snap.impaired, eventSeen: !!snap.impairedSeen});
  const bs = snap.bsShares ?? {};
  if ((bs.USDC ?? 0n) > 0n || (bs.XLM ?? 0n) > 0n) out.push({key: 'opens_refused', msg: 'margin opens refused (#25 BackstopDebtOutstanding): backstop bad debt unsettled', bsDebtSharesUsdc: bs.USDC ?? 0n, bsDebtSharesXlm: bs.XLM ?? 0n});
  // Inventory vs the largest debt: USDC for longs (debt USDC), XLM for shorts (debt XLM).
  for (const [side, asset] of [['long', 'USDC'], ['short', 'XLM']]) {
    const largest = (snap.positions ?? []).filter(p => p.side === side && p.worstPay != null).reduce((m, p) => (p.worstPay > m ? p.worstPay : m), 0n);
    const have = snap.inventory?.[asset] ?? 0n;
    if (largest > 0n && have < largest) out.push({key: `inventory:${asset}`, msg: `liquidator ${asset} inventory below the largest margin ${side} debt payment`, asset, haveRaw: have, largestPayRaw: largest});
  }
  for (const id of snap.unavailable ?? []) out.push({key: `unavailable:${id}`, msg: 'margin position views failed (price unavailable #11?): health unknown', id});
  return out;
}

/** Re-alert gate: an alert key fires on first sight, then at most every `everyMs` while it persists. Keys that cleared are forgotten. */
export function gateAlerts(last, alerts, nowMs, everyMs) {
  const next = {}, fire = [];
  for (const a of alerts) {
    const t = last?.[a.key];
    if (t == null || nowMs - t >= everyMs) { fire.push(a); next[a.key] = nowMs; } else next[a.key] = t;
  }
  return {fire, last: next};
}

/** Classify a simulation error string by contract error code (margin Error enum, types.rs; SAC #10 = balance). */
export function simErrorCode(err) { const m = /Error\(Contract, #(\d+)\)/.exec(String(err ?? '')); return m ? Number(m[1]) : null; }
export const ERR = {BadParams: 6, NotOpen: 13, NotLiquidatable: 14, PriceUnavailable: 11, BackstopEmpty: 34, BadAmount: 4, NoRequest: 28, MaxPay: 32, PooledShortfall: 35};

/**
 * From simulated contract events: the seize (margin_liquidate data[4], held pTokens) and the keeper's net outflow of
 * the debt token (SAC transfer events from/to the keeper). Returns null when the margin_liquidate event is absent.
 * events: [{contract, topics:[...native], data}]
 */
export function liquidationFromEvents(events, {margin, keeper, debtToken}) {
  const liq = (events ?? []).find(e => e.contract === margin && e.topics?.[0] === 'margin_liquidate');
  if (!liq || !Array.isArray(liq.data)) return null;
  let out = 0n, seen = false;
  for (const e of events) {
    if (e.contract !== debtToken || e.topics?.[0] !== 'transfer') continue;
    const amt = typeof e.data === 'object' && e.data !== null && 'amount' in e.data ? BigInt(e.data.amount) : BigInt(e.data);
    if (e.topics[1] === keeper) { out += amt; seen = true; }
    if (e.topics[2] === keeper) out -= amt;
  }
  return {seizePt: BigInt(liq.data[4]), repaid: BigInt(liq.data[3]), paid: seen ? out : null};
}

/** Cash pass exclusion: the margin pooled account (and any configured account) is never liquidated, only alerted. */
export function excludedBorrower(neverLiquidate, user) { return neverLiquidate.has(user); }
