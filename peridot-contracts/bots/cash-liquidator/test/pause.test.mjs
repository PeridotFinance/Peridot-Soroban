import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import * as S from '@stellar/stellar-sdk';
import {pauseStep, stalenessAlerts, pauseSignals, pauseRoundKey} from '../src/plan.mjs';
import {pauseTargets, pauseBorrowArgs, assertPauseTx, watchOnce} from '../src/pause.mjs';
import {verifyPins} from '../src/keeper.mjs';
import {loadPauseConfig} from '../src/config.mjs';
import {createKeyFile} from '../src/keyfile.mjs';
import {makeChain} from '../src/chain.mjs';

const pins = JSON.parse(fs.readFileSync(new URL('../config/mainnet.json', import.meta.url), 'utf8'));
const P14 = 10n ** 14n;
const quiet = fn => async () => { const l = console.log; const lines = []; console.log = (...a) => lines.push(a.join(' ')); try { await fn(lines); } finally { console.log = l; } };
// Throwaway test key, generated per run in a private temp dir.
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cashliq-pause-')); fs.chmodSync(dir, 0o700);
const keyFile = path.join(dir, 'pause.secret'), PAUSE = createKeyFile(keyFile), SECRET = fs.readFileSync(keyFile, 'utf8').trim();
test.after(() => fs.rmSync(dir, {recursive: true}));

const txOf = (source, contract, fn, args) => new S.TransactionBuilder(new S.Account(source, '1'), {fee: '100', networkPassphrase: S.Networks.PUBLIC})
  .addOperation(new S.Contract(contract).call(fn, ...args)).setTimeout(0).build();
const A = a => S.nativeToScVal(a, {type: 'address'});

test('targets: only USDC, XLM, EURC; never PYUSD or the CL staged receipt', () => {
  assert.deepEqual(pauseTargets(pins).map(t => t.market), ['USDC', 'XLM', 'EURC']);
  assert.throws(() => pauseBorrowArgs(pins, PAUSE, 'PYUSD'), /not a borrowable/);
  const withCl = {...pins, markets: {...pins.markets, CL: {receipt: pins.neverTouch[0], borrowable: true}}};
  assert.throws(() => pauseBorrowArgs(withCl, PAUSE, 'CL'), /not a borrowable/);
});

test('restriction: only controller.pause_borrow_g(pauseKey, borrowable, true) passes; redeem/liquidation/deposit pauses are refused', () => {
  const R = pins.markets.USDC.receipt;
  assert.equal(assertPauseTx(txOf(PAUSE, pins.controller, 'pause_borrow_g', pauseBorrowArgs(pins, PAUSE, 'USDC')), pins, PAUSE).fn, 'pause_borrow_g');
  for (const fn of ['pause_redeem_g', 'pause_liquidation_g', 'pause_deposit_g'])
    assert.throws(() => assertPauseTx(txOf(PAUSE, pins.controller, fn, [A(PAUSE), A(R), S.xdr.ScVal.scvBool(true)]), pins, PAUSE), /is not pause_borrow_g/);
  assert.throws(() => assertPauseTx(txOf(PAUSE, pins.controller, 'pause_borrow_g', [A(PAUSE), A(R), S.xdr.ScVal.scvBool(false)]), pins, PAUSE), /unexpected/);
  assert.throws(() => assertPauseTx(txOf(PAUSE, pins.controller, 'pause_borrow_g', [A(PAUSE), A(pins.markets.PYUSD.receipt), S.xdr.ScVal.scvBool(true)]), pins, PAUSE), /unexpected/);
  assert.throws(() => assertPauseTx(txOf(PAUSE, pins.controller, 'pause_borrow_g', [A(PAUSE), A(pins.neverTouch[0]), S.xdr.ScVal.scvBool(true)]), pins, PAUSE), /unexpected/);
  assert.throws(() => assertPauseTx(txOf(PAUSE, R, 'pause_borrow_g', pauseBorrowArgs(pins, PAUSE, 'USDC')), pins, PAUSE), /pinned controller/);
  assert.throws(() => assertPauseTx(txOf(S.Keypair.random().publicKey(), pins.controller, 'pause_borrow_g', pauseBorrowArgs(pins, PAUSE, 'USDC')), pins, PAUSE), /source/);
  assert.throws(() => assertPauseTx(txOf(PAUSE, pins.controller, 'set_pause_guardian', [A(PAUSE)]), pins, PAUSE), /not pause_borrow_g/);
});

