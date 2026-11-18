'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const C = require('../ledger-owner-claim.cjs');
const F = n => fs.readFileSync(path.join(__dirname, 'fixtures', n), 'utf8').trim();
// The REAL live unsigned claim built by `prepare-claim --allow-zero` (17 Nov 2026, simulated ledger 65526994, never signed):
// returned 0 AQUA because the vault had no invested position yet.
const claim = S.TransactionBuilder.fromXDR(F('claim-zero-71887f09.unsigned.xdr'), S.Networks.PUBLIC);
const claimOp = claim.toEnvelope().v1().tx().operations()[0];
const claimAuth = claimOp.body().invokeHostFunctionOp().auth();
const claimData = claim.toEnvelope().v1().tx().ext().sorobanData();
const seed = S.TransactionBuilder.fromXDR(F('seed-open-3dd02ba6.signed.xdr'), S.Networks.PUBLIC);
const seedAuth = seed.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().auth();
const clone = e => S.xdr.SorobanAuthorizationEntry.fromXDR(e.toXDR());
const OTHER = 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4';
const opFor = (contract, method, holder) => S.xdr.Operation.fromXDR(new S.Contract(contract).call(method, S.nativeToScVal(holder, {type: 'address'})).toXDR());
const entryFor = (op, subs = []) => new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
  rootInvocation: new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(op.body().invokeHostFunctionOp().hostFunction().invokeContract()), subInvocations: subs})});

test('claim pins: vault/AQUA/pool/owner as expected; reviewed ledger-owner-call.cjs hash pinned', () => {
  assert.equal(C.PINS.vault, 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63');
  assert.equal(C.PINS.vaultWasm, 'e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9');
  assert.equal(C.PINS.aqua, 'CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK');
  assert.equal(C.PINS.pool, 'CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX');
  assert.equal(C.PINS.owner, 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP');
  assert.equal(C.REVIEWED_OWNER_CALL_SHA256, '77a79a6cc96a0ece84c2b549942e9b6b533c925ac76bffc555369bae7a4f1b08');
});
test('claim auth: the REAL simulated tree is one source-account entry, root == claim_rewards(owner), nothing nested', () => {
  assert.equal(claim.hash().toString('hex'), '71887f09e5cfb1fa55b1581a3a9d100c6515b593170a1ad48a28ce8d472a553e');
  assert.equal(claimAuth.length, 1);
  assert.equal(claimAuth[0].credentials().switch().name, 'sorobanCredentialsSourceAccount');
  assert.equal(claimAuth[0].rootInvocation().subInvocations().length, 0);
  C.checkClaimAuth(claimAuth, claimOp);
  C.checkClaimShape(claim);
  assert.equal(claim.signatures.length, 0);
});
test('NEGATIVE claim auth: a nested call, a second entry, address credentials or no entry are refused', () => {
  const nested = clone(claimAuth[0]); nested.rootInvocation().subInvocations([seedAuth[0].rootInvocation().subInvocations()[0]]);
  assert.throws(() => C.checkClaimAuth([nested], claimOp), /nested/);
  assert.throws(() => C.checkClaimAuth([claimAuth[0], claimAuth[0]], claimOp), /exactly one authorization/);
  assert.throws(() => C.checkClaimAuth([], claimOp), /exactly one authorization/);
  const addrCred = clone(claimAuth[0]);
  addrCred.credentials(S.xdr.SorobanCredentials.sorobanCredentialsAddress(new S.xdr.SorobanAddressCredentials({address: new S.Address(C.PINS.owner).toScAddress(), nonce: S.xdr.Int64.fromString('1'), signatureExpirationLedger: 1, signature: S.xdr.ScVal.scvVoid()})));
  assert.throws(() => C.checkClaimAuth([addrCred], claimOp), /source-account/);
});
test('NEGATIVE claim call: another holder, the receipt instead of the vault, or another function is refused (even with matching auth)', () => {
  const other = opFor(C.PINS.vault, 'claim_rewards', OTHER);
  assert.throws(() => C.checkClaimAuth([entryFor(other)], other), /holder other than the owner/);
  assert.throws(() => C.checkClaimAuth(claimAuth, other), /holder other than the owner/);
  const receipt = opFor(C.PINS.receipt, 'claim_rewards', C.PINS.owner);
  assert.throws(() => C.checkClaimAuth([entryFor(receipt)], receipt), /pinned vault/);
  const fn = opFor(C.PINS.vault, 'reward_account', C.PINS.owner);
  assert.throws(() => C.checkClaimAuth([entryFor(fn)], fn), /not claim_rewards/);
  // Correct call but the authorized root is something else.
  assert.throws(() => C.checkClaimAuth([entryFor(fn)], claimOp), /root differs/);
  // A hand-built, correct claim passes.
  const good = opFor(C.PINS.vault, 'claim_rewards', C.PINS.owner);
  C.checkClaimAuth([entryFor(good)], good);
});
test('NEGATIVE claim shape: the seed stage_open (owner-signed, other call) is not a claim', () => {
  assert.throws(() => C.checkClaimShape(seed), /pinned vault|not claim_rewards/);
});
test('resources: the real claim counts 19 SDK entries (15 memory reads + 0 disk reads + 4 writes), 15 unique keys, 4 writes, 6.95M CPU', () => {
  const m = C.checkClaimResources(claimData, 0);
  assert.deepEqual({cpu: m.cpu, sdkEntries: m.sdkEntries, memoryReads: m.memoryReads, diskReads: m.diskReads, writes: m.writes, uniqueKeys: m.uniqueKeys},
    {cpu: 6954675, sdkEntries: 19, memoryReads: 15, diskReads: 0, writes: 4, uniqueKeys: 15});
  assert.throws(() => C.checkClaimResources(claimData, BigInt(claimData.resourceFee().toString()) + 1n), /minimum/);
});
const keyN = i => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(C.PINS.vault).toScAddress(),
  key: S.xdr.ScVal.scvU32(i), durability: S.xdr.ContractDataDurability.persistent()}));
