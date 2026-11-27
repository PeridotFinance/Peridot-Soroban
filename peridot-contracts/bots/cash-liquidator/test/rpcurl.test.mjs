// RPC_URL_FILE: private endpoint in a 0600 file, host pinned, never logged.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {loadRpcUrlFile, resolveRpc, scrubText, PUBLIC_RPC} from '../src/rpcurl.mjs';
import {loadConfig} from '../src/config.mjs';

const HOST = 'rpc.example-provider.test', KEY = 'k3yK3yK3yK3yK3yK3yK3yK3y0123456789', URL1 = `https://${HOST}/v2/${KEY}`;
const tmp = t => { const d = fs.mkdtempSync(path.join(os.tmpdir(), 'rpcurl-')); t.after(() => fs.rmSync(d, {recursive: true, force: true})); return d; };
const write = (d, body, mode = 0o600) => { const f = path.join(d, 'rpc-url'); fs.writeFileSync(f, body); fs.chmodSync(f, mode); return f; };

test('loadRpcUrlFile: valid 0600 https file with the pinned host', t => {
  const f = write(tmp(t), URL1 + '\n');
  assert.equal(loadRpcUrlFile(f, HOST), URL1);
});

test('loadRpcUrlFile: refusals never echo the file contents', t => {
  const cases = [
    [write(tmp(t), URL1, 0o640), /too open/],
    [(() => { const f = write(tmp(t), URL1); fs.renameSync(f, f + '.r'); fs.symlinkSync(f + '.r', f); return f; })(), /symlink/],
  ];
  for (const [f, re] of cases) assert.throws(() => loadRpcUrlFile(f, HOST), e => re.test(e.message) && !e.message.includes(KEY));
  const d2 = tmp(t);
  for (const [body, re] of [[KEY, /valid URL/], [`http://${HOST}/v2/${KEY}`, /https/], [`https://evil.test/v2/${KEY}`, /does not match RPC_HOST/], [`https://u:${KEY}@${HOST}/`, /user:password/]]) {
    const f = write(d2, body);
    assert.throws(() => loadRpcUrlFile(f, HOST), e => re.test(e.message) && !e.message.includes(KEY));
    fs.rmSync(f);
  }
  assert.throws(() => loadRpcUrlFile('relative/path', HOST), /absolute/);
  assert.throws(() => loadRpcUrlFile(path.join(d2, 'missing'), HOST), /missing/);
  assert.throws(() => loadRpcUrlFile(write(d2, URL1), undefined), /RPC_HOST/);
});

test('resolveRpc: file | env | default; both set refused', t => {
  const f = write(tmp(t), URL1);
  assert.deepEqual(resolveRpc({RPC_URL_FILE: f, RPC_HOST: HOST}), {url: URL1, host: HOST, source: 'file'});
  assert.throws(() => resolveRpc({RPC_URL_FILE: f, RPC_HOST: HOST, RPC_URL: PUBLIC_RPC}), /not both/);
  assert.deepEqual(resolveRpc({RPC_URL: PUBLIC_RPC}), {url: PUBLIC_RPC, host: 'soroban-rpc.mainnet.stellar.gateway.fm', source: 'env'});
  assert.equal(resolveRpc({}).source, 'default');
});

test('scrubText: the URL, its path and the key segment are all removed', t => {
  loadRpcUrlFile(write(tmp(t), URL1), HOST);
  for (const s of [`fetch ${URL1} failed`, `GET /v2/${KEY}`, `key=${KEY}`]) { const o = scrubText(s); assert(!o.includes(KEY), o); assert(o.includes('<rpc-secret>')); }
  assert.equal(scrubText('plain text'), 'plain text');
});

test('loadConfig: RPC_URL_FILE selects the private endpoint; the start fields expose only host and source', t => {
  const f = write(tmp(t), URL1);
  const cfg = loadConfig({RPC_URL_FILE: f, RPC_HOST: HOST, LIQUIDATOR_PUBLIC_KEY: 'G'});
  assert.equal(cfg.rpcUrl, URL1); assert.equal(cfg.rpcHost, HOST); assert.equal(cfg.rpcSource, 'file');
  assert.equal(loadConfig({LIQUIDATOR_PUBLIC_KEY: 'G'}).rpcUrl, PUBLIC_RPC);
});

test('keeper log line is scrubbed before stdout', async t => {
  loadRpcUrlFile(write(tmp(t), URL1), HOST);
  const {log} = await import('../src/keeper.mjs');
  const out = []; const orig = console.log; console.log = s => out.push(s);
  try { log('warn', 'x', {err: `request to ${URL1} failed`}); } finally { console.log = orig; }
  assert.equal(out.length, 1); assert(!out[0].includes(KEY)); assert(out[0].includes('<rpc-secret>'));
});
