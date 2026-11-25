import {loadConfig} from './config.mjs';
import {makeChain} from './chain.mjs';
import {fullCycle, loadState, verifyPins, verifyRoles, log, ensureAllAllowances, waitForNextCycle, setNotifier} from './keeper.mjs';
import {makeNotifier} from './notify.mjs';
import {verifyMargin} from './margin.mjs';

const cfg = loadConfig();
const c = makeChain(cfg);
// Alert delivery first, so even a failing start is reported. A missing/invalid Telegram file never stops the keeper.
const {notifier, pinger, problem} = makeNotifier(cfg, {warn: (msg, extra) => log('warn', msg, extra)});
if (notifier) { setNotifier(notifier); log('info', 'alert delivery: telegram', {prefix: cfg.alerts.prefix, dedupeSecs: cfg.alerts.dedupeMs / 1000, healthcheckPing: !!pinger}); }
if (!notifier) log('alert', 'alert delivery NOT configured: alerts stay in the log only', {problem});
else if (!pinger) log('alert', 'external dead-man NOT configured: add HEALTHCHECK_PING_URL to the telegram file', {});
await verifyPins(cfg, c);
await verifyRoles(cfg, c);
log('info', 'start', {network: cfg.network, publicKey: cfg.publicKey, dryRun: cfg.dryRun, pollMs: cfg.pollMs, signer: cfg.keyFile ? 'key file' : cfg.signerCliAlias ? 'cli alias' : 'LIQUIDATOR_SECRET (legacy) or none', refresh: cfg.refresh});
if (cfg.margin.enabled) {
  // A margin pin mismatch never stops the cash pass: the margin cycle re-verifies every time and does nothing until it passes.
  try { const v = await verifyMargin(cfg, c); log('info', 'margin pass enabled', {contract: cfg.margin.contract, wasm: v.wasm, dryRun: cfg.dryRun || cfg.margin.dryRun, cpuStop: cfg.margin.cpuStop, maxFeeStroops: cfg.margin.maxFeeStroops}); }
  catch (e) { log('alert', 'margin pin check failed at start: margin pass inactive until it passes', {err: e.message}); }
} else log('info', 'margin pass disabled', {contract: cfg.margin.contract, neverLiquidateBorrowers: [...cfg.neverLiquidateBorrowers]});
await ensureAllAllowances(cfg, c);
const st = loadState(cfg.stateFile);
// Optional seed of known borrowers (comma-separated G-addresses) so a restart outside RPC event retention loses nobody.
if (process.env.BORROWERS_SEED) st.borrowers = [...new Set([...st.borrowers, ...process.env.BORROWERS_SEED.split(',').map(s => s.trim()).filter(Boolean)])].sort();
let failures = 0;
for (;;) {
  const started = Date.now();
  try {
    const {results: r} = await fullCycle(cfg, c, st);
    failures = 0;
    pinger?.ping('ok');   // external dead-man (Healthchecks): only after a healthy cycle
    log('info', 'heartbeat', {borrowers: st.borrowers.length, actions: r.length, cursor: st.cursor});
  } catch (e) {
    failures++;
    log(failures >= 3 ? 'alert' : 'warn', 'cycle failed', {err: e.message, failures});
    if (failures >= cfg.alerts.pingFailAfter) pinger?.ping('fail');
    if (failures >= 10) { await notifier?.flush(10_000); process.exit(1); }   // let the platform restart us; RESTART_COUNT alert fires
  }
  notifier?.tick();
  if (cfg.runOnce) { await notifier?.flush(15_000); break; }
  // Cycle period <= POLL_INTERVAL_MS (sleep only the remainder); with margin enabled, wake early on a new Reflector round.
  await waitForNextCycle(cfg, c, st, started);
}
