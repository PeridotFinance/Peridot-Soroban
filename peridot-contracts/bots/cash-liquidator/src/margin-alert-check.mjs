// Margin alert test (operator, on the Droplet): `node src/margin-alert-check.mjs [liquidatable,pooled_health,impaired,inventory,opens_refused]`
// with the service env file. It NEVER signs or sends: DRY_RUN and MARGIN_DRY_RUN are forced on, sign/submit throw, and
// the state file is read but never written. Step 1 (margin.enabled only): one real read-only margin cycle against the
// deployed contract (pins, discovery, positions, backstop, pooled_health; real alerts if any). Step 2: the synthetic
// snapshot through the same alert path, every line tagged alertTest:true. Exit 0 when every requested alert fired.
import {loadConfig} from './config.mjs';
import {makeChain} from './chain.mjs';
import {loadState, log, journal, inventoryOf} from './keeper.mjs';
import {marginCycle, marginAlertSelfTest} from './margin.mjs';

const cfg = loadConfig({...process.env, DRY_RUN: 'true', MARGIN_DRY_RUN: 'true'});
const c = makeChain(cfg);
c.sign = () => { throw Error('alert test never signs'); };
c.submit = async () => { throw Error('alert test never submits'); };
const names = process.argv[2] ? process.argv[2].split(',') : null;
log('info', 'margin alert test start', {contract: cfg.margin.contract, enabled: cfg.margin.enabled, names});
if (cfg.margin.enabled) {
  const st = loadState(cfg.stateFile);   // in memory only, never saved
  if (st.margin) st.margin = {...st.margin, lastCycleLedger: null, alertLast: {}};   // always run, and fire every real alert once
  try {
    const r = await marginCycle(cfg, c, st, {log, journal: () => { throw Error('alert test never journals'); }, inventoryOf});
    log('info', 'margin alert test: live read-only cycle ok', {ledger: r.ledger, positions: r.positions, liquidatable: r.liquidatable, actions: {liquidations: r.actions?.liquidations.length, settles: r.actions?.settles.length}, alerts: r.alerts});
  } catch (e) { log('alert', 'margin alert test: live read-only cycle failed', {err: e.message}); }
}
const fired = marginAlertSelfTest(log, {names});
const want = names ?? ['liquidatable', 'pooled_health', 'impaired', 'opens_refused', 'inventory'];
const missing = want.filter(n => !fired.some(k => k.startsWith(n)));
log(missing.length ? 'alert' : 'info', 'margin alert test done', {fired, missing, alertTest: true});
process.exit(missing.length ? 1 : 0);
