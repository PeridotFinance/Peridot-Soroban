import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {marginCycle, marginPass} from '../src/margin.mjs';
import {syntheticAlerts, ALERT_TYPES} from '../src/margin-alert-check.mjs';
import {cycle, waitForNextCycle, remainingMs} from '../src/keeper.mjs';
import {loadConfig, loadMarginConfig} from '../src/config.mjs';
import {makeChain} from '../src/chain.mjs';
import {stubChain, marginCfg, quietLog, FIX, PINS, MARGIN, KEEPER, BACKER} from './support/margin-stub.mjs';

const FUNDED = {USDC: 500_0000000n, XLM: 2_500_0000000n, EURC: 250_0000000n};   // funding.md T1 inventory
const deps = (o = {}) => { const q = quietLog(); const j = []; return {q, j, d: {log: q.log, journal: (f, r) => j.push(r), inventoryOf: async () => o.inventory ?? FUNDED, nowMs: o.nowMs ?? 1_000_000}}; };
const writes = c => c.calls.filter(x => x[0] === 'sign' || x[0] === 'submit');
const alerts = q => q.lines.filter(l => l.level === 'alert');

test('DRY RUN on fork -20 % responses: bootstrap from the counter, would liquidate L5 only, simulate reconcile, never sign', async () => {
  const c = stubChain('minus20'), st = {}, {q, j, d} = deps();
  const r = await marginCycle(marginCfg(), c, st, d);
  assert.equal(r.dryRun, true); assert.equal(r.positions, 3);
  assert.deepEqual(r.liquidatable, ['1']);
  const liq = r.actions.liquidations[0];
  assert.equal(liq.dryRun, true); assert.equal(liq.source, 'sim events');
  assert.equal(liq.pay, BigInt(FIX.checks.L5_underwater_minus20.fork.paid)); assert.equal(liq.underwater, true);
  assert(liq.profitUsd7 > 0n);
  const p = c.calls.find(x => x[0] === 'prepare' && x[1] === 'liquidate');
  assert.deepEqual(p[2], {cpuStop: 289_000_000, maxFeeStroops: 30_000_000});   // margin caps, not the 97 M cash cpuStop
  assert.deepEqual(c.calls.filter(x => x[0] === 'prepare').map(x => x[1]), ['liquidate', 'reconcile']);
  assert.equal(writes(c).length, 0); assert.equal(j.length, 0);
  assert.deepEqual(alerts(q), []);
  assert.deepEqual(st.margin.active.sort(), ['1', '2', '3']); assert.equal(st.margin.lastEventLedger, FIX.scenarios.minus20.latestLedger);
  JSON.stringify(st);   // state stays JSON-serialisable (no bigints)
});

test('DRY RUN on fork seizure responses: settle sized by settle_max_ptokens, reconcile true is alerted but not sent', async () => {
  const c = stubChain('seizure'), st = {}, {q, d} = deps();
  const r = await marginCycle(marginCfg(), c, st, d);
  assert.equal(r.actions.settles.length, 1);
  assert.equal(r.actions.settles[0].asset, 'XLM'); assert.equal(r.actions.settles[0].maxPtokens, 418_478_166n); assert.equal(r.actions.settles[0].dryRun, true);
  assert.equal(r.actions.reconcile.newlyImpaired, true);
  assert.equal(writes(c).length, 0);
  const keys = alerts(q).map(a => a.key).filter(Boolean);
  assert.deepEqual(keys.sort(), ['impaired', 'opens_refused', 'pooled_health']);
  assert(alerts(q).some(a => /reconcile found/.test(a.msg)));
});

