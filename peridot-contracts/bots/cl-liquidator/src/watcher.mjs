// Off-chain PYUSD depeg watcher (D2). RECOMMENDATION ONLY: this release never pauses anything itself.
// Compares the on-chain feed price (Reflector Stellar-DEX feed, lastprice(Stellar(PYUSD)) in USDC, 14 decimals) with an
// optional external reference ratio from a public JSON endpoint (no API key; disabled by default; fetcher injectable).

const P14 = 1e14;
const pick = (obj, p) => (p ? p.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj) : obj);

/** Reference PYUSD/USDC ratio, or null. `fetchJson(url)` is injected (tests use a stub). Never throws. */
export async function fetchReference(ref, fetchJson) {
  if (!ref?.enabled || !ref.url || !fetchJson) return null;
  try {
    const j = await fetchJson(ref.url);
    const num = Number(pick(j, ref.path)), den = ref.divPath ? Number(pick(j, ref.divPath)) : 1;
    return Number.isFinite(num) && Number.isFinite(den) && num > 0 && den > 0 ? num / den : null;
  } catch { return null; }
}
export const httpJson = async url => { const r = await fetch(url, {headers: {accept: 'application/json'}, signal: AbortSignal.timeout(10_000)}); if (!r.ok) throw Error(`HTTP ${r.status}`); return r.json(); };

/**
 * feed: {price: bigint|string (14 dec), timestamp} | null; now: ledger time; ref: number|null.
 * Returns {alerts, recommend: [{contract, fn, args}]} — recommend is empty when healthy.
 */
export function assessDepeg({feed, now, ref}, cfg) {
  const depegBps = cfg.watcher?.depegBps ?? 100, maxAge = cfg.watcher?.feedMaxAgeSec ?? 600, alerts = [];
  if (!feed) alerts.push({level: 'alert', msg: 'PYUSD feed returned no price'});
  else {
    const age = now - Number(feed.timestamp);
    if (age > maxAge) alerts.push({level: 'alert', msg: `PYUSD feed ${age} s old (> ${maxAge} s)`, age});
    const p = Number(BigInt(feed.price)) / P14;
    const dev1 = Math.round(Math.abs(p - 1) * 10_000);
    if (dev1 > depegBps) alerts.push({level: 'alert', msg: `PYUSD feed ${p} is ${dev1} bps from par`, bps: dev1});
    if (ref != null) {
      const bps = Math.round((Math.abs(p - ref) / ref) * 10_000);
      if (bps > depegBps) alerts.push({level: 'alert', msg: `feed ${p} vs reference ${ref}: ${bps} bps (> ${depegBps})`, bps});
    }
  }
  const c = cfg.contracts ?? {};
  const recommend = alerts.length ? [
    {contract: c.receipt, fn: 'stage_pause_deposits', args: {guardian: '<guardian>'}, note: 'guardian: stop new staged deposits'},
    {contract: c.controller, fn: 'pause_borrow_g', args: {guardian: '<guardian>', market: c.usdcMarket, paused: true}, note: 'guardian: stop new USDC borrows'},
  ] : [];
  return {alerts, recommend};
}
