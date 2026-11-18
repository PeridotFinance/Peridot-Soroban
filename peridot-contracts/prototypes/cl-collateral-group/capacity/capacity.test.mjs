import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {amounts, sqrtAtTick, inKindPayout, capacityFromQuotes, capacityFromPool, poolSwap, caps, run} from './capacity.mjs';

const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) <= eps * Math.max(1, Math.abs(b)), `${a} != ${b}`);
const pos = {liquidityRaw: 1e13, decimals: 7, tickLower: -20, tickUpper: 20, tick: 0, idlePyusd: 10, idleUsdc: 20, totalUnits: 1000};

test('(a) in-kind mixture: in range both legs, at/below the lower edge all PYUSD; pro-rata incl. idle', () => {
  const L = 1e6, a = amounts(L, 1, -20, 20);
  near(a.pyusd, L * (1 - 1 / sqrtAtTick(20))); near(a.usdc, L * (1 - sqrtAtTick(-20)));
  const p = inKindPayout(pos, 100, 1);
  near(p.now.pyusd, 0.1 * (a.pyusd + 10)); near(p.now.usdc, 0.1 * (a.usdc + 20));
  near(p.worst.usdc, 0.1 * 20);
  near(p.worst.pyusd, 0.1 * (L * (1 / sqrtAtTick(-20) - 1 / sqrtAtTick(20)) + 10));
  assert.ok(p.now.pyusdValueShare > 0.45 && p.now.pyusdValueShare < 0.55);
  // Matches the staged_v2 fallback a0 = L(1/s_lo - 1/s_hi) with the S_LO/S_HI constants (1e18).
  near(1 / sqrtAtTick(-20) - 1 / sqrtAtTick(20), 1e18 / 999_000_549_780_071_480 - 1e18 / 1_001_000_450_120_021_002, 1e-12);
});

test('(b) quotes: linear interpolation on slippage; lower bound when every quote fits', () => {
  const q = [{in: 100, out: 99.95}, {in: 1000, out: 995}, {in: 10000, out: 9700}];
  near(capacityFromQuotes(q, 1, 0.01).pyusd, 1000 + (0.005 / 0.025) * 9000);
  near(capacityFromQuotes(q, 1, 0.0004).pyusd, 80);
  assert.match(capacityFromQuotes(q, 1, 0.05).bound, /lower-bound/);
});

test('(b) pool model: capacity scales with effective liquidity, 2% > 1%, range edge respected', () => {
  const pool = {activeLiquidity: 1e6, ourLiquidity: 0, thirdPartyHaircut: 0, tick: 0, tickLower: -10_000, feeBps: 0};
  const c1 = capacityFromPool(pool, 1, 0.01).pyusd, c2 = capacityFromPool(pool, 1, 0.02).pyusd;
  assert.ok(c2 > c1 && c1 > 0);
  near(capacityFromPool({...pool, thirdPartyHaircut: 0.5}, 1, 0.01).pyusd, c1 / 2, 1e-6);
  near(capacityFromPool({...pool, ourLiquidity: 5e5}, 1, 0.01).pyusd, c1 / 2, 1e-6);
  // zero-fee CL: slippage(dx) = dx / (L/s + dx) at s = 1, so 1% <=> dx = L/99
  near(c1, 1e6 / 99, 1e-6);
  const narrow = capacityFromPool({...pool, tickLower: -2}, 1, 0.05);
  assert.equal(narrow.bound, 'range edge');
  assert.ok(!poolSwap({...pool, tickLower: -2}, narrow.pyusd).exhausted);
});

test('(c) caps: full debt 1.08 D vs one round 0.54 D, safety factor 2, PYUSD share, backing cap', () => {
  const c = caps(1000, {collateralFactor: 0.8});
  near(c.borrowCapFullDebt, 1000 / (2 * 1.08)); near(c.borrowCapOneRound, 1000 / (2 * 0.54));
  assert.equal(c.borrowCap, c.borrowCapFullDebt);
  near(c.backingCap, c.borrowCap / 0.8);
  near(caps(1000, {pyusdShare: 0.5}).borrowCap, 2 * c.borrowCap);
});

test('example input runs offline end to end', () => {
  const r = run(JSON.parse(fs.readFileSync(new URL('./example.input.json', import.meta.url))));
  assert.equal(r.budgets.length, 2); assert.equal(r.payouts.length, 2);
  assert.ok(r.budgets[1].borrowCap > r.budgets[0].borrowCap && r.budgets[0].backingCap > r.budgets[0].borrowCap);
});
