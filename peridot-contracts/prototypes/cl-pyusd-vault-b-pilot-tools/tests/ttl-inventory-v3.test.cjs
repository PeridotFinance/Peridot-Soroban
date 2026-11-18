'use strict';
// Offline tests for ttl-inventory-v3.cjs. Fake RPC only; nothing is signed or sent.
const test = require('node:test'), assert = require('node:assert/strict');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const T = require('../ttl-inventory-v3.cjs');
const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
const L = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
test('static keys: instances and codes of vault/receipt/controller plus every confirmed journal footprint key; no cash market; TTL-bearing keys only', () => {
  const keys = T.staticKeys(), list = [...keys.values()];
  for (const id of [T.V, T.R, T.CONTROLLER]) assert(keys.has(instanceKey(id).toXDR('base64')), id);
  for (const h of Object.values(T.CODES)) assert(list.some(x => x.key.switch().name === 'contractCode' && x.key.contractCode().hash().toString('hex') === h));
  assert(list.every(x => ['contractData', 'contractCode'].includes(x.key.switch().name)));
  assert(list.some(x => [...x.sources].some(s => s.startsWith('journal:bind-custody'))));
  const cash = ['CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F', 'CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS', 'CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC', 'CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4'];
  assert(!list.some(x => x.key.switch().name === 'contractData' && cash.includes(S.Address.fromScAddress(x.key.contractData().contract()).toString())));
});
test('rows: role, label, ledgers left and indicative days; absent entries are listed, not hidden', () => {
  const info = {key: instanceKey(T.V), sources: new Set(['instance'])};
  const r = T.row({liveUntilLedgerSeq: 1000 + 14897}, info, 1000, {[T.V]: 'vault'}); // 14,897 ledgers ~ 1 day at 5.8 s
  assert.equal(r.ownerRole, 'vault'); assert.equal(r.label, 'instance'); assert.equal(r.ledgersLeft, 14897); assert(Math.abs(r.approxDaysLeft - 1) < 0.01);
  const absent = T.row(undefined, info, 1000, {[T.V]: 'vault'}); assert.equal(absent.present, false); assert.equal(absent.liveUntilLedger, null);
});
function fakeRpc({restore = false, fee = '5000'} = {}) {
  const calls = {sim: 0};
  return {calls, getLatestLedger: async () => ({sequence: 100, closeTime: '1000'}), getAccount: async a => new S.Account(a, '41'),
    simulateTransaction: async tx => {calls.sim++; return {id: '1', latestLedger: 100, minResourceFee: fee, transactionData: new S.SorobanDataBuilder(tx.toEnvelope().v1().tx().ext().sorobanData().toXDR('base64')).setResourceFee(fee),
      ...(restore ? {restorePreamble: {minResourceFee: '1', transactionData: new S.SorobanDataBuilder()}} : {}), result: {auth: [], retval: S.xdr.ScVal.scvVoid()}, events: [], _parsed: true}}};
}
test('prepare-extension: unsigned, 20-ledger window, read-only footprint, warns when the Ledger is the source; refuses cash keys, archived entries, >40 keys, fee above cap', async () => {
  const keysXdr = [instanceKey(T.V).toXDR('base64'), instanceKey(T.R).toXDR('base64')];
  const out = await T.prepareExtension(fakeRpc(), {source: L, keysXdr, extendTo: 3110000});
  assert.equal(out.status, 'UNSIGNED_EXTENSION_OPERATOR_REVIEW_REQUIRED'); assert.equal(out.signing, false); assert.equal(out.submission, false);
  assert.deepEqual(out.validLedgers, {min: 101, max: 120}); assert.match(out.warning, /MOVES its sequence/);
  const tx = S.TransactionBuilder.fromXDR(out.unsignedXdr, S.Networks.PUBLIC); assert.equal(tx.signatures.length, 0); assert.equal(tx.operations[0].type, 'extendFootprintTtl');
  assert.equal(tx.toEnvelope().v1().tx().ext().sorobanData().resources().footprint().readWrite().length, 0);
  const fee = await T.prepareExtension(fakeRpc(), {source: T.KEEPER, keysXdr, extendTo: 3110000}); assert.match(fee.warning, /fee-only account/);
  await assert.rejects(T.prepareExtension(fakeRpc(), {source: L, keysXdr: [instanceKey('CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC').toXDR('base64')], extendTo: 1000}), /cash market/);
  await assert.rejects(T.prepareExtension(fakeRpc({restore: true}), {source: L, keysXdr, extendTo: 1000}), /archived/);
  await assert.rejects(T.prepareExtension(fakeRpc(), {source: L, keysXdr: Array(41).fill(keysXdr[0]), extendTo: 1000}), /at most 40/);
  await assert.rejects(T.prepareExtension(fakeRpc(), {source: L, keysXdr, extendTo: 3110401}));
  await assert.rejects(T.prepareExtension(fakeRpc({fee: '60000000'}), {source: L, keysXdr, extendTo: 1000}), /above cap/);
});
