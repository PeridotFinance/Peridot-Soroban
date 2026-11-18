import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {assertLiveAllowed, makeExecutor, cliEnv, MAINNET, TESTNET} from '../src/chain.mjs';
import {normalize} from '../src/config.mjs';
import {openJournal, unresolvedL3, lastL4Failure, INTENT_TTL} from '../src/journal.mjs';
import {plan} from '../src/plan.mjs';
import {stageCall} from '../src/main.mjs';

test('stage calls use the contract argument names (controller liquidate, receipt sync_seized, SAC approve)', () => {
  const c = {contracts: {receipt: 'CR', vault: 'CV', controller: 'CC', usdcMarket: 'CM', usdc: 'CU'}, liquidator: 'GL'};
  const l3 = stageCall({stage: 'L3', borrower: 'GB', repay: 5n}, c, 100);
  assert.deepEqual([l3.contract, l3.fn, Object.keys(l3.args)], ['CC', 'liquidate', ['borrower', 'repay_market', 'collateral_market', 'repay_amount', 'liquidator']]);
  assert.deepEqual([l3.args.repay_market, l3.args.collateral_market], ['CM', 'CR']);
  assert.deepEqual(stageCall({stage: 'L5', id: 7}, c, 1), {contract: 'CR', fn: 'sync_seized', args: {id: 7}});
  assert.deepEqual(Object.keys(stageCall({stage: 'APPROVE', amount: 1n}, c, 1).args), ['from', 'spender', 'amount', 'expiration_ledger']);
  assert.deepEqual([stageCall({stage: 'L4'}, c, 1).fn, stageCall({stage: 'L2'}, c, 1).args.token], ['checkpoint_rewards', 'CU']);
});

const G = 'G' + 'A'.repeat(55);
const cfg = o => ({networkPassphrase: TESTNET, rpcUrl: 'https://soroban-testnet.stellar.org', executorIdentity: 'cl-liq-testnet', ...o});

test('live refused on Mainnet and on any non-Testnet passphrase', () => {
  assert.throws(() => assertLiveAllowed(cfg({networkPassphrase: MAINNET})), /never allowed on Mainnet/);
  assert.throws(() => makeExecutor(cfg({networkPassphrase: MAINNET})), /Mainnet/);
  assert.throws(() => assertLiveAllowed(cfg({networkPassphrase: 'Test SDF Future Network ; October 2022'})), /exact Testnet/);
  assert.doesNotThrow(() => assertLiveAllowed(cfg()));
});
test('executor re-checks per call and refuses a mutated Mainnet config', async () => {
  const c = cfg(), calls = [];
  const inv = makeExecutor(c, {execFile: (bin, argv, opts, cb) => { calls.push({bin, argv, opts}); cb(null, 'true', ''); }});
  const r = await inv('CRECEIPT', 'sync_seized', {id: 3});
  assert.equal(r.ok, true);
  assert.equal(calls[0].bin, 'stellar');
  assert.deepEqual(calls[0].argv.slice(-4), ['--', 'sync_seized', '--id', '3']);
  assert.ok(calls[0].argv.includes('--send') && calls[0].argv.includes('cl-liq-testnet'));
  const env = cliEnv({STELLAR_ACCOUNT: 'a', STELLAR_NETWORK: 'mainnet', STELLAR_RPC_URL: 'x', STELLAR_SIGN_WITH_LEDGER: '1', SOROBAN_NETWORK: 'm', STELLAR_CONFIG_HOME: '/c', PATH: '/bin'});
  assert.deepEqual(env, {STELLAR_CONFIG_HOME: '/c', PATH: '/bin'});
  c.networkPassphrase = MAINNET;
  assert.throws(() => inv('CRECEIPT', 'checkpoint_nav'), /Mainnet/);
  assert.equal(calls.length, 1);
  // Timeout / signal => unknown (intent stays unresolved); a non-zero exit is a plain failure.
  const t = makeExecutor(cfg(), {execFile: (b, a, o, cb) => cb(Object.assign(Error('t'), {killed: true, signal: 'SIGTERM'}), '', '')});
  assert.equal((await t('C', 'liquidate')).unknown, true);
  const f = makeExecutor(cfg(), {execFile: (b, a, o, cb) => cb(Object.assign(Error('x'), {code: 1}), '', 'Error(Contract, #2109)')});
  assert.deepEqual([(await f('C', 'sync_seized')).ok, (await f('C', 'sync_seized')).unknown], [false, false]);
});
test('identity must be an alias, never a key or seed phrase', () => {
  assert.throws(() => assertLiveAllowed(cfg({executorIdentity: 'S' + 'B'.repeat(55)})), /strkey/);
  assert.throws(() => assertLiveAllowed(cfg({executorIdentity: 'kite urban mango apple'})), /alias/);
  assert.throws(() => normalize({contracts: {receipt: 1, vault: 1, controller: 1, usdcMarket: 1, usdc: 1, pyusd: 1, oracle: 1}, liquidator: G, executorIdentity: 'x', secretKey: 'no'}), /key material/);
  const ok = normalize({contracts: {receipt: 1, vault: 1, controller: 1, usdcMarket: 1, usdc: 1, pyusd: 1, oracle: 1}, liquidator: G, executorIdentity: 'x'});
  assert.equal(ok.networkPassphrase, TESTNET);
});