test('staleness: old or missing price alerts; fresh does not', () => {
  assert.deepEqual(stalenessAlerts({USDC: {timestamp: 1000n}}, 1500, 900), []);
  assert.equal(stalenessAlerts({USDC: {timestamp: 1000n}}, 2000, 900)[0].ageSeconds, 1000);
  assert.equal(stalenessAlerts({XLM: null}, 2000, 900)[0].reason, 'no price');
});

test('state machine: depeg counts once per feed round; pause until all done, then latched; re-arm; stale never pauses', () => {
  const o = {confirmations: 2, clearCycles: 3}, dep = [{kind: 'depeg', asset: 'USDC'}], stale = [{kind: 'stale', asset: 'XLM'}];
  let s = {}, r;
  r = pauseStep(s, dep, o, {roundKey: '100'}); assert.equal(r.action, 'confirming'); s = r.state;
  r = pauseStep(s, dep, o, {roundKey: '100'}); assert.equal(r.action, 'confirming'); s = r.state;   // same round: one observation
  r = pauseStep(s, dep, o, {roundKey: '400'}); assert.equal(r.action, 'pause'); s = r.state;
  r = pauseStep(s, dep, o, {roundKey: '400', allDone: false}); assert.equal(r.action, 'pause'); s = r.state;   // retry undone markets
  r = pauseStep(s, dep, o, {roundKey: '400', allDone: true}); assert.equal(r.action, 'latched'); s = r.state;  // no second pause
  for (let i = 0; i < 2; i++) { r = pauseStep(s, [], o); assert.equal(r.action, 'none'); s = r.state; }
  r = pauseStep(s, dep, o, {roundKey: '700', allDone: true}); assert.equal(r.action, 'latched'); s = r.state;   // clean streak broken
  for (let i = 0; i < 3; i++) { r = pauseStep(s, [], o); s = r.state; }
  assert.equal(r.action, 'rearmed'); assert.equal(s.incident, false); assert.deepEqual(s.done, {});
  // Policy 2026-11-21: stale/missing prices and alert-only notices never open an incident, however long they last.
  let t = {};
  for (let i = 0; i < 50; i++) { r = pauseStep(t, [...stale, {kind: 'usdc-deviation', asset: 'USDC'}], o, {roundKey: String(i)}); assert.equal(r.action, 'none'); t = r.state; }
  assert.equal(t.incident, false);
});

test('config: refuses the admin key, the liquidator key, liquidator credentials and a relative key path', () => {
  const env = {PAUSE_PUBLIC_KEY: PAUSE, PAUSE_KEY_FILE: keyFile};
  assert.equal(loadPauseConfig(env).dryRun, true);                           // dry run by default
  assert.throws(() => loadPauseConfig({...env, PAUSE_PUBLIC_KEY: pins.admin}), /admin/);
  assert.throws(() => loadPauseConfig({...env, LIQUIDATOR_PUBLIC_KEY: PAUSE}), /liquidator key/);
  assert.throws(() => loadPauseConfig({...env, LIQUIDATOR_SECRET: 'x'}), /liquidator credentials/);
  assert.throws(() => loadPauseConfig({...env, LIQUIDATOR_KEY_FILE: '/x'}), /liquidator credentials/);
  assert.throws(() => loadPauseConfig({...env, PAUSE_KEY_FILE: 'pause.secret'}), /absolute/);
});

// Fake chain for the watcher; prepare builds the real op the watcher asked for.
function fakeChain(cfg, {usdc = P14, ts, eurTs, guardianOk = true, paused = false, failFirst = 0, band} = {}) {
  let failures = failFirst;
  const calls = [], c = makeChain({...cfg, rpcUrl: 'http://127.0.0.1:9'});   // real sign() (key file); network never called
  const now = ts ?? BigInt(Math.floor(Date.now() / 1000));
  return Object.assign(c, {
    calls,
    async view(contract, method, args) {
      calls.push(['view', method]);
      if (method === 'lastprice') { const s = S.scValToNative(args[0])[1]; return {price: s === 'USDC' ? usdc : s === 'XLM' ? P14 / 7n : s === 'EUR' ? P14 : 11n * P14 / 10n, timestamp: s === 'EUR' && eurTs != null ? eurTs : now}; }
      if (method === 'is_borrow_paused') return paused;
      if (method === 'get_price_band') { if (band === undefined) throw Error('get_price_band: non-existent function'); return band; }
      throw Error('unexpected view ' + method);
    },
    async prepare(contract, method, args) {
      calls.push(['prepare', method, S.scValToNative(args[1])]);
      if (!guardianOk) return {ok: false, error: 'HostError: invalid guardian'};
      if (failures > 0) { failures--; return {ok: false, error: 'rpc timeout'}; }
      return {ok: true, prepared: txOf(cfg.publicKey, contract, method, args), auth: [], metrics: {cpu: 3_000_000}};
    },
    async submit(tx) { calls.push(['submit', tx.signatures.length]); return {hash: tx.hash().toString('hex'), status: 'SUCCESS', ledger: 1}; },
  });
}
const wcfg = dryRun => ({...loadPauseConfig({PAUSE_PUBLIC_KEY: PAUSE, PAUSE_KEY_FILE: keyFile, PAUSE_STATE_FILE: path.join(dir, `st-${Math.random()}.json`), PAUSE_JOURNAL_FILE: path.join(dir, 'j.jsonl')}), dryRun});

