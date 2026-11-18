import test from 'node:test';
import assert from 'node:assert/strict';
import {plan, shortfallAt, repayFor, fallbackEligible} from '../src/plan.mjs';

const NOW = 1_800_000_000, U = 10_000_000n; // 7 decimals
const base = (o = {}) => ({
  now: NOW, navSim: 'ok', navPreview: 1_000_000n, fallbackPreview: null,
  status: {nav: {rate: 1_000_000n, ts: NOW - 10}, fallback: null, chain: null, next_pending: 0},
  priceCache: {timestamp: NOW - 100, resolution: 300, k: 2}, cf: 800_000n, closeFactor: 500_000n, li: 1_080_000n,
  inventory: 10_000n * U, allowance: 10_000n * U, borrowers: [{user: 'GB', units: 1000n * U, debt: 900n * U}],
  pending: [], lastClaim: null, l4FailedAt: null, blockedL3: new Set(), ...o,
});
const names = p => p.stages.map(s => s.stage);
const withNav = (age, o = {}) => base({navPreview: null, status: {...base().status, nav: {rate: 1_000_000n, ts: NOW - age}}, ...o});

test('shortfall, repay at 50% close factor, capped by collateral / incentive and cap', () => {
  const b = {units: 1000n * U, debt: 900n * U};
  assert.equal(shortfallAt(b, 1_000_000n, 800_000n), 100n * U);
  assert.equal(repayFor(b, 1_000_000n, {closeFactor: 500_000n, li: 1_080_000n}), 450n * U);
  assert.equal(repayFor(b, 1_000_000n, {closeFactor: 500_000n, li: 1_080_000n, repayCap: 7n * U}), 7n * U);
  assert.equal(repayFor({units: 100n * U, debt: 900n * U}, 1_000_000n, {closeFactor: 500_000n, li: 1_080_000n}), 925_925_925n);
});

test('NAV 45 s old: valid at submission, L3 directly', () => {
  assert.deepEqual(names(plan(withNav(45))), ['L3']);
});
test('NAV 46 s old: refresh first, L3 still on the NAV path', () => {
  const p = plan(withNav(46));
  assert.deepEqual(names(p), ['L1a', 'L3']);
});
test('NAV 60 s and 61 s old: L1a; L3 waits for a checkpoint valid at submission', () => {
  for (const age of [60, 61]) assert.deepEqual(names(plan(withNav(age))), ['L1a']);
  assert.deepEqual(names(plan(withNav(61, {navPreview: 1_000_000n}))), ['L1a', 'L3']);
  assert.equal(plan(withNav(61, {navPreview: 1_000_000n})).stages[1].rateSource, 'nav-after-L1a');
});
test('healthy borrower: no L3; NAV kept warm only while debt exists', () => {
  const healthy = [{user: 'GB', units: 1000n * U, debt: 100n * U}];
  assert.deepEqual(names(plan(withNav(50, {borrowers: healthy}))), ['L1a']);
  assert.deepEqual(names(plan(withNav(50, {borrowers: [{user: 'GB', units: 0n, debt: 0n}]}))), []);
});

const blocked = (since, last, o = {}) => base({navSim: 'blocked', navPreview: null, fallbackPreview: 700_000n,
  status: {nav: null, fallback: null, chain: {since: NOW - since, last: NOW - last, n: 10}, next_pending: 0}, ...o});
test('D3 chain 9:59 refused, 10:00 admitted (task rule last - since)', () => {
  assert.equal(fallbackEligible({since: NOW - 599, last: NOW}, NOW), false);
  assert.equal(fallbackEligible({since: NOW - 600, last: NOW}, NOW), true);
  assert.deepEqual(names(plan(blocked(599, 0))), []);
  assert.match(plan(blocked(599, 0)).alerts.map(a => a.msg).join(), /fallback admissible in about 1 s/);
  const p = plan(blocked(600, 0));
  assert.deepEqual(names(p), ['L1b', 'L3']);
  assert.equal(p.stages[1].rateSource, 'fallback-after-L1b');
});
test('D3 gap: 60 s alive (heartbeat then L1b), 61 s broken (restart only)', () => {
  assert.deepEqual(names(plan(blocked(700, 60))), ['L1a', 'L1b', 'L3']);
  const p = plan(blocked(700, 61));
  assert.deepEqual(names(p), ['L1a']);
  assert.match(p.alerts.map(a => a.msg).join(), /lapsed/);
});
test('D3 heartbeat every 20 s while blocked; healthy pool clears the chain', () => {
  assert.deepEqual(names(plan(blocked(100, 14, {borrowers: []}))), []);
  assert.equal(plan(blocked(100, 14, {borrowers: []})).waitSec, 1);
  assert.deepEqual(names(plan(blocked(100, 15, {borrowers: []}))), ['L1a']);
  assert.deepEqual(names(plan(base({navSim: 'blocked', status: {...base().status, nav: null, chain: null}, borrowers: []}))), ['L1a']);
  const clear = plan(base({borrowers: [], status: {...base().status, chain: {since: NOW - 30, last: NOW - 5, n: 2}}}));
  assert.deepEqual(names(clear), ['L1a']); assert.match(clear.stages[0].reason, /clear/);
});
test('valid fallback checkpoint is used while blocked; expired one is not', () => {
  const fb = ts => blocked(100, 0, {fallbackPreview: null, status: {nav: null, fallback: {rate: 700_000n, ts: NOW - ts}, chain: {since: NOW - 100, last: NOW, n: 5}, next_pending: 0}});
  assert.deepEqual(names(plan(fb(50))), ['L3']);
  assert.deepEqual(names(plan(fb(51))), []);
});

