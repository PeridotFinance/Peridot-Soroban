// Bounded READ retry on HTTP 429/503 (public RPC rate limits). sendTransaction is never retried.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import {isRateLimited, withReadRetry, makeChain, READ_RETRY_METHODS} from '../src/chain.mjs';

const noSleep = {sleep: async () => {}, random: () => 0};

test('isRateLimited: axios-style, status-style and message-style 429/503 only', () => {
  assert.equal(isRateLimited({response: {status: 429}}), true);
  assert.equal(isRateLimited({status: 503}), true);
  assert.equal(isRateLimited(Error('Request failed with status code 429')), true);
  assert.equal(isRateLimited(Error('Request failed with status code 500')), false);
  assert.equal(isRateLimited(Error('simulation failed: HostError')), false);
  assert.equal(isRateLimited(undefined), false);
});

test('withReadRetry: succeeds after two 429s, logs each retry with growing waits', async () => {
  let n = 0; const seen = [];
  const r = await withReadRetry(async () => { if (n++ < 2) throw Error('Request failed with status code 429'); return 'ok'; },
    {...noSleep, onRetry: (e, attempt, waitMs) => seen.push([attempt, waitMs])});
  assert.equal(r, 'ok'); assert.equal(n, 3);
  assert.deepEqual(seen, [[1, 400], [2, 800]]);
});

test('withReadRetry: bounded (3 attempts at most), then the 429 surfaces', async () => {
  let n = 0;
  await assert.rejects(withReadRetry(async () => { n++; throw Error('Request failed with status code 429'); }, noSleep), /429/);
  assert.equal(n, 3);
});

test('withReadRetry: non-rate-limit errors are not retried', async () => {
  let n = 0;
  await assert.rejects(withReadRetry(async () => { n++; throw Error('boom'); }, noSleep), /boom/);
  assert.equal(n, 1);
});

test('withReadRetry: retries 0 disables', async () => {
  let n = 0;
  await assert.rejects(withReadRetry(async () => { n++; throw Error('Request failed with status code 429'); }, {...noSleep, retries: 0}), /429/);
  assert.equal(n, 1);
});

test('withReadRetry: jitter stays within [base*2^i, 2*base*2^i]', async () => {
  const waits = [];
  await assert.rejects(withReadRetry(async () => { throw {status: 429}; }, {sleep: async () => {}, random: () => 1, onRetry: (e, a, w) => waits.push(w)}));
  assert.deepEqual(waits, [800, 1600]);
});

test('makeChain: read methods wrapped, sendTransaction NOT wrapped', () => {
  const c = makeChain({rpcUrl: 'http://127.0.0.1:1', network: 'x', publicKey: 'G'});
  const proto = Object.getPrototypeOf(c.rpc);
  for (const m of READ_RETRY_METHODS) assert.notEqual(c.rpc[m], proto[m], `${m} should be wrapped`);
  assert.equal(c.rpc.sendTransaction, proto.sendTransaction, 'sendTransaction must not be wrapped');
});

test('makeChain: raw getLatestLedger retries through two 429s from a local server, logging each', async () => {
  let hits = 0;
  const srv = http.createServer((req, res) => {
    hits++;
    if (hits <= 2) { res.writeHead(429, {'content-type': 'text/plain'}); return res.end('Too Many Requests'); }
    res.writeHead(200, {'content-type': 'application/json'});
    res.end(JSON.stringify({jsonrpc: '2.0', id: 1, result: {sequence: 123, closeTime: '1700000000', protocolVersion: 30}}));
  });
  await new Promise(r => srv.listen(0, '127.0.0.1', r));
  try {
    const logs = [];
    const c = makeChain({rpcUrl: `http://127.0.0.1:${srv.address().port}`, network: 'x', publicKey: 'G'},
      {log: (lvl, msg, x) => logs.push([lvl, msg, x.method, x.attempt]), retry: {sleep: async () => {}}});
    const L = await c.latestLedger();
    assert.equal(L.sequence, 123); assert.equal(hits, 3);
    assert.deepEqual(logs, [['warn', 'rpc read rate-limited; retrying', 'getLatestLedger', 1], ['warn', 'rpc read rate-limited; retrying', 'getLatestLedger', 2]]);
  } finally { srv.close(); }
});

test('makeChain: a third 429 surfaces as an error (bounded)', async () => {
  let hits = 0;
  const srv = http.createServer((req, res) => { hits++; res.writeHead(429); res.end(); });
  await new Promise(r => srv.listen(0, '127.0.0.1', r));
  try {
    const c = makeChain({rpcUrl: `http://127.0.0.1:${srv.address().port}`, network: 'x', publicKey: 'G'}, {retry: {sleep: async () => {}}});
    await assert.rejects(c.latestLedger(), /status code 429/);
    assert.equal(hits, 3);
  } finally { srv.close(); }
});