const acctKey = () => S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: S.Keypair.fromPublicKey(OTHER).xdrPublicKey()}));
const dataWith = ({ro = 0, rw = 0, cpu = 1000, dupe = false, disk = 0}) => {
  const d = S.xdr.SorobanTransactionData.fromXDR(claimData.toXDR()), fp = d.resources().footprint();
  const r = [...Array(ro)].map((_, i) => keyN(i)), w = [...Array(rw)].map((_, i) => keyN(1000 + i));
  for (let i = 0; i < disk; i++) r.push(acctKey());
  if (dupe) w.push(keyN(0));
  fp.readOnly(r); fp.readWrite(w); d.resources().instructions(cpu); return d;
};
test('NEGATIVE resources: writes count twice toward the 100 SDK entries; unique keys, writes and CPU are checked separately', () => {
  assert.equal(C.checkClaimResources(dataWith({ro: 0, rw: 50}), 0).sdkEntries, 100);
  assert.throws(() => C.checkClaimResources(dataWith({ro: 1, rw: 50}), 0), /SDK entries/);          // 51 unique keys, 101 SDK entries
  assert.throws(() => C.checkClaimResources(dataWith({ro: 0, rw: 51}), 0), /SDK entries|writes/);
  assert.throws(() => C.checkClaimResources(dataWith({ro: 3, rw: 2, dupe: true}), 0), /duplicate/);
  assert.throws(() => C.checkClaimResources(dataWith({ro: 3, rw: 2, cpu: 97000001}), 0), /97M/);
  const m = C.checkClaimResources(dataWith({ro: 3, rw: 2, disk: 1}), 0);
  assert.deepEqual({memoryReads: m.memoryReads, diskReads: m.diskReads, writes: m.writes, sdkEntries: m.sdkEntries, uniqueKeys: m.uniqueKeys},
    {memoryReads: 5, diskReads: 1, writes: 2, sdkEntries: 8, uniqueKeys: 6});
});
test('return: a simulated 0 is refused unless --allow-zero; a positive u128 passes; non-u128 is refused', () => {
  assert.throws(() => C.checkReturn(0n), /returns 0 AQUA/);
  assert.equal(C.checkReturn(0n, {allowZero: true}), 0n);
  assert.equal(C.checkReturn(1234567n), 1234567n);
  assert.throws(() => C.checkReturn(5), /u128/);
  assert.throws(() => C.checkReturn(undefined, {allowZero: true}), /u128/);
});
const tl = over => ({asset_type: 'credit_alphanum4', asset_code: 'AQUA', asset_issuer: C.PINS.aquaIssuer, balance: '0.0000000', limit: '922337203685.4775807',
  buying_liabilities: '0.0000000', is_authorized: true, ...over});
