// Public raw reads only. No keys, signatures, submissions, or retired contracts.
const fs = require('node:fs'), assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const root = 'research/stellar-redeployment-20261111/';
const plan = JSON.parse(fs.readFileSync(root + 'fresh-instance-plan.json'));
const init = JSON.parse(fs.readFileSync(root + 'initialization-preflight-20261112.json'));
const admin = init.source, controller = plan.instances[0].expectedContract;
const mode = process.argv[2] || 'initialized';
assert(['initialized', 'guardian', 'configured', 'post-withdraw'].includes(mode));
const configured = mode === 'configured' || mode === 'post-withdraw';
const finalCash = {USDC:910000000n,XLM:6221082991n,PYUSD:910000000n,EURC:821856294n};
const rpc = new S.rpc.Server(plan.endpoint, {timeout:45});
const out = {observedAt:new Date().toISOString(), mode, instances:[]};
const encode = (id, name) => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(), key:name === '$instance' ? S.xdr.ScVal.scvLedgerKeyContractInstance() : S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(name)]), durability:S.xdr.ContractDataDurability.persistent()}));
const assets = {USDC:'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', XLM:'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA', PYUSD:'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2', EURC:'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV'};
const save = () => fs.writeFileSync(root + 'fresh-cash-' + mode + '-20261112.json', JSON.stringify(out, (_,v) => typeof v === 'bigint' ? v.toString() : v, 2) + '\n');
(async () => {
  assert.equal((await rpc.getNetwork()).passphrase, S.Networks.PUBLIC);
  for (const p of plan.instances) {
    const isController = p.label === 'controller';
    const names = ['$instance','Admin','PendingAdmin','PendingUpgradeHash','PendingUpgradeEta', ...(isController ? ['PauseGuardian','Oracle','PeridotToken','SupportedMarkets','PauseBorrow','PauseBorrowUntil','PauseDeposit','PauseRedeem'] : ['Initialized','UnderlyingToken','RatesReady','InterestModel','BoostedVault','MarginController','Peridottroller','TotalBorrowed','ManagedCash','TotalDeposited','TotalReserves','TotalAdminFees','TotalBadDebt','SupplyCap','BorrowCap'])];
    const keys = names.map(n => encode(p.expectedContract,n));
    const res = await rpc.getLedgerEntries(...keys);
    const map = new Map(res.entries.map(e => [e.key.toXDR('base64'),e]));
    const instance = map.get(keys[0].toXDR('base64')); assert(instance,'instance missing/archived');
    const cv = instance.val.contractData().val().instance();
    assert.equal(cv.executable().wasmHash().toString('hex'),p.wasmHash);
    const iv = Object.fromEntries((cv.storage() || []).map(e => [S.scValToNative(e.key())[0],S.scValToNative(e.val())]));
    const values = Object.fromEntries(names.slice(1).map((n,i) => {const e = map.get(keys[i+1].toXDR('base64')); return [n,e ? S.scValToNative(e.val.contractData().val()) : null];}));
    out.instances.push({label:p.label,contract:p.expectedContract,ledger:res.latestLedger,wasmHash:p.wasmHash,instance:iv,values}); save();
    assert.equal(values.Admin,admin);
    for (const name of ['PendingAdmin','PendingUpgradeHash','PendingUpgradeEta']) assert.equal(values[name],null,name);
    if (isController) {
      assert.equal(iv.Initialized,true);
      assert.equal(values.Oracle,null); assert.equal(values.PeridotToken,null);
      assert.equal(values.PauseGuardian, mode !== 'initialized' ? admin : null);
      if (configured) for (const market of plan.instances.slice(1)) {
        assert.equal(values.SupportedMarkets[market.expectedContract],true);
        assert.equal(values.PauseBorrow[market.expectedContract],true);
        assert.notEqual(values.PauseDeposit[market.expectedContract],true);
        assert.notEqual(values.PauseRedeem[market.expectedContract],true);
      }
    } else {
      assert.equal(values.Initialized,true); assert.equal(values.UnderlyingToken,assets[p.label]);
      assert.equal(values.RatesReady,false);
      for (const name of ['InterestModel','BoostedVault','MarginController']) assert.equal(values[name],null,name);
      assert.equal(values.Peridottroller,configured ? controller : null);
      assert.equal(iv.CashBorrowingEnabled,false);
      assert.equal(iv.MinCashReserveBps,configured ? 10000 : 0);
      for (const name of ['TotalBorrowed','ManagedCash','TotalDeposited','TotalReserves','TotalAdminFees']) {
        const expected = mode === 'post-withdraw' && ['ManagedCash','TotalDeposited'].includes(name) ? finalCash[p.label] : 0n;
        assert.notEqual(values[name],null,name+' missing');
        assert.equal(BigInt(values[name]),expected,name);
      }
      assert(values.TotalBadDebt === null || BigInt(values.TotalBadDebt) === 0n);
    }
  }
  out.result = 'PASS'; save();
  console.log('PASS: '+ mode + ' state on all five fresh instances; Ledger admin, exact hashes, no external bindings.');
})().catch(e => {out.result='FAIL';out.error=String(e);save();console.error(e);process.exitCode=1;});
