// Pause watcher: on a confirmed depeg or oracle-staleness alert, pause NEW BORROWS on the borrowable cash markets with
// the pause-only guardian key (controller pause_borrow_g, 72 h). It can build no other call: redeem, liquidation and
// deposit pauses are deliberately unreachable from this code even though the guardian key could sign them. Unpausing
// is the Ledger's job (research/cash-borrowing-activation-20261117/tools/ceremony.cjs unpause-borrow).
import fs from 'node:fs';
import path from 'node:path';
import * as S from '@stellar/stellar-sdk';
import {depegAlerts, stalenessAlerts, pauseStep} from './plan.mjs';

const log = (level, msg, extra = {}) => console.log(JSON.stringify({t: new Date().toISOString(), level, mode: 'pause-watcher', msg, ...extra}, (_, v) => (typeof v === 'bigint' ? String(v) : v)));
const ALLOWED_FN = 'pause_borrow_g';

export function pauseTargets(pins) {
  const never = new Set(pins.neverTouch || []);
  return Object.entries(pins.markets).filter(([, m]) => m.borrowable && !never.has(m.receipt)).map(([n, m]) => ({market: n, receipt: m.receipt}));
}

/** The only arguments this watcher will ever send: (pauseKey, borrowable receipt, true) to the pinned controller. */
export function pauseBorrowArgs(pins, pauseKey, market) {
  const t = pauseTargets(pins).find(x => x.market === market);
  if (!t) throw Error(`refused: ${market} is not a borrowable, touchable market`);
  return [S.nativeToScVal(pauseKey, {type: 'address'}), S.nativeToScVal(t.receipt, {type: 'address'}), S.xdr.ScVal.scvBool(true)];
}

/** Decode a prepared transaction and refuse anything other than exactly one controller.pause_borrow_g(pauseKey, borrowable, true). */
export function assertPauseTx(tx, pins, pauseKey, auth = []) {
  if (tx.source !== pauseKey) throw Error('refused: source is not the pause key');
  if (tx.operations.length !== 1) throw Error('refused: exactly one operation');
  const env = tx.toEnvelope().v1().tx().operations()[0].body();
  if (env.switch().name !== 'invokeHostFunction') throw Error('refused: not a contract call');
  const ic = env.invokeHostFunctionOp().hostFunction().invokeContract();
  const target = S.Address.fromScAddress(ic.contractAddress()).toString(), fn = ic.functionName().toString();
  const args = ic.args().map(a => S.scValToNative(a));
  if (target !== pins.controller) throw Error('refused: target is not the pinned controller');
  if (fn !== ALLOWED_FN) throw Error(`refused: ${fn} is not ${ALLOWED_FN}`);
  const receipts = new Set(pauseTargets(pins).map(t => t.receipt));
  if (args.length !== 3 || args[0] !== pauseKey || !receipts.has(args[1]) || args[2] !== true) throw Error('refused: unexpected pause arguments');
  for (const a of auth) {
    const root = a.rootInvocation();
    if (root.subInvocations().length) throw Error('refused: nested authorization');
    const f = root.function().contractFn();
    if (f.functionName().toString() !== ALLOWED_FN || S.Address.fromScAddress(f.contractAddress()).toString() !== pins.controller) throw Error('refused: authorization for another call');
  }
  return {target, fn, market: args[1]};
}

export function loadPauseState(file) { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return {}; } }
function savePauseState(file, st) { fs.mkdirSync(path.dirname(file), {recursive: true}); fs.writeFileSync(file + '.tmp', JSON.stringify(st, null, 1)); fs.renameSync(file + '.tmp', file); }
function journal(file, rec) { fs.mkdirSync(path.dirname(file), {recursive: true}); fs.appendFileSync(file, JSON.stringify({t: new Date().toISOString(), ...rec}) + '\n'); }

const other = s => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(s)]);

/** Read the feeds the controller uses (and the fiat EUR reference); returns alerts plus read errors (never alerts). */
export async function observe(cfg, c, nowSec = Math.floor(Date.now() / 1000)) {
  const prices = {}, readErrors = [];
  for (const [n, sym] of Object.entries(cfg.pins.oracle.symbols)) {
    try { prices[n] = await c.view(cfg.pins.oracle.feed, 'lastprice', [other(sym)]); } catch (e) { readErrors.push({asset: n, err: e.message}); }
  }
  let eur = null;
  try { eur = await c.view(cfg.pins.depeg.fiat, 'lastprice', [other('EUR')]); } catch (e) { readErrors.push({asset: 'EUR', err: e.message}); }
  // A stale EUR reference must not trigger an EURC "depeg" pause: drop it (reported, not alerted).
  if (eur && nowSec - Number(eur.timestamp) > cfg.staleAfterSeconds) { readErrors.push({asset: 'EUR', err: 'fiat EUR reference stale; EURC check skipped'}); eur = null; }
  const okPrices = Object.fromEntries(Object.entries(prices));
  const ts = Object.values(prices).filter(Boolean).map(p => BigInt(p.timestamp));
  const roundKey = ts.length ? String(ts.reduce((a, b) => (a > b ? a : b))) : null;
  const alerts = [
    ...depegAlerts({usdc: prices.USDC && BigInt(prices.USDC.price), eurc: prices.EURC && BigInt(prices.EURC.price), eurFiat: eur && BigInt(eur.price)},
      {usdcBps: cfg.usdcDepegBps, eurcBps: cfg.eurcVsEurBps}).map(a => ({kind: 'depeg', ...a})),
    ...stalenessAlerts(okPrices, nowSec, cfg.staleAfterSeconds),
  ];
  return {alerts, readErrors, roundKey};
}

