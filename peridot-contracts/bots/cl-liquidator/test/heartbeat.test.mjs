import test from 'node:test';
import assert from 'node:assert/strict';
import {heartbeatDue, gapStats, makeHeartbeat, failoverRpc, HEARTBEAT_PERIOD_MS} from '../src/heartbeat.mjs';
import {normalize} from '../src/config.mjs';
import {hashFromCli} from '../src/main.mjs';

const base = {blocked: true, obsAtMs: 0, nowMs: 0, lastSubmitMs: null, submitting: false};

test('heartbeatDue: fires when blocked, then every period from the last SUBMIT', () => {
  assert.equal(heartbeatDue(base), true);
  assert.equal(heartbeatDue({...base, lastSubmitMs: 0, nowMs: HEARTBEAT_PERIOD_MS - 1}), false);
  assert.equal(heartbeatDue({...base, lastSubmitMs: 0, nowMs: HEARTBEAT_PERIOD_MS, obsAtMs: 5000}), true);
});
test('heartbeatDue: never while not blocked, mid-submit, or on a stale observation', () => {
  assert.equal(heartbeatDue({...base, blocked: false}), false);
  assert.equal(heartbeatDue({...base, submitting: true}), false);
  assert.equal(heartbeatDue({...base, obsAtMs: null}), false);
  assert.equal(heartbeatDue({...base, nowMs: 60_001, obsAtMs: 0}), false);
  assert.equal(heartbeatDue({...base, nowMs: 60_000, obsAtMs: 0}), true);
});
test('gapStats: max, p95, counts over 20 / 60 s', () => {
  assert.deepEqual(gapStats([0]), {n: 0});
  const s = gapStats([0, 12, 24, 41, 53, 120]);
  assert.equal(s.n, 5); assert.equal(s.max, 67); assert.equal(s.over20, 1); assert.equal(s.over60, 1); assert.equal(s.min, 12);
});
test('hashFromCli: last 64-hex token on stderr', () => {
  const h = 'a'.repeat(64), g = 'b'.repeat(64);
  assert.equal(hashFromCli(`Transaction hash is ${h}\nhttps://x/tx/${g}`), g);
  assert.equal(hashFromCli('no hash'), null);
  assert.equal(hashFromCli(`Signing transaction: ${h}\nEvent: upgraded ${g}\nhttps://stellar.expert/explorer/testnet/tx/${h}`), h, 'CLI markers beat event payloads');
});
test('config: heartbeat identity/account must be distinct from the liquidator', () => {
  const c = {contracts: {receipt: 'C', vault: 'C', controller: 'C', usdcMarket: 'C', usdc: 'C', pyusd: 'C', oracle: 'C'},
    liquidator: 'G' + 'A'.repeat(55), executorIdentity: 'liq'};
  assert.doesNotThrow(() => normalize({...c, heartbeatIdentity: 'hb', heartbeatAccount: 'G' + 'B'.repeat(55)}));
  assert.throws(() => normalize({...c, heartbeatIdentity: 'liq', heartbeatAccount: 'G' + 'B'.repeat(55)}), /differ/);
  assert.throws(() => normalize({...c, heartbeatIdentity: 'hb', heartbeatAccount: c.liquidator}), /distinct/);
  assert.throws(() => normalize({...c, heartbeatIdentity: 'hb', heartbeatAccount: 'G' + 'B'.repeat(55), heartbeatPeriodMs: 20000}), /3000..15000/);
});

