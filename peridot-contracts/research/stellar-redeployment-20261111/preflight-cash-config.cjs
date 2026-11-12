// Independent unsigned simulations only, before configuration. Never signs/sends.
const fs=require('node:fs'), assert=require('node:assert/strict');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const root='research/stellar-redeployment-20261111/';
const plan=JSON.parse(fs.readFileSync(root+'fresh-instance-plan.json'));
const source='GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
const controller=plan.instances[0].expectedContract;
const rpc=new S.rpc.Server(plan.endpoint,{timeout:45});
const addr=a=>new S.Address(a).toScVal();
const out={observedAt:new Date().toISOString(),scope:'independent unsigned prestate simulations; pause calls require listing first and are deferred to operator simulation',rows:[]};
const save=()=>fs.writeFileSync(root+'configuration-preflight-20261112.json',JSON.stringify(out,null,2)+'\n');
(async()=>{
 assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);
 const account=await rpc.getAccount(source);
 const calls=[['controller',controller,'set_pause_guardian',[addr(source)]]];
 for(const p of plan.instances.slice(1)) calls.push(
  [p.label,p.expectedContract,'set_min_cash_reserve_bps',[addr(source),S.nativeToScVal(10000,{type:'u32'})]],
  [p.label,p.expectedContract,'set_cash_borrowing_enabled',[addr(source),S.nativeToScVal(false)]],
  [p.label,controller,'add_market',[addr(p.expectedContract)]],
  [p.label,p.expectedContract,'set_peridottroller',[addr(controller)]]);
 for(const [label,id,method,args] of calls){
  const tx=new S.TransactionBuilder(new S.Account(source,account.sequenceNumber()),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(id).call(method,...args)).setTimeout(300).build();
  const sim=await rpc.simulateTransaction(tx);
  const row={label,contract:id,method,ledger:sim.latestLedger};
  if(!S.rpc.Api.isSimulationSuccess(sim)){row.error=sim.error;row.result='FAIL';out.rows.push(row);save();continue;}
  const r=sim.transactionData.build().resources(),fp=r.footprint();
  Object.assign(row,{instructions:r.instructions(),uniqueKeys:fp.readOnly().length+fp.readWrite().length,writes:fp.readWrite().length,writeBytes:r.writeBytes(),minResourceFee:sim.minResourceFee,restore:!!sim.restorePreamble,auth:sim.result.auth.map(a=>a.credentials().switch().name)});
  assert(row.instructions<=100000000&&row.uniqueKeys<=100&&row.writes<=50&&!row.restore);
  assert(row.auth.length>0&&row.auth.every(a=>a==='sorobanCredentialsSourceAccount'));
  row.result='PASS';out.rows.push(row);save();
 }
 out.result=out.rows.every(r=>r.result==='PASS')?'PASS':'FAIL';save();console.log(JSON.stringify(out,null,2));
})().catch(e=>{out.error=String(e);save();console.error(e);process.exitCode=1;});
