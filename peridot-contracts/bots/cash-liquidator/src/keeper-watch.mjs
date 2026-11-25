// Second, independent keeper-liveness watcher. It runs on the PAUSE HOST (a different machine from the keeper), started
// every minute by peridot-keeper-watch.timer. It reads the keeper's Healthchecks.io check with a READ-ONLY API key
// (least privilege: it cannot ping, pause or edit checks, and needs no Stellar key or keeper access) and sends its own
// Telegram alert when the check is down or paused, plus a resolved message when it is up again. If the Healthchecks API
// is unreachable for 3 runs in a row it alerts that liveness is unknown. Healthchecks' own Telegram integration is the
// first channel; this watcher is the second, so either alone still reaches the owner.
//
// Secrets: ONLY from a chmod-600 file owned by the watcher user (KEEPER_WATCH_FILE, default
// /etc/peridot-keeper-watch/keeper-watch.env):
//   TELEGRAM_BOT_TOKEN=...   TELEGRAM_CHAT_ID=...
//   HEALTHCHECKS_API_KEY=... (read-only key)   HEALTHCHECKS_CHECK=<unique_key or uuid of the keeper check>
// Optional, not secret: HEALTHCHECKS_API (default https://healthchecks.io/api/v3), ALERT_PREFIX (default [Stellar]).
// `node src/keeper-watch.mjs --list` prints the checks visible to the key (name, status, unique_key), never a secret.
// Always exits 0 (the timer keeps running), except --list errors.
import fs from 'node:fs';
import path from 'node:path';
import {loadSecretsFile, TelegramNotifier, formatAlert, isMain, TOKEN_RE, CHAT_RE} from './notify.mjs';

export const KEEPER_DOWN_MSG = 'keeperDown: the cash liquidator stopped pinging its Healthchecks check (process down, hung or crash-looping)';
export const WATCH_BLIND_MSG = 'keeperDown: liveness UNKNOWN, the pause-host watcher cannot read Healthchecks';

export function keeperDownAlert(obs, nowMs) {
  const last = obs?.lastPing ? Date.parse(obs.lastPing) : NaN;
  return {type: 'keeperDown', key: 'keeper_down', status: obs?.status ?? null, lastPing: obs?.lastPing ?? null,
    minutesSincePing: Number.isFinite(last) ? Math.floor((nowMs - last) / 60_000) : null, source: 'pause-host watcher'};
}

/**
 * Pure state machine. obs: {status: 'up'|'grace'|'new'|'started'|'down'|'paused'|'unreachable', lastPing}.
 * state: {down, lastAlertMs, unreachable, blind, lastBlindMs}. Returns {actions: [{kind:'alert'|'resolved', key}], state}.
 */
export function watchStep(state, obs, nowMs, {realertMs = 600_000, blindAfterRuns = 3} = {}) {
  const s = {down: false, lastAlertMs: null, unreachable: 0, blind: false, lastBlindMs: null, ...(state || {})}, actions = [];
  if (obs.status === 'unreachable') {
    s.unreachable += 1;
    if (s.unreachable >= blindAfterRuns && (!s.blind || nowMs - s.lastBlindMs >= realertMs)) { s.blind = true; s.lastBlindMs = nowMs; actions.push({kind: 'alert', key: 'watch_blind'}); }
    return {actions, state: s};
  }
  s.unreachable = 0;
  if (s.blind) { s.blind = false; s.lastBlindMs = null; actions.push({kind: 'resolved', key: 'watch_blind'}); }
  if (obs.status === 'down' || obs.status === 'paused') {
    if (!s.down || nowMs - s.lastAlertMs >= realertMs) { s.down = true; s.lastAlertMs = nowMs; actions.push({kind: 'alert', key: 'keeper_down'}); }
  } else if (s.down) { s.down = false; s.lastAlertMs = null; actions.push({kind: 'resolved', key: 'keeper_down'}); }
  return {actions, state: s};
}

export function loadWatchSecrets(file, opts) {
  const kv = loadSecretsFile(file, opts);
  if (!TOKEN_RE.test(kv.TELEGRAM_BOT_TOKEN ?? '')) throw Error('watch file: TELEGRAM_BOT_TOKEN missing or malformed');
  if (!CHAT_RE.test(kv.TELEGRAM_CHAT_ID ?? '')) throw Error('watch file: TELEGRAM_CHAT_ID missing or malformed');
  if (!/^[A-Za-z0-9_-]{20,}$/.test(kv.HEALTHCHECKS_API_KEY ?? '')) throw Error('watch file: HEALTHCHECKS_API_KEY missing or malformed');
  if (!/^[A-Za-z0-9-]{8,}$/.test(kv.HEALTHCHECKS_CHECK ?? '')) throw Error('watch file: HEALTHCHECKS_CHECK missing or malformed');
  return {token: kv.TELEGRAM_BOT_TOKEN, chatId: kv.TELEGRAM_CHAT_ID, apiKey: kv.HEALTHCHECKS_API_KEY, check: kv.HEALTHCHECKS_CHECK};
}