test('journal: a restart never re-sends L3 for a borrower with an unresolved or queued seizure', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clliq-'));
  const T = 1_800_000_000;
  let j = openJournal(dir);
  const s1 = j.intent('L3', 'GB', T, {nextPending: 4});            // crash: no result written
  j = openJournal(dir);                                             // restart
  let blocked = unresolvedL3(j.records(), T + 30, 4);
  assert.ok(blocked.has('GB'));
  const obs = {now: T + 30, navSim: 'ok', status: {nav: {rate: 1_000_000n, ts: T + 25}, chain: null, next_pending: 4}, priceCache: {timestamp: T, resolution: 300, k: 2},
    cf: 800_000n, closeFactor: 500_000n, li: 1_080_000n, inventory: 10n ** 12n, allowance: 10n ** 12n, borrowers: [{user: 'GB', units: 10n ** 10n, debt: 9n * 10n ** 9n}], pending: [], blockedL3: new Set(blocked.keys())};
  assert.ok(!plan(obs).stages.some(s => s.stage === 'L3'));
  // The seizure landed (queue moved): stays blocked, operator review.
  assert.equal(unresolvedL3(j.records(), T + INTENT_TTL + 1, 5).get('GB').reviewRequired, true);
  // Nothing landed anywhere after the TTL: released.
  assert.equal(unresolvedL3(j.records(), T + INTENT_TTL + 1, 4).size, 0);
  // A recorded result resolves it; sequence numbers continue across restarts.
  j.result(s1, 'L3', 'GB', T + 40, false);
  assert.equal(unresolvedL3(j.records(), T + 41, 4).size, 0);
  assert.equal(openJournal(dir).intent('L4', 'L4', T + 50), s1 + 1);
  assert.equal(lastL4Failure(openJournal(dir).records()), null);
  openJournal(dir).result(s1 + 1, 'L4', 'L4', T + 51, true, {retval: false});
  assert.equal(lastL4Failure(openJournal(dir).records()), T + 51);
  const lines = fs.readFileSync(path.join(dir, 'journal.jsonl'), 'utf8').trim().split('\n');
  openJournal(dir).result(s1 + 1, 'L4', 'L4', T + 52, false);            // CLI error: no claim information
  assert.equal(lastL4Failure(openJournal(dir).records()), T + 51);
  assert.equal(fs.readFileSync(path.join(dir, 'journal.jsonl'), 'utf8').trim().split('\n').length, 5);  // append-only
  fs.rmSync(dir, {recursive: true, force: true});
});
