// Offline tests for the 2026-11-21 TTL keeper repair (tasks a, b, c). No network.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const K=require('../cash_ttl_keeper.cjs'),I=require('../cash_ttl_inventory.cjs');
const {SOURCE,EXTEND}=K,{UPGRADE_CODE,SEALED_CODE,manifest}=I;
const R=Object.values(manifest.markets).map(m=>m.receipt),C=manifest.controller;
const ck=h=>S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:Buffer.from(h,'hex')}));
function tx(ro){const data=new S.SorobanDataBuilder().setReadOnly(ro).setReadWrite([]).build();return new S.TransactionBuilder(new S.Account(SOURCE,'1'),{fee:'100',networkPassphrase:S.Networks.PUBLIC,sorobanData:data}).addOperation(S.Operation.extendFootprintTtl({extendTo:EXTEND})).setTimeout(300).build();}
const liveAllUpgraded=new Map([[C,UPGRADE_CODE.controller],...R.map(r=>[r,UPGRADE_CODE.receipt])]);
const liveMixed=new Map([[C,UPGRADE_CODE.controller],[R[0],UPGRADE_CODE.receipt],...R.slice(1).map(r=>[r,SEALED_CODE.receipt])]);

// (a) hold
test('a: upgrade code is held (never extendable) even while live',()=>{const p=K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true,today:'2026-11-21'});assert(!p.extendable.has(UPGRADE_CODE.controller));assert(!p.extendable.has(UPGRADE_CODE.receipt));assert.equal(p.decisions.filter(d=>d.decision==='held').length,2);});
test('a: validate rejects held upgrade code keys although describe() approves them',()=>{for(const h of Object.values(UPGRADE_CODE)){assert(I.describe(ck(h)));const p=K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true});assert.throws(()=>K.validate(tx([ck(h)]),[ck(h).toXDR('base64')],{extendableCode:p.extendable}),/not extendable/);}});
test('a: hold constant is disabled and dated; alert threshold 1,500,000 ledgers',()=>{assert.deepEqual(K.UPGRADE_CODE_HOLD,{enabled:false,heldOn:'2026-11-21',reviewOn:'2026-12-21',alertBelowLedgers:1500000});});
test('a: LIVE_CODE_TTL_LOW fires only below threshold; review-due fires from reviewOn',()=>{const hi=new Map([[UPGRADE_CODE.controller,2019000],[UPGRADE_CODE.receipt,2019000]]),lo=new Map([[UPGRADE_CODE.controller,1499999],[UPGRADE_CODE.receipt,2019000]]);
 assert.equal(K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true,codeTtl:hi,today:'2026-11-21'}).alerts.length,0);
 const a=K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true,codeTtl:lo,today:'2026-11-21'}).alerts;assert.deepEqual(a.map(x=>x.code),['LIVE_CODE_TTL_LOW']);
 assert(K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true,codeTtl:hi,today:'2026-12-21'}).alerts.some(x=>x.code==='UPGRADE_CODE_HOLD_REVIEW_DUE'));});
test('a: lifting the hold makes in-use upgrade code extendable',()=>{const p=K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true,hold:{...K.UPGRADE_CODE_HOLD,enabled:true}});assert(p.extendable.has(UPGRADE_CODE.controller)&&p.extendable.has(UPGRADE_CODE.receipt));});

