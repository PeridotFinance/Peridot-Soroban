// Telegram alert delivery (Bot API sendMessage). Every `alert`-level log line goes to the owner's chat, deduplicated
// per alert key, with a resolved message once the condition clears. Delivery is asynchronous, bounded (queue, retries,
// timeout) and never blocks or throws into the keeper.
//
// Secrets: ONLY from a chmod-600 file owned by the service user (alerts.telegramFile, default
// /etc/peridot-cash-liquidator/telegram.env, override TELEGRAM_FILE = a path only):
//   TELEGRAM_BOT_TOKEN=123456789:AA...      (a new bot, or the owner's existing Avalanche bot)
//   TELEGRAM_CHAT_ID=123456789
//   HEALTHCHECK_PING_URL=https://hc-ping.com/<uuid>   (optional: external dead-man, pinged every healthy cycle)
// Every message starts with alerts.prefix (default "[Stellar]", override ALERT_PREFIX) so one bot/chat can serve several
// chains. The token, the ping URL and the request URLs are never logged; error text is scrubbed of them first.
import fs from 'node:fs';
import path from 'node:path';

import {fileURLToPath} from 'node:url';

/** True when `metaUrl` is the entry script. Compares real paths: on the Droplet argv[1] goes through the `current` symlink. */
export function isMain(metaUrl) {
  try { return !!process.argv[1] && fs.realpathSync(process.argv[1]) === fs.realpathSync(fileURLToPath(metaUrl)); } catch { return false; }
}

const TOKEN_RE = /^\d{5,}:[A-Za-z0-9_-]{30,}$/;
const CHAT_RE = /^(-?\d{1,20}|@[A-Za-z0-9_]{5,64})$/;
const MAX_TEXT = 3500;