test('watcher live: confirmed USDC depeg pauses borrow on the three markets with one signature each, then latches', quiet(async lines => {
  const cfg = wcfg(false), now = Math.floor(Date.now() / 1000);
  let c = fakeChain(cfg, {usdc: 97n * P14 / 100n, ts: BigInt(now - 300)});
  let r = await watchOnce(cfg, c, {}, now);
  assert.equal(r.action, 'confirming'); assert.equal(c.calls.filter(x => x[0] === 'prepare').length, 0);
  r = await watchOnce(cfg, c, r.state, now); assert.equal(r.action, 'confirming');            // same round: not a 2nd observation
  const calls = c.calls; c = fakeChain(cfg, {usdc: 97n * P14 / 100n, ts: BigInt(now)}); c.calls.push(...calls);
  r = await watchOnce(cfg, c, r.state, now);
  assert.equal(r.action, 'pause');
  assert.deepEqual(c.calls.filter(x => x[0] === 'prepare').map(x => [x[1], x[2]]), pauseTargets(pins).map(t => ['pause_borrow_g', t.receipt]));
  assert.deepEqual(c.calls.filter(x => x[0] === 'submit').map(x => x[1]), [1, 1, 1]);
  r = await watchOnce(cfg, c, r.state, now); assert.equal(r.action, 'latched');
  assert.equal(c.calls.filter(x => x[0] === 'submit').length, 3);
  assert(!lines.join('\n').includes(SECRET), 'secret must never be logged');
  assert(!fs.readFileSync(cfg.journalFile, 'utf8').includes(SECRET));
}));

test('watcher: a stale feed alone never pauses, however long; it is logged as an alert only', quiet(async lines => {
  for (const dryRun of [true, false]) {
    const cfg = wcfg(dryRun), now = Math.floor(Date.now() / 1000);
    // Every feed (USDC frozen OUT of band at 0.90, XLM, EURC, EUR) is 2000 s old: alert only, no simulation, no signature.
    const c = fakeChain(cfg, {usdc: 9n * P14 / 10n, ts: BigInt(now - 2000), eurTs: BigInt(now - 2000)});
    let r = {state: {}};
    for (let i = 0; i < 40; i++) { r = await watchOnce(cfg, c, r.state, now + i * 30); assert.equal(r.action, 'none'); assert.equal(r.alerts.length, 0); }
    assert.equal(c.calls.filter(x => x[0] === 'prepare' || x[0] === 'submit').length, 0);
    assert(r.notices.some(n => n.kind === 'stale' && n.asset === 'USDC'));
  }
  assert(lines.some(l => /ALERT ONLY \(never pauses\): stale/.test(l)));
}));

test('watcher dry run: confirmed USDC band breach simulates only; no signature, no submit; invalid guardian is expected', quiet(async lines => {
  const cfg = wcfg(true), now = Math.floor(Date.now() / 1000);
  let r = await watchOnce(cfg, fakeChain(cfg, {usdc: 97n * P14 / 100n, ts: BigInt(now - 300), guardianOk: false}), {}, now);
  const c = fakeChain(cfg, {usdc: 97n * P14 / 100n, ts: BigInt(now), guardianOk: false});
  r = await watchOnce(cfg, c, r.state, now);
  assert.equal(r.action, 'pause');
  assert.equal(c.calls.filter(x => x[0] === 'submit').length, 0);
  assert(r.paused.every(p => /invalid guardian/.test(p.simError)));
  assert(lines.some(l => /not yet PauseGuardian/.test(l)));
  let r2 = await watchOnce(cfg, fakeChain(cfg, {usdc: 9n * P14 / 10n, ts: BigInt(now - 300)}), {}, now);
  const c2 = fakeChain(cfg, {usdc: 9n * P14 / 10n, ts: BigInt(now)});
  r2 = await watchOnce(cfg, c2, r2.state, now);
  assert(r2.paused.every(p => p.dryRun)); assert.equal(c2.calls.filter(x => x[0] === 'submit').length, 0);
  // Switching the same state file to live discards the dry-run incident: live starts confirming from scratch.
  const live = await watchOnce({...cfg, dryRun: false}, fakeChain(cfg, {usdc: 9n * P14 / 10n, ts: BigInt(now)}), r2.state, now);
  assert.equal(live.action, 'confirming'); assert.equal(live.state.mode, 'live');
  assert(!lines.join('\n').includes(SECRET));
}));

