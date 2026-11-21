// Public, read-only TTL inventory. No signing, submissions or secret reads.
//
// Discovery repaired 2026-11-21 (user approval "TTL: a, b, c"). The original
// discovery decoded EVERY Mainnet transaction (~330 per ledger), could not keep
// up (page budget exhausted / fetch failed; stuck near ledger 65549980) and
// would fail closed for ever once its cursor fell behind RPC retention. The
// registry is now the union of three independent sources:
//  1. Contract-event index: getEvents filtered server-side to the five fresh
//     contracts; each matching transaction's full ledger-change meta is walked
//     exactly as before. Every user flow (deposit/withdraw/transfer/borrow/
//     repay/seize/flash loan) publishes AccrueInterest or an OZ token event.
//  2. State probe: every storage-key template of both contracts (global keys,
//     and keys parameterised by every address known from events, transaction
//     sources, invocation arguments, current stored values and earlier keys) is
//     read directly from CURRENT ledger state. This covers the writes that emit
//     no event (audit 2026-11-21: propose_upgrade_wasm, cache_price PriceCache,
//     accrue_user_market/claim reward indices, admin recovery, lazy HasBorrowed)
//     and is the rebuild path after any history gap.
//  3. Deterministic keys: instances, approved code, SAC instances and each
//     receipt's SAC custody balance.
// No history older than RPC retention is ever required. If the event cursor
// falls behind retention, the missed range is recorded as an explicit gap,
// the probe rebuilds from state and an alert stays raised; maintenance of all
// known keys continues (halting would only let every key archive).
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const S=require('../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const ROOT=path.resolve(__dirname,'..'),DIR=path.join(ROOT,'research/stellar-cash-ttl-20261112');
const manifest=JSON.parse(fs.readFileSync(path.join(ROOT,'research/stellar-redeployment-20261111/fresh-deployment-manifest.json')));
const endpoint='https://soroban-rpc.mainnet.stellar.gateway.fm';
const ids=new Map([[manifest.controller,'controller'],...Object.entries(manifest.markets).map(([n,m])=>[m.receipt,n])]);
const assets=new Map(Object.entries(manifest.markets).map(([n,m])=>[m.underlying,n]));
// Sealed fresh-deployment code (manifest) and the reviewed cash-borrow upgrade
// targets (2026-11-21) are both APPROVED: inventoried, reported, and accepted by
// guards() as an instance's live code. Whether a code entry may be EXTENDED is
// decided separately by codeExtensionPolicy() in cash_ttl_keeper.cjs.
const SEALED_CODE={controller:manifest.controllerWasmHash,receipt:Object.values(manifest.markets)[0].wasmHash};
const UPGRADE_CODE={controller:'f41e5260513390abcb3c18d74f4c0e9e9d65b0638bc797d11ddaec45de5fe2d8',receipt:'5630a701bc7f29ea1334cfd1c12e349664da7376fd86df080d587ea9f685e452'};
const CODE_ROLE=new Map([[SEALED_CODE.controller,'controller'],[UPGRADE_CODE.controller,'controller'],...Object.values(manifest.markets).map(m=>[m.wasmHash,'receipt']),[UPGRADE_CODE.receipt,'receipt']]);
const APPROVED_CODE=new Set(CODE_ROLE.keys());
const GAP_RESIDUAL_RISK='Inside a recorded gap a key can be missed only if it was created there for an address that never appears afterwards in any fresh-contract event, transaction source, invocation argument, stored value or earlier key (or under a template the probe does not list). Reconcile gaps from a full-history source (history archive / Hubble) before calling the inventory complete.';
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v,2)+'\n';
const atomic=(p,v)=>{fs.writeFileSync(p+'.tmp',json(v));fs.renameSync(p+'.tmp',p);};
const nat=v=>JSON.stringify(v,(_,x)=>typeof x==='bigint'?x.toString():x);
function describe(k){
 if(k.switch().name==='contractCode'){
  const hash=k.contractCode().hash().toString('hex');if(!APPROVED_CODE.has(hash))return null;const role=CODE_ROLE.get(hash);
  return {group:role==='controller'?'controller code':'shared receipt code',purpose:'WASM',kind:'code',hash,codeRole:role,codeRelease:Object.values(UPGRADE_CODE).includes(hash)?'cash-borrow-upgrade-20261121':'sealed-20261111'};
 }
 if(k.switch().name!=='contractData')return null;
 const d=k.contractData(),contract=S.Address.fromScAddress(d.contract()).toString();
 const instance=d.key().switch().name==='scvLedgerKeyContractInstance';
 const native=instance?'INSTANCE':S.scValToNative(d.key());
 if(ids.has(contract))return {group:ids.get(contract),contract,purpose:instance?'INSTANCE':nat(native),kind:instance?'instance':d.durability().name,keyScValXdr:d.key().toXDR('base64')};
 if(assets.has(contract)&&(instance||(Array.isArray(native)&&native[0]==='Balance'&&ids.has(native[1]))))return {group:assets.get(contract)+' SAC dependency',contract,purpose:instance?'shared SAC instance':nat(native),kind:instance?'instance':'persistent',keyScValXdr:d.key().toXDR('base64')};
 return null;
}
function dataKey(d){if(d.switch().name==='contractCode')return S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:d.contractCode().hash()}));if(d.switch().name==='contractData'){const c=d.contractData();return S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:c.contract(),key:c.key(),durability:c.durability()}));}return null;}
const instanceKey=id=>S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(),key:S.xdr.ScVal.scvLedgerKeyContractInstance(),durability:S.xdr.ContractDataDurability.persistent()}));
const codeKey=h=>S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:Buffer.from(h,'hex')}));
const STRKEY=/^[GC][A-Z2-7]{55}$/;
function addressesIn(v,out=new Set()){if(typeof v==='string'){if(STRKEY.test(v))out.add(v);}else if(Array.isArray(v))v.forEach(x=>addressesIn(x,out));else if(v instanceof Map)v.forEach((x,k)=>{addressesIn(k,out);addressesIn(x,out);});else if(v&&typeof v==='object'&&!Buffer.isBuffer(v)&&!(v instanceof Uint8Array))Object.entries(v).forEach(([k,x])=>{addressesIn(k,out);addressesIn(x,out);});return out;}
function scAddresses(sc){try{return addressesIn(S.scValToNative(sc));}catch{return new Set();}}

