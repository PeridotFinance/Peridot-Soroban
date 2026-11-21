// Fee-only TTL keeper. User authorized deployer signing ONLY for allowlisted
// native ExtendFootprintTTLOp transactions after the first-half deposit gate.
// No financial invocation, restore, deployment, upgrade, payment, or role change.
// This job has no use for unrelated credentials inherited from the GUI session.
//
// Repair 2026-11-21 (user approval "TTL: a, b, c"):
//  a) Reviewed upgrade code is on an explicit dated HOLD (never extended, see
//     UPGRADE_CODE_HOLD); guards() still accepts it as live code.
//  b) Event-index + state-probe discovery (cash_ttl_inventory.cjs); runs while
//     borrowing is live (the fee-only/allowlist invariant is validate(), not
//     market state); urgency-ordered partial runs under a rolling 24h cap.
//  c) Sealed code is dropped from extension automatically once no live
//     instance (and no pending upgrade) uses it, read on chain every run.
delete process.env.ALMANAX_API_KEY;
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),cp=require('node:child_process');
const S=require('../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const inventory=require('./cash_ttl_inventory.cjs'),{DIR,manifest,endpoint,ROOT,describe,UPGRADE_CODE,SEALED_CODE}=inventory;
const {firstHalves,read}=require('./cash_operator_checks.cjs');
const SOURCE='GDBS26DYO2FMMLXGFPZ6JX3ZTHJBM2FVAAKD34J7K6ZWVKGLLANZTOYE';
const ALIAS='peridot-cash-fresh-20261111',CLI='/Users/joshua/.local/bin/stellar';
// MAX_RUN is a ROLLING 24-hour ceiling (sum of fees charged by this keeper's
// confirmed transactions in the last 24h plus the current run's allowances).
const EXTEND=2592000,THRESHOLD=2419200,BATCH=30,MAX_TX=400000000n,MAX_RUN=800000000n,MIN_REMAINDER=100000000n,DAY_MS=86400000;
// (a) HOLD, 2026-11-21: the reviewed cash-borrow upgrade code (controller
// f41e5260..., receipt 5630a701...) is NOT extended for now; extending both to
// 150 days would cost ~60-72 XLM today. On 2026-11-21 each had ~2,019,000
// ledgers (~117 days) left, archiving ~2027-03-18. RE-ADD in ~1 month: on or
// after 2026-12-21 the user decides, then set `enabled:true` here in a reviewed
// commit. Nothing re-enables this automatically. Alerts: LIVE_CODE_TTL_LOW when
// live code has < 1,500,000 ledgers (~87 days; expected ~2026-12-21) and
// UPGRADE_CODE_HOLD_REVIEW_DUE from reviewOn. Measured 2026-11-21: about
// 4.78e-5 XLM/ledger (controller) and 5.80e-5 XLM/ledger (receipt) of
// extension. At ~1,500,000 ledgers left, extending to 2,592,000 needs ~63 and
// ~76 XLM of allowance: each exceeds MAX_TX (40 XLM) and together MAX_RUN
// (80 XLM/24h), so setting enabled:true ALONE would only defer them. A re-add
// also needs a user-approved cap change (or a shorter EXTEND for code) and
// ~150 XLM in the deployer (allowance + 10 XLM remainder). Ongoing cost for
// both codes is ~55 XLM per 30 days.
const UPGRADE_CODE_HOLD={enabled:false,heldOn:'2026-11-21',reviewOn:'2026-12-21',alertBelowLedgers:1500000};
const rpc=new S.rpc.Server(endpoint,{timeout:45});
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?String(v):v,2)+'\n';
const save=(p,v)=>{fs.writeFileSync(p+'.tmp',json(v));fs.renameSync(p+'.tmp',p);};
function validate(tx,keys,{extendableCode=new Set()}={}){
 assert.equal(tx.networkPassphrase,S.Networks.PUBLIC);assert.equal(tx.source,SOURCE);assert.equal(tx.operations.length,1);
 const op=tx.operations[0];assert.equal(op.type,'extendFootprintTtl');assert(!op.source||op.source===SOURCE);assert.equal(op.extendTo,EXTEND);
 assert(BigInt(tx.fee)<=MAX_TX);assert.equal(tx.memo.type,'none');
 const data=tx.toEnvelope().v1().tx().ext().sorobanData(),fp=data.resources().footprint();
 assert.equal(fp.readWrite().length,0);assert(fp.readOnly().length>0&&fp.readOnly().length<=BATCH);
 const actual=fp.readOnly().map(k=>k.toXDR('base64'));assert.deepEqual([...actual].sort(),[...keys].sort());assert.equal(new Set(actual).size,actual.length);
 for(const k of fp.readOnly()){const d=describe(k);assert(d,'unapproved footprint');if(d.kind==='code')assert(extendableCode.has(d.hash),'code '+d.hash.slice(0,8)+' is approved but not extendable (held or unused)');}
 assert(Number(tx.timeBounds.maxTime)>0&&Number(tx.timeBounds.maxTime)-Date.now()/1000<=360);
 assert(data.resources().instructions()<=100000000);
}
// (c) + (a): which approved code may be extended this run. Pure; inputs read on chain.
function codeExtensionPolicy({liveCode,pendingTargets=new Set(),readsComplete,hold=UPGRADE_CODE_HOLD,codeTtl=new Map(),today=new Date().toISOString().slice(0,10)}){
 const extendable=new Set(),decisions=[],alerts=[];const used=new Set([...liveCode.values(),...pendingTargets]);
 for(const [role,h] of Object.entries(SEALED_CODE)){
  if(!readsComplete){extendable.add(h);decisions.push({hash:h,role,release:'sealed',decision:'extend',reason:'instance reads incomplete: keep extending'});}
  else if(used.has(h)){extendable.add(h);decisions.push({hash:h,role,release:'sealed',decision:'extend',reason:'used by a live instance or pending upgrade'});}
  else decisions.push({hash:h,role,release:'sealed',decision:'dropped',reason:'no live allowlisted instance and no pending upgrade uses it (read on chain)'});
 }
 for(const [role,h] of Object.entries(UPGRADE_CODE)){
  if(!hold.enabled)decisions.push({hash:h,role,release:'upgrade',decision:'held',reason:'UPGRADE_CODE_HOLD since '+hold.heldOn+'; review on '+hold.reviewOn});
  else if(!readsComplete||used.has(h)){extendable.add(h);decisions.push({hash:h,role,release:'upgrade',decision:'extend',reason:'hold lifted; in use'});}
  else decisions.push({hash:h,role,release:'upgrade',decision:'dropped',reason:'no live allowlisted instance and no pending upgrade uses it (read on chain)'});
 }
 for(const h of new Set(liveCode.values()))if(!extendable.has(h)){const r=codeTtl.get(h);if(r===undefined||r<hold.alertBelowLedgers)alerts.push({code:'LIVE_CODE_TTL_LOW',detail:'live code '+h.slice(0,8)+' is not extended (hold) and has '+(r??'unknown')+' ledgers left (< '+hold.alertBelowLedgers+'); re-add it to extension'});}
 if(!hold.enabled&&today>=hold.reviewOn)alerts.push({code:'UPGRADE_CODE_HOLD_REVIEW_DUE',detail:'upgrade code extension hold from '+hold.heldOn+' is due for review ('+hold.reviewOn+')'});
 return {extendable,decisions,alerts};
}
const rawKey=(id,name)=>S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(),key:S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(name)]),durability:S.xdr.ContractDataDurability.persistent()}));
async function rawValue(id,name){const r=await rpc.getLedgerEntries(rawKey(id,name));return r.entries.length?S.scValToNative(r.entries[0].val.contractData().val()):null;}
// Identity pins only. Market state (borrowing on/off, reserve floor, caps,
// rates, CFs) is recorded, never asserted: it cannot widen what the keeper
// signs, which validate() restricts to one native TTL op on allowlisted keys.
async function guards(){
 assert.equal(manifest.controller,'CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK');
 assert.equal(manifest.controllerWasmHash,'847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902');
 const receipts=['CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F','CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS','CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC','CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4'];
 assert.deepEqual(Object.values(manifest.markets).map(m=>m.receipt),receipts);
 assert.equal(manifest.admin,'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP');assert.notEqual(manifest.admin,SOURCE);
 const all=[[manifest.controller,manifest.controllerWasmHash],...Object.values(manifest.markets).map(m=>[m.receipt,m.wasmHash])];
 const liveCode=new Map(),pendingTargets=new Set(),alerts=[],observed={};
 for(const [id,h] of all){if(id!==manifest.controller)assert.equal(h,'4229b7ef9f40028a20e7bace2484f330dbbea625aa98f29f453861e0c498f79e');
 // Live instance: the sealed hash, or exactly the reviewed cash-borrow upgrade target for that role.
 const allowed=[h,id===manifest.controller?UPGRADE_CODE.controller:UPGRADE_CODE.receipt];const k=S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(),key:S.xdr.ScVal.scvLedgerKeyContractInstance(),durability:S.xdr.ContractDataDurability.persistent()}));const r=await rpc.getLedgerEntries(k);assert.equal(r.entries.length,1,'instance missing/archived');const live=r.entries[0].val.contractData().val().instance().executable().wasmHash().toString('hex');assert(allowed.includes(live),'instance WASM is neither the sealed nor the reviewed upgrade hash');liveCode.set(id,live);assert.equal((await read(id,'get_admin')).value,manifest.admin);
  const pending=await rawValue(id,'PendingUpgradeHash');if(pending!=null)pendingTargets.add(Buffer.from(pending).toString('hex'));}
 const guardian=await rawValue(manifest.controller,'PauseGuardian');assert.notEqual(guardian,SOURCE,'fee-only deployer must hold no pause role');observed.pauseGuardian=guardian;
 const assetPins={USDC:'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',XLM:'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',PYUSD:'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2',EURC:'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV'};
 for(const [name,m] of Object.entries(manifest.markets)){assert.equal(m.underlying,assetPins[name]);assert.equal((await read(m.receipt,'get_underlying_token')).value,m.underlying);assert((await read(m.receipt,'get_boosted_vault')).value==null);
  const o=observed[name]={};for(const g of ['get_cash_borrowing_enabled','get_min_cash_reserve_bps'])try{o[g]=(await read(m.receipt,g)).value;}catch(e){o[g]='unreadable: '+e.message.slice(0,80);}
  const mc=await rawValue(m.receipt,'MarginController');if(mc!=null)alerts.push({code:'MARGIN_CONTROLLER_SET',detail:name+' has a margin controller; its u64 position keys are not in the state probe'});}
 return {liveCode,pendingTargets,readsComplete:liveCode.size===all.length,observed,alerts};
}
async function quoteBatch(keys,{extendableCode=new Set()}={}){
 const a=await rpc.getAccount(SOURCE);const data=new S.SorobanDataBuilder().setReadOnly(keys.map(k=>S.xdr.LedgerKey.fromXDR(k,'base64'))).setReadWrite([]).build();
 const raw=new S.TransactionBuilder(a,{fee:'100',networkPassphrase:S.Networks.PUBLIC,sorobanData:data}).addOperation(S.Operation.extendFootprintTtl({extendTo:EXTEND})).setTimeout(300).build();
 const q=await rpc.simulateTransaction(raw);const soft=m=>Object.assign(Error(m),{deferrable:true});if(!S.rpc.Api.isSimulationSuccess(q))throw soft('simulation failed: '+q.error);if(q.restorePreamble)throw soft('restoration is outside keeper signing permission');
 const allowance=(BigInt(q.minResourceFee)*12n+9n)/10n;const sd=q.transactionData.setResourceFee(allowance.toString()).build();
 const tx=S.TransactionBuilder.cloneFrom(raw,{fee:'100',sorobanData:sd,networkPassphrase:S.Networks.PUBLIC}).build();
 const res=sd.resources();const record={keys,simulationLedger:q.latestLedger,minResourceFee:q.minResourceFee,feeAllowance:tx.fee,declaredInstructions:res.instructions(),uniqueKeys:keys.length,readBytes:res.diskReadBytes?res.diskReadBytes():res.readBytes(),writeBytes:res.writeBytes(),nativeOperation:'ExtendFootprintTTL; no WASM/SDK-memory execution'};
 if(BigInt(tx.fee)>MAX_TX)return {tx:null,record,overTxCap:true};
 try{validate(tx,keys,{extendableCode});}catch(e){save(path.join(DIR,'quote-guard-failure.json'),{at:new Date().toISOString(),keys,descriptions:keys.map(k=>describe(S.xdr.LedgerKey.fromXDR(k,'base64'))),minResourceFee:q.minResourceFee,feeAllowance:tx.fee,error:e.message});throw e;}
 return {tx,record};
}
// (b3) Most urgent first: code alone, data in BATCH-sized groups by liveUntil.
function planBatches(eligible,batchSize=BATCH){
 const sorted=[...eligible].sort((x,y)=>x.liveUntilLedger-y.liveUntilLedger||(x.keyXdr<y.keyXdr?-1:1));const out=[];let data=[];
 const flush=()=>{if(data.length){out.push({keys:data.map(e=>e.keyXdr),urgency:Math.min(...data.map(e=>e.liveUntilLedger))});data=[];}};
 for(const e of sorted){if(e.kind==='code')out.push({keys:[e.keyXdr],urgency:e.liveUntilLedger});else{data.push(e);if(data.length===batchSize)flush();}}flush();
 return out.sort((x,y)=>x.urgency-y.urgency);
}
// Greedy by urgency; an unaffordable batch is deferred, never aborts the rest.
function selectWithinBudget(quoted,{budgetLeft,balance,minRemainder=MIN_REMAINDER,maxTx=MAX_TX}){
 let spent=0n;const selected=[],deferred=[];
 for(const b of [...quoted].sort((x,y)=>x.urgency-y.urgency)){
  if(b.error){deferred.push({...b,reason:'quote failed: '+b.error});continue;}
  const fee=BigInt(b.fee);
  if(fee>maxTx)deferred.push({...b,reason:'over per-transaction cap'});
  else if(spent+fee>budgetLeft)deferred.push({...b,reason:'over rolling 24h cap'});
  else if(balance!=null&&balance-(spent+fee)<minRemainder)deferred.push({...b,reason:'would breach fee-only XLM remainder'});
  else{spent+=fee;selected.push(b);}
 }
 return {selected,deferred,spent};
}
function spentLast24h(dir=DIR,now=Date.now()){let n=0n;for(const f of fs.readdirSync(dir).filter(f=>/^run-\d+\.json$/.test(f))){let r;try{r=JSON.parse(fs.readFileSync(path.join(dir,f)));}catch{continue;}for(const t of r.transactions||[])if(now-Date.parse(t.confirmedAt||r.startedAt)<DAY_MS)n+=BigInt(t.feeChargedStroops||t.feeAllowance||0);}return n;}
async function quoteAll(batches,extendableCode){
 const out=[];for(const b of batches){try{const q=await quoteBatch(b.keys,{extendableCode});out.push({...b,fee:q.record.feeAllowance,record:q.record,overTxCap:!!q.overTxCap});}
  catch(e){if(!e.deferrable)throw e; // guard failures stay fatal; only simulation/restore problems defer
   if(b.keys.length>1){for(const k of b.keys){try{const q=await quoteBatch([k],{extendableCode});out.push({keys:[k],urgency:b.urgency,fee:q.record.feeAllowance,record:q.record,overTxCap:!!q.overTxCap});}catch(e2){out.push({keys:[k],urgency:b.urgency,error:e2.message.slice(0,200)});}}}
   else out.push({...b,error:e.message.slice(0,200)});}}
 return out;
}
async function main(mode){
 assert(['quote','run','scheduled'].includes(mode));assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);fs.mkdirSync(DIR,{recursive:true});
 const lock=path.join(DIR,'keeper.lock');
 if(fs.existsSync(lock)){const owner=JSON.parse(fs.readFileSync(path.join(lock,'owner.json')));let alive=true;try{process.kill(owner.pid,0);}catch(e){if(e.code==='ESRCH')alive=false;else throw e;}assert(!alive,'keeper already running');fs.unlinkSync(path.join(lock,'owner.json'));fs.rmdirSync(lock);}
 fs.mkdirSync(lock);fs.writeFileSync(path.join(lock,'owner.json'),json({pid:process.pid,at:new Date().toISOString()}));
 try{
  const firstFile=path.join(DIR,'first-halves-confirmed.json'),enabled=fs.existsSync(path.join(DIR,'first-extension-confirmed.json'));
  if(mode!=='quote'&&!enabled){if(!fs.existsSync(path.join(DIR,'usdc-redeposited.json'))){save(path.join(DIR,'keeper-status.json'),{state:'WAITING_FOR_USDC_ROUNDTRIP',at:new Date().toISOString()});console.log('WAITING_FOR_USDC_ROUNDTRIP: no signing or submission');return;}try{await firstHalves();}catch(e){save(path.join(DIR,'keeper-status.json'),{state:'WAITING_FOR_FIRST_HALVES',at:new Date().toISOString(),reason:e.message});console.log('WAITING_FOR_FIRST_HALVES: no signing or submission');return;}}
  const lastFile=path.join(DIR,'last-success.json');if(mode==='scheduled'&&fs.existsSync(lastFile)){const last=JSON.parse(fs.readFileSync(lastFile));if(Date.now()-Date.parse(last.completedAt)<DAY_MS){console.log('NOT_DUE: last complete TTL check less than 24h ago');return;}}
  const pendingFile=path.join(DIR,'pending-submission.json');if(fs.existsSync(pendingFile)){const p=JSON.parse(fs.readFileSync(pendingFile)),q=await rpc.getTransaction(p.hash);if(q.status==='SUCCESS'){save(path.join(DIR,'reconciled-'+p.hash+'.json'),{...p,status:q.status,ledger:q.ledger});fs.unlinkSync(pendingFile);}else throw Error('Prior submission '+p.hash+' is '+q.status+'; stop for reconciliation, never resend automatically');}
  const g=await guards();
  if(mode!=='quote')await inventory.main();
  const inv=JSON.parse(fs.readFileSync(path.join(DIR,'inventory-latest.json')));assert(inv.stateArchival.maxEntryTtl>EXTEND);
  const current=await rpc.getLatestLedger();assert(current.sequence-inv.ledger<720,'TTL snapshot older than one hour');
  const codeTtl=new Map(inv.entries.filter(e=>e.kind==='code').map(e=>[e.hash,e.liveUntilLedger-current.sequence]));
  const policy=codeExtensionPolicy({liveCode:g.liveCode,pendingTargets:g.pendingTargets,readsComplete:g.readsComplete,codeTtl});
  const alerts=[...(inv.alerts||[]),...g.alerts,...policy.alerts];
  const due=inv.entries.filter(e=>e.liveUntilLedger-current.sequence<THRESHOLD);
  const eligible=due.filter(e=>e.kind!=='code'||policy.extendable.has(e.hash));
  const notExtendedCode=due.filter(e=>e.kind==='code'&&!policy.extendable.has(e.hash)).map(e=>({hash:e.hash,remainingLedgers:e.liveUntilLedger-current.sequence,decision:policy.decisions.find(d=>d.hash===e.hash)?.decision}));
  const quoted=await quoteAll(planBatches(eligible),policy.extendable);
  const publicAccount=await fetch('https://horizon.stellar.org/accounts/'+SOURCE).then(r=>{assert(r.ok);return r.json();});assert(publicAccount.signers.length===1&&publicAccount.signers[0].key===SOURCE&&publicAccount.signers[0].weight===1,'unexpected deployer signers');const balance=BigInt(publicAccount.balances.find(b=>b.asset_type==='native').balance.replace('.',''));
  const spent24=spentLast24h();const budgetLeft=MAX_RUN>spent24?MAX_RUN-spent24:0n;
  const {selected,deferred,spent}=selectWithinBudget(quoted,{budgetLeft,balance});
  const total=quoted.reduce((n,q)=>n+BigInt(q.fee||0),0n);
  const plan={at:new Date().toISOString(),mode,extendToLedgers:EXTEND,thresholdLedgers:THRESHOLD,batchSize:BATCH,eligibleKeys:eligible.length,totalFeeAllowance:total,selectedFeeAllowance:spent,rolling24hSpent:spent24,rolling24hCap:MAX_RUN,deployerBalance:balance,codePolicy:policy.decisions,notExtendedCode,observedMarketState:g.observed,alerts,selected:selected.map(b=>({urgency:b.urgency,fee:b.fee,keys:b.keys.length})),deferred:deferred.map(b=>({urgency:b.urgency,fee:b.fee,keys:b.keys,reason:b.reason})),quotes:quoted.map(q=>q.record||{keys:q.keys,error:q.error})};save(path.join(DIR,'extension-quote-latest.json'),plan);
  if(mode==='quote'){console.log(json(plan));return;}
  const run={startedAt:new Date().toISOString(),firstHalfMaxLedger:enabled?null:JSON.parse(fs.readFileSync(firstFile)).maxLedger,planSha256:enabled?null:JSON.parse(fs.readFileSync(firstFile)).planSha256,transactions:[],keys:selected.flatMap(b=>b.keys).map(x=>({keyXdr:x,before:eligible.find(e=>e.keyXdr===x).liveUntilLedger})),deferred:plan.deferred,alerts};const runPath=path.join(DIR,'run-'+Date.now()+'.json');save(runPath,run);
  let allowedSpent=0n;const submitted=[];
  for(const b of selected){const keys=b.keys;
   let q;try{q=await quoteBatch(keys,{extendableCode:policy.extendable});}catch(e){if(!e.deferrable)throw e;run.deferred.push({urgency:b.urgency,keys,reason:'re-quote at submit failed: '+e.message.slice(0,160)});save(runPath,run);continue;}const {tx,record,overTxCap}=q;
   if(overTxCap||allowedSpent+BigInt(tx.fee)>budgetLeft||balance-(allowedSpent+BigInt(tx.fee))<MIN_REMAINDER){run.deferred.push({urgency:b.urgency,fee:record.feeAllowance,keys,reason:'re-quote at submit exceeded a cap'});save(runPath,run);continue;}
   allowedSpent+=BigInt(tx.fee);assert(allowedSpent<=budgetLeft);assert(balance-allowedSpent>=MIN_REMAINDER);assert(BigInt(tx.fee)<=MAX_TX);validate(tx,keys,{extendableCode:policy.extendable});
   const unsigned=tx.toXDR();
   const signedText=cp.execFileSync(CLI,['tx','sign','--sign-with-key',ALIAS,'--rpc-url',endpoint,'--network-passphrase',S.Networks.PUBLIC],{cwd:ROOT,input:unsigned,encoding:'utf8',timeout:60000,stdio:['pipe','pipe','pipe']}).trim();
   const signed=S.TransactionBuilder.fromXDR(signedText,S.Networks.PUBLIC);assert.equal(signed.hash().toString('hex'),tx.hash().toString('hex'));validate(signed,keys,{extendableCode:policy.extendable});assert.equal(signed.signatures.length,1);assert(S.Keypair.fromPublicKey(SOURCE).verify(signed.hash(),signed.signatures[0].signature()));
   const hash=signed.hash().toString('hex');save(pendingFile,{hash,preparedAt:new Date().toISOString(),record});
   const sent=await rpc.sendTransaction(signed);assert(['PENDING','DUPLICATE'].includes(sent.status),'submission '+sent.status+'; pending marker retained');
   let applied;for(let i=0;i<30;i++){const q=await rpc.getTransaction(hash);if(q.status!=='NOT_FOUND'){applied=q;break;}await new Promise(r=>setTimeout(r,2000));}assert(applied&&applied.status==='SUCCESS','unknown/failed application; pending marker retained');
   assert(enabled||applied.ledger>run.firstHalfMaxLedger);const fees=applied.resultXdr.feeCharged().toString();run.transactions.push({hash,ledger:applied.ledger,confirmedAt:new Date().toISOString(),feeChargedStroops:fees,feeAllowance:tx.fee,keys});submitted.push(...keys);save(runPath,run);fs.unlinkSync(pendingFile);
  }
  run.keys=run.keys.filter(k=>submitted.includes(k.keyXdr));
  for(let i=0;i<submitted.length;i+=100){const chunk=submitted.slice(i,i+100);const r=await rpc.getLedgerEntries(...chunk.map(x=>S.xdr.LedgerKey.fromXDR(x,'base64')));assert.equal(r.entries.length,chunk.length);for(const e of r.entries){assert(e.liveUntilLedgerSeq-r.latestLedger>THRESHOLD);const item=run.keys.find(k=>k.keyXdr===e.key.toXDR('base64'));item.after=e.liveUntilLedgerSeq;assert(item.after>item.before);}}
  const complete=!run.deferred.length&&inv.discovery?.complete!==false;
  run.completedAt=new Date().toISOString();run.result=complete?'PASS':'DEGRADED';run.minimumAppliedLedger=run.transactions.length?Math.min(...run.transactions.map(t=>t.ledger)):null;save(runPath,run);
  // Only a run with nothing deferred and complete discovery starts the 24h
  // NOT_DUE window; a DEGRADED run retries hourly under the rolling 24h cap.
  if(complete)save(lastFile,run);
  if(!enabled){assert(run.transactions.length>0,'first run must actually extend TTLs');save(path.join(DIR,'first-extension-confirmed.json'),run);}
  save(path.join(DIR,'keeper-status.json'),{state:run.result,at:run.completedAt,transactions:run.transactions.length,deferred:run.deferred.length,alerts:alerts.map(a=>a.code+': '+a.detail)});console.log(run.result+' TTL maintenance; '+run.transactions.length+' confirmed native extension transactions; '+run.deferred.length+' deferred; alerts: '+(alerts.map(a=>a.code).join(', ')||'none'));
 }finally{fs.unlinkSync(path.join(lock,'owner.json'));fs.rmdirSync(lock);}
}
if(require.main===module)main(process.argv[2]||'quote').catch(e=>{console.error(e.message);fs.writeFileSync(path.join(DIR,'keeper-error.json'),json({at:new Date().toISOString(),error:e.message}));process.exitCode=1;});
module.exports={validate,quoteBatch,main,codeExtensionPolicy,planBatches,selectWithinBudget,spentLast24h,SOURCE,EXTEND,THRESHOLD,MAX_TX,MAX_RUN,MIN_REMAINDER,UPGRADE_CODE_HOLD};