test('band: inclusive like the contract (0.98 and 1.02 do not pause; one raw unit outside does); 0.99 is alert only', () => {
  const now = 10_000, f = price => ({price, timestamp: BigInt(now - 10)}), band = [980_000n, 1_020_000n], o = {staleAfterSeconds: 900, usdcAlertBps: 50, eurcBps: 100};
  const sig = usdc => pauseSignals({prices: {USDC: f(usdc), XLM: f(P14 / 7n), EURC: null}, eurFiat: null, usdcBand: band}, now, o);
  for (const p of [98n * P14 / 100n, 102n * P14 / 100n, P14]) assert.deepEqual(sig(p).pause, [], String(p));
  for (const p of [98n * P14 / 100n - 1n, 102n * P14 / 100n + 1n, 97995n * P14 / 100000n, 90n * P14 / 100n]) assert.equal(sig(p).pause[0]?.kind, 'band', String(p));
  const alertOnly = sig(99n * P14 / 100n);
  assert.deepEqual(alertOnly.pause, []); assert.equal(alertOnly.notices.find(n => n.kind === 'usdc-deviation').deviationBps, -100);
  assert.deepEqual(sig(9996n * P14 / 10000n).notices.filter(n => n.kind === 'usdc-deviation'), []);   // 4 bps: nothing
  // Stale out-of-band USDC: alert only.
  const stale = pauseSignals({prices: {USDC: {price: 9n * P14 / 10n, timestamp: BigInt(now - 901)}}, eurFiat: null, usdcBand: band}, now, o);
  assert.deepEqual(stale.pause, []); assert.equal(stale.notices[0].kind, 'stale');
});

test('EURC depeg: exact > 100 bps against a fresh fiat EUR reference; stale EURC or EUR never pauses', () => {
  const now = 10_000, f = (price, age = 10) => ({price, timestamp: BigInt(now - age)}), band = [980_000n, 1_020_000n], o = {staleAfterSeconds: 900, eurcBps: 100};
  const eur = 11n * P14 / 10n, sig = (eurc, eurAge = 10, eurcAge = 10) => pauseSignals({prices: {EURC: f(eurc, eurcAge)}, eurFiat: f(eur, eurAge), usdcBand: band}, now, o).pause;
  assert.deepEqual(sig(eur * 101n / 100n), []);                         // exactly 100 bps: no
  assert.equal(sig(eur * 101n / 100n + 1n)[0].kind, 'depeg');           // just over
  assert.equal(sig(eur * 99n / 100n - 1n)[0].asset, 'EURC');            // below
  assert.deepEqual(sig(eur * 90n / 100n, 901), []);                     // stale EUR reference
  assert.deepEqual(sig(eur * 90n / 100n, 10, 901), []);                 // stale EURC
});

test('confirmation key follows the triggering asset: other assets ticking cannot confirm a frozen breach', () => {
  const band = [980_000n, 1_020_000n], o = {staleAfterSeconds: 900};
  const k = (usdcTs, xlmTs) => pauseRoundKey(pauseSignals({prices: {USDC: {price: 9n * P14 / 10n, timestamp: BigInt(usdcTs)}, XLM: {price: P14 / 7n, timestamp: BigInt(xlmTs)}}, usdcBand: band}, 10_000, o).pause);
  assert.equal(k(9_700, 9_700), k(9_700, 10_000));
  assert.notEqual(k(9_700, 9_700), k(10_000, 10_000));
  let s = {}; const st = {confirmations: 2, clearCycles: 3};
  const breach = pauseSignals({prices: {USDC: {price: 9n * P14 / 10n, timestamp: 9_700n}}, usdcBand: band}, 10_000, o).pause;
  for (let i = 0; i < 10; i++) { const r = pauseStep(s, breach, st, {roundKey: k(9_700, 9_000 + i * 60)}); assert.equal(r.action, 'confirming'); s = r.state; }
});