// Storage-key templates of the deployed ReceiptVault / SimplePeridottroller
// (sealed 4c98bdd storage.rs DataKey enums; the cash-borrow upgrade adds only
// INSTANCE keys, which ride on the instance entry). A contracttype enum variant
// is Vec[Symbol(name), ...fields]. Arg kinds: A = every known address (users,
// markets, tokens, roles), M = fresh market (receipt).
const TEMPLATES={
 receipt:{
  unit:['UnderlyingToken','ManagedCash','TotalDeposited','InterestRatePerSecond','LastUpdateTime','AccumulatedInterest','YearlyRateScaled','InitialExchangeRate','TotalBorrowed','BorrowIndex','BorrowYearlyRateScaled','CollateralFactorScaled','Admin','PendingAdmin','Peridottroller','InterestModel','ReserveFactorScaled','AdminFeeScaled','FlashLoanFeeScaled','TotalAdminFees','TotalReserves','SupplyCap','BorrowCap','Initialized','BoostedVault','BoostedUnderlyingCached','BoostedUnderlyingUpdatedAt','TotalBorrowPrincipal','RatesReady','IdleCashBufferBps','FlashLoanActive','MarginController','PendingUpgradeHash','PendingUpgradeEta','DebtStateVersion','DebtStateMigratedAt','TotalBadDebt','BoostedAssetCount','BoostedHealthCache'],
  // Balance(A) is the OpenZeppelin FungibleStorageKey pToken balance. Allowances are
  // temporary storage and not maintained. Margin u64-position keys are unreachable
  // while MarginController is unset (guards() alerts if one is ever set).
  args:[['Balance','A'],['BorrowSnapshots','A'],['HasBorrowed','A'],['BorrowPrincipal','A'],['MarginWithdrawBypass','A'],['MarginWithdrawBypassV2','A']],
 },
 controller:{
  unit:['Admin','PendingAdmin','PauseGuardian','PauseExpiryMigrationDone','PauseExpiryMigrationCursor','SupportedMarkets','Oracle','CloseFactorScaled','LiquidationIncentiveScaled','MarginLiquidationControllers','ReserveRecipient','PauseBorrow','PauseBorrowUntil','PauseRedeem','PauseRedeemUntil','PauseLiquidation','PauseLiquidationUntil','PauseDeposit','PauseDepositUntil','LiquidationFeeScaled','OracleMaxAgeMultiplier','PeridotToken','PendingUpgradeHash','PendingUpgradeEta','PendingOracle','PendingOracleEta','PendingCloseFactorScaled','PendingCloseFactorEta','PendingLiqIncentiveScaled','PendingLiqIncentiveEta'],
  args:[['UserMarkets','A'],['Accrued','A'],['OracleAssetSymbol','A'],['MarketCF','A'],['SupplySpeed','A'],['BorrowSpeed','A'],['SupplyIndex','A'],['BorrowIndex','A'],['SupplyIndexTime','A'],['BorrowIndexTime','A'],['PriceCache','A'],['FallbackPrice','A'],['FallbackPriceSetAt','A'],['SupportedToken','A'],['MarketUnderlying','A'],['MarketZeroTotalsVerifiedAt','A'],['BoostedVaultOwner','A'],['PendingMarketCF','A'],['PendingMarketCFEta','A'],['UserSupplyIndex','A','M'],['UserBorrowIndex','A','M']],
 },
};
function templateKey(contract,name,args){return S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(contract).toScAddress(),key:S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(name),...args.map(a=>new S.Address(a).toScVal())]),durability:S.xdr.ContractDataDurability.persistent()}));}
function product(kinds,pools){return kinds.reduce((acc,k)=>acc.flatMap(p=>pools[k].map(x=>[...p,x])),[[]]);}
function probeKeys(pools,templates=TEMPLATES){
 const out=[];
 for(const [contract,role] of ids){const t=templates[role==='controller'?'controller':'receipt'];
  for(const name of t.unit)out.push(templateKey(contract,name,[]));
  for(const [name,...kinds] of t.args)for(const args of product(kinds,pools))out.push(templateKey(contract,name,args));
 }
 return out;
}