test('LIVE: settle repeats until BsDebtShares is 0, journals intent before every send, then reconcile is sent', async () => {
  const c = stubChain('seizure', {settleStep: 2_000_000_000n}), st = {}, {j, d} = deps();
  const r = await marginCycle(marginCfg({dryRun: false}), c, st, d);
  assert.equal(r.actions.settles.filter(s => s.status === 'SUCCESS').length, 3);   // 4.49e9 shares in 2e9 steps
  assert.equal(c.state.bs.XLM, 0n);
  assert.deepEqual(writes(c).map(x => x.join(':')), ['sign:settle_backstop_debt', 'submit:settle_backstop_debt', 'sign:settle_backstop_debt', 'submit:settle_backstop_debt',
    'sign:settle_backstop_debt', 'submit:settle_backstop_debt', 'sign:reconcile', 'submit:reconcile']);
  assert.deepEqual(j.map(x => (x.intent ? 'intent' : 'result')), ['intent', 'result', 'intent', 'result', 'intent', 'result', 'intent', 'result']);
});

test('LIVE: PooledShortfall #35 alerts (recapitalise) and sends nothing; no progress stops the loop', async () => {
  let c = stubChain('seizure', {shortfall: true}), {q, d} = deps();
  await marginCycle(marginCfg({dryRun: false}), c, {}, d);
  assert(alerts(q).some(a => /PooledShortfall #35/.test(a.msg)));
  assert(!writes(c).some(x => x[1] === 'settle_backstop_debt'));
  c = stubChain('seizure', {settleStep: 0n}); ({q, d} = deps());
  const r = await marginCycle(marginCfg({dryRun: false}), c, {}, d);
  assert.equal(r.actions.settles.length, 1); assert(alerts(q).some(a => /no progress/.test(a.msg)));
});

test('LIVE: liquidates L5 with the journal, the position then drops out; DRY_RUN=true overrides MARGIN_DRY_RUN=false', async () => {
  const c = stubChain('minus20'), st = {}, {j, d} = deps();
  const r = await marginCycle(marginCfg({dryRun: false}), c, st, d);
  assert.equal(r.actions.liquidations[0].status, 'SUCCESS'); assert.equal(j[0].kind, 'margin_liquidate'); assert(j[0].intent);
  c.state.seq += 1;
  const r2 = await marginCycle(marginCfg({dryRun: false}), c, st, d);
  assert.deepEqual(r2.liquidatable, []); assert(!st.margin.active.includes('1'));   // NotOpen #13 on the ltv read drops it
  const c3 = stubChain('minus20');
  const cfg = {...marginCfg({dryRun: false}), dryRun: true};
  await marginCycle(cfg, c3, {}, deps().d);
  assert.equal(writes(c3).length, 0);
});

test('inventory below the payment: no simulation, alert; below the largest debt: inventory alert per side', async () => {
  const c = stubChain('minus20'), {q, d} = deps({inventory: {USDC: 50_0000000n, XLM: 0n}});
  const r = await marginCycle(marginCfg(), c, {}, d);
  assert.equal(r.actions.liquidations[0].skipped, 'inventory');
  assert(!c.calls.some(x => x[1] === 'liquidate'));
  assert(alerts(q).some(a => a.key === 'inventory:USDC'));
});

test('CAP-88: the same ledger sequence runs once; a new sequence with the same timestamp runs; 10-min alert fires once', async () => {
  const c = stubChain('minus20', {closeTime: 1_795_000_000}), st = {};
  let t = 1_000_000, fired = [];
  const run = async () => { const {q, d} = deps({nowMs: t}); const r = await marginCycle(marginCfg(), c, st, d); fired.push(...alerts(q).filter(a => a.key?.startsWith('liquidatable'))); return r; };
  await run();
  t += 30_000; assert.equal((await run()).skipped, 'same ledger');            // no double count
  for (let i = 0; i < 21; i++) { c.state.seq += 1; t += 30_000; const r = await run(); assert.notEqual(r.skipped, 'same ledger'); }   // closeTime never changes
  assert.equal(st.margin.liqSince['1'].observations, 22);
  assert.equal(fired.length, 1); assert.equal(fired[0].id, '1'); assert.equal(fired[0].minutes, 10);
});

test('discovery: events after bootstrap add/remove positions and track backer exit requests; expiry is strict (now > expires_at)', async () => {
  const c = stubChain('minus20', {closeTime: 2_000n}), st = {};
  await marginCycle(marginCfg(), c, st, deps().d);
  c.state.positions['9'] = {id: 9, side: 'short', ltv: '500000', liqLtv: '900000', heldPt: '1000', debt: '10', funding: '0', open: true};
  c.state.withdrawReq[BACKER] = {shares: 5n, ready_at: 1_000n, expires_at: 2_000n};
  c.state.events = [{ledger: c.state.seq + 1, name: 'margin_open', data: [9n]}, {ledger: c.state.seq + 1, name: 'margin_close', data: [3n]},
    {ledger: c.state.seq + 1, name: 'bs_request', data: [BACKER, 5n, 1_000n, 2_000n]}];
  c.state.seq += 1;
  let {q, d} = deps(); let r = await marginCycle(marginCfg(), c, st, d);
  assert.deepEqual(st.margin.active.sort(), ['1', '2', '9']); assert.equal(st.margin.backers[BACKER], '2000');
  assert.deepEqual(r.actions.cleared, []);                                     // now == expires_at: still live
  c.state.seq += 1; c.state.closeTime = 2_001;
  ({q, d} = deps()); r = await marginCycle(marginCfg(), c, st, d);
  assert.equal(r.actions.cleared[0].dryRun, true); assert.equal(r.actions.cleared[0].backer, BACKER);
  c.state.seq += 1;
  r = await marginCycle(marginCfg({dryRun: false}), c, st, deps().d);
  assert.equal(r.actions.cleared[0].status, 'SUCCESS'); assert.equal(st.margin.backers[BACKER], undefined);
});

test('discovery: >100 events page by cursor (no ledger range with it), a lagging events node bounds the checkpoint, counter catch-up finds a missed open', async () => {
  const c = stubChain('minus20', {eventsLag: 0}), st = {};
  await marginCycle(marginCfg(), c, st, deps().d);
  const base = c.state.seq;
  // 150 open/close pairs across two ledgers: needs cursor pages; ends with id 1 closed
  c.state.events = Array.from({length: 150}, (_, i) => ({ledger: base + 1, name: i % 2 ? 'margin_close' : 'margin_open', data: [BigInt(100 + (i >> 1))]}));
  c.state.events.push({ledger: base + 2, name: 'margin_close', data: [1n]});
  c.state.seq = base + 2;
  await marginCycle(marginCfg(), c, st, deps().d);
  assert.deepEqual(st.margin.active.sort(), ['2', '3']); assert.equal(st.margin.lastEventLedger, base + 2);
  assert(c.calls.some(x => x[0] === 'rawEvents' && x[2] === 'cursor'));
  // events node lags 5 ledgers: an event in that window is not yet visible and the checkpoint stops at the node's ledger
  const lag = stubChain('minus20', {eventsLag: 5}), st2 = {};
  await marginCycle(marginCfg(), lag, st2, deps().d);
  const b2 = lag.state.seq;
  lag.state.events = [{ledger: b2 + 7, name: 'margin_close', data: [2n]}];
  lag.state.seq = b2 + 10;
  await marginCycle(marginCfg(), lag, st2, deps().d);
  assert.equal(st2.margin.lastEventLedger, b2 + 5); assert(st2.margin.active.includes('2'));
  lag.state.seq = b2 + 14;
  await marginCycle(marginCfg(), lag, st2, deps().d);
  assert(!st2.margin.active.includes('2'));   // picked up once the events node caught up: nothing skipped
  // an open whose event never arrives is still found through the counter
  lag.state.positions['4'] = {id: 4, side: 'short', ltv: '500000', liqLtv: '900000', heldPt: '1000', debt: '10', funding: '0', open: true};
  lag.state.counter = 4; lag.state.seq += 1;
  await marginCycle(marginCfg(), lag, st2, deps().d);
  assert(st2.margin.active.includes('4')); assert.equal(st2.margin.maxSeenId, '4');
});

test('10-min timer survives a price outage on the position views (not observed healthy), and fires', async () => {
  const c = stubChain('minus20'), st = {};
  let t = 0;
  await marginCycle(marginCfg(), c, st, deps({nowMs: t}).d);
  assert(st.margin.liqSince['1']);
  const out = stubChain('minus20', {failView: {get_position_ltv: 'HostError: Error(Contract, #11)'}});
  out.state.seq = c.state.seq;
  let fired = [];
  for (let i = 1; i <= 25; i++) { out.state.seq += 1; t += 30_000; const {q, d} = deps({nowMs: t}); await marginCycle(marginCfg(), out, st, d); fired.push(...alerts(q).filter(a => a.key === 'liquidatable:1')); }
  assert.equal(fired.length, 1); assert.equal(st.margin.liqSince['1'].firstSeenMs, 0);
});

test('pin checks: wrong WASM or keeper == margin admin -> margin pass does nothing (alerting), never throws', async () => {
  for (const o of [{wasm: 'ff'.repeat(32)}, {adminKey: KEEPER}]) {
    const c = stubChain('minus20', o), st = {}, {q, d} = deps();
    for (let i = 0; i < 3; i++) { c.state.seq += 1; const r = await marginPass(marginCfg(), c, st, d); assert(r.error); }
    assert(!c.calls.some(x => x[0] === 'prepare')); assert(alerts(q).some(a => a.msg === 'margin cycle failed'));
  }
});

test('margin disabled: zero margin RPC calls', async () => {
  const c = stubChain('minus20');
  const r = await marginPass(marginCfg({enabled: false}), c, {}, deps().d);
  assert.equal(r.skipped, 'disabled'); assert.equal(c.calls.length, 0);
});

test('price unavailable (#11) on a position view: alert for that position, the cycle continues', async () => {
  const c = stubChain('minus20', {failView: {get_position_ltv: 'HostError: Error(Contract, #11)'}}), {q, d} = deps();
  const r = await marginCycle(marginCfg(), c, {}, d);
  assert.equal(r.positions, 0); assert.equal(alerts(q).filter(a => a.key?.startsWith('unavailable:')).length, 3);
});

test('alert check: one synthetic alert per required type (all eight), live messages and types', () => {
  const s = syntheticAlerts(5_000_000);
  assert.deepEqual(s.map(a => a.type).sort(), [...ALERT_TYPES].sort());
  assert.equal(ALERT_TYPES.length, 8);
  for (const a of s) assert.equal(a.extra.type, a.type);
});

test('cash pass: the margin pooled account in shortfall is alerted, never planned or simulated', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cl-')), calls = [];
  const pins = {...PINS, depeg: null, oracle: null, minInventoryRaw: {}};
  const cfg = {pins, publicKey: KEEPER, dryRun: true, stateFile: path.join(dir, 's.json'), journalFile: path.join(dir, 'j'), eventsLookbackLedgers: 10,
    minFeeXlmRaw: 0n, accountReserveXlmRaw: 0n, minProfitBps: 200, neverTouch: new Set(), neverLiquidateBorrowers: new Set([MARGIN])};
  const c = {
    rpc: {getLatestLedger: async () => ({sequence: 100})},
    addr: a => a, u128: x => x,
    borrowEvents: async () => ({events: [{borrower: MARGIN}], cursor: 'x'}),
    async view(contract, method) {
      calls.push(method);
      if (method === 'account_liquidity') return [0n, 1_000_000n];
      if (method === 'balance') return 1_000_0000000n;
      return 500_000n;
    },
    async prepare(contract, method) { calls.push('prepare:' + method); return {ok: false, error: 'x'}; },
  };
  const q = quietLog(); const l = console.log; const out = []; console.log = s => out.push(JSON.parse(s));
  try { await cycle(cfg, c, {borrowers: []}); } finally { console.log = l; }
  assert(out.some(x => x.level === 'alert' && /never-liquidate borrower/.test(x.msg) && x.user === MARGIN));
  assert(!calls.includes('get_user_markets') && !calls.some(x => x.startsWith('prepare:')));
  void q;
});

test('config: margin is off unless enabled with a contract id; the contract is always excluded from cash liquidation; overlay cannot repin WASM', () => {
  const env = {LIQUIDATOR_PUBLIC_KEY: KEEPER};
  let cfg = loadConfig(env);
  assert.equal(cfg.margin.enabled, false); assert.equal(cfg.margin.dryRun, true); assert.equal(cfg.neverLiquidateBorrowers.size, 0);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-')), f = path.join(dir, 'margin.json');
  fs.writeFileSync(f, JSON.stringify({contract: MARGIN}));
  cfg = loadConfig({...env, MARGIN_CONFIG_FILE: f});
  assert.equal(cfg.margin.enabled, false); assert(cfg.neverLiquidateBorrowers.has(MARGIN));   // excluded even while disabled
  fs.writeFileSync(f, JSON.stringify({enabled: true, contract: MARGIN}));
  cfg = loadConfig({...env, MARGIN_CONFIG_FILE: f, MARGIN_DRY_RUN: 'false'});
  assert.equal(cfg.margin.enabled, true); assert.equal(cfg.margin.dryRun, false); assert.deepEqual(cfg.margin.wasmAllowed, [FIX.source.marginWasmSha256]);
  fs.writeFileSync(f, JSON.stringify({enabled: true, contract: MARGIN, wasmAllowed: ['00']}));
  assert.throws(() => loadMarginConfig(PINS, {MARGIN_CONFIG_FILE: f}), /may not override/);
  assert.equal(loadMarginConfig({...PINS, margin: {enabled: true}}, {}).enabled, false);   // no contract id
});

test('main loop: the period is measured from the cycle start; with margin on it wakes early on a new Reflector round', async () => {
  assert.equal(remainingMs(0, 12_000, 30_000), 18_000); assert.equal(remainingMs(0, 45_000, 30_000), 0);
  let t = 0; const sleeps = [];
  const clock = {now: () => t, sleep: async ms => { sleeps.push(ms); t += ms; }};
  let ts = 1000n;
  const c = {view: async () => ({timestamp: ts})};
  const cfg = {pollMs: 30_000, pins: PINS, margin: {enabled: true, roundPollMs: 5_000}};
  const st = {};
  assert.equal(await waitForNextCycle(cfg, c, st, 0, clock), 'period');    // first sight only records the round
  t = 0; sleeps.length = 0; ts = 1000n; c.view = async () => { ts += t >= 10_000 ? 300n : 0n; return {timestamp: ts}; };
  assert.equal(await waitForNextCycle(cfg, c, st, 0, clock), 'new round'); assert.equal(t, 10_000);
  t = 4_000; sleeps.length = 0;   // cycle took 4 s: margin off sleeps only the 26 s remainder, in one sleep
  assert.equal(await waitForNextCycle({...cfg, margin: {enabled: false}}, c, st, 0, clock), 'period'); assert.deepEqual(sleeps, [26_000]);
});

test('SDK on protocol 30: latest ledger is raw JSON (header never decoded), closeTime read when present', async () => {
  const f = globalThis.fetch;
  globalThis.fetch = async (url, init) => ({json: async () => ({jsonrpc: '2.0', id: 1, result: JSON.parse(init.body).method === 'getLatestLedger'
    ? {id: 'x', protocolVersion: 30, sequence: 65662216, closeTime: '1795000000', headerXdr: 'not-xdr', metadataXdr: 'not-xdr'} : {}})});
  try {
    const c = makeChain({rpcUrl: 'http://127.0.0.1:1', network: PINS.network, publicKey: KEEPER});
    assert.deepEqual(await c.latestLedger(), {sequence: 65662216, closeTime: 1_795_000_000});
    assert.deepEqual(await c.rpc.getLatestLedger(), {sequence: 65662216, closeTime: 1_795_000_000});
  } finally { globalThis.fetch = f; }
});
