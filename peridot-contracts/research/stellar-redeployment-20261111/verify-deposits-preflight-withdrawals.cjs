// Public deposit verification and unsigned withdrawal preflights only. Never signs or submits.
const fs=require('node:fs'),assert=require('node:assert/strict');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const root='research/stellar-redeployment-20261111/';
const state=JSON.parse(fs.readFileSync(root+'fresh-cash-configured-20261112.json'));
const seedEvidence=JSON.parse(fs.readFileSync(root+'seed-verification-deposit-preflight-20261112.json'));
assert.equal(seedEvidence.result,'PASS');
const before=seedEvidence.account;
const source=state.instances[0].values.Admin;
const amounts={USDC:1000000000n,XLM:6856758878n,PYUSD:1000000000n,EURC:902062548n};
const withdrawals={USDC:100000000n,XLM:685675887n,PYUSD:100000000n,EURC:90206254n};
const rpc=new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm',{timeout:45});
const out={observedAt:new Date().toISOString(),scope:'actual deposit public readbacks; independent unsigned withdrawal preflights, not applied',markets:[]};
const save=()=>fs.writeFileSync(root+'deposit-verification-withdraw-preflight-20261112.json',JSON.stringify(out,(_,v)=>typeof v==='bigint'?String(v):v,2)+'\n');
const raw=s=>BigInt(s.replace('.',''));
const balance=(a,label)=>raw(a.balances.find(b=>label==='XLM'?b.asset_type==='native':b.asset_code===label).balance);
(async()=>{
 assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);
 const responses=await Promise.all([fetch('https://horizon.stellar.org/accounts/'+source),fetch('https://horizon.stellar.org/accounts/'+source+'/transactions?order=desc&limit=10')]);
 assert(responses.every(r=>r.ok));
 const [account,history]=await Promise.all(responses.map(r=>r.json()));
 out.account={account_id:account.account_id,sequence:account.sequence,balances:account.balances,subentry_count:account.subentry_count};
 out.transactions=history._embedded.records.filter(r=>r.source_account===source&&BigInt(r.source_account_sequence)>BigInt(before.sequence)).map(r=>Object.fromEntries(['hash','successful','ledger','created_at','source_account','source_account_sequence','fee_charged'].map(k=>[k,r[k]])));save();
 assert.equal(out.transactions.length,4,'reconcile additional or missing account transactions');
 assert(out.transactions.every(t=>t.successful&&t.source_account===source));
 const fees=out.transactions.reduce((n,t)=>n+BigInt(t.fee_charged),0n);
 assert.equal(account.subentry_count,before.subentry_count);
 assert.equal(balance(before,'XLM')-balance(account,'XLM'),amounts.XLM+fees);
 for(const a of ['USDC','PYUSD','EURC'])assert.equal(balance(before,a)-balance(account,a),amounts[a]);
 out.depositFeesStroops=fees;out.walletDeltas='PASS';save();
 async function simulate(id,method,args=[]){
  const tx=new S.TransactionBuilder(new S.Account(source,account.sequence),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(id).call(method,...args)).setTimeout(300).build();
  assert.equal(tx.signatures.length,0);
  const sim=await rpc.simulateTransaction(tx);assert(S.rpc.Api.isSimulationSuccess(sim),sim.error);assert(!sim.restorePreamble);return sim;
 }
 const addr=a=>new S.Address(a).toScVal();
 for(const p of state.instances.slice(1)){
  const seed=p.label==='XLM'?50000000n:10000000n;
  const total=seed+amounts[p.label];
  const calls=[['get_admin',p.contract,[],source],['get_minimum_supply',p.contract,[],seed],['get_ptoken_balance',p.contract,[addr(p.contract)],seed],['get_ptoken_balance',p.contract,[addr(source)],amounts[p.label]],['get_total_ptokens',p.contract,[],total],['get_total_underlying',p.contract,[],total],['get_exchange_rate',p.contract,[],1000000n],['get_total_borrowed',p.contract,[],0n],['get_cash_borrowing_enabled',p.contract,[],false],['get_min_cash_reserve_bps',p.contract,[],10000],['get_cl_custody',p.contract,[],false],['balance',p.values.UnderlyingToken,[addr(p.contract)],total]];
  const reads=[];
  for(const [method,id,args,expected] of calls){const sim=await simulate(id,method,args);const value=S.scValToNative(sim.result.retval);reads.push({method,contract:id,value,ledger:sim.latestLedger});assert.equal(value,expected,method);}
  const row={market:p.label,contract:p.contract,seedRaw:seed,depositRaw:amounts[p.label],reads,depositReadback:'PASS'};out.markets.push(row);save();
  const sim=await simulate(p.contract,'withdraw',[addr(source),S.nativeToScVal(withdrawals[p.label],{type:'u128'})]);
  const r=sim.transactionData.build().resources(),fp=r.footprint(),auth=sim.result.auth.map(a=>a.credentials().switch().name);
  assert(auth.length>0&&auth.every(a=>a==='sorobanCredentialsSourceAccount'));
  assert(r.instructions()<=100000000&&fp.readOnly().length+fp.readWrite().length<=100&&fp.readWrite().length<=50);
  row.withdrawPreflight={burnRaw:withdrawals[p.label],expectedPayoutRaw:withdrawals[p.label],ledger:sim.latestLedger,instructions:r.instructions(),uniqueKeys:fp.readOnly().length+fp.readWrite().length,writes:fp.readWrite().length,writeBytes:r.writeBytes(),minResourceFee:sim.minResourceFee,auth,authXdr:sim.result.auth.map(a=>a.toXDR('base64')),result:'PASS'};save();
 }
 out.result='PASS';save();console.log(JSON.stringify({result:out.result,depositFeesStroops:String(fees),walletDeltas:out.walletDeltas,markets:out.markets.map(m=>({market:m.market,depositReadback:m.depositReadback,withdrawPreflight:{...m.withdrawPreflight,authXdr:undefined}}))},(_,v)=>typeof v==='bigint'?String(v):v,2));
})().catch(e=>{out.result='FAIL';out.error=String(e);save();console.error(e);process.exitCode=1;});
