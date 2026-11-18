// Pure stage planner for the CL collateral group (DESIGN.md §2-5, D3-CONTINUITY.md). No I/O, no SDK.
// Times are LEDGER close times (seconds): every contract check uses the ledger timestamp, not the wall clock.
// Amounts are raw bigints. Collateral and debt are both USDC (the sleeve reports in USDC), so prices cancel.

export const MAX_AGE = 60;        // checkpoint validity (staged_v2.rs STG2_MAX_AGE)
export const BLOCK_WAIT = 600;    // D3 continuous-block age before the fallback (d3_chain.rs)
export const BLOCK_GAP = 60;      // D3 max spacing between blocked observations
export const NAV_REFRESH_AGE = 45;
export const HEARTBEAT = 20;      // keeper cadence while a chain is open (D3-CONTINUITY residual)
export const HB_LATENCY = 5;      // observe + send latency allowance: heartbeat is due at 15 s
export const SEIZE_CLAIM_FRESH = 60; // L5 within 60 s of the last successful L4 claim, else a reward boundary
const S6 = 1_000_000n;

const big = v => (v == null ? null : BigInt(v));
/** Seconds since checkpoint `ck` at time `t`; null when absent or in the future. */
export function ageAt(ck, t) { if (!ck) return null; const ts = Number(ck.ts); return t >= ts ? t - ts : null; }
/** staged_v2.rs `fresh`: valid iff 0 <= t - ts <= 60. */
export function validAt(ck, t) { const a = ageAt(ck, t); return a != null && a <= MAX_AGE; }
/** Controller cached_price_fresh: cached.timestamp (the ORACLE timestamp) + resolution * k >= t. */
export function cacheValidAt(pc, t) { return !!pc && Number(pc.timestamp) <= t && Number(pc.timestamp) + Number(pc.resolution) * Number(pc.k ?? 2) >= t; }

/** USDC shortfall of one borrower at sleeve rate `rate` (1e6 = 1 USDC per unit) and collateral factor `cf` (1e6). */
export function shortfallAt(b, rate, cf) {
  const coll = (((big(b.units) * big(rate)) / S6) * big(cf)) / S6;
  const debt = big(b.debt);
  return debt > coll ? debt - coll : 0n;
}

/** Repay = min(debt x close factor, cap, inventory, collateral value / incentive). */
export function repayFor(b, rate, p) {
  let r = (big(b.debt) * big(p.closeFactor)) / S6;
  for (const lim of [p.repayCap, p.inventory]) if (lim != null && big(lim) < r) r = big(lim);
  const byColl = (((big(b.units) * big(rate)) / S6) * S6) / big(p.li);
  return r < byColl ? r : byColl;
}

/** D3 admission, using the TASK rule (stricter than the contract's now - since >= 600; heartbeats close the gap). */
export function fallbackEligible(chain, now) {
  if (!chain) return false;
  return Number(chain.last) - Number(chain.since) >= BLOCK_WAIT && now - Number(chain.last) <= BLOCK_GAP && now >= Number(chain.last);
}

/**
 * obs: {now, status: {nav, fallback, chain, next_pending}, navSim: 'ok'|'blocked'|'oracle_stale'|'error',
 *       navPreview, fallbackPreview, priceCache: {timestamp, resolution, k}|null, cf, closeFactor, li, inventory,
 *       allowance, borrowers: [{user, units, debt}], pending: [{id, from, to, n}], lastClaim, l4FailedAt, blockedL3: Set}
 * opts: {submitMargin, repayCap, navRefreshAge}
 * Returns {mode, stages: [{stage, reason, ...}], alerts: [], waitSec}. Stages are in send order; the caller sends the
 * head, re-observes and re-plans.
 */