test('trustline: the AQUA trustline must exist, be authorized and have limit headroom', () => {
  assert.deepEqual(C.checkTrustline([tl()], 10n), {balance: '0.0000000', limit: '922337203685.4775807', authorized: true});
  assert.throws(() => C.checkTrustline([], 0n), /no AQUA trustline/);
  assert.throws(() => C.checkTrustline([tl({asset_issuer: OTHER})], 0n), /no AQUA trustline/);
  assert.throws(() => C.checkTrustline([tl({is_authorized: false})], 0n), /not authorized/);
  assert.throws(() => C.checkTrustline([tl({limit: '1.0000000', balance: '0.9999999'})], 2n), /limit/);
});
test('readback: delta == return == paid delta, return >= simulated (accrual until inclusion); any mismatch is reported', () => {
  const base = {simulated: '1000', returned: '1004', holderBefore: '50', holderAfter: '1054', paidBefore: '7', paidAfter: '1011', pendingAfter: '0'};
  const ok = C.compareReadback(base);
  assert.equal(ok.status, 'READBACK_OK'); assert.equal(ok.aquaRaw.aboveSimulation, '4'); assert.equal(ok.aqua.returned, '0.0001004');
  assert.equal(C.compareReadback({...base, returned: '1000', holderAfter: '1050', paidAfter: '1007'}).status, 'READBACK_OK');
  for (const [k, v, check] of [['holderAfter', '1053', 'balanceDeltaEqualsReturn'], ['paidAfter', '1010', 'paidDeltaEqualsReturn'],
    ['pendingAfter', '1', 'pendingCleared']]) {
    const r = C.compareReadback({...base, [k]: v}); assert.equal(r.status, 'READBACK_MISMATCH'); assert.equal(r.checks[check], false, k);
  }
  const low = C.compareReadback({...base, returned: '999', holderAfter: '1049', paidAfter: '1006'});
  assert.equal(low.status, 'READBACK_MISMATCH'); assert.equal(low.checks.returnAtLeastSimulated, false);
});
test('verify-signed: an owner-signed transaction that is not this claim is refused; an unsigned copy is refused', () => {
  assert.throws(() => C.verifySigned(F('claim-zero-71887f09.unsigned.xdr'), F('seed-open-3dd02ba6.signed.xdr')), /differs/);
  assert.throws(() => C.verifySigned(F('claim-zero-71887f09.unsigned.xdr'), F('claim-zero-71887f09.unsigned.xdr')), /one signature/);
  // The real owner-signed seed pair verifies as a signature, but is still refused because it is not claim_rewards(owner).
  assert.throws(() => C.verifySigned(F('seed-open-3dd02ba6.unsigned.xdr'), F('seed-open-3dd02ba6.signed.xdr')), /pinned vault|not claim_rewards/);
});
test('aquaDecimal formats 7-decimal raw amounts', () => {
  assert.equal(C.aquaDecimal(0n), '0.0000000'); assert.equal(C.aquaDecimal(123456789n), '12.3456789'); assert.equal(C.aquaDecimal('10000000'), '1.0000000');
});
