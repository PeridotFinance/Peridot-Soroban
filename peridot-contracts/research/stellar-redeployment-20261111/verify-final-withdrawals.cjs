// Public final withdrawal accounting verification only. Never signs or submits.
const fs=require('node:fs'),assert=require('node:assert/strict');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const root='research/stellar-redeployment-20261111/';
const state=JSON.parse(fs.readFileSync(root+'fresh-cash-configured-20261112.json'));
const seedEvidence=JSON.parse(fs.readFileSync(root+'deposit-verification-withdraw-preflight-20261112.json'));
assert.equal(seedEvidence.result,'PASS');
const before=seedEvidence.account;
const source=state.instances[0].values.Admin;
const amounts={USDC:1000000000n,XLM:6856758878n,PYUSD:1000000000n,EURC:902062548n};
const withdrawals={USDC:100000000n,XLM:685675887n,PYUSD:100000000n,EURC:90206254n};
const rpc=new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm',{timeout:45});
const out={observedAt:new Date().toISOString(),scope:'actual withdrawal transaction outcomes and public financial readbacks; no transaction submitted',markets:[]};
const save=()=>fs.writeFileSync(root+'final-withdrawal-verification-20261112.json',JSON.stringify(out,(_,v)=>typeof v==='bigint'?String(v):v,2)+'\n');
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
 assert.equal(balance(account,'XLM')-balance(before,'XLM'),withdrawals.XLM-fees);
 for(const a of ['USDC','PYUSD','EURC'])assert.equal(balance(account,a)-balance(before,a),withdrawals[a]);
 out.withdrawalFeesStroops=fees;out.walletDeltas='PASS';save();
 async function simulate(id,method,args=[]){
  const tx=new S.TransactionBuilder(new S.Account(source,account.sequence),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(id).call(method,...args)).setTimeout(300).build();
  assert.equal(tx.signatures.length,0);
  const sim=await rpc.simulateTransaction(tx);assert(S.rpc.Api.isSimulationSuccess(sim),sim.error);assert(!sim.restorePreamble);return sim;
 }
 const addr=a=>new S.Address(a).toScVal();
 for(const p of state.instances.slice(1)){
  const seed=p.label==='XLM'?50000000n:10000000n;
  const remaining=amounts[p.label]-withdrawals[p.label];
  const total=seed+remaining;
  const calls=[['get_admin',p.contract,[],source],['get_minimum_supply',p.contract,[],seed],['get_ptoken_balance',p.contract,[addr(p.contract)],seed],['get_ptoken_balance',p.contract,[addr(source)],remaining],['get_total_ptokens',p.contract,[],total],['get_total_underlying',p.contract,[],total],['get_exchange_rate',p.contract,[],1000000n],['get_total_borrowed',p.contract,[],0n],['get_cash_borrowing_enabled',p.contract,[],false],['get_min_cash_reserve_bps',p.contract,[],10000],['get_cl_custody',p.contract,[],false],['balance',p.values.UnderlyingToken,[addr(p.contract)],total]];
  const reads=[];
  for(const [method,id,args,expected] of calls){const sim=await simulate(id,method,args);const value=S.scValToNative(sim.result.retval);reads.push({method,contract:id,value,ledger:sim.latestLedger});assert.equal(value,expected,method);}
  const row={market:p.label,contract:p.contract,seedRaw:seed,depositRaw:amounts[p.label],withdrawalRaw:withdrawals[p.label],ownerSharesRaw:remaining,marketUnderlyingRaw:total,reads,withdrawalReadback:'PASS'};out.markets.push(row);save();

 }
 out.result='PASS';save();console.log(JSON.stringify({result:out.result,withdrawalFeesStroops:String(fees),walletDeltas:out.walletDeltas,markets:out.markets.map(({reads,...m})=>m)},(_,v)=>typeof v==='bigint'?String(v):v,2));
})().catch(e=>{out.result='FAIL';out.error=String(e);save();console.error(e);process.exitCode=1;});
