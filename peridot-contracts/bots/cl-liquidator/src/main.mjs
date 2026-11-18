#!/usr/bin/env node
// cl-liquidator: staged CL-collateral liquidation keeper (L1a/L1b -> L2 -> L3 -> L4 -> L5).
//   node src/main.mjs --config cfg.json [--once | --watch] [--live]   (default: --once dry run, sends nothing)
//   node src/main.mjs --obs observation.json                          (offline: plan a recorded observation)
import fs from 'node:fs';
import {plan} from './plan.mjs';
import {loadConfig} from './config.mjs';
import {makeReader, makeExecutor, assertLiveAllowed, loadSdk} from './chain.mjs';
import {openJournal, unresolvedL3, lastL4Failure, lastL2CacheTs} from './journal.mjs';
import {assessDepeg, fetchReference, httpJson} from './watcher.mjs';

const out = o => console.log(JSON.stringify({t: new Date().toISOString(), ...o}, (_, v) => (typeof v === 'bigint' ? String(v) : v)));
const argv = process.argv.slice(2), has = f => argv.includes(f), arg = f => (argv.includes(f) ? argv[argv.indexOf(f) + 1] : null);

/** Contract call for a planned stage: CLI args (live) and SDK ScVals (simulation). */
export function stageCall(s, cfg, seq) {
  const c = cfg.contracts;
  switch (s.stage) {
    case 'L1a': return {contract: c.receipt, fn: 'checkpoint_nav', args: {}};
    case 'L1b': return {contract: c.receipt, fn: 'checkpoint_fallback', args: {}};
    case 'L2': return {contract: c.controller, fn: 'cache_price', args: {token: c.usdc}};
    case 'APPROVE': return {contract: c.usdc, fn: 'approve', args: {from: cfg.liquidator, spender: c.usdcMarket, amount: s.amount, expiration_ledger: seq + 100_000}};
    case 'L3': return {contract: c.controller, fn: 'liquidate', args: {borrower: s.borrower, repay_market: c.usdcMarket, collateral_market: c.receipt, repay_amount: s.repay, liquidator: cfg.liquidator}};
    case 'L4': return {contract: c.vault, fn: 'checkpoint_rewards', args: {}};
    case 'L5': return {contract: c.receipt, fn: 'sync_seized', args: {id: s.id}};
    default: throw Error(`unknown stage ${s.stage}`);
  }
}
const TYPES = {id: 'u64', expiration_ledger: 'u32', amount: 'i128', repay_amount: 'u128'};
const scArgs = call => Object.entries(call.args).map(([k, v]) =>
  loadSdk().nativeToScVal(TYPES[k] === 'u32' ? Number(v) : TYPES[k] ? BigInt(v) : v, {type: TYPES[k] ?? 'address'}));

async function observe(cfg, r, j, st) {
  const c = cfg.contracts, {now, sequence} = await r.ledgerNow();
  const status = await r.view(c.receipt, 'stage2_status');
  const nav = await r.simulate(c.receipt, 'checkpoint_nav');
  const navSim = nav.ok ? (BigInt(nav.retval) === 0n ? 'blocked' : 'ok') : nav.code === 2105 ? 'oracle_stale' : 'error';
  let fallbackPreview = null;
  if (navSim === 'blocked' && status.chain) { const f = await r.simulate(c.receipt, 'checkpoint_fallback'); fallbackPreview = f.ok ? f.retval : null; if (!f.ok) out({level: 'info', msg: 'fallback not admissible now', err: f.error}); }
  const pc = await r.data(c.controller, ['PriceCache', c.usdc]), k = await r.data(c.controller, ['OracleMaxAgeMultiplier']);
  const borrowers = [];
  for (const user of cfg.borrowers) {
    const snap = await r.view(c.receipt, 'get_liquidation_account_snapshot', [r.addr(user)]);
    borrowers.push({user, units: BigInt(snap[0]), debt: BigInt(await r.view(c.usdcMarket, 'get_user_borrow_balance', [r.addr(user)]))});
  }
  // Pending seizures from the low-water mark; ids below it are known synced.
  const pending = [], next = Number(status.next_pending);
  for (let id = st.lowWater ?? 0; id < next && id < (st.lowWater ?? 0) + 50; id++) {
    const p = await r.view(c.receipt, 'stage2_pending', [r.u64(id)]);
    if (p) pending.push({id, ...p}); else if (!pending.length) st.lowWater = id + 1;
  }
  const recs = j.records(), blocked = unresolvedL3(recs, now, next);
  for (const [user, b] of blocked) if (b.reviewRequired) out({level: 'alert', msg: 'unresolved L3 intent and the seizure queue moved: operator review', user});
  const S = loadSdk(), asset = S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'), r.addr(c.pyusd)]);
  const feed = await r.view(c.oracle, 'lastprice', [asset]).catch(() => null);
  return {now, sequence, status, navSim, navError: nav.ok ? null : nav.error, navPreview: navSim === 'ok' ? nav.retval : null, fallbackPreview,
    priceCache: pc ? {...pc, k: k ?? 2} : null, borrowers, pending, feed,
    cf: await r.view(c.controller, 'get_market_cf', [r.addr(c.receipt)]),
    closeFactor: await r.view(c.controller, 'get_close_factor_scaled'), li: await r.view(c.controller, 'get_liquidation_incentive_scaled'),
    inventory: await r.view(c.usdc, 'balance', [r.addr(cfg.liquidator)]).catch(() => 0n),
    allowance: await r.view(c.usdc, 'allowance', [r.addr(cfg.liquidator), r.addr(c.usdcMarket)]).catch(() => 0n),
    lastClaim: await r.view(c.vault, 'stage_last_claim'), l4FailedAt: lastL4Failure(recs), l2TriedFor: lastL2CacheTs(recs), blockedL3: new Set(blocked.keys())};
}

