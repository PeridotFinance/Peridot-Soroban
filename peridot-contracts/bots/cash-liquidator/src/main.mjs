import {loadConfig} from './config.mjs';
import {makeChain} from './chain.mjs';
import {cycle, loadState, verifyPins, verifyRoles, log, ensureAllAllowances} from './keeper.mjs';

const cfg = loadConfig();
const c = makeChain(cfg);
await verifyPins(cfg, c);
await verifyRoles(cfg, c);
log('info', 'start', {network: cfg.network, publicKey: cfg.publicKey, dryRun: cfg.dryRun, pollMs: cfg.pollMs, signer: cfg.keyFile ? 'key file' : cfg.signerCliAlias ? 'cli alias' : 'LIQUIDATOR_SECRET (legacy) or none', refresh: cfg.refresh});
await ensureAllAllowances(cfg, c);
const st = loadState(cfg.stateFile);
// Optional seed of known borrowers (comma-separated G-addresses) so a restart outside RPC event retention loses nobody.
if (process.env.BORROWERS_SEED) st.borrowers = [...new Set([...st.borrowers, ...process.env.BORROWERS_SEED.split(',').map(s => s.trim()).filter(Boolean)])].sort();
let failures = 0;
for (;;) {
  try {
    const r = await cycle(cfg, c, st);
    failures = 0;
    log('info', 'heartbeat', {borrowers: st.borrowers.length, actions: r.length, cursor: st.cursor});
  } catch (e) {
    failures++;
    log(failures >= 3 ? 'alert' : 'warn', 'cycle failed', {err: e.message, failures});
    if (failures >= 10) process.exit(1);   // let the platform restart us; RESTART_COUNT alert fires
  }
  if (cfg.runOnce) break;
  await new Promise(res => setTimeout(res, cfg.pollMs));
}
