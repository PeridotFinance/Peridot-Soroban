// Alert check (operator, on the Droplet; the ceremony gate needs all eight types delivered to Telegram):
//   node src/margin-alert-check.mjs [type,type,...]
// Types: liquidatableOver10min, pooledHealthLow, impaired, inventoryLow, opensRefusedBadDebt, keeperDown, conversionFailed,
// dustLiquidation. It NEVER signs or sends a transaction: DRY_RUN and MARGIN_DRY_RUN are forced on, sign/submit/journal
// throw, and the state file is read but never written.
// 1. margin.enabled: one real read-only margin cycle against the deployed contract (real alerts, if any, are delivered too).
// 2. Each requested type: one synthetic alert line (same messages as the live code, tagged alertTest:true, key test:<type>)
//    through the SAME path as live alerts: log('alert') -> notifier -> Telegram sendMessage, dedupe bypassed.
// Exit 0 only when alerts.channel is "telegram" and every requested type was delivered (Telegram returned ok:true).
import {loadConfig} from './config.mjs';
import {makeChain} from './chain.mjs';
import {loadState, log, inventoryOf, setNotifier} from './keeper.mjs';
import {marginCycle, DUST_MSG} from './margin.mjs';
import {marginAlerts} from './margin-plan.mjs';
import {CONVERSION_FAILED_MSG} from './convert.mjs';
import {KEEPER_DOWN_MSG, keeperDownAlert} from './keeper-watch.mjs';
import {makeNotifier, isMain} from './notify.mjs';

export const ALERT_TYPES = ['liquidatableOver10min', 'pooledHealthLow', 'impaired', 'inventoryLow', 'opensRefusedBadDebt', 'keeperDown', 'conversionFailed', 'dustLiquidation'];

/** One synthetic alert per type: {type, msg, extra}. Margin-state types come from the live evaluator (marginAlerts). */
export function syntheticAlerts(nowMs = Date.now()) {
  const snap = {
    positions: [{id: 'TEST-L', side: 'long', worstPay: 9_617_192_890n}, {id: 'TEST-S', side: 'short', worstPay: 69_027_881_770n}],
    liqSince: {'TEST-L': {firstLedger: 1, firstSeenMs: nowMs - 11 * 60_000, lastLedger: 133, observations: 23}},
    health: {hf: 1_200_000n, hfDown: 1_050_000n}, headroom: {hf_target: 1_250_000n},
    impaired: true, bsShares: {USDC: 29_005_989n, XLM: 0n}, inventory: {USDC: 0n, XLM: 0n}, unavailable: [],
  };
  const out = [];
  for (const {msg, ...extra} of marginAlerts(snap, {liquidatableAlertMs: 600_000, hfDownMin: 1_100_000n}, nowMs)) if (!out.some(o => o.type === extra.type)) out.push({type: extra.type, msg, extra});
  out.push({type: 'keeperDown', msg: KEEPER_DOWN_MSG, extra: keeperDownAlert({status: 'down', lastPing: new Date(nowMs - 7 * 60_000).toISOString()}, nowMs)});
  out.push({type: 'conversionFailed', msg: CONVERSION_FAILED_MSG, extra: {type: 'conversionFailed', lotId: 'TEST-1-65662400', seized: 'XLM', target: 'USDC', stage: 'swap', underlying: '9970749395', attempts: 3, heldMinutes: 16, err: 'quote below the oracle floor'}});
  out.push({type: 'dustLiquidation', msg: DUST_MSG, extra: {type: 'dustLiquidation', id: 'TEST-D', side: 'long', pay: '5000000', lossUsd7: '1500000', profitUsd7: '-1500000'}});
  return out;
}

async function main() {
  const cfg = loadConfig({...process.env, DRY_RUN: 'true', MARGIN_DRY_RUN: 'true'});
  const want = process.argv[2] ? process.argv[2].split(',') : ALERT_TYPES;
  for (const t of want) if (!ALERT_TYPES.includes(t)) { log('warn', 'unknown alert type', {type: t, known: ALERT_TYPES}); process.exit(2); }
  const {notifier, problem} = makeNotifier(cfg, {warn: (msg, extra) => log('warn', msg, extra)});
  if (!notifier) { log('warn', 'alert check: Telegram not configured', {problem, channel: cfg.alerts.channel}); process.exit(2); }
  notifier.force = true;   // bypass dedupe for the test
  setNotifier(notifier);
  const c = makeChain(cfg);
  c.sign = () => { throw Error('alert check never signs'); };
  c.submit = async () => { throw Error('alert check never submits'); };
  log('info', 'margin alert check start', {contract: cfg.margin.contract, enabled: cfg.margin.enabled, types: want, channel: cfg.alerts.channel});
  if (cfg.margin.enabled) {
    const st = loadState(cfg.stateFile);   // in memory only, never saved
    if (st.margin) st.margin = {...st.margin, lastCycleLedger: null, alertLast: {}};
    try {
      const r = await marginCycle(cfg, c, st, {log, journal: () => { throw Error('alert check never journals'); }, inventoryOf});
      log('info', 'margin alert check: live read-only cycle ok', {ledger: r.ledger, positions: r.positions, liquidatable: r.liquidatable, alerts: r.alerts, ...r.view});
    } catch (e) { log('warn', 'margin alert check: live read-only cycle failed', {err: e.message}); }
  }
  for (const a of syntheticAlerts().filter(a => want.includes(a.type))) log('alert', a.msg, {...a.extra, type: a.type, key: `test:${a.type}`, alertTest: true});
  const results = await notifier.flush(90_000);
  const delivered = want.filter(t => results.some(r => r.key === `test:${t}` && r.ok));
  const missing = want.filter(t => !delivered.includes(t));
  log(missing.length ? 'warn' : 'info', 'margin alert check done', {channel: 'telegram', delivered, missing, alertTest: true});
  process.exit(missing.length ? 1 : 0);
}

if (isMain(import.meta.url)) await main();