// Fake SDK/RPC: the timer submits on schedule even while earlier submissions are not yet included.
function fakes() {
  let n = 0; const sent = [], journal = [];
  class Account { constructor(a, s) { this.a = a; this.s = BigInt(s); } }
  const S = {Account, Contract: class { call() { return {}; } },
    TransactionBuilder: class {
      constructor(acct) { this.acct = acct; } addOperation() { return this; } setTimeout() { return this; }
      build() { const seq = this.acct.s + 1n; return {seq, toXDR: () => `xdr:${seq}`}; }
      static fromXDR(x) { const seq = x.split(':')[1]; return {seq, hash: () => Buffer.from(seq.padStart(32, '0'))}; }
    },
    rpc: {Api: {isSimulationSuccess: () => true}, assembleTransaction: tx => ({build: () => tx})}};
  const rpc = {getAccount: async () => ({sequenceNumber: () => '100'}), simulateTransaction: async () => ({}),
    sendTransaction: async tx => { sent.push(String(tx.seq)); n++; return {status: 'PENDING'}; },
    getTransaction: async () => ({status: 'NOT_FOUND'})};
  const j = {intent: (...a) => { journal.push(['intent', ...a]); return journal.length; }, result: (...a) => journal.push(['result', ...a])};
  return {S, rpc, j, sent, journal};
}
test('makeHeartbeat: 12 s cadence from the last submit, sequence advances without waiting for inclusion, hash journalled', async () => {
  const f = fakes(); let now = 0;
  const hb = makeHeartbeat({S: f.S, rpc: f.rpc, cfg: {heartbeatAccount: 'G', contracts: {receipt: 'C'}, networkPassphrase: 'T'}, journal: f.j,
    sign: async x => x, out: () => {}, clock: () => now});
  const settle = () => new Promise(r => setImmediate(r));
  for (now = 0; now <= 60_000; now += 500) {
    if (now % 10_000 === 0) hb.observe(true);   // plan loop observes every 10 s
    await hb.tick(); await settle(); await settle(); await settle();
  }
  assert.deepEqual(f.sent, ['101', '102', '103', '104', '105', '106']);   // t = 0, 12, 24, 36, 48, 60 s
  const intents = f.journal.filter(x => x[0] === 'intent');
  assert.equal(intents.length, 6); assert.ok(intents.every(x => typeof x[4].hash === 'string' && x[4].hash.length > 0));
  hb.observe(false); const before = f.sent.length;
  for (let k = 0; k < 40; k++) { now += 500; await hb.tick(); await settle(); }
  assert.equal(f.sent.length, before, 'no heartbeat once the checkpoint is healthy');
});
test('makeHeartbeat: a rejected send resyncs the sequence from the network', async () => {
  const f = fakes(); let now = 0, calls = 0;
  f.rpc.sendTransaction = async tx => { calls++; f.sent.push(String(tx.seq)); return {status: calls === 1 ? 'ERROR' : 'PENDING'}; };
  const hb = makeHeartbeat({S: f.S, rpc: f.rpc, cfg: {heartbeatAccount: 'G', contracts: {receipt: 'C'}, networkPassphrase: 'T'}, journal: f.j,
    sign: async x => x, out: () => {}, clock: () => now});
  hb.observe(true); await hb.tick(); for (let k = 0; k < 5; k++) await new Promise(r => setImmediate(r));
  now = 500; hb.observe(true); await hb.tick(); for (let k = 0; k < 5; k++) await new Promise(r => setImmediate(r));
  assert.deepEqual(f.sent, ['101'], 'no retry inside the 2 s backoff');
  now = 2500; hb.observe(true); await hb.tick(); for (let k = 0; k < 5; k++) await new Promise(r => setImmediate(r));
  assert.deepEqual(f.sent, ['101', '101'], 'retry after an ERROR resyncs to the network sequence, not a skipped one');
});
test('makeHeartbeat: TRY_AGAIN_LATER keeps the local sequence and retries after 2 s', async () => {
  const f = fakes(); let now = 0, calls = 0;
  f.rpc.getAccount = async () => { calls++; return {sequenceNumber: () => '100'}; };
  let first = true;
  f.rpc.sendTransaction = async tx => { f.sent.push(String(tx.seq)); const s = first ? 'PENDING' : f.sent.length === 2 ? 'TRY_AGAIN_LATER' : 'PENDING'; first = false; return {status: s}; };
  const hb = makeHeartbeat({S: f.S, rpc: f.rpc, cfg: {heartbeatAccount: 'G', contracts: {receipt: 'C'}, networkPassphrase: 'T'}, journal: f.j,
    sign: async x => x, out: () => {}, clock: () => now});
  const run = async t => { now = t; hb.observe(true); await hb.tick(); for (let k = 0; k < 5; k++) await new Promise(r => setImmediate(r)); };
  await run(0); await run(12_000); await run(13_000); await run(14_100);
  assert.deepEqual(f.sent, ['101', '102', '102']); assert.equal(calls, 1, 'no network resync on TRY_AGAIN_LATER');
});

test('failoverRpc: a transport failure moves to the next server and stays; answers never rotate', async () => {
  const calls = [];
  const a = {getAccount: async () => { calls.push('a'); throw Error('fetch failed'); }, sendTransaction: async () => { calls.push('a-send'); return {status: 'ERROR'}; }};
  const b = {getAccount: async () => { calls.push('b'); return 'B'; }, sendTransaction: async () => { calls.push('b-send'); return {status: 'TRY_AGAIN_LATER'}; }};
  const r = failoverRpc([a, b]);
  assert.equal(await r.getAccount('G'), 'B');
  assert.equal((await r.sendTransaction({})).status, 'TRY_AGAIN_LATER');
  assert.deepEqual(calls, ['a', 'b', 'b-send']);
  b.getAccount = async () => { throw Error('down'); };
  await assert.rejects(() => r.getAccount('G'), /fetch failed|down/);
});
