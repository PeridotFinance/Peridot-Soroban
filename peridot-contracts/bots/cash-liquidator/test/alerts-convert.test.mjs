import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {TelegramNotifier, loadTelegramFile, alertKey, makeNotifier, HealthPinger} from '../src/notify.mjs';
import {watchStep, runWatch, keeperDownAlert, readCheck} from '../src/keeper-watch.mjs';
import {oracleMinOut, belowOracleBps, backoffMs, swapTarget, reservedPt, conversionAlertDue} from '../src/convert.mjs';
import {marginCycle} from '../src/margin.mjs';
import {log, setNotifier} from '../src/keeper.mjs';
import {syntheticAlerts, ALERT_TYPES} from '../src/margin-alert-check.mjs';
import {stubChain, marginCfg, quietLog, PINS, FIX} from './support/margin-stub.mjs';

const TOKEN = '123456789:AAFakeTokenForTestsOnly_abcdefghijklmnop';
const CHAT = '987654321';
const M = PINS.markets;
const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'tg-'));

// Mock Telegram Bot API: records requests; `plan` gives per-request responses ([status, body]).
async function mockTelegram(plan = []) {
  const reqs = [];
  const server = http.createServer((req, res) => {
    let b = ''; req.on('data', d => { b += d; }); req.on('end', () => {
      reqs.push({url: req.url, body: JSON.parse(b || '{}')});
      const [status, body] = plan.shift() ?? [200, {ok: true, result: {message_id: reqs.length}}];
      if (status === 'hang') return;   // never answers
      res.writeHead(status, {'content-type': 'application/json'}); res.end(JSON.stringify(body));
    });
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  return {reqs, base: `http://127.0.0.1:${server.address().port}`, close: () => { server.closeAllConnections?.(); server.close(); }};
}
const mk = (base, o = {}) => { const warns = []; let t = o.t ?? 0; const n = new TelegramNotifier({token: TOKEN, chatId: CHAT, apiBase: base, sleep: async () => {}, now: () => t, warn: (m, e) => warns.push({m, ...e}), timeoutMs: 300, ...o}); return {n, warns, setT: v => { t = v; }}; };

test('telegram: sendMessage via the Bot API path with chat id and plain text; dedupe 10 min per key; resolved message', async () => {
  const tg = await mockTelegram();
  try {
    const {n, setT} = mk(tg.base);
    n.notify('alert', 'margin_impaired: x', {key: 'impaired', type: 'impaired', hf: 5n});
    n.notify('alert', 'margin_impaired: x', {key: 'impaired', type: 'impaired'});   // same key, same minute: suppressed
    n.notify('info', 'heartbeat', {});                                              // non-alert lines are not sent
    await n.flush();
    assert.equal(tg.reqs.length, 1);
    assert.equal(tg.reqs[0].url, `/bot${TOKEN}/sendMessage`);
    assert.equal(tg.reqs[0].body.chat_id, CHAT); assert.match(tg.reqs[0].body.text, /^\[Stellar\] ALERT \[peridot-cashliq\] impaired\nmargin_impaired/);
    assert.match(tg.reqs[0].body.text, /"hf":"5"/);
    setT(600_000); n.notify('alert', 'margin_impaired: x', {key: 'impaired', type: 'impaired'}); await n.flush();
    assert.equal(tg.reqs.length, 2);                                                // re-sent after 10 min
    n.notify('info', 'alert resolved', {key: 'impaired', resolved: true}); await n.flush();
    assert.match(tg.reqs[2].body.text, /^\[Stellar\] RESOLVED \[peridot-cashliq\] impaired/);
    n.notify('info', 'alert resolved', {key: 'impaired', resolved: true}); await n.flush();
    assert.equal(tg.reqs.length, 3);                                                // only once, only for keys that were sent
    // cash alerts (no explicit clear) resolve after the quiet period
    n.notify('alert', 'liquidator XLM below fee floor', {xlm: 1n}); await n.flush();
    setT(600_000 + 1_799_999); n.tick(); await n.flush(); assert.equal(tg.reqs.length, 4);
    setT(600_000 + 1_800_000); n.tick(); await n.flush(); assert.match(tg.reqs[4].body.text, /^\[Stellar\] RESOLVED .*cash\nliquidator XLM below fee floor/);
  } finally { tg.close(); }
});

test('telegram: bounded retries (5xx/429 retried, 4xx not), failures never throw or block, token scrubbed from warnings', async () => {
  const tg = await mockTelegram([[500, {ok: false}], [429, {ok: false, parameters: {retry_after: 1}}], [200, {ok: true, result: {message_id: 9}}], [401, {ok: false, description: `Unauthorized ${TOKEN}`}]]);
  try {
    const {n, warns} = mk(tg.base);
    n.notify('alert', 'a', {key: 'a'}); await n.flush();
    assert.equal(tg.reqs.length, 3); assert.equal(n.results[0].ok, true); assert.equal(n.results[0].attempts, 3);
    n.notify('alert', 'b', {key: 'b'}); await n.flush();
    assert.equal(tg.reqs.length, 4); assert.equal(n.results[1].ok, false); assert.equal(n.results[1].attempts, 1);
    assert.equal(warns.length, 1); assert(!JSON.stringify(warns).includes(TOKEN)); assert.match(warns[0].err, /\*\*\*/);
  } finally { tg.close(); }
  const hang = await mockTelegram(Array(5).fill(['hang']));
  try {
    const {n} = mk(hang.base, {retries: 2, timeoutMs: 50});
    const t0 = Date.now(); const r = n.notify('alert', 'c', {key: 'c'}); assert(Date.now() - t0 < 20); assert(r);   // synchronous, non-blocking
    await n.flush(); assert.equal(n.results[0].ok, false); assert.equal(n.results[0].err, 'timeout');
  } finally { hang.close(); }
  const {n: down, warns: w2} = mk('http://127.0.0.1:1', {fetchImpl: async url => { throw Error(`connect ECONNREFUSED ${url}`); }});
  down.notify('alert', 'd', {key: 'd'}); await down.flush();
  assert(!JSON.stringify(w2).includes(TOKEN));
});

test('telegram secret file: chmod 600, owner, no symlink, well-formed; errors never echo the token', () => {
  const d = tmp(), f = path.join(d, 'telegram.env');
  fs.writeFileSync(f, `# owner bot\nTELEGRAM_BOT_TOKEN=${TOKEN}\nTELEGRAM_CHAT_ID=${CHAT}\n`, {mode: 0o600});
  assert.deepEqual(loadTelegramFile(f), {token: TOKEN, chatId: CHAT, pingUrl: null});
  fs.chmodSync(f, 0o640); assert.throws(() => loadTelegramFile(f), /chmod 600/); fs.chmodSync(f, 0o600);
  assert.throws(() => loadTelegramFile(f, {uid: 12345}), /owned by the service user/);
  const l = path.join(d, 'link.env'); fs.symlinkSync(f, l); assert.throws(() => loadTelegramFile(l), /symlink/);
  assert.throws(() => loadTelegramFile('relative.env'), /absolute/);
  const bad = path.join(d, 'bad.env'); fs.writeFileSync(bad, `TELEGRAM_BOT_TOKEN=${TOKEN}x y\n`, {mode: 0o600});
  try { loadTelegramFile(bad); assert.fail('accepted'); } catch (e) { assert(!e.message.includes(TOKEN)); }
  // config wiring: channel telegram + file -> notifier; anything else -> problem, no throw
  assert(makeNotifier({alerts: {channel: 'telegram', telegramFile: f}}).notifier);
  assert.match(makeNotifier({alerts: {channel: 'log'}}).problem, /not telegram/);
  assert.match(makeNotifier({alerts: {channel: 'telegram', telegramFile: path.join(d, 'missing')}}).problem, /ENOENT/);
  assert.equal(PINS.alerts.channel, 'telegram');
  assert.equal(alertKey('m', {key: 'k'}), 'k'); assert.equal(alertKey('m', {market: 'USDC'}), 'm|USDC');
});

test('alert check path: all eight types go log("alert") -> notifier -> Telegram, each delivered', async () => {
  const tg = await mockTelegram();
  const l = console.log; console.log = () => {};
  try {
    const {n} = mk(tg.base); n.force = true;
    setNotifier(n);
    for (const a of syntheticAlerts()) log('alert', a.msg, {...a.extra, type: a.type, key: `test:${a.type}`, alertTest: true});
    const res = await n.flush();
    assert.deepEqual(res.filter(r => r.ok).map(r => r.type).sort(), [...ALERT_TYPES].sort());
    assert.equal(tg.reqs.length, 8);
    for (const t of ALERT_TYPES) assert(tg.reqs.some(r => r.body.text.startsWith(`[Stellar] ALERT [peridot-cashliq] ${t}\n`)), t);
  } finally { setNotifier(null); console.log = l; tg.close(); }
});

test('external dead-man: the keeper pings Healthchecks after healthy cycles (/fail after failures); URL never logged; never blocks', async () => {
  const hits = [];
  const srv = http.createServer((req, res) => { hits.push(req.url); res.writeHead(req.url.includes('broken') ? 500 : 200); res.end('OK'); });
  await new Promise(r => srv.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${srv.address().port}`;
  try {
    const warns = [];
    const p = new HealthPinger({url: `${base}/0f1e2d3c-aaaa-bbbb-cccc-000000000001`, warn: (m, e) => warns.push({m, ...e})});
    const t0 = Date.now(); const pr = p.ping('ok'); assert(Date.now() - t0 < 20);   // returns at once
    assert.equal(await pr, true); assert.equal(await p.ping('fail'), true);
    assert.deepEqual(hits, ['/0f1e2d3c-aaaa-bbbb-cccc-000000000001', '/0f1e2d3c-aaaa-bbbb-cccc-000000000001/fail']);
    const bad = new HealthPinger({url: `${base}/broken-ping-0000000000`, warn: (m, e) => warns.push({m, ...e})});
    assert.equal(await bad.ping(), false); assert.equal(await bad.ping(), false);
    assert.equal(warns.length, 1);                                               // rate-limited
    const off = new HealthPinger({url: 'https://hc-ping.example/secret-uuid-1234567', warn: (m, e) => warns.push({m, ...e}), fetchImpl: async u => { throw Error(`getaddrinfo ENOTFOUND ${u}`); }});
    assert.equal(await off.ping(), false);
    assert(!JSON.stringify(warns).includes('secret-uuid') && !JSON.stringify(warns).includes(base + '/0f1e'));
  } finally { srv.close(); }
  // the secrets file carries the ping URL; the notifier/pinger pair is built from it
  const d = tmp(), f = path.join(d, 'telegram.env');
  fs.writeFileSync(f, `TELEGRAM_BOT_TOKEN=${TOKEN}\nTELEGRAM_CHAT_ID=${CHAT}\nHEALTHCHECK_PING_URL=https://hc-ping.com/0f1e2d3c-aaaa-bbbb-cccc-000000000001\n`, {mode: 0o600});
  const made = makeNotifier({alerts: {channel: 'telegram', telegramFile: f, prefix: '[Stellar]'}});
  assert(made.notifier && made.pinger); assert.equal(made.notifier.prefix, '[Stellar]');
  fs.writeFileSync(f, `TELEGRAM_BOT_TOKEN=${TOKEN}\nTELEGRAM_CHAT_ID=${CHAT}\nHEALTHCHECK_PING_URL=http://insecure/x\n`, {mode: 0o600});
  assert.match(makeNotifier({alerts: {channel: 'telegram', telegramFile: f}}).problem, /PING_URL malformed/);
});

test('prefix: default [Stellar] (owner shares the Avalanche bot/chat), configurable', async () => {
  const tg = await mockTelegram();
  try {
    let {n} = mk(tg.base); n.notify('alert', 'x', {key: 'p1', type: 'impaired'}); await n.flush();
    ({n} = mk(tg.base, {prefix: '[Stellar-test]'})); n.notify('alert', 'x', {key: 'p2', type: 'impaired'}); await n.flush();
    assert.match(tg.reqs[0].body.text, /^\[Stellar\] ALERT \[peridot-cashliq\] impaired\n/);
    assert.match(tg.reqs[1].body.text, /^\[Stellar-test\] ALERT /);
  } finally { tg.close(); }
});

test('pause-host watcher: keeperDown when the Healthchecks check is down/paused (realert 10 min), resolved when up, blind alert after 3 unreachable reads', async () => {
  let r = watchStep(null, {status: 'up'}, 0); assert.deepEqual(r.actions, []);
  r = watchStep(r.state, {status: 'grace'}, 1); assert.deepEqual(r.actions, []);              // Healthchecks grace: not yet
  r = watchStep(r.state, {status: 'down'}, 1_000); assert.deepEqual(r.actions, [{kind: 'alert', key: 'keeper_down'}]);
  r = watchStep(r.state, {status: 'down'}, 600_999); assert.deepEqual(r.actions, []);
  r = watchStep(r.state, {status: 'down'}, 601_000); assert.deepEqual(r.actions, [{kind: 'alert', key: 'keeper_down'}]);
  r = watchStep(r.state, {status: 'up'}, 700_000); assert.deepEqual(r.actions, [{kind: 'resolved', key: 'keeper_down'}]);
  assert.deepEqual(watchStep(null, {status: 'paused'}, 0).actions, [{kind: 'alert', key: 'keeper_down'}]);
  let b = {state: null};
  for (let i = 0; i < 2; i++) { b = watchStep(b.state, {status: 'unreachable'}, i); assert.deepEqual(b.actions, []); }
  b = watchStep(b.state, {status: 'unreachable'}, 3); assert.deepEqual(b.actions, [{kind: 'alert', key: 'watch_blind'}]);
  b = watchStep(b.state, {status: 'up'}, 4); assert.deepEqual(b.actions, [{kind: 'resolved', key: 'watch_blind'}]);
  assert.equal(keeperDownAlert({status: 'down', lastPing: new Date(0).toISOString()}, 420_000).minutesSincePing, 7);

  // end to end: mock Healthchecks API (read-only key header) + mock Telegram, files on disk
  let hcStatus = 'down';
  const seen = [];
  const hc = http.createServer((req, res) => { seen.push({url: req.url, key: req.headers['x-api-key']}); res.writeHead(200, {'content-type': 'application/json'}); res.end(JSON.stringify({name: 'peridot-cashliq', status: hcStatus, last_ping: new Date(Date.now() - 300_000).toISOString()})); });
  await new Promise(x => hc.listen(0, '127.0.0.1', x));
  const tg = await mockTelegram();
  const RO = 'readonlykey_0123456789abcdef';
  try {
    const d = tmp(), f = path.join(d, 'keeper-watch.env');
    fs.writeFileSync(f, `TELEGRAM_BOT_TOKEN=${TOKEN}\nTELEGRAM_CHAT_ID=${CHAT}\nHEALTHCHECKS_API_KEY=${RO}\nHEALTHCHECKS_CHECK=abcd1234efgh5678\n`, {mode: 0o600});
    const tgFetch = (url, init) => fetch(url.replace('https://api.telegram.org', tg.base), init);
    const env = {KEEPER_WATCH_FILE: f, KEEPER_WATCH_STATE: path.join(d, 'state.json'), HEALTHCHECKS_API: `http://127.0.0.1:${hc.address().port}/api/v3`};
    const l = console.log, out = []; console.log = x => out.push(x);
    try {
      await runWatch(env, {fetchImpl: tgFetch, sleep: async () => {}});
      await runWatch(env, {fetchImpl: tgFetch, sleep: async () => {}});   // suppressed (same 10 min)
      hcStatus = 'up';
      await runWatch(env, {fetchImpl: tgFetch, sleep: async () => {}});
    } finally { console.log = l; }
    assert.equal(seen[0].url, '/api/v3/checks/abcd1234efgh5678'); assert.equal(seen[0].key, RO);
    assert.equal(tg.reqs.length, 2);
    assert.match(tg.reqs[0].body.text, /^\[Stellar\] ALERT \[peridot-cashliq via pause host\] keeperDown\n/);
    assert.match(tg.reqs[1].body.text, /^\[Stellar\] RESOLVED /);
    assert(!out.join('\n').includes(TOKEN) && !out.join('\n').includes(RO));
    fs.chmodSync(f, 0o644);
    console.log = x => out.push(x);
    try { assert.equal(await runWatch(env, {fetchImpl: tgFetch}), null); } finally { console.log = l; }
    assert.match(out.at(-1), /secrets file unusable/);
  } finally { hc.close(); tg.close(); }
  assert.equal((await readCheck({api: 'http://127.0.0.1:1', apiKey: RO, check: 'x', fetchImpl: async () => { throw Error(`boom ${RO}`); }})).err, 'boom ***');
  const unit = fs.readFileSync(new URL('../deploy/pausehost/peridot-keeper-watch.service', import.meta.url), 'utf8');
  assert.match(unit, /User=keeperwatch/); assert.match(unit, /InaccessiblePaths=-\/mnt\/pausewatch/); assert.match(unit, /ExecStart=\/usr\/local\/bin\/node src\/keeper-watch\.mjs/);
  assert.match(fs.readFileSync(new URL('../deploy/pausehost/peridot-keeper-watch.timer', import.meta.url), 'utf8'), /OnUnitActiveSec=1min/);
});

test('conversion math: oracle floor at 1.5 %, below-oracle bps, backoff, targets, reserved lots, alert threshold', () => {
  const pX = [12424660835662n, 10n ** 14n], pU = [100049927484869n, 10n ** 14n];
  const minOut = oracleMinOut(10_000_000_000n, pX, pU, 150);   // 1,000 XLM
  const fair = (10_000_000_000n * pX[0] / pX[1]) * pU[1] / pU[0];
  assert(minOut < fair && minOut >= fair * 9850n / 10_000n - 1n);
  assert.equal(belowOracleBps(10_000_000_000n, fair * 9_900n / 10_000n, pX, pU), 100);
  assert.deepEqual([1, 2, 3, 4, 9].map(a => backoffMs(a, 60_000, 900_000)), [60_000, 120_000, 240_000, 480_000, 900_000]);
  assert.equal(swapTarget('XLM', {}), 'USDC');
  assert.equal(swapTarget('USDC', {XLM: 10n}, {XLM: '25000000000'}), 'XLM');
  assert.equal(swapTarget('USDC', {XLM: 25_000_000_000n}, {XLM: '25000000000'}), null);
  const st = {convert: {lots: [{stage: 'redeem', receipt: 'R', ptokens: '5'}, {stage: 'swap', receipt: 'R', ptokens: '0'}, {stage: 'redeem', receipt: 'Q', ptokens: '7'}]}};
  assert.equal(reservedPt(st, 'R'), 5n); assert.equal(reservedPt({}, 'R'), 0n);
  const cv = {alertAfterAttempts: 3, alertAfterMs: 900_000};
  assert.equal(conversionAlertDue({attempts: 2, firstAt: 0, alerted: false}, 899_999, cv), false);
  assert.equal(conversionAlertDue({attempts: 3, firstAt: 0, alerted: false}, 1, cv), true);
  assert.equal(conversionAlertDue({attempts: 1, firstAt: 0, alerted: false}, 900_000, cv), true);
  assert.equal(conversionAlertDue({attempts: 9, firstAt: 0, alerted: true}, 900_000, cv), false);
});

const OTHER_XLM = 2_500_0000000n;   // pre-existing XLM inventory: must never be sold
const depsOf = (nowMs, inv = {USDC: 500_0000000n, XLM: OTHER_XLM}) => { const q = quietLog(); const j = []; return {q, j, d: {log: q.log, journal: (f, r) => j.push(r), inventoryOf: async () => inv, nowMs}}; };

test('LIVE conversion within 1.5 %: liquidation -> redeem exactly the seized pTokens -> swap exactly the redeemed XLM; other inventory untouched', async () => {
  const c = stubChain('minus20', {poolBelowBps: 100, bal: {[M.USDC.underlying]: 500_0000000n, [M.XLM.underlying]: OTHER_XLM}});
  const st = {}, {q, j, d} = depsOf(1_000_000);
  const r = await marginCycle(marginCfg({dryRun: false}), c, st, d);
  const seized = BigInt(FIX.scenarios.minus20.positions[0].heldPt);
  assert.deepEqual(j.filter(x => x.intent).map(x => x.kind), ['margin_liquidate', 'margin_redeem', 'margin_swap']);
  const sw = r.actions.conversions.find(x => x.amountIn != null);
  assert.equal(sw.amountIn, seized); assert.equal(sw.belowOracleBps, 100); assert(sw.quotedOut >= sw.minOut);
  assert.equal(c.state.bal[M.XLM.underlying], OTHER_XLM);      // only the seized XLM was sold
  assert.equal(c.state.pt[M.XLM.receipt], 0n);
  assert.deepEqual(st.convert.lots, []);
  assert(!q.lines.some(l => l.level === 'alert'));
  const swapPrep = c.calls.find(x => x[0] === 'prepare' && x[1] === 'swap');
  assert.deepEqual(swapPrep[2], {cpuStop: 289_000_000, maxFeeStroops: 30_000_000});
});

test('LIVE conversion outside 1.5 %: hold with backoff, conversionFailed after 3 attempts, swap + resolved once the pool recovers', async () => {
  const c = stubChain('minus20', {poolBelowBps: 200, bal: {[M.USDC.underlying]: 500_0000000n, [M.XLM.underlying]: OTHER_XLM}});
  const st = {}, cfg = marginCfg({dryRun: false});
  let t = 1_000_000, alerts = [], resolved = [];
  const run = async () => { c.state.seq += 1; const {q, d} = depsOf(t); await marginCycle(cfg, c, st, d); alerts.push(...q.lines.filter(l => l.type === 'conversionFailed')); resolved.push(...q.lines.filter(l => l.resolved && l.type === 'conversionFailed')); };
  await run();
  const lot = st.convert.lots[0];
  assert.equal(lot.stage, 'swap'); assert.equal(lot.attempts, 1); assert.equal(lot.nextAt, t + 60_000);
  const swapsSent = () => c.calls.filter(x => x[0] === 'submit' && x[1] === 'swap').length;
  t += 30_000; await run(); assert.equal(st.convert.lots[0].attempts, 1);      // inside the backoff: not retried
  t += 31_000; await run(); assert.equal(st.convert.lots[0].attempts, 2);
  t += 120_000; await run(); assert.equal(st.convert.lots[0].attempts, 3);
  assert.equal(alerts.length, 1); assert.equal(alerts[0].key, `conversion:${lot.lotId}`);
  t += 240_000; await run(); assert.equal(alerts.length, 1);                  // alerted once per lot
  assert.equal(swapsSent(), 0); assert.equal(c.state.bal[M.XLM.underlying], OTHER_XLM + BigInt(lot.underlying));
  c.state.poolBelowBps = 140; t += 480_000; await run();
  assert.equal(swapsSent(), 1); assert.deepEqual(st.convert.lots, []); assert.equal(resolved.length, 1);
  assert.equal(c.state.bal[M.XLM.underlying], OTHER_XLM);
});

test('conversion: short seizure redeems USDC and buys XLM only when XLM inventory is short; unknown outcome is parked; redeem failures back off', async () => {
  const base = {bal: {[M.USDC.underlying]: 0n, [M.XLM.underlying]: OTHER_XLM}, pt: {[M.USDC.receipt]: 10_000_000_000n}};
  const lot = over => ({lotId: 'S-1', positionId: '7', seized: 'USDC', receipt: M.USDC.receipt, ptokens: '10000000000', stage: 'redeem', underlying: null, target: null, attempts: 0, firstAt: 0, nextAt: 0, alerted: false, ...over});
  let c = stubChain('seizure', base), st = {convert: {lots: [lot()]}};
  await marginCycle(marginCfg({dryRun: false}), c, st, depsOf(1_000).d);
  assert(c.calls.some(x => x[0] === 'submit' && x[1] === 'withdraw')); assert(!c.calls.some(x => x[1] === 'swap'));
  assert.equal(c.state.bal[M.USDC.underlying], 10_000_000_000n); assert.deepEqual(st.convert.lots, []);
  // only the lot's pTokens are redeemed, even when the keeper holds more on that receipt
  c = stubChain('seizure', {...base, pt: {[M.USDC.receipt]: 99_000_000_000n}}); st = {convert: {lots: [lot()]}};
  await marginCycle(marginCfg({dryRun: false}), c, st, depsOf(1_000).d);
  assert.equal(c.state.pt[M.USDC.receipt], 89_000_000_000n);
  // UNKNOWN swap outcome -> parked for review, alerted, never retried
  c = stubChain('seizure', {...base, bal: {[M.XLM.underlying]: 5_000_000_000n, [M.USDC.underlying]: 0n}, submitStatus: () => 'UNKNOWN'});
  st = {convert: {lots: [lot({lotId: 'L-1', seized: 'XLM', receipt: M.XLM.receipt, stage: 'swap', underlying: '5000000000', target: 'USDC'})]}};
  let {q, d} = depsOf(1_000);
  await marginCycle(marginCfg({dryRun: false}), c, st, d);
  assert.equal(st.convert.lots[0].stage, 'review'); assert(q.lines.some(l => l.type === 'conversionFailed' && /unknown/.test(l.msg)));
  c.state.seq += 1; ({q, d} = depsOf(999_999_999));
  await marginCycle(marginCfg({dryRun: false}), c, st, d);
  assert.equal(c.calls.filter(x => x[0] === 'submit' && x[1] === 'swap').length, 1);
  // redeem failing (cash lent out): held with backoff, no send
  c = stubChain('seizure', {...base, failWithdraw: true}); st = {convert: {lots: [lot()]}};
  await marginCycle(marginCfg({dryRun: false}), c, st, depsOf(1_000).d);
  assert.equal(st.convert.lots[0].attempts, 1); assert(!c.calls.some(x => x[0] === 'submit' && x[1] === 'withdraw'));
  // dry run never sends a conversion step
  c = stubChain('seizure', base); st = {convert: {lots: [lot()]}};
  await marginCycle(marginCfg(), c, st, depsOf(1_000).d);
  assert(!c.calls.some(x => x[0] === 'submit')); assert.equal(st.convert.lots[0].stage, 'redeem');
});

test('dust: expected loss <= $2 (oracle USD incl. fee) is liquidated with a dustLiquidation alert; > $2 is not sent and alerted', async () => {
  const pX = FIX.scenarios.minus20.priceXlm.map(BigInt), pU = FIX.pricesP0.usdc.map(BigInt);
  const pay = BigInt(FIX.checks.L5_underwater_minus20.fork.paid);
  const payUsd = pay * pU[0] / pU[1], feeUsd = 2_000_000n * pX[0] / pX[1];
  const seizeFor = lossUsd7 => (payUsd + feeUsd - lossUsd7) * pX[1] / pX[0];
  for (const [loss, sent] of [[19_000_000n, true], [21_000_000n, false]]) {
    const c = stubChain('minus20', {liqSeize: {1: seizeFor(loss)}}), {q, j, d} = depsOf(1_000);
    const r = await marginCycle(marginCfg({dryRun: false}), c, {}, d);
    const liq = r.actions.liquidations[0];
    const a = q.lines.filter(l => l.level === 'alert');
    if (sent) {
      assert.equal(liq.status, 'SUCCESS'); assert.equal(liq.dust, true); assert(j.some(x => x.kind === 'margin_liquidate' && x.intent));
      assert.equal(a.filter(x => x.type === 'dustLiquidation').length, 1); assert(a[0].lossUsd7 <= 20_000_000n);
    } else {
      assert.equal(liq.skipped, 'unprofitable'); assert(!c.calls.some(x => x[0] === 'submit'));
      assert(a.some(x => x.type === 'unprofitableSkipped' && x.lossUsd7 > 20_000_000n)); assert(!a.some(x => x.type === 'dustLiquidation'));
    }
  }
});

test('LIVE short liquidation: target decided from inventory AFTER paying XLM; buys back only the XLM shortfall (+2 %), keeps the rest as USDC', async () => {
  const MIN_XLM = BigInt(PINS.minInventoryRaw.XLM);
  const c = stubChain('minus20', {poolBelowBps: 50, bal: {[M.USDC.underlying]: 0n, [M.XLM.underlying]: MIN_XLM}});
  const p = c.state.positions['1']; p.side = 'short';            // held USDC pTokens, debt XLM
  const inventoryOf = async () => ({USDC: c.state.bal[M.USDC.underlying], XLM: c.state.bal[M.XLM.underlying]});
  const q = quietLog(), j = [];
  const st = {};
  await marginCycle(marginCfg({dryRun: false}), c, st, {log: q.log, journal: (f, r) => j.push(r), inventoryOf, nowMs: 1_000});
  assert.deepEqual(j.filter(x => x.intent).map(x => x.kind), ['margin_liquidate', 'margin_redeem', 'margin_swap']);
  const paid = BigInt(FIX.checks.L5_underwater_minus20.fork.paid), seized = BigInt(p.heldPt);
  const xlmAfter = c.state.bal[M.XLM.underlying], usdcAfter = c.state.bal[M.USDC.underlying];
  assert(xlmAfter >= MIN_XLM && xlmAfter <= MIN_XLM + paid * 3n / 100n, `XLM back to about the minimum: ${xlmAfter}`);
  assert(usdcAfter > 0n && usdcAfter < seized, 'only part of the seized USDC was sold');
  assert.deepEqual(st.convert.lots, []);
  // with XLM already above its minimum after the payment, the seized USDC is only redeemed
  const c2 = stubChain('minus20', {bal: {[M.USDC.underlying]: 0n, [M.XLM.underlying]: MIN_XLM * 2n}});
  c2.state.positions['1'].side = 'short';
  const j2 = [];
  await marginCycle(marginCfg({dryRun: false}), c2, {}, {log: quietLog().log, journal: (f, r) => j2.push(r), inventoryOf: async () => ({USDC: c2.state.bal[M.USDC.underlying], XLM: c2.state.bal[M.XLM.underlying]}), nowMs: 1_000});
  assert.deepEqual(j2.filter(x => x.intent).map(x => x.kind), ['margin_liquidate', 'margin_redeem']);
});
