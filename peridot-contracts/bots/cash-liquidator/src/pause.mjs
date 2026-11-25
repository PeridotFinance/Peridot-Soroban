// Pause watcher: on a confirmed USDC price-band breach or stablecoin depeg (plan.mjs pauseSignals), pause NEW BORROWS on
// the borrowable cash markets with the pause-only guardian key (controller pause_borrow_g, 72 h). A stale or missing price
// is an alert only and never pauses (user rule 2026-11-21). It can build no other call: redeem, liquidation and
// deposit pauses are deliberately unreachable from this code even though the guardian key could sign them. Unpausing
// is the Ledger's job (research/cash-borrowing-activation-20261117/tools/ceremony.cjs unpause-borrow).
import fs from 'node:fs';
import path from 'node:path';
import * as S from '@stellar/stellar-sdk';
import {pauseSignals, pauseRoundKey, pauseStep} from './plan.mjs';

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

/**
 * The USDC band the controller enforces: its on-chain get_price_band (PriceBand, 1e6 scale) when readable and set,
 * otherwise the pinned band (pre-upgrade WASM has no get_price_band). A differing on-chain band is reported and used.
 */
export async function usdcBandOf(cfg, c) {
  const pin = cfg.usdcBand.map(BigInt);
  try {
    const b = await c.view(cfg.pins.controller, 'get_price_band', [S.nativeToScVal(cfg.pins.markets.USDC.underlying, {type: 'address'})]);
    if (Array.isArray(b) && b.length === 2) {
      const band = [BigInt(b[0]), BigInt(b[1])];
      return {band, source: 'chain', differsFromPin: band[0] !== pin[0] || band[1] !== pin[1]};
    }
    return {band: pin, source: 'pin (no on-chain band set)'};
  } catch { return {band: pin, source: 'pin (get_price_band unreadable)'}; }
}

/** Read the feeds the controller uses (and the fiat EUR reference). Returns pause alerts, alert-only notices and read errors. */
export async function observe(cfg, c, nowSec = Math.floor(Date.now() / 1000)) {
  const prices = {}, readErrors = [];
  for (const [n, sym] of Object.entries(cfg.pins.oracle.symbols)) {
    try { prices[n] = await c.view(cfg.pins.oracle.feed, 'lastprice', [other(sym)]); } catch (e) { readErrors.push({asset: n, err: e.message}); }
  }
  let eur;
  try { eur = await c.view(cfg.pins.depeg.fiat, 'lastprice', [other('EUR')]); } catch (e) { readErrors.push({asset: 'EUR', err: e.message}); }
  const band = await usdcBandOf(cfg, c);
  if (band.differsFromPin) readErrors.push({asset: 'USDC band', err: 'on-chain band differs from the pin; using the on-chain band', band: band.band});
  const {pause, notices} = pauseSignals({prices, eurFiat: eur, usdcBand: band.band, decimals: cfg.pins.oracle.decimals ?? 14}, nowSec,
    {staleAfterSeconds: cfg.staleAfterSeconds, usdcAlertBps: cfg.usdcDepegBps, eurcBps: cfg.eurcVsEurBps});
  return {alerts: pause, notices, readErrors, roundKey: pauseRoundKey(pause), band};
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
  const {alerts, notices, readErrors, roundKey} = await observe(cfg, c, nowSec);
  for (const e of readErrors) log('warn', 'feed read problem (not treated as staleness)', e);
  for (const n of notices) log('alert', 'ALERT ONLY (never pauses): ' + n.kind, n);
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
  return {state, action, alerts, notices, paused};
}

export {log};