export function plan(obs, opts = {}) {
  const margin = opts.submitMargin ?? 10, now = obs.now, tSub = now + margin;
  const refreshAge = Math.min(opts.navRefreshAge ?? NAV_REFRESH_AGE, MAX_AGE - margin);
  const st = obs.status ?? {}, stages = [], alerts = [];
  const pre = [];   // L2 / L1 stages that must precede an L3
  let hb = null;    // a due D3 heartbeat is always sent first
  let mode = 'normal', waitSec = HEARTBEAT;

  // --- Rewards / seizure sync (L4, L5): independent of prices. ---
  const tail = [];
  const pending = [...(obs.pending ?? [])].sort((a, b) => Number(a.id) - Number(b.id));
  if (pending.length) {
    const claimFresh = obs.lastClaim != null && now >= Number(obs.lastClaim) && now - Number(obs.lastClaim) <= SEIZE_CLAIM_FRESH - margin;
    const l4Failed = obs.l4FailedAt != null && now - Number(obs.l4FailedAt) <= 300;
    if (!claimFresh && !l4Failed) tail.push({stage: 'L4', reason: 'claim AQUA at the old balances before syncing seizures'});
    if (!claimFresh && l4Failed) alerts.push({level: 'alert', msg: 'L4 claim failed recently: L5 will open a reward boundary (quarantine, D6)'});
    for (const p of pending) tail.push({stage: 'L5', id: Number(p.id), reason: `sync seizure ${p.from} -> ${p.to}`});
  }

  // --- Repay-only: oracle stale => both checkpoints revert (#2105) and record nothing. ---
  if (obs.navSim === 'oracle_stale') {
    mode = 'repay-only';
    alerts.push({level: 'alert', msg: 'PYUSD oracle stale/missing: checkpoints impossible, accounts are repay-only (D4); no liquidation'});
    return {mode, stages: tail, alerts, waitSec};
  }

  const blocked = obs.navSim === 'blocked';
  const chain = st.chain ?? null;
  if (blocked) {
    mode = 'blocked';
    const since = chain ? now - Number(chain.last) : Infinity;
    if (since >= HEARTBEAT - HB_LATENCY) hb = {stage: 'L1a', reason: chain ? 'D3 heartbeat: extend the blocked chain' : 'D3: start a blocked chain'};
    else waitSec = HEARTBEAT - HB_LATENCY - since;
    if (chain && now - Number(chain.last) > BLOCK_GAP) alerts.push({level: 'warn', msg: 'D3 chain lapsed (gap > 60 s): it restarts at the next observation'});
  } else if (chain) {
    stages.push({stage: 'L1a', reason: 'pool healthy: clear the D3 chain'});
  } else if (!validAt(st.nav, now) || ageAt(st.nav, now) > refreshAge) {
    pre.push({stage: 'L1a', reason: st.nav ? `NAV checkpoint ${ageAt(st.nav, now)} s old` : 'no NAV checkpoint'});
  }

  // --- Which rate would L3 read at submission? NAV if valid at tSub, else fallback if valid at tSub. ---
  const willBeValid = ck => ck && validAt(ck, tSub);
  let rate = null, source = null;
  if (!blocked && !pre.length && willBeValid(st.nav)) { rate = big(st.nav.rate); source = 'nav'; }
  else if (!blocked && obs.navPreview) { rate = big(obs.navPreview); source = 'nav-after-L1a'; }
  else if (willBeValid(st.nav)) { rate = big(st.nav.rate); source = 'nav'; }
  else if (willBeValid(st.fallback)) { rate = big(st.fallback.rate); source = 'fallback'; }
  else if (blocked && obs.fallbackPreview && fallbackEligible(chain, now)) { rate = big(obs.fallbackPreview); source = 'fallback-after-L1b'; }
  if (blocked && rate == null && chain && !fallbackEligible(chain, now) && (obs.borrowers ?? []).some(b => big(b.debt) > 0n)) {
    const left = BLOCK_WAIT - (Number(chain.last) - Number(chain.since));
    alerts.push({level: 'info', msg: `NAV blocked; fallback admissible in about ${Math.max(left, 0)} s of continuous block`});
  }

  // --- Candidates: shortfall at that rate, not already queued/in-flight. ---
  const cands = [];
  if (rate != null && rate > 0n) for (const b of obs.borrowers ?? []) {
    const sf = shortfallAt(b, rate, obs.cf);
    if (sf === 0n) continue;
    if (pending.some(p => p.from === b.user)) { alerts.push({level: 'info', msg: `L3 skipped for ${b.user}: seizure queued, sync first`}); continue; }
    if (obs.blockedL3?.has(b.user)) { alerts.push({level: 'warn', msg: `L3 skipped for ${b.user}: unresolved journal intent`}); continue; }
    cands.push({...b, shortfall: sf});
  }
  cands.sort((a, b) => (b.shortfall > a.shortfall ? 1 : b.shortfall < a.shortfall ? -1 : 0));
  if (cands.length) {
    const b = cands[0];
    const repay = repayFor(b, rate, {closeFactor: obs.closeFactor, li: obs.li, repayCap: opts.repayCap, inventory: obs.inventory});
    if (repay <= 0n) alerts.push({level: 'alert', msg: 'shortfall but no repay inventory'});
    else {
      // Send order: APPROVE, L2, then the checkpoint, so the 60 s checkpoint clock starts last.
      // Required when the cache is invalid at submission; preferred (once per oracle round) when it expires within
      // 60 s after. cache_price stores the ORACLE timestamp, so it cannot extend the cache past a late round.
      const pc = obs.priceCache, need = !cacheValidAt(pc, tSub), tried = pc && Number(obs.l2TriedFor) === Number(pc.timestamp);
      if (need && tried) {
        alerts.push({level: 'alert', msg: 'L2 did not advance the cached USDC price (oracle round late): L3 waits, it would revert'});
        return {mode, stages: [...(hb ? [hb] : []), ...stages, ...tail], alerts, waitSec};
      }
      if (need || (!cacheValidAt(pc, tSub + 60) && Number(obs.l2TriedFor) !== Number(pc.timestamp)))
        stages.unshift({stage: 'L2', reason: !pc ? 'no cached USDC price (L3 is cache-only)' : need ? 'cached USDC price invalid at submission' : 'cached USDC price expires within 60 s'});
      if (source === 'fallback-after-L1b') stages.push({stage: 'L1b', reason: 'D3 chain >= 600 s: write the liquidation-only fallback'});
      stages.push(...pre);
      // The allowance goes first of all: it does not depend on time and must not eat the 60 s checkpoint window.
      if (big(obs.allowance ?? 0) < repay) stages.unshift({stage: 'APPROVE', amount: opts.repayCap != null && big(opts.repayCap) > repay ? big(opts.repayCap) : repay, reason: 'repay_on_behalf pulls USDC by allowance'});
      stages.push({stage: 'L3', borrower: b.user, repay, rateSource: source, shortfall: b.shortfall, reason: 'shortfall at a checkpoint valid at submission'});
    }
  } else if (pre.length && (obs.borrowers ?? []).some(b => big(b.debt) > 0n)) {
    stages.push(...pre);   // keep the NAV checkpoint warm only while somebody owes something
  }
  return {mode, stages: [...(hb ? [hb] : []), ...stages, ...tail], alerts, waitSec};
}