test('repay-only: oracle stale => no checkpoint, no liquidation, alert; L4/L5 still run', () => {
  const p = plan(base({navSim: 'oracle_stale', pending: [{id: 0, from: 'GB', to: 'GL', n: 1n}]}));
  assert.equal(p.mode, 'repay-only');
  assert.deepEqual(names(p), ['L4', 'L5']);
  assert.match(p.alerts[0].msg, /repay-only/);
});

test('pending queue: L4 first, then L5 in id order; fresh claim skips L4; queued borrower gets no L3', () => {
  const pending = [3, 1, 2].map(id => ({id, from: 'GX', to: 'GL', n: 5n}));
  const p = plan(base({pending}));
  assert.deepEqual(names(p), ['L3', 'L4', 'L5', 'L5', 'L5']);
  assert.deepEqual(p.stages.filter(s => s.stage === 'L5').map(s => s.id), [1, 2, 3]);
  assert.deepEqual(names(plan(base({pending, lastClaim: NOW - 50, borrowers: []}))), ['L5', 'L5', 'L5']);
  assert.deepEqual(names(plan(base({pending, lastClaim: NOW - 51, borrowers: []}))), ['L4', 'L5', 'L5', 'L5']);
  const f = plan(base({pending, l4FailedAt: NOW - 10, borrowers: []}));
  assert.deepEqual(names(f), ['L5', 'L5', 'L5']); assert.match(f.alerts[0].msg, /boundary/);
  assert.ok(!names(plan(base({pending: [{id: 0, from: 'GB', to: 'GL', n: 1n}]}))).includes('L3'));
});

test('L2 precedes the checkpoint; APPROVE before L3; journal-blocked borrower skipped', () => {
  assert.deepEqual(names(plan(withNav(50, {priceCache: null, navPreview: 1_000_000n}))), ['L2', 'L1a', 'L3']);
  assert.deepEqual(names(plan(base({priceCache: {timestamp: NOW - 531, resolution: 300, k: 2}}))), ['L2', 'L3']);
  assert.deepEqual(names(plan(base({priceCache: {timestamp: NOW - 530, resolution: 300, k: 2}}))), ['L3']);
  assert.deepEqual(names(plan(base({allowance: 0n}))), ['APPROVE', 'L3']);
  assert.deepEqual(names(plan(blocked(600, 0, {allowance: 0n, priceCache: null}))), ['APPROVE', 'L2', 'L1b', 'L3']);
  // A due heartbeat is always the head, ahead of APPROVE / L2 (else the 60 s chain gap breaks).
  assert.deepEqual(names(plan(blocked(700, 55, {allowance: 0n, priceCache: null}))), ['L1a', 'APPROVE', 'L2', 'L1b', 'L3']);
  // APPROVE uses the stable cap as ceiling, so a drifting repay does not re-trigger it.
  const ap = plan(base({allowance: 0n}), {repayCap: 1000n * U}).stages[0];
  assert.equal(ap.amount, 1000n * U);
  assert.deepEqual(names(plan(base({allowance: 1000n * U}), {repayCap: 1000n * U})), ['L3']);
  // L2 preference (expires within 60 s after submission) is tried once per oracle round; required when invalid.
  const soon = {timestamp: NOW - 531, resolution: 300, k: 2};
  assert.deepEqual(names(plan(base({priceCache: soon, l2TriedFor: NOW - 531}))), ['L3']);
  assert.deepEqual(names(plan(base({priceCache: {...soon, timestamp: NOW - 591}}))), ['L2', 'L3']);
  const late = plan(base({priceCache: {...soon, timestamp: NOW - 591}, l2TriedFor: NOW - 591}));
  assert.deepEqual(names(late), []); assert.match(late.alerts[0].msg, /round late/);
  assert.deepEqual(names(plan(base({blockedL3: new Set(['GB'])}))), []);
});
