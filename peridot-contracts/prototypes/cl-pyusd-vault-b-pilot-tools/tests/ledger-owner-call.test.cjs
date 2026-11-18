'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const L = require('../ledger-owner-call.cjs');
const F = n => fs.readFileSync(path.join(__dirname, 'fixtures', n), 'utf8').trim();
// The REAL seed stage_open (3dd02ba6…, SUCCESS ledger 65507068, 16 Nov 2026): id 0, 800 PYUSD, reserve 38,592.
const seed = S.TransactionBuilder.fromXDR(F('seed-open-3dd02ba6.signed.xdr'), S.Networks.PUBLIC);
const seedOp = seed.toEnvelope().v1().tx().operations()[0];
const seedAuth = seedOp.body().invokeHostFunctionOp().auth();
const seedQuote = JSON.parse(F('seed-open-3dd02ba6.quote.json'));
const clone = e => S.xdr.SorobanAuthorizationEntry.fromXDR(e.toXDR());

test('open auth: the real seed stage_open authorization tree passes with its own id, principal and reserve', () => {
  assert.equal(seed.hash().toString('hex'), '3dd02ba6589e50ce30520647c0acc64a9170e3c01cb0c2bac9cdd4cf3860f03b');
  L.checkOpenAuth(seedAuth, seedOp, {id: 0, principal: 8000000000n, reserve: 38592n});
});
test('NEGATIVE open auth: another id, reserve, principal or owner is refused', () => {
  for (const bad of [{id: 1, principal: 8000000000n, reserve: 38592n}, {id: 0, principal: 8000000000n, reserve: 38593n},
    {id: 0, principal: 7999999999n, reserve: 38592n}, {id: 0, principal: 8000000000n, reserve: 38592n, owner: 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4'}])
    assert.throws(() => L.checkOpenAuth(seedAuth, seedOp, bad), undefined, JSON.stringify(bad, (_, x) => typeof x === 'bigint' ? String(x) : x));
});
test('NEGATIVE open auth: an extra nested call, a second entry, or address credentials are refused', () => {
  const extra = clone(seedAuth[0]); const root = extra.rootInvocation();
  root.subInvocations([...root.subInvocations(), root.subInvocations()[0]]);
  assert.throws(() => L.checkOpenAuth([extra], seedOp, {id: 0, principal: 8000000000n, reserve: 38592n}), /exactly one nested/);
  const deeper = clone(seedAuth[0]); const t = deeper.rootInvocation().subInvocations()[0].subInvocations()[0];
  t.subInvocations([t]);
  assert.throws(() => L.checkOpenAuth([deeper], seedOp, {id: 0, principal: 8000000000n, reserve: 38592n}));
  assert.throws(() => L.checkOpenAuth([seedAuth[0], seedAuth[0]], seedOp, {id: 0, principal: 8000000000n, reserve: 38592n}), /exactly one authorization/);
  const addrCred = clone(seedAuth[0]);
  addrCred.credentials(S.xdr.SorobanCredentials.sorobanCredentialsAddress(new S.xdr.SorobanAddressCredentials({address: new S.Address(L.PINS.owner).toScAddress(), nonce: S.xdr.Int64.fromString('1'), signatureExpirationLedger: 1, signature: S.xdr.ScVal.scvVoid()})));
  assert.throws(() => L.checkOpenAuth([addrCred], seedOp, {id: 0, principal: 8000000000n, reserve: 38592n}), /source-account/);
});
test('NEGATIVE open auth: the authorized root must be the operation itself', () => {
  const other = new S.Contract(L.PINS.receipt).call('stage_open', S.nativeToScVal(L.PINS.owner, {type: 'address'}),
    ...[8000000000n, 1n, 1n, 1n, 38592n].map(x => S.nativeToScVal(x, {type: 'u128'})), S.nativeToScVal(1n, {type: 'u64'}));
  const otherOp = S.xdr.Operation.fromXDR(other.toXDR());
  assert.throws(() => L.checkOpenAuth(seedAuth, otherOp, {id: 0, principal: 8000000000n, reserve: 38592n}), /root differs/);
});
test('open quote: the real seed quote passes the 20 bps / +50% rules; any drift is refused', () => {
  L.checkOpenQuote(seedQuote, {ledger: seedQuote.ledger});
  for (const [k, v] of [['min_swap', String(BigInt(seedQuote.min_swap) - 1n)], ['min_shares', String(BigInt(seedQuote.min_shares) + 1n)], ['reserve', '38591'],
    ['in_band', false], ['oracle_fresh', false], ['deposits_paused', true], ['composition_dirty', true], ['admissible', false], ['vault_slippage_bps', 150]])
    assert.throws(() => L.checkOpenQuote({...seedQuote, [k]: v}, {ledger: seedQuote.ledger}), undefined, k);
  assert.throws(() => L.checkOpenQuote(seedQuote, {ledger: seedQuote.ledger + 1}), /simulated ledger/);
});
// Live values of 16 Nov 16:42 UTC (ledger 65512256): full position 7,996,448,370 units.
const closeQ = {ledger: 65512256, min_swap: '3671453751', out0: '4313787926', out1: '3681941400', shares: '7996448370', swap_out: '3678811374', tick: 0,
  timestamp: '1794847322', total: '7992599300', valid: true, vault_divergence_bps: 200, vault_slippage_bps: 100};
test('close quote: full position, 20 bps floor, same ledger; partial, drifted or invalid quotes are refused', () => {
  L.checkCloseQuote(closeQ, {units: 7996448370n, ledger: 65512256});
  assert.throws(() => L.checkCloseQuote(closeQ, {units: 7996448371n, ledger: 65512256}), /full position/);
  assert.throws(() => L.checkCloseQuote({...closeQ, min_swap: '3671453750'}, {units: 7996448370n, ledger: 65512256}), /20 bps/);
  assert.throws(() => L.checkCloseQuote({...closeQ, valid: false}, {units: 7996448370n, ledger: 65512256}));
  assert.throws(() => L.checkCloseQuote(closeQ, {units: 7996448370n, ledger: 65512257}));
});
test('close auth: source-account root equal to the operation, nothing nested', () => {
  const op = S.xdr.Operation.fromXDR(new S.Contract(L.PINS.receipt).call('stage_prepare_close', S.nativeToScVal(L.PINS.owner, {type: 'address'}),
    S.nativeToScVal(7996448370n, {type: 'u128'}), S.nativeToScVal(3671453751n, {type: 'u128'}), S.nativeToScVal(1794850000n, {type: 'u64'})).toXDR());
  const fn = op.body().invokeHostFunctionOp().hostFunction().invokeContract();
  const entry = subs => new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
    rootInvocation: new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(fn), subInvocations: subs})});
  L.checkCloseAuth([entry([])], op);
  assert.throws(() => L.checkCloseAuth([entry([seedAuth[0].rootInvocation().subInvocations()[0]])], op), /nested/);
  assert.throws(() => L.checkCloseAuth([], op), /exactly one/);
});
test('idle and deadline gates', () => {
  L.checkIdle({open_jobs: 0}, []);
  assert.throws(() => L.checkIdle({open_jobs: 1}, []), /open command/);
  assert.throws(() => L.checkIdle({open_jobs: 0}, [{id: 1}]), /open commands/);
  L.checkDeadline(1794850000n, 1794847322, 240);
  assert.throws(() => L.checkDeadline(1794847322n + 3601n, 1794847322, 240), /3600/);
  assert.throws(() => L.checkDeadline(1794847322n + 100n, 1794847322, 240), /validity/);
  assert.throws(() => L.checkDeadline(1794847322n, 1794847322, 240));
});
test('verify-signed: the real seed signature verifies against the owner key; any other transaction is refused', () => {
  const r = L.verifySigned(F('seed-open-3dd02ba6.unsigned.xdr'), F('seed-open-3dd02ba6.signed.xdr'));
  assert.equal(r.status, 'SIGNED_OK'); assert.equal(r.hash, '3dd02ba6589e50ce30520647c0acc64a9170e3c01cb0c2bac9cdd4cf3860f03b');
  const probe = F('probe-failed-be05d42d.envelope.xdr');   // keeper-signed, other transaction
  assert.throws(() => L.verifySigned(F('seed-open-3dd02ba6.unsigned.xdr'), probe), /differs/);
  assert.throws(() => L.verifySigned(F('seed-open-3dd02ba6.unsigned.xdr'), F('seed-open-3dd02ba6.unsigned.xdr')), /one signature/);
});
test('prepare-step rule (no authorization entries) matches the real confirmed Ledger stage_cancel 8addeee1… (SUCCESS 65508497)', () => {
  const t = S.TransactionBuilder.fromXDR(F('cancel-8addeee1.envelope.xdr'), S.Networks.PUBLIC);
  const op = t.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp();
  assert.equal(op.hostFunction().invokeContract().functionName().toString(), 'stage_cancel');
  assert.equal(op.auth().length, 0);
});
