import {test} from 'node:test';
import assert from 'node:assert/strict';
import {loadConfig} from '../src/config.mjs';

const base = {
  RPC_URL: 'https://rpc.example', NETWORK_PASSPHRASE: 'Public Global Stellar Network ; September 2015',
  KEEPER_PUBLIC_KEY: 'GDEDICATEDSENTORAKEEPERPLACEHOLDER', CONTROLLER: 'C1', PEG_ORACLE: 'C2', PYUSD_TOKEN: 'C3',
  RECEIPTS: 'R1,R2,R3', ADAPTERS: 'A1,A2,A3', SENTORA_VAULTS: 'S1,S2,S3',
};

test('defaults to dry run without a secret', () => {
  const c = loadConfig(base);
  assert.equal(c.dryRun, true);
  assert.equal(c.secret, undefined);
  assert.ok(c.refreshMs < 300_000);
});

test('rejects a key shared with an existing keeper or admin', () => {
  assert.throws(() => loadConfig({...base, KEEPER_PUBLIC_KEY: 'GBLVLGECHVZAKOJ6XLLF4YRNNBTAZBM4O6ZMIL336BYIHKUZLACLXSE3'}));
  assert.throws(() => loadConfig({...base, KEEPER_PUBLIC_KEY: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP'}));
});

test('live mode requires the secret; lists must align', () => {
  assert.throws(() => loadConfig({...base, DRY_RUN: 'false'}));
  assert.throws(() => loadConfig({...base, ADAPTERS: 'A1,A2'}));
});
