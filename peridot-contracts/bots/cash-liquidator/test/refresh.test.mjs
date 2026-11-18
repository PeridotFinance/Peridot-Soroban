import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import * as S from '@stellar/stellar-sdk';
import {refreshDecision, budgetFor, netInventory, inventoryShortfalls} from '../src/plan.mjs';
import {refreshPrices} from '../src/keeper.mjs';

const pins = JSON.parse(fs.readFileSync(new URL('../config/mainnet.json', import.meta.url), 'utf8'));
const base = {oracleMatches: true, live: {timestamp: 1000n}, cached: {timestamp: 700n}, spentTodayStroops: '0', dailyBudgetStroops: 30_000_000, maxFeeStroops: 50_000};

test('refresh decision: every new round, never twice per round, idle without oracle, budget-capped', () => {
  assert.equal(refreshDecision(base).refresh, true);
  assert.equal(refreshDecision({...base, cached: null}).refresh, true);                       // first observation
  assert.match(refreshDecision({...base, cached: {timestamp: 1000n}}).reason, /latest round/);  // someone already refreshed
  assert.match(refreshDecision({...base, oracleMatches: false}).reason, /oracle not set/);
  assert.match(refreshDecision({...base, live: null}).reason, /no live/);
  const r = refreshDecision({...base, spentTodayStroops: '29960000'});
  assert.equal(r.refresh, false); assert.equal(r.alert, true);
});

test('budget rolls per UTC day', () => {
  const t = Date.UTC(2026, 10, 18, 12);
  assert.deepEqual(budgetFor(null, t), {day: '2026-11-18', spentStroops: '0'});
  assert.deepEqual(budgetFor({day: '2026-11-18', spentStroops: '5'}, t), {day: '2026-11-18', spentStroops: '5'});
  assert.deepEqual(budgetFor({day: '2026-11-17', spentStroops: '5'}, t), {day: '2026-11-18', spentStroops: '0'});
});

test('XLM inventory excludes the fee floor and reserves', () => {
  assert.deepEqual(netInventory({XLM: 2_522_0000000n, USDC: 5n}, {feeFloorXlmRaw: 20_0000000n, accountReserveXlmRaw: 2_0000000n}), {XLM: 2_500_0000000n, USDC: 5n});
  assert.equal(netInventory({XLM: 10_0000000n}, {feeFloorXlmRaw: 20_0000000n}).XLM, 0n);
});

// Fake chain: records calls, never touches the network.
function fakeChain({oracle = pins.oracle.feed, liveTs = 1000n, cachedTs = 700n, retval = [1n, 1n], fee = '20000', fail = {}} = {}) {
  const calls = [];
  const kp = S.Keypair.random();
  const acct = new S.Account(kp.publicKey(), '1');
  const c = {
    calls,
    addr: a => S.nativeToScVal(a, {type: 'address'}),
    async view(contract, method) {
      calls.push(['view', method]);
      if (method === 'get_oracle') { if (fail.oracle) throw Error('rpc down'); return oracle; }
      if (method === 'lastprice') { if (fail.feed) throw Error('feed read failed'); return {price: 100000000000000n, timestamp: liveTs}; }
      throw Error('unexpected view ' + method);
    },
    async raw() { calls.push(['raw']); return cachedTs == null ? null : {price: 1n, scale: 1n, timestamp: cachedTs, resolution: 300}; },
    async prepare(contract, method, args, opts) {
      calls.push(['prepare', method, opts]);
      const tx = new S.TransactionBuilder(acct, {fee, networkPassphrase: S.Networks.PUBLIC}).addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(0).build();
      return {ok: true, prepared: tx, retval, metrics: {cpu: 3_500_000}};
    },
    sign(tx) { calls.push(['sign']); return tx; },
    async submit(tx) { calls.push(['submit']); return {hash: tx.hash().toString('hex'), status: 'SUCCESS', ledger: 1}; },
  };
  return c;
}
const cfgFor = (dryRun, extra = {}) => ({pins, dryRun, refresh: {inclusionFee: 1_000, maxFeeStroops: 50_000, dailyBudgetStroops: 30_000_000}, ...extra});
const withFee = (c, fee) => Object.assign(c, {async submit(tx) { c.calls.push(['submit']); return {hash: 'h', status: 'SUCCESS', ledger: 1, feeCharged: fee}; }});
const quiet = fn => async () => { const l = console.log; console.log = () => {}; try { await fn(); } finally { console.log = l; } };

test('refresher: live mode refreshes all three tokens with its own fee caps and charges the budget', quiet(async () => {
  const c = fakeChain(), st = {};
  const r = await refreshPrices(cfgFor(false), c, st, Date.UTC(2026, 10, 18));
  assert.deepEqual(r.map(x => x.status), ['SUCCESS', 'SUCCESS', 'SUCCESS']);
  assert.deepEqual(r.map(x => x.market), Object.keys(pins.oracle.symbols));
  for (const p of c.calls.filter(x => x[0] === 'prepare')) { assert.equal(p[1], 'cache_price'); assert.deepEqual(p[2], {inclusionFee: 1_000, maxFeeStroops: 50_000, timeoutSec: 30}); }
  assert.equal(st.refreshBudget.spentStroops, '60000');
}));

test('refresher: dry run never signs; idle while the controller oracle is unset; skips None and same-round', quiet(async () => {
  let c = fakeChain(); let r = await refreshPrices(cfgFor(true), c, {});
  assert.equal(c.calls.filter(x => x[0] === 'sign' || x[0] === 'submit').length, 0); assert(r.every(x => x.dryRun));
  c = fakeChain({oracle: null}); r = await refreshPrices(cfgFor(false), c, {});
  assert.deepEqual(r, []); assert.equal(c.calls.filter(x => x[0] === 'prepare').length, 0);
  c = fakeChain({retval: null}); r = await refreshPrices(cfgFor(false), c, {});
  assert(r.every(x => /None/.test(x.skipped))); assert.equal(c.calls.filter(x => x[0] === 'submit').length, 0);
  c = fakeChain({cachedTs: 1000n}); r = await refreshPrices(cfgFor(false), c, {});
  assert(r.every(x => /latest round/.test(x.skipped)));
}));

test('refresher: exhausted budget and read failures never throw (liquidation pass continues)', quiet(async () => {
  let c = fakeChain(); const st = {refreshBudget: {day: new Date().toISOString().slice(0, 10), spentStroops: '29990000'}};
  let r = await refreshPrices(cfgFor(false), c, st);
  assert(r.every(x => /budget/.test(x.skipped)));
  c = fakeChain({fail: {feed: true}}); r = await refreshPrices(cfgFor(false), c, {});
  assert(r.every(x => x.error));
  c = fakeChain({fail: {oracle: true}}); r = await refreshPrices(cfgFor(false), c, {});
  assert.deepEqual(r, []);
}));

test('inventory shortfalls against the funded minimums', () => {
  assert.deepEqual(inventoryShortfalls({USDC: 500_0000000n, XLM: 2_500_0000000n, EURC: 250_0000000n}, pins.minInventoryRaw), []);
  const sh = inventoryShortfalls({USDC: 100n, XLM: 2_500_0000000n}, pins.minInventoryRaw);
  assert.deepEqual(sh.map(x => x.asset), ['USDC', 'EURC']);
});

test('refresher: the budget is charged the actual fee when known, the max fee otherwise', async () => {
  const l = console.log; console.log = () => {};
  try {
    const st = {}; await refreshPrices(cfgFor(false), withFee(fakeChain(), '8123'), st);
    assert.equal(st.refreshBudget.spentStroops, String(3 * 8123));
  } finally { console.log = l; }
});
