// Fee-only TTL keeper. User authorized deployer signing ONLY for allowlisted
// native ExtendFootprintTTLOp transactions after the first-half deposit gate.
// No financial invocation, restore, deployment, upgrade, payment, or role change.
// This job has no use for unrelated credentials inherited from the GUI session.
delete process.env.ALMANAX_API_KEY;
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),cp=require('node:child_process');
const S=require('../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const inventory=require('./cash_ttl_inventory.cjs'),{DIR,manifest,endpoint,ROOT,describe}=inventory;
const {firstHalves,read}=require('./cash_operator_checks.cjs');
const SOURCE='GDBS26DYO2FMMLXGFPZ6JX3ZTHJBM2FVAAKD34J7K6ZWVKGLLANZTOYE';
const ALIAS='peridot-cash-fresh-20261111',CLI='/Users/joshua/.local/bin/stellar';
const EXTEND=2592000,THRESHOLD=2419200,BATCH=30,MAX_TX=400000000n,MAX_RUN=800000000n,MIN_REMAINDER=100000000n;
const rpc=new S.rpc.Server(endpoint,{timeout:45});
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?String(v):v,2)+'\n';
const save=(p,v)=>{fs.writeFileSync(p+'.tmp',json(v));fs.renameSync(p+'.tmp',p);};
function validate(tx,keys){
 assert.equal(tx.networkPassphrase,S.Networks.PUBLIC);assert.equal(tx.source,SOURCE);assert.equal(tx.operations.length,1);
 const op=tx.operations[0];assert.equal(op.type,'extendFootprintTtl');assert(!op.source||op.source===SOURCE);assert.equal(op.extendTo,EXTEND);
 assert(BigInt(tx.fee)<=MAX_TX);assert.equal(tx.memo.type,'none');
 const data=tx.toEnvelope().v1().tx().ext().sorobanData(),fp=data.resources().footprint();
 assert.equal(fp.readWrite().length,0);assert(fp.readOnly().length>0&&fp.readOnly().length<=BATCH);
 const actual=fp.readOnly().map(k=>k.toXDR('base64'));assert.deepEqual([...actual].sort(),[...keys].sort());assert.equal(new Set(actual).size,actual.length);
 for(const k of fp.readOnly())assert(describe(k),'unapproved footprint');
 assert(Number(tx.timeBounds.maxTime)>0&&Number(tx.timeBounds.maxTime)-Date.now()/1000<=360);
 assert(data.resources().instructions()<=100000000);
}
async function guards(){
 assert.equal(manifest.controller,'CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK');
 assert.equal(manifest.controllerWasmHash,'847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902');
 const receipts=['CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F','CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS','CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC','CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4'];
 assert.deepEqual(Object.values(manifest.markets).map(m=>m.receipt),receipts);
 const all=[[manifest.controller,manifest.controllerWasmHash],...Object.values(manifest.markets).map(m=>[m.receipt,m.wasmHash])];
 for(const [id,h] of all){if(id!==manifest.controller)assert.equal(h,'4229b7ef9f40028a20e7bace2484f330dbbea625aa98f29f453861e0c498f79e');const k=S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(),key:S.xdr.ScVal.scvLedgerKeyContractInstance(),durability:S.xdr.ContractDataDurability.persistent()}));const r=await rpc.getLedgerEntries(k);assert.equal(r.entries.length,1,'instance missing/archived');assert.equal(r.entries[0].val.contractData().val().instance().executable().wasmHash().toString('hex'),h);assert.equal((await read(id,'get_admin')).value,manifest.admin);}
 assert.equal(manifest.admin,'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP');
 const assetPins={USDC:'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',XLM:'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',PYUSD:'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2',EURC:'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV'};
 for(const [name,m] of Object.entries(manifest.markets)){assert.equal(m.underlying,assetPins[name]);assert.equal((await read(m.receipt,'get_underlying_token')).value,m.underlying);assert.equal((await read(m.receipt,'get_cash_borrowing_enabled')).value,false);assert.equal((await read(m.receipt,'get_min_cash_reserve_bps')).value,10000);assert((await read(m.receipt,'get_boosted_vault')).value==null);}
}
async function quoteBatch(keys){
 const a=await rpc.getAccount(SOURCE);const data=new S.SorobanDataBuilder().setReadOnly(keys.map(k=>S.xdr.LedgerKey.fromXDR(k,'base64'))).setReadWrite([]).build();
 const raw=new S.TransactionBuilder(a,{fee:'100',networkPassphrase:S.Networks.PUBLIC,sorobanData:data}).addOperation(S.Operation.extendFootprintTtl({extendTo:EXTEND})).setTimeout(300).build();
 const q=await rpc.simulateTransaction(raw);assert(S.rpc.Api.isSimulationSuccess(q),q.error);assert(!q.restorePreamble,'restoration is outside keeper signing permission');
 const allowance=(BigInt(q.minResourceFee)*12n+9n)/10n;const sd=q.transactionData.setResourceFee(allowance.toString()).build();
 const tx=S.TransactionBuilder.cloneFrom(raw,{fee:'100',sorobanData:sd,networkPassphrase:S.Networks.PUBLIC}).build();
 try{validate(tx,keys);}catch(e){save(path.join(DIR,'quote-guard-failure.json'),{at:new Date().toISOString(),keys,descriptions:keys.map(k=>describe(S.xdr.LedgerKey.fromXDR(k,'base64'))),minResourceFee:q.minResourceFee,feeAllowance:tx.fee,error:e.message});throw e;}
 const res=sd.resources();return {tx,record:{keys,simulationLedger:q.latestLedger,minResourceFee:q.minResourceFee,feeAllowance:tx.fee,declaredInstructions:res.instructions(),uniqueKeys:keys.length,readBytes:res.diskReadBytes?res.diskReadBytes():res.readBytes(),writeBytes:res.writeBytes(),nativeOperation:'ExtendFootprintTTL; no WASM/SDK-memory execution'}};
}
async function main(mode){
 assert(['quote','run','scheduled'].includes(mode));assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);fs.mkdirSync(DIR,{recursive:true});
 const lock=path.join(DIR,'keeper.lock');
 if(fs.existsSync(lock)){const owner=JSON.parse(fs.readFileSync(path.join(lock,'owner.json')));let alive=true;try{process.kill(owner.pid,0);}catch(e){if(e.code==='ESRCH')alive=false;else throw e;}assert(!alive,'keeper already running');fs.unlinkSync(path.join(lock,'owner.json'));fs.rmdirSync(lock);}
 fs.mkdirSync(lock);fs.writeFileSync(path.join(lock,'owner.json'),json({pid:process.pid,at:new Date().toISOString()}));
 try{
  const firstFile=path.join(DIR,'first-halves-confirmed.json'),enabled=fs.existsSync(path.join(DIR,'first-extension-confirmed.json'));
  if(mode!=='quote'&&!enabled){if(!fs.existsSync(path.join(DIR,'usdc-redeposited.json'))){save(path.join(DIR,'keeper-status.json'),{state:'WAITING_FOR_USDC_ROUNDTRIP',at:new Date().toISOString()});console.log('WAITING_FOR_USDC_ROUNDTRIP: no signing or submission');return;}try{await firstHalves();}catch(e){save(path.join(DIR,'keeper-status.json'),{state:'WAITING_FOR_FIRST_HALVES',at:new Date().toISOString(),reason:e.message});console.log('WAITING_FOR_FIRST_HALVES: no signing or submission');return;}}
  const lastFile=path.join(DIR,'last-success.json');if(mode==='scheduled'&&fs.existsSync(lastFile)){const last=JSON.parse(fs.readFileSync(lastFile));if(Date.now()-Date.parse(last.completedAt)<86400000){console.log('NOT_DUE: last complete TTL check less than 24h ago');return;}}
  const pendingFile=path.join(DIR,'pending-submission.json');if(fs.existsSync(pendingFile)){const p=JSON.parse(fs.readFileSync(pendingFile)),q=await rpc.getTransaction(p.hash);if(q.status==='SUCCESS'){save(path.join(DIR,'reconciled-'+p.hash+'.json'),{...p,status:q.status,ledger:q.ledger});fs.unlinkSync(pendingFile);}else throw Error('Prior submission '+p.hash+' is '+q.status+'; stop for reconciliation, never resend automatically');}
  await guards();
  if(mode!=='quote')await inventory.main();
  const inv=JSON.parse(fs.readFileSync(path.join(DIR,'inventory-latest.json')));assert(inv.stateArchival.maxEntryTtl>EXTEND);assert(!inv.missing.some(x=>x.lastAction!=='removed'));
  const current=await rpc.getLatestLedger();assert(current.sequence-inv.ledger<720,'TTL snapshot older than one hour');
  const eligible=inv.entries.filter(e=>e.liveUntilLedger-current.sequence<THRESHOLD);
  const batches=[];let data=[];for(const e of eligible){if(e.kind==='code'){batches.push([e.keyXdr]);}else data.push(e.keyXdr);}for(let i=0;i<data.length;i+=BATCH)batches.push(data.slice(i,i+BATCH));
  const quotes=[];for(const keys of batches)quotes.push((await quoteBatch(keys)).record);
  const total=quotes.reduce((n,q)=>n+BigInt(q.feeAllowance),0n);assert(total<=MAX_RUN,'per-run gas ceiling exceeded');
  const plan={at:new Date().toISOString(),mode,extendToLedgers:EXTEND,thresholdLedgers:THRESHOLD,batchSize:BATCH,eligibleKeys:eligible.length,totalFeeAllowance:total,quotes};save(path.join(DIR,'extension-quote-latest.json'),plan);
  if(mode==='quote'){console.log(json(plan));return;}
  const publicAccount=await fetch('https://horizon.stellar.org/accounts/'+SOURCE).then(r=>{assert(r.ok);return r.json();});assert(publicAccount.signers.length===1&&publicAccount.signers[0].key===SOURCE&&publicAccount.signers[0].weight===1,'unexpected deployer signers');const balance=BigInt(publicAccount.balances.find(b=>b.asset_type==='native').balance.replace('.',''));assert(balance-total>=MIN_REMAINDER,'insufficient fee-only XLM buffer');
  const run={startedAt:new Date().toISOString(),firstHalfMaxLedger:enabled?null:JSON.parse(fs.readFileSync(firstFile)).maxLedger,planSha256:enabled?null:JSON.parse(fs.readFileSync(firstFile)).planSha256,transactions:[],keys:eligible.map(e=>({keyXdr:e.keyXdr,before:e.liveUntilLedger}))};const runPath=path.join(DIR,'run-'+Date.now()+'.json');save(runPath,run);
  let allowedSpent=0n;
  for(const keys of batches){
   const {tx,record}=await quoteBatch(keys);allowedSpent+=BigInt(tx.fee);assert(allowedSpent<=MAX_RUN);assert(balance-allowedSpent>=MIN_REMAINDER);assert(BigInt(tx.fee)<=MAX_TX);validate(tx,keys);
   const unsigned=tx.toXDR();
   const signedText=cp.execFileSync(CLI,['tx','sign','--sign-with-key',ALIAS,'--rpc-url',endpoint,'--network-passphrase',S.Networks.PUBLIC],{cwd:ROOT,input:unsigned,encoding:'utf8',timeout:60000,stdio:['pipe','pipe','pipe']}).trim();
   const signed=S.TransactionBuilder.fromXDR(signedText,S.Networks.PUBLIC);assert.equal(signed.hash().toString('hex'),tx.hash().toString('hex'));validate(signed,keys);assert.equal(signed.signatures.length,1);assert(S.Keypair.fromPublicKey(SOURCE).verify(signed.hash(),signed.signatures[0].signature()));
   const hash=signed.hash().toString('hex');save(pendingFile,{hash,preparedAt:new Date().toISOString(),record});
   const sent=await rpc.sendTransaction(signed);assert(['PENDING','DUPLICATE'].includes(sent.status),'submission '+sent.status+'; pending marker retained');
   let applied;for(let i=0;i<30;i++){const q=await rpc.getTransaction(hash);if(q.status!=='NOT_FOUND'){applied=q;break;}await new Promise(r=>setTimeout(r,2000));}assert(applied&&applied.status==='SUCCESS','unknown/failed application; pending marker retained');
   assert(enabled||applied.ledger>run.firstHalfMaxLedger);const fees=applied.resultXdr.feeCharged().toString();run.transactions.push({hash,ledger:applied.ledger,feeChargedStroops:fees,keys});save(runPath,run);fs.unlinkSync(pendingFile);
  }
  for(let i=0;i<eligible.length;i+=100){const chunk=eligible.slice(i,i+100);const r=await rpc.getLedgerEntries(...chunk.map(e=>S.xdr.LedgerKey.fromXDR(e.keyXdr,'base64')));assert.equal(r.entries.length,chunk.length);for(const e of r.entries){assert(e.liveUntilLedgerSeq-r.latestLedger>THRESHOLD);const item=run.keys.find(k=>k.keyXdr===e.key.toXDR('base64'));item.after=e.liveUntilLedgerSeq;assert(item.after>item.before);}}
  run.completedAt=new Date().toISOString();run.result='PASS';run.minimumAppliedLedger=run.transactions.length?Math.min(...run.transactions.map(t=>t.ledger)):null;save(runPath,run);save(lastFile,run);
  if(!enabled){assert(run.transactions.length>0,'first run must actually extend TTLs');save(path.join(DIR,'first-extension-confirmed.json'),run);}
  save(path.join(DIR,'keeper-status.json'),{state:'PASS',at:run.completedAt,transactions:run.transactions.length});console.log('PASS TTL maintenance; '+run.transactions.length+' confirmed native extension transactions');
 }finally{fs.unlinkSync(path.join(lock,'owner.json'));fs.rmdirSync(lock);}
}
if(require.main===module)main(process.argv[2]||'quote').catch(e=>{console.error(e.message);fs.writeFileSync(path.join(DIR,'keeper-error.json'),json({at:new Date().toISOString(),error:e.message}));process.exitCode=1;});
module.exports={validate,quoteBatch,main,SOURCE,EXTEND};
