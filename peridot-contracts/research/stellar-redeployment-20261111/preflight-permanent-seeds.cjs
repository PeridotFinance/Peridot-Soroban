// Public unsigned simulations only; no seed transaction is signed or submitted.
const fs=require('node:fs'),assert=require('node:assert/strict');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const root='research/stellar-redeployment-20261111/';
const state=JSON.parse(fs.readFileSync(root+'fresh-cash-configured-20261112.json'));
const empty=JSON.parse(fs.readFileSync(root+'preseed-empty-20261112.json'));
assert.equal(state.result,'PASS');assert.equal(empty.result,'PASS');
const source=state.instances[0].values.Admin;
const rpc=new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm',{timeout:45});
const out={observedAt:new Date().toISOString(),source,scope:'independent unsigned permanent-seed preflights from current state; not applied',rows:[]};
const save=()=>fs.writeFileSync(root+'permanent-seed-preflight-20261112.json',JSON.stringify(out,null,2)+'\n');
(async()=>{
 assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);
 const account=await rpc.getAccount(source);
 for(const p of state.instances.slice(1)){
  const amount=p.label==='XLM'?50000000n:10000000n;
  const tx=new S.TransactionBuilder(new S.Account(source,account.sequenceNumber()),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(p.contract).call('seed_minimum_supply',new S.Address(source).toScVal(),S.nativeToScVal(amount,{type:'u128'}))).setTimeout(300).build();
  assert.equal(tx.signatures.length,0);
  const sim=await rpc.simulateTransaction(tx);assert(S.rpc.Api.isSimulationSuccess(sim),sim.error);assert(!sim.restorePreamble);
  assert.equal(S.scValToNative(sim.result.retval),amount);
  const r=sim.transactionData.build().resources(),fp=r.footprint();
  const auth=sim.result.auth.map(a=>a.credentials().switch().name);
  assert(auth.length>0&&auth.every(a=>a==='sorobanCredentialsSourceAccount'));
  assert(r.instructions()<=100000000&&fp.readOnly().length+fp.readWrite().length<=100&&fp.readWrite().length<=50);
  out.rows.push({market:p.label,contract:p.contract,asset:p.values.UnderlyingToken,amountRaw:String(amount),ledger:sim.latestLedger,instructions:r.instructions(),uniqueKeys:fp.readOnly().length+fp.readWrite().length,writes:fp.readWrite().length,writeBytes:r.writeBytes(),minResourceFee:sim.minResourceFee,auth,authXdr:sim.result.auth.map(a=>a.toXDR('base64')),result:'PASS'});save();
 }
 out.result='PASS';save();console.log(JSON.stringify({...out,rows:out.rows.map(({authXdr,...r})=>r)},null,2));
})().catch(e=>{out.result='FAIL';out.error=String(e);save();console.error(e);process.exitCode=1;});