test('watcher uses the on-chain USDC band when set (pinned 0.98-1.02 otherwise)', quiet(async () => {
  const cfg = wcfg(false), now = Math.floor(Date.now() / 1000);
  // 0.985 is inside the pinned band, outside a tighter on-chain band 0.99-1.01.
  const usdc = r => r.alerts.filter(x => x.asset === 'USDC');   // the fake EURC (1.10 vs EUR 1.00) alerts independently
  let r = await watchOnce(cfg, fakeChain(cfg, {usdc: 985n * P14 / 1000n, ts: BigInt(now)}), {}, now);
  assert.equal(usdc(r).length, 0);
  r = await watchOnce(cfg, fakeChain(cfg, {usdc: 985n * P14 / 1000n, ts: BigInt(now), band: [990_000n, 1_010_000n]}), {}, now);
  assert.equal(usdc(r)[0].kind, 'band'); assert.deepEqual(usdc(r)[0].band, ['990000', '1010000']);
  r = await watchOnce(cfg, fakeChain(cfg, {usdc: 985n * P14 / 1000n, ts: BigInt(now), band: null}), {}, now);   // band unset on chain
  assert.equal(usdc(r).length, 0);
}));

test('pins: each instance may run its pinned WASM or exactly the reviewed upgrade target, nothing else', async () => {
  const cfg = {network: pins.network, pins};
  const chain = hashes => ({rpc: {getNetwork: async () => ({passphrase: pins.network})}, codeHash: async id => hashes[id] ?? pins.marketWasm});
  await verifyPins(cfg, chain({[pins.controller]: pins.controllerWasm}));
  await verifyPins(cfg, chain({[pins.controller]: pins.upgradeWasm.controller, [pins.markets.USDC.receipt]: pins.upgradeWasm.market}));
  await assert.rejects(verifyPins(cfg, chain({[pins.controller]: pins.upgradeWasm.market})), /controller code changed/);
  await assert.rejects(verifyPins(cfg, chain({[pins.controller]: pins.controllerWasm, [pins.markets.XLM.receipt]: 'ff'.repeat(32)})), /XLM code changed/);
});

test('watcher: already-paused markets are not re-paused (would extend the 72 h)', quiet(async () => {
  const cfg = wcfg(false), now = Math.floor(Date.now() / 1000), c = fakeChain(cfg, {usdc: 9n * P14 / 10n, paused: true});
  let r = await watchOnce(cfg, fakeChain(cfg, {usdc: 9n * P14 / 10n, ts: BigInt(now - 300)}), {}, now); r = await watchOnce(cfg, c, r.state, now);
  assert(r.paused.every(p => p.skipped === 'already paused')); assert.equal(c.calls.filter(x => x[0] === 'prepare').length, 0);
}));

test('watcher: a failed pause attempt is retried on the next alerting cycle (no latch without a submission)', quiet(async () => {
  const cfg = wcfg(false), now = Math.floor(Date.now() / 1000);
  let r = await watchOnce(cfg, fakeChain(cfg, {usdc: 9n * P14 / 10n, ts: BigInt(now - 300)}), {}, now);
  const c = fakeChain(cfg, {usdc: 9n * P14 / 10n, ts: BigInt(now), failFirst: 2});
  r = await watchOnce(cfg, c, r.state, now);
  assert.equal(r.action, 'pause'); assert.deepEqual(Object.keys(r.state.done), ['EURC']);      // USDC, XLM simulations failed
  r = await watchOnce(cfg, c, r.state, now);
  assert.equal(r.action, 'pause'); assert.deepEqual(r.paused.map(p => p.market), ['USDC', 'XLM']);
  assert.deepEqual(Object.keys(r.state.done).sort(), ['EURC', 'USDC', 'XLM']);
  r = await watchOnce(cfg, c, r.state, now); assert.equal(r.action, 'latched');
  assert.equal(c.calls.filter(x => x[0] === 'submit').length, 3);
}));

test('watcher: a stale fiat EUR reference cannot trigger an EURC depeg pause', quiet(async () => {
  const cfg = wcfg(false), now = Math.floor(Date.now() / 1000);
  const c = fakeChain(cfg, {ts: BigInt(now), eurTs: BigInt(now - 5000)});      // EURC 1.10 vs EUR 1.00 would be a 10% "depeg"
  let r = await watchOnce(cfg, c, {}, now); r = await watchOnce(cfg, c, r.state, now);
  assert.equal(r.alerts.length, 0); assert.equal(r.action, 'none');
  const c2 = fakeChain(cfg, {ts: BigInt(now)});
  r = await watchOnce(cfg, c2, {}, now); assert.equal(r.alerts[0].asset, 'EURC');               // fresh reference: alert
}));
