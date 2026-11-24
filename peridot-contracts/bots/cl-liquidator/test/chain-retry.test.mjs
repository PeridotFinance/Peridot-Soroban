// Read-path retry (chain.mjs): transient RPC transport errors (HTTP 429 seen live on Mainnet 24 Nov) are retried with
// bounded backoff; everything else is rethrown at once, and a 4th transient failure still fails the cycle.
import test from 'node:test';
import assert from 'node:assert/strict';
import {withRpcRetry, isTransientRpcError} from '../src/chain.mjs';

const fast = {sleep: async () => {}};
const flaky = (errs, value = 'ok') => { let n = 0; const f = async () => { n++; if (errs.length) throw errs.shift(); return value; }; f.calls = () => n; return f; };

test('isTransientRpcError: 429/502/503/504 and connection drops only', () => {
  for (const m of ['Request failed with status code 429', 'Request failed with status code 503', 'fetch failed', 'read ECONNRESET', 'socket hang up', 'getaddrinfo EAI_AGAIN x'])
    assert.equal(isTransientRpcError(Error(m)), true, m);
  for (const m of ['Request failed with status code 400', 'Request failed with status code 500', 'HostError: Error(Contract, #2108)', 'checkpoint_nav: boom', 'status code 4290'])
    assert.equal(isTransientRpcError(Error(m)), false, m);
});

test('withRpcRetry: two 429s then success returns the value after 3 calls with 1 s / 2 s backoff', async () => {
  const waits = [], f = flaky([Error('Request failed with status code 429'), Error('Request failed with status code 429')]);
  assert.equal(await withRpcRetry(f, {sleep: async ms => { waits.push(ms); }}), 'ok');
  assert.equal(f.calls(), 3); assert.deepEqual(waits, [1000, 2000]);
});

test('withRpcRetry: a non-transient error is rethrown immediately (no retry)', async () => {
  const f = flaky([Error('Request failed with status code 400')]);
  await assert.rejects(withRpcRetry(f, fast), /status code 400/); assert.equal(f.calls(), 1);
});

test('withRpcRetry: four transient failures still fail (bounded; the cycle fails as before)', async () => {
  const e = () => Error('Request failed with status code 429'), f = flaky([e(), e(), e(), e(), e()]);
  await assert.rejects(withRpcRetry(f, fast), /429/); assert.equal(f.calls(), 4);
});
