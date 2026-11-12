// Public state reads and independent unsigned simulations only; never signs/sends.
const fs = require('node:fs');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const root = 'research/stellar-redeployment-20261111/';
const plan = JSON.parse(fs.readFileSync(root + 'fresh-instance-plan.json'));
const source = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
const assets = {
  USDC: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
  XLM: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
  PYUSD: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2',
  EURC: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV',
};
const rpc = new S.rpc.Server(plan.endpoint, {timeout: 45});
const out = {observedAt: new Date().toISOString(), source, scope: 'public reads; independent unsigned initialization preflights, not applied', instances: []};
const save = () => fs.writeFileSync(root + 'initialization-preflight-20261112.json', JSON.stringify(out, null, 2) + '\n');
const key = (id, v) => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(), key: v, durability: S.xdr.ContractDataDurability.persistent()}));
(async () => {
  assert.equal((await rpc.getNetwork()).passphrase, S.Networks.PUBLIC);
  const account = await rpc.getAccount(source);
  for (const p of plan.instances) {
    const instanceKey = key(p.expectedContract, S.xdr.ScVal.scvLedgerKeyContractInstance());
    const adminKey = key(p.expectedContract, S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Admin')]));
    const initializedKey = key(p.expectedContract, S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Initialized')]));
    const read = await rpc.getLedgerEntries(instanceKey, adminKey, initializedKey);
    assert.equal(read.entries.length, 1, 'expected only uninitialized instance');
    assert.equal(read.entries[0].key.toXDR('base64'), instanceKey.toXDR('base64'));
    const instance = read.entries[0].val.contractData().val().instance();
    assert.equal(instance.executable().wasmHash().toString('hex'), p.wasmHash);
    const args = p.label === 'controller' ? [new S.Address(source).toScVal()] : [new S.Address(assets[p.label]).toScVal(), S.nativeToScVal(0n, {type: 'u128'}), S.nativeToScVal(0n, {type: 'u128'}), new S.Address(source).toScVal()];
    const tx = new S.TransactionBuilder(new S.Account(source, account.sequenceNumber()), {fee: '100', networkPassphrase: S.Networks.PUBLIC}).addOperation(new S.Contract(p.expectedContract).call('initialize', ...args)).setTimeout(300).build();
    assert.equal(tx.signatures.length, 0);
    const sim = await rpc.simulateTransaction(tx);
    assert(S.rpc.Api.isSimulationSuccess(sim), sim.error);
    assert(!sim.restorePreamble, 'restore required');
    const d = sim.transactionData.build(), r = d.resources(), fp = r.footprint();
    const auth = sim.result.auth.map(a => a.credentials().switch().name);
    assert(auth.length > 0 && auth.every(a => a === 'sorobanCredentialsSourceAccount'));
    assert(r.instructions() <= 100000000 && fp.readOnly().length + fp.readWrite().length <= 100 && fp.readWrite().length <= 50);
    out.instances.push({label: p.label, contract: p.expectedContract, wasmHash: p.wasmHash, exactExecutableMatch: true, uninitialized: true, readLedger: read.latestLedger, liveUntilLedgerSeq: read.entries[0].liveUntilLedgerSeq, simulationLedger: sim.latestLedger, instructions: r.instructions(), uniqueKeys: fp.readOnly().length + fp.readWrite().length, writes: fp.readWrite().length, writeBytes: r.writeBytes(), minResourceFeeStroops: sim.minResourceFee, auth, result: S.scValToNative(sim.result.retval), transactionDataXdr: d.toXDR('base64')});
    save();
  }
  out.totalEstimatedFeeIncludingNominalInclusionStroops = out.instances.reduce((n, r) => n + BigInt(r.minResourceFeeStroops) + 100n, 0n).toString();
  save();
  console.log(JSON.stringify(out, null, 2));
})().catch(e => {out.error = String(e); save(); console.error(e); process.exitCode = 1;});