// (c) safe automatic drop
test('c: sealed code dropped only when no live instance uses it',()=>{const p=K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true});assert.equal(p.extendable.size,0);assert.equal(p.decisions.filter(d=>d.decision==='dropped').length,2);});
test('c: sealed receipt code kept while any receipt still runs it; unused controller code dropped',()=>{const p=K.codeExtensionPolicy({liveCode:liveMixed,readsComplete:true});assert(p.extendable.has(SEALED_CODE.receipt));assert(!p.extendable.has(SEALED_CODE.controller));});
test('c: pending upgrade target keeps sealed code',()=>{const p=K.codeExtensionPolicy({liveCode:liveAllUpgraded,pendingTargets:new Set([SEALED_CODE.controller]),readsComplete:true});assert(p.extendable.has(SEALED_CODE.controller));});
test('c: incomplete instance reads keep extending sealed code',()=>{const p=K.codeExtensionPolicy({liveCode:new Map([[C,UPGRADE_CODE.controller]]),readsComplete:false});assert(p.extendable.has(SEALED_CODE.controller)&&p.extendable.has(SEALED_CODE.receipt));assert(!p.extendable.has(UPGRADE_CODE.receipt));});
test('c: validate rejects dropped sealed code and accepts it when still used',()=>{const k=ck(SEALED_CODE.receipt),x=[k.toXDR('base64')];assert.throws(()=>K.validate(tx([k]),x,{extendableCode:K.codeExtensionPolicy({liveCode:liveAllUpgraded,readsComplete:true}).extendable}));assert.doesNotThrow(()=>K.validate(tx([k]),x,{extendableCode:K.codeExtensionPolicy({liveCode:liveMixed,readsComplete:true}).extendable}));});
test('c: validate default rejects every code key',()=>{const k=ck(SEALED_CODE.controller);assert.throws(()=>K.validate(tx([k]),[k.toXDR('base64')]));});

// (b3) urgency ordering and partial runs
const e=(n,live,kind='persistent')=>({keyXdr:'k'+String(n).padStart(3,'0'),liveUntilLedger:live,kind});
test('b3: batches are urgency ordered, code alone, data chunked by urgency',()=>{const items=[e(1,900),e(2,100),e(3,500,'code'),...Array.from({length:31},(_,i)=>e(10+i,1000+i))];const b=K.planBatches(items,30);
 assert.deepEqual(b.map(x=>x.urgency),[100,500,1028]);assert.deepEqual(b[0].keys.slice(0,2),['k002','k001']);assert.deepEqual(b[1].keys,['k003']);assert.equal(b[0].keys.length,30);assert.equal(b[2].keys.length,3);});
test('b3: an over-cap item is deferred, the rest still run',()=>{const q=[{urgency:1,keys:['a'],fee:'500000000'},{urgency:2,keys:['b'],fee:'1000'},{urgency:3,keys:['c'],error:'simulation failed'},{urgency:4,keys:['d'],fee:'2000'}];
 const r=K.selectWithinBudget(q,{budgetLeft:800000000n,balance:703000000n});assert.deepEqual(r.selected.map(x=>x.keys[0]),['b','d']);assert.deepEqual(r.deferred.map(x=>[x.keys[0],x.reason.split(':')[0]]),[['a','over per-transaction cap'],['c','quote failed']]);});
test('b3: rolling budget and remainder defer later (less urgent) batches only',()=>{const q=[{urgency:1,keys:['a'],fee:'300000000'},{urgency:2,keys:['b'],fee:'300000000'},{urgency:3,keys:['c'],fee:'100'}];
 const r=K.selectWithinBudget(q,{budgetLeft:500000000n,balance:10000000000n});assert.deepEqual(r.selected.map(x=>x.keys[0]),['a','c']);assert.equal(r.deferred[0].reason,'over rolling 24h cap');
 const r2=K.selectWithinBudget(q,{budgetLeft:800000000n,balance:450000000n});assert.deepEqual(r2.selected.map(x=>x.keys[0]),['a','c']);assert.match(r2.deferred[0].reason,/remainder/);});
