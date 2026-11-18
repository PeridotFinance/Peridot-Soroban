import test from 'node:test';
import assert from 'node:assert/strict';
import {planLiquidation, seizeIsProfitable, depegAlerts, mergeBorrowers} from '../src/plan.mjs';

const P14 = 10n ** 14n;
const params = {closeFactor: 500_000n, li: 1_080_000n, minProfitBps: 200, neverTouch: new Set(['CLRECEIPT'])};
const pos = (market, o) => ({market, receipt: market + 'R', pbal: 0n, debt: 0n, rate: 1_000_000n, price: P14, scale: P14, cf: 750_000n, ...o});

test('picks largest priced debt and largest CF>0 collateral in different markets', () => {
  const p = planLiquidation([
    pos('XLM', {pbal: 100_000_0000000n, price: P14 / 10n, cf: 550_000n}),          // $10k XLM collateral
    pos('USDC', {debt: 7_000_0000000n}),                                            // $7k USDC debt
    pos('EURC', {debt: 1_000_0000000n, price: 11n * P14 / 10n, cf: 700_000n}),
  ], {USDC: 10_000_0000000n, EURC: 10_000_0000000n, XLM: 0n}, params);
  assert.equal(p.action, 'liquidate'); assert.equal(p.repayMarket, 'USDC'); assert.equal(p.collateralMarket, 'XLM');
  assert.equal(p.repay, 3_500_0000000n);                                            // close factor 50%
});

test('repay is capped by inventory and by collateral / incentive', () => {
  const p = planLiquidation([pos('XLM', {pbal: 1_000_0000000n, price: P14 / 10n}), pos('USDC', {debt: 1_000_0000000n})],
    {USDC: 1_000_0000000n}, params);
  // collateral $100 / 1.08 = $92.59 → repay ≤ 92.5925925 USDC
  assert.equal(p.repay, 925925925n);
  const q = planLiquidation([pos('XLM', {pbal: 100_000_0000000n, price: P14 / 10n}), pos('USDC', {debt: 1_000_0000000n})], {USDC: 10_0000000n}, params);
  assert.equal(q.repay, 10_0000000n);
});

test('same-market loop is skipped with an alert reason', () => {
  const p = planLiquidation([pos('USDC', {pbal: 100_0000000n, debt: 90_0000000n})], {USDC: 1_000_0000000n}, params);
  assert.equal(p.action, 'skip'); assert.match(p.reason, /same-market/);
});

test('debt in an unpriced market is indeterminate; CF-0 or unpriced collateral is never seized', () => {
  assert.match(planLiquidation([pos('PYUSD', {debt: 1n, price: null, scale: null}), pos('XLM', {pbal: 10n})], {PYUSD: 10n}, params).reason, /indeterminate/);
  const p = planLiquidation([pos('PYUSD', {pbal: 1_000_0000000n, price: null, scale: null, cf: 0n}), pos('EURC', {pbal: 1_000_0000000n, cf: 0n}), pos('USDC', {debt: 50_0000000n})], {USDC: 1_000_0000000n}, params);
  assert.equal(p.action, 'skip'); assert.match(p.reason, /no seizable/);
});

test('never-touch receipts (CL staged) are excluded', () => {
  const p = planLiquidation([{...pos('CL', {pbal: 1_000_0000000n}), receipt: 'CLRECEIPT'}, pos('USDC', {debt: 50_0000000n})], {USDC: 1_000_0000000n}, params);
  assert.equal(p.action, 'skip');
});

test('seize profitability uses exchange rate and price', () => {
  const coll = {rate: 1_000_000n, price: P14 / 10n, scale: P14};
  // repay $100, seize 1080 XLM @ $0.10 = $108 → 8% ≥ 2%
  assert.equal(seizeIsProfitable(1_080_0000000n, coll, 100_0000000n, 200).ok, true);
  assert.equal(seizeIsProfitable(1_010_0000000n, coll, 100_0000000n, 200).ok, false);
});

test('depeg alerts', () => {
  assert.deepEqual(depegAlerts({usdc: P14, eurc: 11n * P14 / 10n, eurFiat: 11n * P14 / 10n}), []);
  assert.equal(depegAlerts({usdc: 99n * P14 / 100n})[0].asset, 'USDC');
  assert.equal(depegAlerts({eurc: 108n * P14 / 100n, eurFiat: 110n * P14 / 100n})[0].asset, 'EURC');
});

test('borrower merge is unique and sorted', () => {
  assert.deepEqual(mergeBorrowers(['B'], [{borrower: 'A'}, {borrower: 'B'}, {}]), ['A', 'B']);
});
