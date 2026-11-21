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
import {makeHeartbeat, failoverRpc} from './heartbeat.mjs';
import {execFile} from 'node:child_process';
import {cliEnv, assertIdentityName, MAINNET} from './chain.mjs';
import {isMainnet, makeRpc, startChecks, makeSender, botAllow, makeHeartbeatSigner, makeHeartbeatRpc, openBudget, openGate, acquireRunLock, trustlineStatus, requireTrustlines} from './mainnet.mjs';

/** CLI-held key signs an SDK-built envelope (stdin -> stdout). No key material enters this process. */
export function makeCliSigner(cfg, identity, {execFileImpl = execFile} = {}) {
  assertLiveAllowed(cfg); assertIdentityName(identity);
  return xdr => new Promise((resolve, reject) => {
    assertLiveAllowed(cfg);
    const child = execFileImpl('stellar', ['tx', 'sign', '--sign-with-key', identity, '--rpc-url', cfg.rpcUrl, '--network-passphrase', cfg.networkPassphrase, '--quiet'],
      {env: cliEnv(), timeout: 30_000}, (err, stdout) => (err ? reject(err) : resolve(String(stdout).trim())));
    child.stdin.end(xdr);
  });
}
/** Transaction hash printed by `stellar contract invoke` on stderr (last 64-hex token), or null. */
export function hashFromCli(stderr) {
  // Prefer the CLI's own markers: contract events printed on stderr can contain other 64-hex values (e.g. a wasm hash).
  const t = String(stderr ?? ''), m = /tx\/([0-9a-f]{64})/.exec(t) || /Signing transaction: ([0-9a-f]{64})/.exec(t);
  if (m) return m[1];
  const all = t.match(/\b[0-9a-f]{64}\b/g); return all ? all.at(-1) : null;
}

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
  // Mainnet sender: the hash is journalled (note `of` = intent seq) after signing and BEFORE sending.
  const res = await exec(call.contract, call.fn, call.args, {onSigned: hash => j.note({of: seq, stage: s.stage, key, hash, signed: true})});
  if (res.unknown) { out({level: 'alert', msg: `${s.stage} outcome unknown (timeout/signal/no final status): intent left unresolved`, hash: res.hash ?? null, stderr: res.stderr}); return false; }
  j.result(seq, s.stage, key, obs.now, res.ok, {hash: res.hash ?? hashFromCli(res.stderr), retval: res.stdout === 'false' ? false : res.stdout === 'true' ? true : res.stdout.slice(0, 200), stderr: res.ok ? undefined : res.stderr});
  out({level: res.ok ? 'info' : res.refused ? 'alert' : 'warn', msg: res.refused ? `${s.stage} refused before signing` : `sent ${s.stage}`, ok: res.ok, hash: res.hash ?? hashFromCli(res.stderr), stdout: res.stdout, stderr: res.ok ? undefined : res.stderr});
  return res.ok;
}

async function step(cfg, r, exec, j, hb = null) {
  const st = j.loadState(), obs = await observe(cfg, r, j, st);
  j.saveState(st);
  if (hb) hb.observe(obs.navSim === 'blocked');
  const p = plan(obs, {submitMargin: cfg.submitMarginSec, repayCap: cfg.repayCapRaw});
  // The fixed-timer heartbeat owns D3 chain extension; the plan loop keeps every other L1a (clear, refresh).
  if (hb) p.stages = p.stages.filter(x => !(x.stage === 'L1a' && /^D3 heartbeat/.test(x.reason ?? '')));
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
  if (isMainnet(cfg)) return mainMainnet(cfg, live);
  if (live) { assertLiveAllowed(cfg); if (cfg.repayCapRaw == null) throw Error('--live requires repayCapRaw (also the approval ceiling)'); }
  const r = makeReader(cfg);
  if ((await r.passphrase()) !== cfg.networkPassphrase) throw Error('RPC network passphrase does not match config');
  const exec = live ? makeExecutor(cfg) : null, j = openJournal(cfg.dataDir);
  let hb = null;
  if (live && has('--watch') && cfg.heartbeatIdentity) {
    const S = loadSdk(), rpc = failoverRpc([cfg.rpcUrl, ...(cfg.heartbeatRpcUrls ?? [])].map(u => new S.rpc.Server(u)), out);
    hb = makeHeartbeat({S, rpc, cfg, journal: j, sign: makeCliSigner(cfg, cfg.heartbeatIdentity), out}).start();
  }
  out({msg: 'start', live, watch: has('--watch'), network: cfg.networkPassphrase, heartbeat: hb ? {account: cfg.heartbeatAccount, periodMs: cfg.heartbeatPeriodMs ?? 12000} : null});
  if (!has('--watch')) { for (let i = 0; i < (live ? 8 : 1); i++) if (!(await step(cfg, r, exec, j)).sent) break; return; }
  for (;;) {
    let wait = cfg.watchIntervalSec;
    try { const s = await step(cfg, r, exec, j, hb); wait = s.sent ? 1 : Math.min(cfg.watchIntervalSec, Math.max(1, s.waitSec)); } catch (e) { out({level: 'warn', msg: 'cycle failed', err: e.message}); }
    await new Promise(res => setTimeout(res, wait * 1000));
  }
}