// Raw JSON-RPC with bounded retry for transient transport failures
// (the old job died on single "fetch failed"/"terminated" errors).
function makeClient(url=endpoint,fetchFn=fetch){
 return {async call(method,params){let last;for(let i=0;i<4;i++){try{const r=await fetchFn(url,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params})});if(!r.ok)throw Error('RPC HTTP '+r.status);const b=await r.json();if(b.error){const e=Error(method+': '+JSON.stringify(b.error));e.rpc=b.error;throw e;}return b.result;}catch(e){last=e;if(e.rpc)throw e;if(i<3)await new Promise(res=>setTimeout(res,500*2**i));}}throw last;}};
}
async function getEntries(client,keys){const out=[];let latest=0;for(let i=0;i<keys.length;i+=100){const part=keys.slice(i,i+100);const r=await client.call('getLedgerEntries',{keys:part.map(k=>typeof k==='string'?k:k.toXDR('base64'))});latest=Math.max(latest,r.latestLedger);for(const e of r.entries||[])out.push({keyXdr:e.key,val:S.xdr.LedgerEntryData.fromXDR(e.xdr,'base64'),liveUntilLedgerSeq:e.liveUntilLedgerSeq});}return {entries:out,latestLedger:latest};}
async function classifyAbsent(client,keyXdr){try{const acct=await client.call('getLedgerEntries',{keys:[S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId:S.Keypair.fromPublicKey(manifest.admin).xdrPublicKey()})).toXDR('base64')]});const seq=S.xdr.LedgerEntryData.fromXDR(acct.entries[0].xdr,'base64').account().seqNum().toString();
 const data=new S.SorobanDataBuilder().setReadOnly([S.xdr.LedgerKey.fromXDR(keyXdr,'base64')]).build();const tx=new S.TransactionBuilder(new S.Account(manifest.admin,seq),{fee:'100',networkPassphrase:S.Networks.PUBLIC,sorobanData:data}).addOperation(S.Operation.extendFootprintTtl({extendTo:2592000})).setTimeout(120).build();
 const q=await client.call('simulateTransaction',{transaction:tx.toXDR()});if(q.error)return 'unclassified: '+String(q.error).slice(0,120);return q.restorePreamble?'archived':'deleted';}catch(e){return 'unclassified: '+e.message.slice(0,120);}}