export async function guardianOf(cfg, c) {
  return c.raw(cfg.pins.controller, S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('PauseGuardian')]));
}

/** Pause borrowing on every borrowable market that is not already paused. Dry run: simulate only, never sign or send. */
export async function pauseAll(cfg, c, reason, skip = new Set()) {
  const out = [];
  for (const t of pauseTargets(cfg.pins)) {
    if (skip.has(t.market)) continue;
    try {
      if (await c.view(cfg.pins.controller, 'is_borrow_paused', [S.nativeToScVal(t.receipt, {type: 'address'})])) { out.push({...t, skipped: 'already paused'}); continue; }
      const sim = await c.prepare(cfg.pins.controller, ALLOWED_FN, pauseBorrowArgs(cfg.pins, cfg.publicKey, t.market), {inclusionFee: cfg.inclusionFee, maxFeeStroops: cfg.maxFeeStroops, timeoutSec: 60});
      if (!sim.ok) {
        const expected = /invalid guardian|no guardian/.test(sim.error);
        log(cfg.dryRun && expected ? 'info' : 'alert', cfg.dryRun && expected ? 'DRY_RUN pause simulation: key is not yet PauseGuardian (ceremony step controller.set_pause_guardian pending)' : 'pause simulation failed: use the Ledger kill switch', {market: t.market, err: sim.error});
        out.push({...t, simError: sim.error}); continue;
      }
      assertPauseTx(sim.prepared, cfg.pins, cfg.publicKey, sim.auth);
      if (cfg.dryRun) { log('alert', 'DRY_RUN would pause_borrow_g', {market: t.market, reason, metrics: sim.metrics}); out.push({...t, dryRun: true}); continue; }
      const signed = c.sign(sim.prepared), hash = signed.hash().toString('hex');
      journal(cfg.journalFile, {kind: 'pause_borrow_g', intent: hash, market: t.market, reason});
      const res = await c.submit(signed, 70_000);  // UNKNOWN is never resent: the market counts as done
      journal(cfg.journalFile, {kind: 'pause_borrow_g', result: res.status, hash, ledger: res.ledger});
      log('alert', 'PAUSED new borrows for 72 h (Ledger unpauses)', {market: t.market, reason, ...res});
      out.push({...t, ...res});
    } catch (e) { log('alert', 'pause failed: use the Ledger kill switch', {market: t.market, err: e.message}); out.push({...t, error: e.message}); }
  }
  return out;
}

// A market is done for this incident once a pause was submitted (SUCCESS, or UNKNOWN: never resent) or it is already
// paused. Anything else (RPC/simulation failure, FAILED) is retried on the next alerting cycle. Dry-run marks markets done
// only in dry-run state; state from the other mode is discarded at start, so a dry-run incident never suppresses live pauses.
export async function watchOnce(cfg, c, st, nowSec) {
  const mode = cfg.dryRun ? 'dry-run' : 'live';
  if (st.mode !== mode) st = {mode};
  const {alerts, readErrors, roundKey} = await observe(cfg, c, nowSec);
  for (const e of readErrors) log('warn', 'feed read problem (not treated as staleness)', e);
  const targets = pauseTargets(cfg.pins).map(t => t.market), done = {...(st.done || {})};
  let {state, action} = pauseStep(st, alerts, {confirmations: cfg.confirmations, clearCycles: cfg.clearCycles}, {roundKey, allDone: targets.every(m => done[m])});
  state.mode = mode;
  if (alerts.length) log('alert', `watcher ${action}`, {alerts, streak: state.streak});
  let paused = [];
  if (action === 'pause') {
    paused = await pauseAll(cfg, c, alerts, new Set(targets.filter(m => done[m])));
    for (const r of paused) if (r.skipped === 'already paused' || r.dryRun || ['SUCCESS', 'UNKNOWN', 'PENDING'].includes(r.status)) done[r.market] = true;
    state.done = done;
  }
  if (action === 'rearmed') log('info', 'watcher re-armed after clean cycles');
  savePauseState(cfg.stateFile, state);
  return {state, action, alerts, paused};
}

export {log};
