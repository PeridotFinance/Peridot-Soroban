'use strict';
// funding-preflight.cjs. Offline. The "14:05 UTC" figures are the lead's public read of 14 Nov 2026, not a read by these tests.
const test = require('node:test'), assert = require('node:assert/strict');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const F = require('../funding-preflight.cjs');
const ACCOUNT = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 1)), KEEPER = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 2)), ISSUER = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 3));
const requirements = (over = {}) => ({network: S.Networks.PUBLIC, rpcUrl: 'https://rpc.invalid', account: ACCOUNT, keeperPublicAddress: KEEPER,
  assets: {funding: {code: 'PYUSD', issuer: ISSUER}, other: {code: 'USDC', issuer: ISSUER}, reward: {code: 'AQUA', issuer: ISSUER}},
  pyusd: {seedPrincipalRaw: '10000000000', permanentFloorRaw: '10000000', reserveAllowanceRaw: '50000000'},
  xlm: {uploadsStroops: '3100115681', creationAndConfigurationStroops: '200000000', pilotFeesStroops: '100000000', walletFloorStroops: '100000000'}, ...over});
const line = (code, balance) => ({code, present: true, authorized: true, balance});
const observed = (over = {}) => ({xlmSpendableStroops: '4000000000', trustlines: {funding: line('PYUSD', '10060000000'), other: line('USDC', '177062510'), reward: line('AQUA', '0')}, ...over});
test('ready only when PYUSD covers principal + floor + reserve, XLM covers every part, and both extra trustlines exist', () => {
  const r = F.evaluate(requirements(), observed()); assert.equal(r.status, 'FUNDING_READY_FOR_STEP_1'); assert.deepEqual(r.shortfalls, []); assert.deepEqual(r.notSupplied, []);
  assert.equal(r.required.pyusdRaw, '10060000000'); assert.equal(r.required.xlmStroops, '3500115681'); assert.equal(r.signing, false); assert.equal(r.submission, false);
});
test('NEGATIVE the wallet as read at about 14:05 UTC on 14 Nov 2026 is BLOCKED on PYUSD, XLM and the AQUA trustline', () => {
  const live = {xlmSpendableStroops: '62066669', trustlines: {funding: line('PYUSD', '8646435532'), other: line('USDC', '177062510'), reward: {code: 'AQUA', present: false}}};
  const r = F.evaluate(requirements({keeperPublicAddress: null, pyusd: {seedPrincipalRaw: '10000000000', permanentFloorRaw: '10000000', reserveAllowanceRaw: null},
    xlm: {uploadsStroops: '3100115681', creationAndConfigurationStroops: null, pilotFeesStroops: null, walletFloorStroops: null}}), live);
  assert.equal(r.status, 'BLOCKED');
  assert.deepEqual(r.shortfalls, ['PYUSD: holds 864.6435532, needs AT LEAST (some requirements are not supplied yet) 1001.0000000 (1,000 principal + 1 floor + reserve allowance): short by 136.3564468',
    'XLM: spendable 6.2066669, needs AT LEAST (some requirements are not supplied yet) 310.0115681: short by 303.8049012',
    'TRUSTLINE: no AQUA trustline (a reward claim pays AQUA and rolls back without the trustline)']);
  assert.deepEqual(r.notSupplied, ['reserveAllowanceRaw', 'creationAndConfigurationStroops', 'pilotFeesStroops', 'walletFloorStroops', 'keeperPublicAddress']);
});
test('NEGATIVE each single shortfall or missing input blocks; the principal and the floor cannot be edited down', () => {
  for (const [req, obs, pattern] of [[{}, {xlmSpendableStroops: '3500115680'}, /XLM: spendable 350\.0115680, needs 350\.0115681: short by 0\.0000001/],
    [{}, {trustlines: {...observed().trustlines, funding: line('PYUSD', '10059999999')}}, /PYUSD: holds 1005\.9999999, needs 1006\.0000000 .*short by 0\.0000001/],
    [{}, {trustlines: {...observed().trustlines, other: {code: 'USDC', present: false}}}, /no USDC trustline/], [{}, {trustlines: {...observed().trustlines, funding: {code: 'PYUSD', present: false}}}, /no PYUSD trustline/],
    [{}, {trustlines: {...observed().trustlines, reward: {...line('AQUA', '0'), authorized: false}}}, /AQUA trustline is not authorized/]]) {
    const r = F.evaluate(requirements(req), observed(obs)); assert.equal(r.status, 'BLOCKED'); assert(r.shortfalls.some(x => pattern.test(x)), JSON.stringify(r.shortfalls));
  }
  assert.equal(F.evaluate(requirements({keeperPublicAddress: null}), observed()).status, 'BLOCKED');
  assert.throws(() => F.evaluate(requirements({keeperPublicAddress: ACCOUNT}), observed()), /keeper must not be the Ledger account/);
  for (const pyusd of [{seedPrincipalRaw: '8640000000', permanentFloorRaw: '10000000', reserveAllowanceRaw: '0'}, {seedPrincipalRaw: '10000000000', permanentFloorRaw: '0', reserveAllowanceRaw: '0'}])
    assert.throws(() => F.evaluate(requirements({pyusd}), observed()), /fixed at/);
  assert.throws(() => F.evaluate(requirements({xlm: {...requirements().xlm, uploadsStroops: 310.0115681}}), observed()), /exact raw unsigned integer string/);
});
test('fake RPC: one read of the account and three trustlines; spendable XLM excludes selling liabilities', async () => {
  let calls = 0; const req = requirements();
  const rpc = {getNetwork: async () => ({passphrase: S.Networks.PUBLIC}), getLedgerEntries: async (...keys) => {calls++; assert.deepEqual(keys.map(k => k.switch().name), ['account', 'trustline', 'trustline', 'trustline']);
    return {latestLedger: 100, entries: [{key: keys[0], val: {account: () => ({balance: () => S.xdr.Int64.fromString('4100000000'), ext: () => ({switch: () => 1, v1: () => ({liabilities: () => ({selling: () => S.xdr.Int64.fromString('100000000')})})})})}},
      {key: keys[1], val: {trustLine: () => ({flags: () => 1, balance: () => S.xdr.Int64.fromString('10060000000')})}}, {key: keys[2], val: {trustLine: () => ({flags: () => 1, balance: () => S.xdr.Int64.fromString('5')})}}]};}};
  const r = await F.check(req, rpc); assert.equal(calls, 1); assert.equal(r.status, 'BLOCKED'); assert.equal(r.observed.xlmSpendableStroops, '4000000000'); assert.equal(r.observed.ledger, 100);
  assert.deepEqual(r.shortfalls, ['TRUSTLINE: no AQUA trustline (a reward claim pays AQUA and rolls back without the trustline)']);
  await assert.rejects(F.check({...req, network: S.Networks.TESTNET}, rpc));
});