function absentIsDeleted({classification,lastKnownLiveUntil,firstSeenLedger,minPersistentTtl,ledger}){const cannotHaveArchived=(Number.isInteger(lastKnownLiveUntil)&&lastKnownLiveUntil>ledger)||(Number.isInteger(firstSeenLedger)&&firstSeenLedger+minPersistentTtl-1>ledger);return classification==='deleted'&&cannotHaveArchived;}
const cursorLedger=c=>Number(BigInt(String(c).split('-')[0])>>32n);

function emptyRegistry(){return {startLedger:65437362,keys:{},matchedTransactions:{},participants:{}};}
function noteParticipant(reg,addr,ledger,via){if(STRKEY.test(addr)&&!reg.participants[addr])reg.participants[addr]={firstSeenLedger:ledger,via};}
function recordKey(reg,k,{ledger,action,origin}){const desc=k&&describe(k);if(!desc)return false;const x=k.toXDR('base64');const prev=reg.keys[x];
 reg.keys[x]={...desc,keyXdr:x,origin:prev?.origin||origin,firstSeenLedger:prev?.firstSeenLedger||ledger||null,lastSeenLedger:Math.max(prev?.lastSeenLedger||0,ledger||0)||null,lastAction:action==='state'?(prev?.lastAction||'live'):action};return true;}
function walkMeta(reg,metaB64,ledger){let matched=false;
 (function walk(x){if(!x||typeof x!=='object'||Buffer.isBuffer(x))return;
  if(x instanceof S.xdr.LedgerEntryChange){const tag=x.switch().name;let k=null,action;if(tag==='ledgerEntryRemoved'){k=x.value();action='removed';}else if(['ledgerEntryCreated','ledgerEntryUpdated','ledgerEntryState','ledgerEntryRestored'].includes(tag)){k=dataKey(x.value().data());action=tag==='ledgerEntryState'?'state':'live';}
   if(k&&recordKey(reg,k,{ledger,action,origin:'event-indexed transaction meta'})){matched=true;const d=describe(k);if(d.keyScValXdr)for(const a of scAddresses(S.xdr.ScVal.fromXDR(d.keyScValXdr,'base64')))noteParticipant(reg,a,ledger,'ledger key');}return;}
  if(Array.isArray(x))x.forEach(walk);else Object.values(x).forEach(walk);})(S.xdr.TransactionMeta.fromXDR(metaB64,'base64'));
 return matched;}
function envelopeParticipants(reg,envB64,ledger){try{const env=S.xdr.TransactionEnvelope.fromXDR(envB64,'base64');const inner=env.switch().name==='envelopeTypeTxFeeBump'?env.feeBump().tx().innerTx().v1().tx():env.v1().tx();
 noteParticipant(reg,S.StrKey.encodeEd25519PublicKey(S.xdr.MuxedAccount.fromXDR(inner.sourceAccount().toXDR()).switch().name==='keyTypeEd25519'?inner.sourceAccount().ed25519():inner.sourceAccount().med25519().ed25519()),ledger,'transaction source');
 for(const op of inner.operations()){if(op.body().switch().name!=='invokeHostFunction')continue;const f=op.body().invokeHostFunctionOp().hostFunction();if(f.switch().name==='hostFunctionTypeInvokeContract')for(const a of f.invokeContract().args())for(const x of scAddresses(a))noteParticipant(reg,x,ledger,'invocation argument');}}catch{}}