/** Reads a KEY=value secrets file after the file checks (symlink, chmod 600, owner). Never echoes contents. */
export function loadSecretsFile(file, {uid = process.getuid?.()} = {}) {
  if (!file || !path.isAbsolute(file)) throw Error('telegram file must be an absolute path');
  const st = fs.lstatSync(file);
  if (st.isSymbolicLink() || !st.isFile()) throw Error('telegram file must be a regular file (no symlink)');
  if ((st.mode & 0o077) !== 0) throw Error('telegram file must be chmod 600 (no group/other access)');
  if (uid != null && st.uid !== uid) throw Error('telegram file must be owned by the service user');
  const kv = {};
  for (const line of fs.readFileSync(file, 'utf8').split('\n')) {
    const m = /^\s*([A-Z_]+)\s*=\s*(.*?)\s*$/.exec(line);
    if (m && !line.trim().startsWith('#')) kv[m[1]] = m[2].replace(/^["']|["']$/g, '');
  }
  return kv;
}

const PING_RE = /^https:\/\/[A-Za-z0-9.-]+(:\d+)?\/[A-Za-z0-9/_-]{8,}$/;
/** Telegram token + chat id (required) and the optional Healthchecks ping URL, from the chmod-600 file. */
export function loadTelegramFile(file, opts = {}) {
  const kv = loadSecretsFile(file, opts);
  if (!TOKEN_RE.test(kv.TELEGRAM_BOT_TOKEN ?? '')) throw Error('telegram file: TELEGRAM_BOT_TOKEN missing or malformed');
  if (!CHAT_RE.test(kv.TELEGRAM_CHAT_ID ?? '')) throw Error('telegram file: TELEGRAM_CHAT_ID missing or malformed');
  if (kv.HEALTHCHECK_PING_URL && !PING_RE.test(kv.HEALTHCHECK_PING_URL)) throw Error('telegram file: HEALTHCHECK_PING_URL malformed (https URL expected)');
  return {token: kv.TELEGRAM_BOT_TOKEN, chatId: kv.TELEGRAM_CHAT_ID, pingUrl: kv.HEALTHCHECK_PING_URL || null};
}
export {TOKEN_RE, CHAT_RE};

/** Dedupe key: the explicit alert key, else message + the identifying field (market/asset/user/id/backer). */
export function alertKey(msg, extra = {}) {
  if (extra.key) return String(extra.key);
  const id = extra.market ?? extra.asset ?? extra.user ?? extra.id ?? extra.backer ?? '';
  return `${msg}|${id}`;
}

const jsonish = v => JSON.stringify(v, (_, x) => (typeof x === 'bigint' ? String(x) : x));
/** Plain-text message (no parse_mode, so nothing needs escaping). */
export function formatAlert({prefix = '[Stellar]', host, kind, type, msg, key, extra}) {
  const {key: _k, type: _t, t: _time, level: _l, ...rest} = extra ?? {};
  const head = `${prefix ? prefix + ' ' : ''}${kind === 'resolved' ? 'RESOLVED' : 'ALERT'} [${host}] ${type ?? ''}`;
  const body = kind === 'resolved' ? `${msg}\nkey: ${key}` : `${msg}\nkey: ${key}\n${jsonish(rest)}`;
  return `${head.trim()}\n${body}`.slice(0, MAX_TEXT);
}

export class TelegramNotifier {
  /**
   * opts: {token, chatId, host, dedupeMs=600000, resolveQuietMs=1800000, apiBase (tests only), fetchImpl, sleep, now,
   *        warn(msg, extra) for delivery problems (never 'alert', so no loop), maxQueue=50, retries=3, timeoutMs=10000, force}
   */
  constructor(o) {
    Object.assign(this, {dedupeMs: 600_000, resolveQuietMs: 1_800_000, apiBase: 'https://api.telegram.org', fetchImpl: globalThis.fetch,
      sleep: ms => new Promise(r => setTimeout(r, ms)), now: () => Date.now(), warn: () => {}, maxQueue: 50, retries: 3, timeoutMs: 10_000,
      host: 'peridot-cashliq', prefix: '[Stellar]', force: false}, Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)));
    this.active = new Map();   // key -> {sentAt, lastSeen, type, msg}
    this.queue = [];
    this.results = [];
    this.dropped = 0;
    this.running = null;
  }

  scrub(s) { return String(s ?? '').split(this.token).join('***'); }

  /** Called for every log line. Synchronous; delivery happens in the background. */
  notify(level, msg, extra = {}) {
    try {
      if (level === 'alert') return this.alert(msg, extra);
      if (extra.resolved && extra.key) return this.resolve(String(extra.key));
    } catch (e) { this.warn('telegram notify error', {err: this.scrub(e.message)}); }
    return null;
  }

  alert(msg, extra) {
    const key = alertKey(msg, extra), now = this.now(), a = this.active.get(key);
    if (a) a.lastSeen = now;
    if (!this.force && a && now - a.sentAt < this.dedupeMs) return null;   // same key at most once per dedupeMs
    this.active.set(key, {sentAt: now, lastSeen: now, type: extra.type ?? 'cash', msg});
    return this.enqueue({key, type: extra.type ?? 'cash', text: formatAlert({prefix: this.prefix, host: this.host, kind: 'alert', type: extra.type ?? 'cash', msg, key, extra})});
  }

  resolve(key) {
    const a = this.active.get(key);
    if (!a) return null;
    this.active.delete(key);
    return this.enqueue({key, type: a.type, resolved: true, text: formatAlert({prefix: this.prefix, host: this.host, kind: 'resolved', type: a.type, msg: a.msg, key})});
  }

  /** Keys with no recurrence for resolveQuietMs are reported resolved (cash alerts have no explicit clear). */
  tick() {
    const now = this.now();
    for (const [key, a] of [...this.active]) if (now - a.lastSeen >= this.resolveQuietMs) this.resolve(key);
  }

  enqueue(item) {
    if (this.queue.length >= this.maxQueue) { this.queue.shift(); this.dropped++; }
    this.queue.push(item);
    if (!this.running) this.running = this.drain().finally(() => { this.running = null; });
    return item;
  }

  async drain() {
    while (this.queue.length) {
      const item = this.queue.shift();
      const r = await this.send(item.text);
      this.results.push({key: item.key, type: item.type, resolved: !!item.resolved, ...r});
      if (this.results.length > 200) this.results.shift();
      if (!r.ok) this.warn('telegram delivery failed', {key: item.key, type: item.type, attempts: r.attempts, status: r.status, err: r.err});
    }
  }

  /** POST sendMessage with bounded retries (429 retry_after honoured, capped). Never throws. */
  async send(text) {
    let attempts = 0, status = null, err = null;
    const url = `${this.apiBase}/bot${this.token}/sendMessage`;
    for (let i = 0; i < this.retries; i++) {
      attempts++;
      const ac = new AbortController(), timer = setTimeout(() => ac.abort(), this.timeoutMs);
      try {
        const res = await this.fetchImpl(url, {method: 'POST', headers: {'content-type': 'application/json'}, signal: ac.signal,
          body: JSON.stringify({chat_id: this.chatId, text, disable_web_page_preview: true})});
        status = res.status;
        let body = null; try { body = await res.json(); } catch { /* non-JSON */ }
        if (res.ok && body?.ok) return {ok: true, attempts, status, messageId: body.result?.message_id};
        err = this.scrub(body?.description ?? `HTTP ${status}`);
        if (status >= 400 && status < 500 && status !== 429) break;   // bad token / chat: retrying will not help
        const wait = status === 429 ? Math.min(10, Number(body?.parameters?.retry_after ?? 1)) * 1000 : 1000 * 2 ** i;
        if (i + 1 < this.retries) await this.sleep(wait);
      } catch (e) {
        err = this.scrub(e.name === 'AbortError' ? 'timeout' : e.message);
        if (i + 1 < this.retries) await this.sleep(1000 * 2 ** i);
      } finally { clearTimeout(timer); }
    }
    return {ok: false, attempts, status, err};
  }

  /** Waits until the queue is empty (bounded); returns the delivery results so far. */
  async flush(maxMs = 60_000) {
    const end = this.now() + maxMs;
    while ((this.running || this.queue.length) && this.now() < end) await (this.running ?? Promise.resolve());
    return this.results;
  }
}