test('b3: the ~60 XLM upgrade-code quote cannot be selected at the current 70.3 XLM balance even if un-held',()=>{const r=K.selectWithinBudget([{urgency:1,keys:['ctl'],fee:'324968324'},{urgency:2,keys:['rcpt'],fee:'394149114'}],{budgetLeft:800000000n,balance:703000000n});assert.deepEqual(r.selected.map(x=>x.keys[0]),['ctl']);assert.match(r.deferred[0].reason,/remainder/);});
test('b3: rolling 24h spend is read from run files',()=>{const d=fs.mkdtempSync(path.join(os.tmpdir(),'ttl-'));const now=Date.parse('2026-11-21T12:00:00Z');
 fs.writeFileSync(path.join(d,'run-1.json'),JSON.stringify({startedAt:'2026-11-21T01:00:00Z',transactions:[{feeChargedStroops:'100'},{confirmedAt:'2026-11-20T11:00:00Z',feeChargedStroops:'7'}]}));fs.writeFileSync(path.join(d,'run-2.json'),JSON.stringify({startedAt:'2026-11-19T01:00:00Z',transactions:[{feeChargedStroops:'999'}]}));fs.writeFileSync(path.join(d,'run-3.json'),'{bad');
 assert.equal(K.spentLast24h(d,now),100n);});

// (b2) guards no longer depend on market state
test('b2: guards assert no borrowing/reserve state; validate stays the only signing allowlist',()=>{const src=fs.readFileSync(path.join(__dirname,'../cash_ttl_keeper.cjs'),'utf8');const g=src.slice(src.indexOf('async function guards'),src.indexOf('async function quoteBatch'));
 assert(!/assert[^;]*get_cash_borrowing_enabled/.test(g));assert(!/assert[^;]*get_min_cash_reserve_bps/.test(g));assert(/notEqual\(guardian,SOURCE/.test(g));assert(/get_boosted_vault/.test(g));
 assert.throws(()=>K.validate(new S.TransactionBuilder(new S.Account(SOURCE,'1'),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(R[0]).call('borrow',new S.Address(SOURCE).toScVal(),S.nativeToScVal(1n,{type:'u128'}))).setTimeout(300).build(),[]));});

// (b1) discovery: fake JSON-RPC client
const addrVal=a=>new S.Address(a).toScVal().toXDR('base64');
function fakeClient({oldest,latest,events=[],txs={},entries={},sim={}}){const calls=[];return {calls,async call(m,p){calls.push([m,p]);
 if(m==='getLatestLedger')return {sequence:latest,protocolVersion:25};
 if(m==='getEvents'){if(p.startLedger===latest&&p.pagination.limit===1)return {events:[],oldestLedger:oldest,latestLedger:latest,cursor:'0'};if(p.startLedger!==undefined&&p.startLedger<oldest)throw Object.assign(Error('startLedger out of range'),{rpc:{}});
  const from=p.startLedger??(Number(BigInt(p.pagination.cursor.split('-')[0])>>32n)+1);const to=Math.min(from+9999,latest);const ev=events.filter(x=>x.ledger>=from&&x.ledger<=to);return {events:ev,latestLedger:latest,oldestLedger:oldest,cursor:(BigInt(to)<<32n|0xffffffffn).toString().padStart(19,'0')+'-4294967295'};}
 if(m==='getTransaction')return txs[p.hash]||{status:'NOT_FOUND'};
 if(m==='getLedgerEntries')return {latestLedger:latest,entries:p.keys.filter(k=>entries[k]).map(k=>({key:k,xdr:entries[k],liveUntilLedgerSeq:latest+100}))};
 if(m==='simulateTransaction')return sim;
 throw Error('unexpected '+m);}};}
const user='GBSK3FYCGNYZLHHPSYRJNE53NZXDFL6ED24KLT3GUUETLFKDAUD7PQX3';
test('b1: behind retention -> explicit gap recorded, no throw, resumes at oldest',async()=>{const reg={startLedger:100,coveredThrough:500,keys:{},matchedTransactions:{},participants:{}};const c=fakeClient({oldest:1000,latest:25000});
 const r=await I.discoverEvents(reg,c);assert.equal(r.complete,true);assert.deepEqual([reg.eventDiscovery.gaps[0].fromLedger,reg.eventDiscovery.gaps[0].toLedger],[501,999]);assert.equal(reg.eventDiscovery.coveredThrough,25000);assert(!c.calls.some(([m,p])=>m==='getEvents'&&p.startLedger!==undefined&&p.startLedger<1000));});
test('b1: within retention -> contiguous with the legacy tx scan, no gap',async()=>{const reg={startLedger:100,coveredThrough:5000,keys:{},matchedTransactions:{},participants:{}};const r=await I.discoverEvents(reg,fakeClient({oldest:1000,latest:25000}));assert(r.complete);assert.equal(reg.eventDiscovery.startLedger,5001);assert.equal(reg.eventDiscovery.gaps.length,0);});
test('b1: page budget is non-fatal and resumes from the saved cursor',async()=>{const reg={startLedger:100,coveredThrough:5000,keys:{},matchedTransactions:{},participants:{}};const c=fakeClient({oldest:1000,latest:65000});
 const r=await I.discoverEvents(reg,c,{pageBudget:2});assert.equal(r.complete,false);assert.equal(reg.eventDiscovery.coveredThrough,25000);const r2=await I.discoverEvents(reg,c,{pageBudget:100});assert(r2.complete);assert.equal(reg.eventDiscovery.coveredThrough,65000);});
test('b1: event participants and walked meta keys are recorded',async()=>{const key=S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(R[0]).toScAddress(),key:S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('BorrowSnapshots'),new S.Address(user).toScVal()]),durability:S.xdr.ContractDataDurability.persistent()}));
 const entry=new S.xdr.LedgerEntryData.contractData(new S.xdr.ContractDataEntry({ext:new S.xdr.ExtensionPoint(0),contract:new S.Address(R[0]).toScAddress(),key:key.contractData().key(),durability:S.xdr.ContractDataDurability.persistent(),val:S.xdr.ScVal.scvU32(1)}));
 const change=S.xdr.LedgerEntryChange.ledgerEntryCreated(new S.xdr.LedgerEntry({lastModifiedLedgerSeq:2000,data:entry,ext:new S.xdr.LedgerEntryExt(0)}));
 const meta=new S.xdr.TransactionMeta(1,new S.xdr.TransactionMetaV1({txChanges:[change],operations:[]})).toXDR('base64');
 const reg={startLedger:100,coveredThrough:1500,keys:{},matchedTransactions:{},participants:{}};
 await I.discoverEvents(reg,fakeClient({oldest:1000,latest:3000,events:[{ledger:2000,txHash:'aa',topic:[S.xdr.ScVal.scvSymbol('borrow').toXDR('base64'),addrVal(user)],value:S.xdr.ScVal.scvVoid().toXDR('base64')}],txs:{aa:{status:'SUCCESS',ledger:2000,resultMetaXdr:meta,envelopeXdr:'AAAA'}}}));
 assert(reg.keys[key.toXDR('base64')]);assert(reg.participants[user]);assert(reg.matchedTransactions.aa.walked);});