// Event-indexed discovery from the saved coverage point to the latest ledger.
// Never throws for history/budget reasons; returns {complete, reason}.
async function discoverEvents(reg,client,{pageBudget=400,limit=1000,save=()=>{}}={}){
 const legacy=reg.coveredThrough??reg.startLedger-1;
 const ed=reg.eventDiscovery??={startLedger:legacy+1,coveredThrough:legacy,cursor:null,gaps:[],eventsSeen:0,transactionsWalked:0,legacyTxScanCoveredThrough:reg.coveredThrough??null};
 const filters=[{type:'contract',contractIds:[...ids.keys()]}];
 // Retention bound from getEvents itself (event retention can be shorter than transaction retention).
 const latest=(await client.call('getLatestLedger',{})).sequence;const oldest=(await client.call('getEvents',{startLedger:latest,filters,pagination:{limit:1}})).oldestLedger;assert(Number.isInteger(oldest));
 if(ed.cursor&&cursorLedger(ed.cursor)<oldest)ed.cursor=null;
 if(!ed.cursor&&ed.coveredThrough+1<oldest){ed.gaps.push({fromLedger:ed.coveredThrough+1,toLedger:oldest-1,detectedAt:new Date().toISOString(),compensatedBy:'state probe rebuild',residualRisk:GAP_RESIDUAL_RISK});ed.coveredThrough=oldest-1;save();}
 let pages=0;
 while(true){
  if(pages>=pageBudget){save();return {complete:false,reason:'event page budget reached at ledger '+ed.coveredThrough+'; resumes from saved cursor next run'};}
  const params=ed.cursor?{filters,pagination:{cursor:ed.cursor,limit}}:{startLedger:ed.coveredThrough+1,filters,pagination:{limit}};
  const page=await client.call('getEvents',params);pages++;
  for(const e of page.events){ed.eventsSeen++;for(const t of [...e.topic,e.value])for(const a of scAddresses(S.xdr.ScVal.fromXDR(t,'base64')))noteParticipant(reg,a,e.ledger,'event');}
  for(const h of new Set(page.events.map(e=>e.txHash))){if(reg.matchedTransactions[h]?.walked)continue;const t=await client.call('getTransaction',{hash:h});
   if(t.status==='NOT_FOUND')throw Error('event transaction '+h+' not retrievable; resumes from saved cursor');
   if(t.status==='SUCCESS'){walkMeta(reg,t.resultMetaXdr,t.ledger);envelopeParticipants(reg,t.envelopeXdr,t.ledger);}
   reg.matchedTransactions[h]={ledger:t.ledger,status:t.status,walked:true,via:'event index'};ed.transactionsWalked++;}
  const cl=cursorLedger(page.cursor),windowDone=page.events.length<limit;
  ed.cursor=page.cursor;ed.coveredThrough=Math.max(ed.coveredThrough,Math.min(windowDone?cl:cl-1,page.latestLedger));save();
  if(windowDone&&cl>=page.latestLedger)return {complete:true};
 }
}

// State probe / rebuild: read every template for every known parameter.
async function probeState(reg,client,templates=TEMPLATES){
 const A=new Set([manifest.admin,manifest.guardian,...ids.keys(),...assets.keys(),...Object.keys(reg.participants||{})]);
 for(const k of Object.values(reg.keys))if(k.keyScValXdr)for(const a of scAddresses(S.xdr.ScVal.fromXDR(k.keyScValXdr,'base64')))A.add(a);
 // Addresses referenced by current stored values (roles, market maps, oracle ...).
 const known=await getEntries(client,Object.keys(reg.keys));for(const e of known.entries)if(e.val.switch().name==='contractData')for(const a of scAddresses(e.val.contractData().val()))A.add(a);
 const P={A:[...A].filter(a=>STRKEY.test(a)).sort(),M:Object.values(manifest.markets).map(m=>m.receipt)};
 const keys=probeKeys(P,templates).filter(k=>!reg.keys[k.toXDR('base64')]);
 const r=await getEntries(client,keys);let added=0;
 for(const e of r.entries)if(recordKey(reg,S.xdr.LedgerKey.fromXDR(e.keyXdr,'base64'),{ledger:r.latestLedger,action:'live',origin:'state probe'}))added++;
 return {templateKeysProbed:keys.length,added,addresses:P.A.length,ledger:r.latestLedger};
}