/**
 * Builds the notifier from config. channel 'telegram' + a valid file -> notifier; otherwise {notifier: null, problem}.
 * Problems are returned (the caller logs them as an alert), never thrown: missing alert delivery must not stop the keeper.
 */
export function makeNotifier(cfg, {warn, uid, fetchImpl} = {}) {
  const a = cfg.alerts ?? {};
  if (a.channel !== 'telegram') return {notifier: null, pinger: null, problem: `alerts.channel is ${a.channel ?? 'unset'}, not telegram`};
  try {
    const {token, chatId, pingUrl} = loadTelegramFile(a.telegramFile, {uid});
    const notifier = new TelegramNotifier({token, chatId, host: a.host ?? 'peridot-cashliq', prefix: a.prefix, dedupeMs: a.dedupeMs, resolveQuietMs: a.resolveQuietMs, warn, fetchImpl});
    return {notifier, pinger: pingUrl ? new HealthPinger({url: pingUrl, warn, fetchImpl}) : null, problem: pingUrl ? null : 'no HEALTHCHECK_PING_URL: external dead-man not pinged'};
  } catch (e) { return {notifier: null, pinger: null, problem: e.message}; }
}

/**
 * External dead-man (Healthchecks.io or compatible): the keeper pings after every healthy cycle and pings <url>/fail
 * after repeated cycle failures. The service's own Telegram integration alerts when pings stop. Fire-and-forget: a ping
 * never blocks or throws; the URL is never logged (errors are scrubbed); repeated ping failures are logged once per 10 min.
 */
export class HealthPinger {
  constructor({url, warn = () => {}, fetchImpl = globalThis.fetch, timeoutMs = 5_000, now = () => Date.now()}) {
    Object.assign(this, {url, warn, fetchImpl, timeoutMs, now, lastWarn: 0, sent: 0, failed: 0, pending: null});
  }
  scrub(s) { return String(s ?? '').split(this.url).join('<ping-url>'); }
  ping(kind = 'ok') {
    const target = kind === 'fail' ? `${this.url}/fail` : this.url;
    const ac = new AbortController(), timer = setTimeout(() => ac.abort(), this.timeoutMs);
    this.pending = (async () => {
      try {
        const r = await this.fetchImpl(target, {method: 'POST', signal: ac.signal, headers: {'content-type': 'text/plain'}, body: kind});
        if (!r.ok) throw Error(`HTTP ${r.status}`);
        this.sent++;
        return true;
      } catch (e) {
        this.failed++;
        if (this.now() - this.lastWarn >= 600_000) { this.lastWarn = this.now(); this.warn('healthcheck ping failed', {kind, err: this.scrub(e.name === 'AbortError' ? 'timeout' : e.message), failed: this.failed}); }
        return false;
      } finally { clearTimeout(timer); }
    })();
    return this.pending;
  }
}