/**
 * Mainnet (HG4). Dry run unless BOTH `--live` and DRY_RUN=false. Start: RPC network, code pins (receipt, vault,
 * controller, USDC market), role separation, single instance per data directory. Live also needs a passed dry-run
 * gate (dryRunGateHours, default 12, of error-free --watch dry-run cycles on the same config and source).
 */
/** Mainnet send mode: live only with BOTH `--live` and DRY_RUN=false (any other DRY_RUN value means dry run). */
export function mainnetLiveMode(liveFlag, env = process.env) {
  const dryEnv = env.DRY_RUN !== 'false';
  if (liveFlag && dryEnv) throw Error('refused: --live on Mainnet also needs DRY_RUN=false in the environment');
  if (!liveFlag && !dryEnv) throw Error('refused: DRY_RUN=false without --live (ambiguous); pass both or neither');
  return liveFlag;
}
async function mainMainnet(cfg, liveFlag, env = process.env) {
  const live = mainnetLiveMode(liveFlag, env);
  const release = acquireRunLock(cfg.dataDir);
  process.on('exit', release);
  const S = loadSdk(), rpc = makeRpc(cfg, {S}), r = makeReader(cfg);
  const checks = await startChecks(cfg, {rpc, view: r.view, S, out});
  const j = openJournal(cfg.dataDir), budget = openBudget(cfg.dataDir, cfg), gate = openGate(cfg.dataDir, cfg);
  let exec = null, hb = null;
  // Both trustlines: refused in live mode, reported in dry run (the soak may start before they exist).
  const trustlines = live ? await requireTrustlines(cfg, rpc, cfg.liquidator, S) : await trustlineStatus(cfg, rpc, cfg.liquidator, S).catch(() => null);
  if (!live && !(trustlines?.USDC && trustlines?.PYUSD)) out({level: 'warn', msg: 'liquidator trustlines incomplete: live mode will refuse to start', trustlines});
  if (live) {
    const armed = gate.requireArmed();
    exec = makeSender(cfg, {rpc, S, account: cfg.liquidator, keyFile: cfg.executorKeyFile, kind: 'liquidator', allow: botAllow, budget});
    // A signer refusal (resource ceiling, fee cap, daily budget) surfaces from heartbeat.mjs as 'heartbeat failed': raise it to an alert.
    const hbOut = o => out(o?.msg === 'heartbeat failed' && /^refused/.test(o.err ?? '') ? {...o, level: 'alert', msg: 'heartbeat refused by the Mainnet signer'} : o);
    if (has('--watch') && cfg.heartbeatAccount) {
      // Failover over heartbeatRpcUrls (each verified Mainnet); the signer re-checks the code pins through it.
      const hbRpc = await makeHeartbeatRpc(cfg, {S, failover: rpcs => failoverRpc(rpcs, hbOut)});
      hb = makeHeartbeat({S, rpc: hbRpc, cfg, journal: j, sign: makeHeartbeatSigner(cfg, {S, budget, rpc: hbRpc}), out: hbOut}).start();
    }
    out({level: 'info', msg: 'dry-run gate passed', since: armed.since, last: armed.last, armedAt: armed.armedAt, cycles: armed.cycles});
  }
  out({msg: 'start', live, watch: has('--watch'), network: MAINNET, signer: live ? 'key file' : 'none (dry run)', trustlines, code: checks.code, pendingUpgrades: checks.pendingUpgrades,
    heartbeat: hb ? {account: cfg.heartbeatAccount, periodMs: cfg.heartbeatPeriodMs ?? 12000, rpcs: cfg.heartbeatRpcUrls.map(u => new URL(u).host)} : null,
    budget: {liquidatorSpentToday: String(budget.spent('liquidator')), heartbeatSpentToday: String(budget.spent('heartbeat'))}, gate: live ? 'armed' : gate.load()});
  if (!has('--watch')) { for (let i = 0; i < (live ? 8 : 1); i++) if (!(await step(cfg, r, exec, j)).sent) break; return; }
  for (;;) {
    let wait = cfg.watchIntervalSec, ok = true;
    try { const s = await step(cfg, r, exec, j, hb); wait = s.sent ? 1 : Math.min(cfg.watchIntervalSec, Math.max(1, s.waitSec)); } catch (e) { ok = false; out({level: 'warn', msg: 'cycle failed', err: e.message}); }
    if (!live) { const g = gate.record(ok); if (!ok || g.cycles % 360 === 1) out({level: ok ? 'info' : 'warn', msg: 'dry-run gate', since: g.since, last: g.last, cycles: g.cycles, resets: g.resets, needHours: cfg.dryRunGateHours}); }
    await new Promise(res => setTimeout(res, wait * 1000));
  }
}

if (import.meta.url === `file://${process.argv[1]}`) main().catch(e => { out({level: 'error', msg: e.message}); process.exitCode = 1; });
