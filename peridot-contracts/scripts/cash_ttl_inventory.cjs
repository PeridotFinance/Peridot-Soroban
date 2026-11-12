// Full public ledger-change discovery; no signing, submissions or secret reads.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const S=require('../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const ROOT=path.resolve(__dirname,'..'),DIR=path.join(ROOT,'research/stellar-cash-ttl-20261112');
const manifest=JSON.parse(fs.readFileSync(path.join(ROOT,'research/stellar-redeployment-20261111/fresh-deployment-manifest.json')));
const endpoint='https://soroban-rpc.mainnet.stellar.gateway.fm';
const rpc=new S.rpc.Server(endpoint,{timeout:45});
const ids=new Map([[manifest.controller,'controller'],...Object.entries(manifest.markets).map(([n,m])=>[m.receipt,n])]);
const assets=new Map(Object.entries(manifest.markets).map(([n,m])=>[m.underlying,n]));
const hashes=new Set([manifest.controllerWasmHash,...Object.values(manifest.markets).map(m=>m.wasmHash)]);
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v,2)+'\n';
const atomic=(p,v)=>{fs.writeFileSync(p+'.tmp',json(v));fs.renameSync(p+'.tmp',p);};
function describe(k){
 if(k.switch().name==='contractCode'){
  const hash=k.contractCode().hash().toString('hex');return hashes.has(hash)?{group:hash===manifest.controllerWasmHash?'controller code':'shared receipt code',purpose:'WASM',kind:'code',hash}:null;
 }
 if(k.switch().name!=='contractData')return null;
 const d=k.contractData(),contract=S.Address.fromScAddress(d.contract()).toString();
 const instance=d.key().switch().name==='scvLedgerKeyContractInstance';
 const native=instance?'INSTANCE':S.scValToNative(d.key());
 if(ids.has(contract))return {group:ids.get(contract),contract,purpose:instance?'INSTANCE':JSON.stringify(native,(_,v)=>typeof v==='bigint'?v.toString():v),kind:instance?'instance':d.durability().name,keyScValXdr:d.key().toXDR('base64')};
 if(assets.has(contract)&&(instance||(Array.isArray(native)&&native[0]==='Balance'&&ids.has(native[1]))))return {group:assets.get(contract)+' SAC dependency',contract,purpose:instance?'shared SAC instance':JSON.stringify(native),kind:instance?'instance':'persistent',keyScValXdr:d.key().toXDR('base64')};
 return null;
}
function dataKey(d){if(d.switch().name==='contractCode')return S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:d.contractCode().hash()}));if(d.switch().name==='contractData'){const c=d.contractData();return S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:c.contract(),key:c.key(),durability:c.durability()}));}return null;}
async function main(){
 fs.mkdirSync(DIR,{recursive:true});assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);
 const latest=await rpc.getLatestLedger(),target=process.env.CASH_TTL_THROUGH?Number(process.env.CASH_TTL_THROUGH):latest.sequence;
 assert(Number.isInteger(target)&&target<=latest.sequence);
 const segment=process.env.CASH_TTL_SEGMENT||'';assert(/^[a-z0-9-]*$/.test(segment));
 const registryPath=path.join(DIR,segment?'key-registry-'+segment+'.json':'key-registry.json');
 let reg=fs.existsSync(registryPath)?JSON.parse(fs.readFileSync(registryPath)):{startLedger:Number(process.env.CASH_TTL_FROM||65437362),cursor:null,scannedTransactions:0,keys:{},matchedTransactions:{}};
 // Explicitly include code uploaded before the creation scan and instance dependencies.
 for(const [id] of [...ids,...assets]){const k=S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(),key:S.xdr.ScVal.scvLedgerKeyContractInstance(),durability:S.xdr.ContractDataDurability.persistent()}));const x=k.toXDR('base64');reg.keys[x]??={...describe(k),keyXdr:x,origin:'explicit dependency'};}
 for(const h of hashes){const k=S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:Buffer.from(h,'hex')}));const x=k.toXDR('base64');reg.keys[x]??={...describe(k),keyXdr:x,origin:'sealed artifact'};}
 let pages=0,done=false;
 while(!done){
  const req=reg.cursor?{pagination:{cursor:reg.cursor,limit:200}}:{startLedger:reg.startLedger,pagination:{limit:200}};
  const response=await fetch(endpoint,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'getTransactions',params:req})});assert(response.ok,'RPC HTTP '+response.status);
  const body=await response.json();assert(!body.error,JSON.stringify(body.error));const page=body.result;assert(page.oldestLedger<=Math.max(reg.startLedger,reg.coveredThrough||0),'RPC history gap; fail closed');
  for(const tx of page.transactions){
   let matched=false;
   function record(k,action){const desc=k&&describe(k);if(!desc)return;const x=k.toXDR('base64');const prev=reg.keys[x];reg.keys[x]={...desc,keyXdr:x,firstSeenLedger:prev?.firstSeenLedger||tx.ledger,lastSeenLedger:tx.ledger,lastAction:action==='state'?(prev?.lastAction||action):action};matched=true;}
   function walk(x){
    if(!x||typeof x!=='object'||Buffer.isBuffer(x))return;
    if(x instanceof S.xdr.LedgerEntryChange){const tag=x.switch().name;if(tag==='ledgerEntryRemoved')record(x.value(),'removed');else if(['ledgerEntryCreated','ledgerEntryUpdated','ledgerEntryState'].includes(tag))record(dataKey(x.value().data()),tag==='ledgerEntryState'?'state':'live');return;}
    if(Array.isArray(x))x.forEach(walk);else Object.values(x).forEach(walk);
   }
   // Native byte search avoids decoding unrelated transaction envelopes/metas.
   // Every relevant data/code mutation has its contract/hash in the transaction
   // footprint or creation arguments. Include result metadata as well, so classic
   // or future envelope variants cannot hide a relevant ledger change.
   const metaBytes=Buffer.from(tx.resultMetaXdr,'base64');
   const needles=[...ids.keys()].map(a=>new S.Address(a).toBuffer());
   needles.push(...[...hashes].map(h=>Buffer.from(h,'hex')));
   if(needles.some(n=>metaBytes.includes(n)))walk(S.xdr.TransactionMeta.fromXDR(metaBytes));
   reg.scannedTransactions++;if(matched)reg.matchedTransactions[tx.txHash]={ledger:tx.ledger,status:tx.status};
  }
  reg.cursor=page.cursor;pages++;
  const last=page.transactions.at(-1)?.ledger;
  if(last>target||page.transactions.length===0){reg.coveredThrough=last>target?last-1:Math.min(page.latestLedger,target);done=true;}
  else reg.coveredThrough=Math.max(reg.coveredThrough||0,(last||reg.startLedger)-1);
  atomic(registryPath,reg);
  if(pages%20===0)console.log('TTL discovery',reg.coveredThrough,'/',target,'transactions',reg.scannedTransactions,'keys',Object.keys(reg.keys).length);
  if(!done&&pages>5000)throw Error('bounded discovery page budget exhausted; resume from saved cursor');
 }
 // Complete the final ledger, since a page may stop within it, on the next pass.
 reg.coverageNote='All transaction pages from creation through the pinned target ledger scanned. Cursor may include part of a later ledger. Current TTL reads are subsequent; rerun discovery to catch concurrent writes.';atomic(registryPath,reg);
 if(process.env.CASH_TTL_THROUGH){console.log('Segment complete',segment||'base',reg.startLedger,reg.coveredThrough);return;}
 const now=await rpc.getLatestLedger();
 const cfgKey=S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId:S.xdr.ConfigSettingId.configSettingStateArchival()}));
 const cfg=await rpc.getLedgerEntries(cfgKey);const settings=cfg.entries[0].val.configSetting().stateArchivalSettings();
 const out={observedAt:new Date().toISOString(),network:S.Networks.PUBLIC,ledger:now.sequence,closeTime:now.closeTime,protocol:now.protocolVersion,secondsPerLedgerEstimate:5,archiveDatesAreEstimates:true,discovery:{startLedger:reg.startLedger,coveredThrough:reg.coveredThrough,scannedTransactions:reg.scannedTransactions,matchedTransactions:Object.keys(reg.matchedTransactions).length,note:reg.coverageNote},stateArchival:{maxEntryTtl:settings.maxEntryTtl(),minPersistentTtl:settings.minPersistentTtl(),minTemporaryTtl:settings.minTemporaryTtl()},entries:[],missing:[]};
 const items=Object.values(reg.keys);
 for(let i=0;i<items.length;i+=100){const part=items.slice(i,i+100),r=await rpc.getLedgerEntries(...part.map(p=>S.xdr.LedgerKey.fromXDR(p.keyXdr,'base64')));const byKey=new Map(r.entries.map(e=>[e.key.toXDR('base64'),e]));
  for(const p of part){const e=byKey.get(p.keyXdr);if(!e){out.missing.push({...p,status:p.lastAction==='removed'?'observed deleted':'missing: archival or discovery race, must reconcile'});continue;}assert(Number.isInteger(e.liveUntilLedgerSeq));const remaining=e.liveUntilLedgerSeq-now.sequence;out.entries.push({...p,readLedger:r.latestLedger,liveUntilLedger:e.liveUntilLedgerSeq,remainingLedgers:remaining,remainingDaysAt5Seconds:remaining*5/86400,estimatedArchiveAfterUTC:new Date((Number(now.closeTime)+(remaining+1)*5)*1000).toISOString()});}
 }
 assert(!out.missing.some(m=>m.lastAction!=='removed'),'unexplained absent key: stop maintenance');
 atomic(path.join(DIR,'inventory-latest.json'),out);
 const groups={};for(const e of out.entries){const g=groups[e.group]??={count:0,minRemainingLedgers:Infinity,maxRemainingLedgers:0,earliestArchiveUTC:e.estimatedArchiveAfterUTC,latestArchiveUTC:e.estimatedArchiveAfterUTC};g.count++;g.minRemainingLedgers=Math.min(g.minRemainingLedgers,e.remainingLedgers);g.maxRemainingLedgers=Math.max(g.maxRemainingLedgers,e.remainingLedgers);g.earliestArchiveUTC=[g.earliestArchiveUTC,e.estimatedArchiveAfterUTC].sort()[0];g.latestArchiveUTC=[g.latestArchiveUTC,e.estimatedArchiveAfterUTC].sort().at(-1);}
 atomic(path.join(DIR,'summary-latest.json'),{ledger:out.ledger,stateArchival:out.stateArchival,groups,missing:out.missing});
 fs.writeFileSync(path.join(DIR,'inventory-latest.tsv'),'group\tkind\tkey\tlive_until_ledger\tremaining_ledgers\testimated_archive_utc\n'+out.entries.map(e=>[e.group,e.kind,e.purpose,e.liveUntilLedger,e.remainingLedgers,e.estimatedArchiveAfterUTC].join('\t')).join('\n')+'\n');
 console.log(JSON.stringify({ledger:out.ledger,groups,keys:out.entries.length,missing:out.missing.length},null,2));
}
if(require.main===module)main().catch(e=>{console.error(e);process.exitCode=1;});
module.exports={describe,main,ROOT,DIR,manifest,endpoint};
