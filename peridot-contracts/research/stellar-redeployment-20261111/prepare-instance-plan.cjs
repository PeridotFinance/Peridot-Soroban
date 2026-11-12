// Exact fresh-instance derivation and public simulation only. No signatures/send.
const fs=require('node:fs'),c=require('node:crypto'),cp=require('node:child_process'),S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const assert=require('node:assert/strict');
const root='research/stellar-redeployment-20261111/';
const source='GDBS26DYO2FMMLXGFPZ6JX3ZTHJBM2FVAAKD34J7K6ZWVKGLLANZTOYE';
const endpoint='https://soroban-rpc.mainnet.stellar.gateway.fm';
const rpc=new S.rpc.Server(endpoint,{timeout:45});
const pins=JSON.parse(fs.readFileSync(root+'medium-final-build-hashes.json'));
const out={observedAt:new Date().toISOString(),source,endpoint,scope:'independent unsigned simulations from one prestate; actual deployments must build sequentially with fresh sequence',code:[],instances:[]};
const save=()=>fs.writeFileSync(root+'fresh-instance-plan.json',JSON.stringify(out,null,2)+'\n');
(async()=>{
 assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);
 const account=await rpc.getAccount(source);out.sourceSequence=account.sequenceNumber();
 for(const name of ['simple_peridottroller.wasm','receipt_vault.wasm']){
  const hash=pins[name].sha256;
  const key=S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:Buffer.from(hash,'hex')}));
  const r=await rpc.getLedgerEntries(key);assert.equal(r.entries.length,1,'uploaded code missing');
  const wasm=r.entries[0].val.contractCode().code();assert.equal(c.createHash('sha256').update(wasm).digest('hex'),hash);assert(wasm.equals(fs.readFileSync('target/lending-medium-final-20261111/artifacts/'+name)));
  out.code.push({name,sha256:hash,bytes:wasm.length,ledger:r.latestLedger,liveUntilLedgerSeq:r.entries[0].liveUntilLedgerSeq,exactLocalMatch:true});save();
 }
 for(const label of ['controller','USDC','XLM','PYUSD','EURC']){
  const name=label==='controller'?'simple_peridottroller.wasm':'receipt_vault.wasm';
  const salt=c.createHash('sha256').update('peridot-fresh-cash-mainnet-20261111:'+source+':'+label).digest();
  const expected=cp.execFileSync('stellar',['contract','id','wasm','--source-account',source,'--salt',salt.toString('hex'),'--rpc-url',endpoint,'--network-passphrase',S.Networks.PUBLIC],{encoding:'utf8',stdio:['ignore','pipe','pipe']}).trim();
  assert(/^C[A-Z2-7]{55}$/.test(expected));assert(!fs.readFileSync('addresses.md','utf8').includes(expected));
  const key=S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(expected).toScAddress(),key:S.xdr.ScVal.scvLedgerKeyContractInstance(),durability:S.xdr.ContractDataDurability.persistent()}));
  const existence=await rpc.getLedgerEntries(key);assert.equal(existence.entries.length,0,'planned instance already exists; reconcile, do not redeploy');
  const tx=new S.TransactionBuilder(new S.Account(source,account.sequenceNumber()),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(S.Operation.createCustomContract({address:new S.Address(source),wasmHash:Buffer.from(pins[name].sha256,'hex'),salt,constructorArgs:[]})).setTimeout(300).build();
  const sim=await rpc.simulateTransaction(tx);assert(S.rpc.Api.isSimulationSuccess(sim),sim.error);assert(!sim.restorePreamble,'unexpected restore');assert.equal(S.scValToNative(sim.result.retval),expected);
  const d=sim.transactionData.build(),r=d.resources(),fp=r.footprint();
  const auth=sim.result.auth.map(a=>a.credentials().switch().name);assert(auth.every(a=>a==='sorobanCredentialsSourceAccount'));
  out.instances.push({label,wasmHash:pins[name].sha256,salt:salt.toString('hex'),expectedContract:expected,absentBeforeDeployment:true,simulationLedger:sim.latestLedger,feeWithNominalInclusionStroops:(BigInt(sim.minResourceFee)+100n).toString(),minResourceFeeStroops:sim.minResourceFee,instructions:r.instructions(),uniqueKeys:fp.readOnly().length+fp.readWrite().length,writes:fp.readWrite().length,writeBytes:r.writeBytes(),auth,transactionDataXdr:d.toXDR('base64')});save();
 }
 out.totalFeeWithNominalInclusionStroops=out.instances.reduce((s,r)=>s+BigInt(r.feeWithNominalInclusionStroops),0n).toString();save();console.log(JSON.stringify(out,null,2));
})().catch(e=>{out.error=String(e);save();console.error(String(e));process.exitCode=1});