async function send(cfg, exec, j, s, obs) {
  const call = stageCall(s, cfg, obs.sequence), key = s.stage === 'L3' ? s.borrower : s.stage === 'L5' ? String(s.id) : s.stage;
  const seq = j.intent(s.stage, key, obs.now, {call, nextPending: Number(obs.status.next_pending), cacheTs: obs.priceCache?.timestamp ?? null});
  const res = await exec(call.contract, call.fn, call.args);
  if (res.unknown) { out({level: 'alert', msg: `${s.stage} outcome unknown (CLI timeout/signal): intent left unresolved`, stderr: res.stderr}); return false; }
  j.result(seq, s.stage, key, obs.now, res.ok, {retval: res.stdout === 'false' ? false : res.stdout === 'true' ? true : res.stdout.slice(0, 200), stderr: res.ok ? undefined : res.stderr});
  out({level: res.ok ? 'info' : 'warn', msg: `sent ${s.stage}`, ok: res.ok, stdout: res.stdout, stderr: res.ok ? undefined : res.stderr});
  return res.ok;
}

async function step(cfg, r, exec, j) {
  const st = j.loadState(), obs = await observe(cfg, r, j, st);
  j.saveState(st);
  const p = plan(obs, {submitMargin: cfg.submitMarginSec, repayCap: cfg.repayCapRaw});
  const ref = await fetchReference(cfg.watcher?.reference, httpJson);
  const dp = assessDepeg({feed: obs.feed, now: obs.now, ref}, cfg);
  if (obs.navSim === 'error') p.alerts.push({level: 'alert', msg: 'checkpoint_nav simulation failed', err: obs.navError});
  if (!exec && p.mode === 'blocked') p.alerts.push({level: 'warn', msg: 'dry run: no observation is SENT, so this process does not keep (or clear) the D3 chain'});
  out({msg: 'plan', now: obs.now, mode: p.mode, stages: p.stages, alerts: [...p.alerts, ...dp.alerts], recommend: dp.recommend});
  if (!exec) {
    for (const [i, s] of p.stages.entries()) {
      const call = stageCall(s, cfg, obs.sequence), sim = await r.simulate(call.contract, call.fn, scArgs(call));
      out({msg: 'simulated', stage: s.stage, ...(sim.ok ? {retval: sim.retval, metrics: sim.metrics} : {error: sim.error, note: i > 0 ? 'planned, not simulable until the earlier stages are sent' : undefined})});
    }
    return {sent: false, waitSec: p.waitSec};
  }
  if (!p.stages.length) return {sent: false, waitSec: p.waitSec};
  return {sent: await send(cfg, exec, j, p.stages[0], obs), waitSec: p.waitSec};
}

async function main() {
  if (arg('--obs')) { const o = JSON.parse(fs.readFileSync(arg('--obs'), 'utf8')); out({msg: 'plan', ...plan({...o, blockedL3: new Set(o.blockedL3 ?? [])}, o.opts ?? {})}); return; }
  if (!arg('--config')) throw Error('usage: main.mjs --config <file.json> [--once|--watch] [--live] | --obs <file.json>');
  const cfg = loadConfig(arg('--config')), live = has('--live');
  if (live) { assertLiveAllowed(cfg); if (cfg.repayCapRaw == null) throw Error('--live requires repayCapRaw (also the approval ceiling)'); }
  const r = makeReader(cfg);
  if ((await r.passphrase()) !== cfg.networkPassphrase) throw Error('RPC network passphrase does not match config');
  const exec = live ? makeExecutor(cfg) : null, j = openJournal(cfg.dataDir);
  out({msg: 'start', live, watch: has('--watch'), network: cfg.networkPassphrase});
  if (!has('--watch')) { for (let i = 0; i < (live ? 8 : 1); i++) if (!(await step(cfg, r, exec, j)).sent) break; return; }
  for (;;) {
    let wait = cfg.watchIntervalSec;
    try { const s = await step(cfg, r, exec, j); wait = s.sent ? 1 : Math.min(cfg.watchIntervalSec, Math.max(1, s.waitSec)); } catch (e) { out({level: 'warn', msg: 'cycle failed', err: e.message}); }
    await new Promise(res => setTimeout(res, wait * 1000));
  }
}

if (import.meta.url === `file://${process.argv[1]}`) main().catch(e => { out({level: 'error', msg: e.message}); process.exitCode = 1; });