test('b1: a retrievable-later tx failure does not advance coverage',async()=>{const reg={startLedger:100,coveredThrough:1500,keys:{},matchedTransactions:{},participants:{}};await assert.rejects(I.discoverEvents(reg,fakeClient({oldest:1000,latest:3000,events:[{ledger:2000,txHash:'bb',topic:[],value:S.xdr.ScVal.scvVoid().toXDR('base64')}]})));assert.equal(reg.eventDiscovery.coveredThrough,1500);});
test('b1: state probe templates reproduce every observed registry key shape from addresses alone',()=>{const reg=JSON.parse(fs.readFileSync(path.join(I.DIR,'key-registry.json')));const A=new Set([manifest.admin,...I.ids.keys(),...I.assets.keys()]);
 for(const k of Object.values(reg.keys))if(k.keyScValXdr)for(const a of I.addressesIn(S.scValToNative(S.xdr.ScVal.fromXDR(k.keyScValXdr,'base64'))))A.add(a);
 const probe=new Set(I.probeKeys({A:[...A],M:R}).map(k=>k.toXDR('base64')));const want=Object.values(reg.keys).filter(k=>I.ids.has(k.contract)&&k.kind==='persistent');assert(want.length>100);for(const k of want)assert(probe.has(k.keyXdr),'probe misses '+k.group+' '+k.purpose);});
