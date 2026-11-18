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
