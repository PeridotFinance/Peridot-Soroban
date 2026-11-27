import {loadPauseConfig} from './config.mjs';
import {makeChain} from './chain.mjs';
import {verifyPins} from './keeper.mjs';
import {loadKeyFile} from './keyfile.mjs';
import {watchOnce, loadPauseState, guardianOf, log} from './pause.mjs';

const cfg = loadPauseConfig();
loadKeyFile(cfg.keyFile, {expectedPublicKey: cfg.publicKey});          // permission + public-key check; value discarded
const c = makeChain(cfg, {log});
await verifyPins(cfg, c);
const guardian = await guardianOf(cfg, c);
if (guardian !== cfg.publicKey) {
  if (!cfg.dryRun) throw Error('refused live mode: on-chain PauseGuardian is not this pause key');
  log('info', 'pause key is not yet PauseGuardian; dry-run only until ceremony step controller.set_pause_guardian', {guardian});
}
log('info', 'start', {network: cfg.network, publicKey: cfg.publicKey, dryRun: cfg.dryRun, pollMs: cfg.pollMs,
  policy: 'pause only on a fresh USDC band breach or EURC/EUR depeg; stale/missing price = alert only', thresholds: {usdcBand1e6: cfg.usdcBand, usdcAlertOnlyBps: cfg.usdcDepegBps, eurcVsEurBps: cfg.eurcVsEurBps, staleAfterSeconds: cfg.staleAfterSeconds, confirmations: cfg.confirmations, clearCycles: cfg.clearCycles}});
let st = loadPauseState(cfg.stateFile), failures = 0;
for (;;) {
  try { const r = await watchOnce(cfg, c, st); st = r.state; failures = 0; log('info', 'heartbeat', {action: r.action, incident: !!st.incident, done: st.done || {}}); }
  catch (e) { failures++; log(failures >= 3 ? 'alert' : 'warn', 'watch cycle failed', {err: e.message, failures}); if (failures >= 10) process.exit(1); }
  if (cfg.runOnce) break;
  await new Promise(res => setTimeout(res, cfg.pollMs));
}