async function main({dir=DIR,client=makeClient(),templates=TEMPLATES,write=true}={}){
 fs.mkdirSync(dir,{recursive:true});assert.equal((await client.call('getNetwork',{})).passphrase,S.Networks.PUBLIC);
 const registryPath=path.join(dir,'key-registry.json');
 const reg=fs.existsSync(registryPath)?JSON.parse(fs.readFileSync(registryPath)):emptyRegistry();reg.participants??={};reg.matchedTransactions??={};
 const persist=()=>{if(write)atomic(registryPath,reg);};
 for(const id of [...ids.keys(),...assets.keys()])recordKey(reg,instanceKey(id),{ledger:null,action:'state',origin:'explicit dependency'});
 for(const m of Object.values(manifest.markets))recordKey(reg,S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(m.underlying).toScAddress(),key:S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Balance'),new S.Address(m.receipt).toScVal()]),durability:S.xdr.ContractDataDurability.persistent()})),{ledger:null,action:'state',origin:'explicit dependency'});
 for(const h of APPROVED_CODE)recordKey(reg,codeKey(h),{ledger:null,action:'state',origin:'approved artifact'});
 const alerts=[];let discovery;
 try{discovery=await discoverEvents(reg,client,{save:persist});}catch(e){persist();discovery={complete:false,reason:'event discovery error: '+e.message};}
 if(!discovery.complete)alerts.push({code:'DISCOVERY_INCOMPLETE',detail:discovery.reason});
 const gaps=reg.eventDiscovery?.gaps||[];if(gaps.length)alerts.push({code:'HISTORY_GAP',detail:gaps.length+' recorded event-history gap(s); the state probe rebuilt every template. '+GAP_RESIDUAL_RISK,gaps});
 const probe=await probeState(reg,client,templates);persist();
 const now=await client.call('getLatestLedger',{});
 const cfgKey=S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId:S.xdr.ConfigSettingId.configSettingStateArchival()}));
 const settings=(await getEntries(client,[cfgKey])).entries[0].val.configSetting().stateArchivalSettings();
 const closeTime=Math.floor(Date.now()/1000);
 const out={observedAt:new Date().toISOString(),network:S.Networks.PUBLIC,ledger:now.sequence,closeTime,protocol:now.protocolVersion,secondsPerLedgerEstimate:5,archiveDatesAreEstimates:true,
  discovery:{method:'contract-event index + transaction-meta walk + state probe + deterministic keys',legacyTxScan:{startLedger:reg.startLedger,coveredThrough:reg.eventDiscovery?.legacyTxScanCoveredThrough??reg.coveredThrough??null},eventIndex:{startLedger:reg.eventDiscovery?.startLedger,coveredThrough:reg.eventDiscovery?.coveredThrough,eventsSeen:reg.eventDiscovery?.eventsSeen,transactionsWalked:reg.eventDiscovery?.transactionsWalked},complete:!!discovery.complete&&!gaps.length,gaps,probe,participants:Object.keys(reg.participants).length},
  stateArchival:{maxEntryTtl:settings.maxEntryTtl(),minPersistentTtl:settings.minPersistentTtl(),minTemporaryTtl:settings.minTemporaryTtl()},entries:[],missing:[],alerts};
 const items=Object.values(reg.keys);const read=await getEntries(client,items.map(p=>p.keyXdr));const byKey=new Map(read.entries.map(e=>[e.keyXdr,e]));
 // An absent key is either deleted (some deletions emit no event, e.g. the
 // PendingUpgrade* removal inside upgrade_wasm) or archived. Classify on chain:
 // a read-only ExtendFootprintTTL simulation of an archived persistent key
 // returns a restore preamble; for a deleted key it does not. A key is only
 // relabelled deleted when it also provably could not have archived yet: its
 // last observed liveUntilLedger, or its first-seen ledger (the legacy scan
 // started at contract creation) plus minPersistentTtl, is still ahead.
 for(const p of items){const e=byKey.get(p.keyXdr);if(!e){if(p.lastAction!=='removed'){const c=await classifyAbsent(client,p.keyXdr);if(absentIsDeleted({classification:c,lastKnownLiveUntil:reg.keys[p.keyXdr].lastKnownLiveUntil,firstSeenLedger:reg.keys[p.keyXdr].firstSeenLedger,minPersistentTtl:settings.minPersistentTtl(),ledger:read.latestLedger})){reg.keys[p.keyXdr].lastAction='removed';reg.keys[p.keyXdr].removalInferred={method:'no restore preamble in read-only extend simulation',atLedger:read.latestLedger};p.lastAction='removed';}else reg.keys[p.keyXdr].absentClassification=c==='deleted'?'deleted-or-archived (not provable)':c;}
  out.missing.push({...p,status:p.lastAction==='removed'?(reg.keys[p.keyXdr].removalInferred?'deleted (simulation shows no archived entry to restore)':'observed deleted'):'missing ('+(reg.keys[p.keyXdr].absentClassification||'unknown')+'): excluded from extension; restoration is outside keeper permission; operator must reconcile'});continue;}
  reg.keys[p.keyXdr].lastKnownLiveUntil=e.liveUntilLedgerSeq;if(p.lastAction==='removed'){reg.keys[p.keyXdr].lastAction='live';delete reg.keys[p.keyXdr].removalInferred;p.lastAction='live';}assert(Number.isInteger(e.liveUntilLedgerSeq));const remaining=e.liveUntilLedgerSeq-now.sequence;out.entries.push({...p,readLedger:read.latestLedger,liveUntilLedger:e.liveUntilLedgerSeq,remainingLedgers:remaining,remainingDaysAt5Seconds:remaining*5/86400,estimatedArchiveAfterUTC:new Date((closeTime+(remaining+1)*5)*1000).toISOString()});}
 const unexplained=out.missing.filter(m=>m.lastAction!=='removed');if(unexplained.length)alerts.push({code:'UNEXPLAINED_MISSING_KEYS',detail:unexplained.length+' registry key(s) absent from live state; excluded from extension (restoration is outside keeper permission); operator must reconcile',keys:unexplained.map(m=>({group:m.group,purpose:m.purpose,keyXdr:m.keyXdr}))});
 persist();
 if(write){atomic(path.join(dir,'inventory-latest.json'),out);
  const groups={};for(const e of out.entries){const g=groups[e.group]??={count:0,minRemainingLedgers:Infinity,maxRemainingLedgers:0,earliestArchiveUTC:e.estimatedArchiveAfterUTC,latestArchiveUTC:e.estimatedArchiveAfterUTC};g.count++;g.minRemainingLedgers=Math.min(g.minRemainingLedgers,e.remainingLedgers);g.maxRemainingLedgers=Math.max(g.maxRemainingLedgers,e.remainingLedgers);g.earliestArchiveUTC=[g.earliestArchiveUTC,e.estimatedArchiveAfterUTC].sort()[0];g.latestArchiveUTC=[g.latestArchiveUTC,e.estimatedArchiveAfterUTC].sort().at(-1);}
  atomic(path.join(dir,'summary-latest.json'),{ledger:out.ledger,stateArchival:out.stateArchival,discovery:out.discovery,groups,missing:out.missing,alerts});
  fs.writeFileSync(path.join(dir,'inventory-latest.tsv'),'group\tkind\tkey\tlive_until_ledger\tremaining_ledgers\testimated_archive_utc\n'+out.entries.map(e=>[e.group,e.kind,e.purpose,e.liveUntilLedger,e.remainingLedgers,e.estimatedArchiveAfterUTC].join('\t')).join('\n')+'\n');}
 return out;
}
if(require.main===module)main({dir:process.env.CASH_TTL_INVENTORY_DIR||DIR}).then(out=>{const groups={};for(const e of out.entries)groups[e.group]=(groups[e.group]||0)+1;console.log(json({ledger:out.ledger,discovery:out.discovery,groups,keys:out.entries.length,missing:out.missing.length,alerts:out.alerts.map(a=>a.code+': '+a.detail)}));}).catch(e=>{console.error(e);process.exitCode=1;});
module.exports={absentIsDeleted,classifyAbsent,describe,main,discoverEvents,probeState,probeKeys,walkMeta,makeClient,getEntries,addressesIn,emptyRegistry,instanceKey,codeKey,cursorLedger,TEMPLATES,GAP_RESIDUAL_RISK,ROOT,DIR,manifest,endpoint,ids,assets,SEALED_CODE,UPGRADE_CODE,APPROVED_CODE,CODE_ROLE};