/** Reads one check. Never throws: any failure is {status:'unreachable'} with a scrubbed reason. */
export async function readCheck({api, apiKey, check, fetchImpl = globalThis.fetch, timeoutMs = 10_000}) {
  const ac = new AbortController(), t = setTimeout(() => ac.abort(), timeoutMs);
  try {
    const r = await fetchImpl(`${api}/checks/${encodeURIComponent(check)}`, {headers: {'X-Api-Key': apiKey}, signal: ac.signal});
    if (!r.ok) return {status: 'unreachable', err: `HTTP ${r.status}`};
    const b = await r.json();
    return {status: String(b.status), lastPing: b.last_ping ?? null, name: b.name ?? null};
  } catch (e) { return {status: 'unreachable', err: String(e.name === 'AbortError' ? 'timeout' : e.message).split(apiKey).join('***')}; }
  finally { clearTimeout(t); }
}

const jlog = (level, msg, extra = {}) => console.log(JSON.stringify({t: new Date().toISOString(), level, msg, ...extra}));

export async function runWatch(env = process.env, {nowMs = Date.now(), fetchImpl = globalThis.fetch, uid, sleep} = {}) {
  const file = env.KEEPER_WATCH_FILE || '/etc/peridot-keeper-watch/keeper-watch.env';
  const stateFile = env.KEEPER_WATCH_STATE || '/var/lib/peridot-keeper-watch/state.json';
  const api = env.HEALTHCHECKS_API || 'https://healthchecks.io/api/v3';
  const prefix = env.ALERT_PREFIX ?? '[Stellar]';
  let sec;
  try { sec = loadWatchSecrets(file, {uid}); } catch (e) { jlog('alert', 'keeper-watch: secrets file unusable; no alerts can be sent', {err: e.message}); return null; }
  const obs = await readCheck({api, apiKey: sec.apiKey, check: sec.check, fetchImpl});
  let st = null; try { st = JSON.parse(fs.readFileSync(stateFile, 'utf8')); } catch { /* first run */ }
  const step = watchStep(st, obs, nowMs);
  const n = new TelegramNotifier({token: sec.token, chatId: sec.chatId, prefix, host: 'peridot-cashliq via pause host', fetchImpl, ...(sleep ? {sleep} : {})});
  for (const a of step.actions) {
    const blind = a.key === 'watch_blind';
    const text = formatAlert({prefix, host: 'peridot-cashliq via pause host', kind: a.kind, type: 'keeperDown', key: a.key,
      msg: blind ? WATCH_BLIND_MSG : KEEPER_DOWN_MSG, extra: blind ? {type: 'keeperDown', unreachableRuns: step.state.unreachable, err: obs.err} : keeperDownAlert(obs, nowMs)});
    const r = await n.send(text);
    jlog(r.ok ? 'info' : 'warn', `keeper-watch ${a.kind} ${a.key} ${r.ok ? 'delivered' : 'NOT delivered'}`, {status: obs.status, attempts: r.attempts, err: r.err});
    if (!r.ok) {   // undo the transition so the next run retries the message
      if (a.kind === 'alert' && a.key === 'keeper_down') step.state.lastAlertMs = -Infinity;
      if (a.kind === 'alert' && blind) step.state.lastBlindMs = -Infinity;
      if (a.kind === 'resolved' && a.key === 'keeper_down') Object.assign(step.state, {down: true, lastAlertMs: nowMs});
      if (a.kind === 'resolved' && blind) Object.assign(step.state, {blind: true, lastBlindMs: nowMs});
    }
  }
  if (!step.actions.length) jlog('info', 'keeper-watch ok', {status: obs.status, lastPing: obs.lastPing, err: obs.err});
  fs.mkdirSync(path.dirname(stateFile), {recursive: true});
  const safe = JSON.stringify(step.state, (_, v) => (v === -Infinity ? 0 : v));
  fs.writeFileSync(stateFile + '.tmp', safe); fs.renameSync(stateFile + '.tmp', stateFile);
  return {obs, step};
}

async function list(env) {
  const sec = loadWatchSecrets(env.KEEPER_WATCH_FILE || '/etc/peridot-keeper-watch/keeper-watch.env');
  const r = await fetch(`${env.HEALTHCHECKS_API || 'https://healthchecks.io/api/v3'}/checks/`, {headers: {'X-Api-Key': sec.apiKey}});
  if (!r.ok) throw Error(`HTTP ${r.status}`);
  for (const c of (await r.json()).checks ?? []) console.log(JSON.stringify({name: c.name, status: c.status, unique_key: c.unique_key ?? null, timeout: c.timeout, grace: c.grace, last_ping: c.last_ping}));
}

if (isMain(import.meta.url)) {
  if (process.argv[2] === '--list') { try { await list(process.env); } catch (e) { jlog('warn', 'keeper-watch --list failed', {err: e.message}); process.exit(1); } process.exit(0); }
  try { await runWatch(); } catch (e) { jlog('warn', 'keeper-watch run failed', {err: e.message}); }
  process.exit(0);
}