test('b1: probe adds a silently-created key for a known participant (e.g. PriceCache, UserSupplyIndex)',async()=>{const k=S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(C).toScAddress(),key:S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('UserSupplyIndex'),new S.Address(user).toScVal(),new S.Address(R[1]).toScVal()]),durability:S.xdr.ContractDataDurability.persistent()})).toXDR('base64');
 const val=new S.xdr.LedgerEntryData.contractData(new S.xdr.ContractDataEntry({ext:new S.xdr.ExtensionPoint(0),contract:new S.Address(C).toScAddress(),key:S.xdr.LedgerKey.fromXDR(k,'base64').contractData().key(),durability:S.xdr.ContractDataDurability.persistent(),val:S.xdr.ScVal.scvU32(1)})).toXDR('base64');
 const reg=I.emptyRegistry();reg.participants[user]={};const r=await I.probeState(reg,fakeClient({oldest:1,latest:10,entries:{[k]:val}}));assert.equal(r.added,1);assert.equal(reg.keys[k].origin,'state probe');});
test('b1: absent key classified archived (restore preamble) vs deleted',async()=>{const k=Object.values(JSON.parse(fs.readFileSync(path.join(I.DIR,'key-registry.json'))).keys)[0].keyXdr;
 const acct=new S.xdr.LedgerEntryData.account(new S.xdr.AccountEntry({accountId:S.Keypair.fromPublicKey(manifest.admin).xdrPublicKey(),balance:S.xdr.Int64.fromString('1'),seqNum:S.xdr.SequenceNumber.fromString('5'),numSubEntries:0,inflationDest:null,flags:0,homeDomain:'',thresholds:Buffer.from([1,0,0,0]),signers:[],ext:new S.xdr.AccountEntryExt(0)})).toXDR('base64');
 const akey=S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId:S.Keypair.fromPublicKey(manifest.admin).xdrPublicKey()})).toXDR('base64');
 assert.equal(await I.classifyAbsent(fakeClient({oldest:1,latest:10,entries:{[akey]:acct},sim:{restorePreamble:{minResourceFee:'1'}}}),k),'archived');
 assert.equal(await I.classifyAbsent(fakeClient({oldest:1,latest:10,entries:{[akey]:acct},sim:{minResourceFee:'1'}}),k),'deleted');
 assert.match(await I.classifyAbsent(fakeClient({oldest:1,latest:10,entries:{}}),k),/unclassified/);});
test('a: upgrade code stays inventoried/approved for reporting and guards',()=>{for(const h of Object.values(UPGRADE_CODE))assert(I.APPROVED_CODE.has(h));});
test('b1: absent key relabelled deleted only if simulation says so AND it provably cannot have archived',()=>{const base={minPersistentTtl:2073600,ledger:65600000};
 assert(I.absentIsDeleted({...base,classification:'deleted',lastKnownLiveUntil:68000000}));
 assert(I.absentIsDeleted({...base,classification:'deleted',firstSeenLedger:65543986}));
 assert(!I.absentIsDeleted({...base,classification:'deleted',lastKnownLiveUntil:65500000}));
 assert(!I.absentIsDeleted({...base,classification:'deleted'}));
 assert(!I.absentIsDeleted({...base,classification:'archived',lastKnownLiveUntil:68000000}));});
test('b3: a deferrable re-quote failure at submit time defers the batch instead of aborting',()=>{const src=fs.readFileSync(path.join(__dirname,'../cash_ttl_keeper.cjs'),'utf8');assert(/catch\(e\)\{if\(!e\.deferrable\)throw e;run\.deferred\.push/.test(src));});
