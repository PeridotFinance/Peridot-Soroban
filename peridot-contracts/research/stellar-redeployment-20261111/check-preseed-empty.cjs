// Public getter simulations on fresh receipts and canonical SACs only.
const fs=require('node:fs'),assert=require('node:assert/strict');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const root='research/stellar-redeployment-20261111/';
const state=JSON.parse(fs.readFileSync(root+'fresh-cash-configured-20261112.json'));
assert.equal(state.result,'PASS');
const source=state.instances[0].values.Admin;
const rpc=new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm',{timeout:45});
const out={observedAt:new Date().toISOString(),scope:'unsigned public getter simulations only; no applied financial operation',rows:[]};
const save=()=>fs.writeFileSync(root+'preseed-empty-20261112.json',JSON.stringify(out,null,2)+'\n');
(async()=>{
 assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);
 const account=await rpc.getAccount(source);
 for(const p of state.instances.slice(1)){
  const calls=[['balance',p.values.UnderlyingToken,[new S.Address(p.contract).toScVal()],0n],['get_total_ptokens',p.contract,[],0n],['get_total_underlying',p.contract,[],0n],['get_exchange_rate',p.contract,[],1000000n],['get_cl_custody',p.contract,[],false]];
  for(const [method,id,args,expected] of calls){
   const tx=new S.TransactionBuilder(new S.Account(source,account.sequenceNumber()),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(id).call(method,...args)).setTimeout(300).build();
   const sim=await rpc.simulateTransaction(tx);assert(S.rpc.Api.isSimulationSuccess(sim),sim.error);assert(!sim.restorePreamble);
   const v=S.scValToNative(sim.result.retval);assert.equal(v,expected);
   out.rows.push({market:p.label,contract:id,method,ledger:sim.latestLedger,value:typeof v==='bigint'?String(v):v});save();
  }
 }
 out.result='PASS';save();console.log('PASS: all four fresh markets have zero actual underlying, zero shares/NAV, initial exchange rate 1000000, CL custody false.');
})().catch(e=>{out.result='FAIL';out.error=String(e);save();console.error(e);process.exitCode=1;});
