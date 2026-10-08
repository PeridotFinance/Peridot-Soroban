import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,readFile,appendFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {Account,Address,Asset,Keypair,Networks,SorobanDataBuilder,StrKey,nativeToScVal,scValToNative,xdr} from '@stellar/stellar-sdk';
import {depthCandidate} from '../src/depth-pricing.mjs';
import {createDepthPublisher,createDepthTransport,validateDepthManifest,depthJournalScope} from '../src/depth-publisher.mjs';
import {openPublicationJournal} from '../src/depth-publication-journal.mjs';
import {SHADOW_POLICY} from '../src/price-depth-shadow.mjs';
import {runObserver} from '../src/price-observer-runtime.mjs';
import {LEGACY_RECOVERY,recoverLegacyIntent,validateFence} from '../src/depth-intent-recovery.mjs';
import {depthDiagnostic,depthStage} from '../src/depth-diagnostics.mjs';
import {TransactionBuilder} from '@stellar/stellar-sdk';
const start=1_800_000_000;
const history=(length=32)=>Array.from({length},(_,index)=>{
  const t=start+index*60;
  return {index,state:'ok',startedAt:t,finishedAt:t+2,sample:{publicationEligible:false,
    point:{policy:SHADOW_POLICY,timestamp:t,ledgerBefore:1000+index*12,ledgerAfter:1001+index*12,
      depth:[1000n,10000n].map(n=>({probeRaw:String(n*10000000n),soldForRaw:String(n*9800000n),boughtRaw:String(n*10180000n)}))},
    aquarius:{pool:'CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F',ledgerMin:1000+index*12,
      ledgerMax:1001+index*12,probeRaw:'1000000000',soldForRaw:'980000000',boughtRaw:'1018000000'}}};
});
const contract=i=>StrKey.encodeContract(Buffer.alloc(32,i));
function fixture(legacy=false) {
  const key=Keypair.random(),records=history(),time={value:records.at(-1).finishedAt};
  const m={kind:'isolated-depth-price-replay-v1',network:Networks.TESTNET,reporter:key.publicKey(),
    router:contract(1),asset:contract(2),pool:contract(3),quote:Asset.native().contractId(Networks.TESTNET),
    routerWasmHash:'a'.repeat(64),poolWasmHash:'b'.repeat(64),inIndex:0};
  if(legacy)Object.assign(m,{
    reporter:LEGACY_RECOVERY.source,router:'CDTUAUSKRXUUOD44WZVCJUZHVAVEHPGA7PFGR4KWDIO6IS2A7KN4P6KZ',
    asset:'CDTQF3ZNVGUX357IXJ5XQT2DXT6IU7EEDS5APQAWFCHHQLMQDHX53R2J',
    pool:'CD7RGNVHWDDPVWLRTGGTK72WFJYYLW6H55MMTEICYMODZFHP23VLESWL',
    routerWasmHash:'3bbd58e9d571ab5fe4fa25b9accae6d00eeac7c1c42af110cdbbb164e3b08098',
    poolWasmHash:'88cf3b042b447da722a73eac945615426626614339b710ef8b38397d2a413fe3'});
  const state={previous:{ratio:0n,start:0n,end:0n,invalidated_at:BigInt(start),valid:false},recovery:null,
    quotes:{probe:1_000_000_000n,sell:980_000_000n,buy:1_018_000_000n},sent:[],simulated:[],status:'SUCCESS',
    network:Networks.TESTNET,fee:'1000',ledger:1000,age:0,auth:'normal',code:true,restore:false,config:true};
  const cfg=()=>({reporter:m.reporter,pool:m.pool,quote_to:m.quote,in_idx:0,out_idx:1,probe_amount:1_000_000_000n,
    window_secs:1800n,max_age_secs:300n,min_interval_secs:120n,max_deviation_bps:100,max_step_bps:100,
    min_ratio:800_000_000_000n,max_ratio:1_050_000_000_000n});
  const server={
    getNetwork:async()=>({passphrase:state.network}),
    getLatestLedger:async()=>({sequence:state.ledger,closeTime:String(time.value-state.age)}),
    getAccount:async()=>new Account(m.reporter,'1'),
    getLedgerEntries:async k=>{
      const id=Address.fromScAddress(k.contractData().contract()).toString();
      return {latestLedger:state.ledger,entries:[{key:k,val:{contractData:()=>({val:()=>({instance:()=>({executable:()=>({
        switch:()=>({name:'contractExecutableWasm'}),wasmHash:()=>Buffer.from(state.code?(id===m.router?m.routerWasmHash:m.poolWasmHash):'c'.repeat(64),'hex')})})})})}}]};
    },
    simulateTransaction:async tx=>{
      const call=tx.operations[0].func.invokeContract(),method=call.functionName().toString();
      state.simulated.push(method);
      let value,auth=[];
      switch(method) {
        case 'get_tokens':value=[m.asset,m.quote];break;
        case 'decimals':value=7;break;
        case 'get_source':value=['Observed',{...cfg(),...(state.config?{}:{max_step_bps:500})}];break;
        case 'get_observation':value=structuredClone(state.previous);break;
        case 'get_observation_recovery':value=state.recovery?{config:cfg(),...structuredClone(state.recovery)}:null;break;
        case 'estimate_swap':value=scValToNative(call.args()[0])===0?state.quotes.sell:state.quotes.buy;break;
        case 'publish_observation':case 'publish_recovery_observation':case 'invalidate_observation': {
          if(state.simulationError)return {error:'controlled simulation failure',latestLedger:state.ledger};
          const root=new xdr.SorobanAuthorizedInvocation({function:xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(call),subInvocations:[]});
          if(state.auth==='wrong_method')root.function().contractFn().functionName('transfer');
          if(state.auth==='nested')root.subInvocations([new xdr.SorobanAuthorizedInvocation({function:root.function(),subInvocations:[]})]);
          auth=state.auth==='missing'?[]:[new xdr.SorobanAuthorizationEntry({credentials:xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),rootInvocation:root})];
          value=undefined;state.onPrepare?.();break;
        }
        default:throw Error('unexpected simulated method');
      }
      // Match real Soroban u32 metadata/config (SDK's untyped JS number defaults
      // to i128 and would make a fake fixture reject before exercising signing).
      const encode=v=>typeof v==='number'?nativeToScVal(v,{type:'u32'}):
        Array.isArray(v)?xdr.ScVal.scvVec(v.map(encode)):
        v&&typeof v==='object'?xdr.ScVal.scvMap(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))
          .map(([k,val])=>new xdr.ScMapEntry({key:nativeToScVal(k,{type:'symbol'}),val:encode(val)}))):nativeToScVal(v);
      return {_parsed:true,id:'test',latestLedger:state.ledger,minResourceFee:state.fee,transactionData:new SorobanDataBuilder().setResourceFee(state.fee),
        result:{retval:encode(value),auth},events:[],...(state.restore?{restorePreamble:{}}:{})};
    },
    sendTransaction:async tx=>{
      assert.equal(tx.networkPassphrase,Networks.TESTNET);assert.equal(tx.signatures.length,1);
      const hash=tx.hash().toString('hex');state.sent.push(tx);
      if(state.sendError)throw Error('secret SDK content');
      return {hash,status:'PENDING'};
    },
    getTransaction:async hash=>({txHash:hash,status:state.status,ledger:1001})
  };
  const transport=createDepthTransport({manifest:m,server,key:legacy?{publicKey:()=>m.reporter}:key,now:()=>time.value});
  return {m,state,time,server,transport,records,input:{records,startedAt:start},now:()=>time.value};
}
async function withPublisher(t,f=fixture()) {
  const dir=await mkdtemp(join(tmpdir(),'peridot-depth-publisher-')),path=join(dir,'journal.jsonl');
  const journal=await openPublicationJournal(path,depthJournalScope(f.m));
  t.after(async()=>{await journal.close();await rm(dir,{recursive:true,force:true});});
  const events=[];
  const publisher=createDepthPublisher({transport:f.transport,journal,now:f.now,sleep:async()=>{},emit:r=>events.push(r)});
  return {...f,journal,publisher,path,events};
}
test('Testnet SDK transaction signs exact simulated invocation and durably resolves public hash',async t=>{
  const f=await withPublisher(t),result=await f.publisher.cycle(f.input);
  assert.equal(result.method,'publish_observation');assert.equal(result.action,'confirmed');
  assert.equal(f.state.sent.length,1);assert.equal(f.journal.pending(),null);
  const log=await readFile(f.path,'utf8'),rows=log.trim().split('\n').map(JSON.parse);
  assert.deepEqual(rows.map(r=>r.state),['intent','SUCCESS']);assert.equal(rows[0].hash,result.hash);
  assert(!log.includes('AAAA'));assert.equal(f.events.at(-1).kind,'depth_confirmed');
  assert(BigInt(f.state.sent[0].fee)<=1_000_000n);
  assert.equal(Number(f.state.sent[0].timeBounds.maxTime),f.time.value+30);
});
test('invalid data invalidates a valid report once and never keeps moving its watermark',async t=>{
  const f=await withPublisher(t),candidate=depthCandidate(f.records,f.now(),start);
  f.state.previous={ratio:BigInt(candidate.ratio),start:BigInt(candidate.start-300),end:BigInt(candidate.end-300),invalidated_at:0n,valid:true};
  f.records[15].state='unavailable';
  assert.equal((await f.publisher.cycle(f.input)).method,'invalidate_observation');
  f.state.previous.valid=false;f.state.previous.invalidated_at=BigInt(f.now());
  assert.equal((await f.publisher.cycle(f.input)).action,'hold');assert.equal(f.state.sent.length,1);
});
test('fresh divergent pool quote invalidates even inside minimum publication interval',async t=>{
  const f=await withPublisher(t),c=depthCandidate(f.records,f.now(),start);
  f.state.previous={ratio:BigInt(c.ratio),start:BigInt(c.start-60),end:BigInt(c.end-60),valid:true,invalidated_at:0n};
  f.state.quotes.sell=800_000_000n;
  assert.equal((await f.publisher.cycle(f.input)).method,'invalidate_observation');
});
test('configuration, code, network, ledger freshness, simulation, restoration and exact auth guards prevent signing',async t=>{
  for(const change of [s=>s.network=Networks.PUBLIC,s=>s.code=false,s=>s.config=false,s=>s.age=61,
    s=>s.fee='1000000',s=>s.restore=true,s=>s.simulationError=true,s=>s.auth='missing',s=>s.auth='wrong_method',s=>s.auth='nested']){
    const f=await withPublisher(t);change(f.state);
    await assert.rejects(f.publisher.cycle(f.input));assert.equal(f.state.sent.length,0);assert.equal(f.journal.pending(),null);
  }
});
test('state change, stale candidate or changed quote after simulation discard unsigned transaction',async t=>{
  for(const change of [f=>f.time.value+=61,f=>f.state.previous.invalidated_at=BigInt(f.now()),f=>f.state.quotes.sell=800_000_000n]){
    const f=await withPublisher(t);f.state.onPrepare=()=>change(f);
    assert.equal((await f.publisher.cycle(f.input)).action,'hold');assert.equal(f.state.sent.length,0);assert.equal(f.journal.pending(),null);
  }
});
test('lost submission response retains journal intent, stops process and reconciles by hash without resend',async t=>{
  const f=await withPublisher(t);f.state.sendError=true;
  await assert.rejects(f.publisher.cycle(f.input));const hash=f.journal.pending().hash;
  await assert.rejects(f.publisher.cycle(f.input),/stopped/);assert.equal(f.state.sent.length,1);
  const restarted=createDepthPublisher({transport:f.transport,journal:f.journal,now:f.now,sleep:async()=>{}});
  await restarted.reconcile();assert.equal(f.journal.pending(),null);assert.equal(f.state.sent.length,1);
  assert.equal(JSON.parse((await readFile(f.path,'utf8')).trim().split('\n').at(-1)).hash,hash);
});
test('NOT_FOUND stays unresolved after timeout/restart; FAILED remains blocked for operator review',async t=>{
  for(const status of ['NOT_FOUND','FAILED']) {
    const f=await withPublisher(t);f.state.status=status;
    await assert.rejects(f.publisher.cycle(f.input));assert.equal(f.state.sent.length,1);
    const restarted=createDepthPublisher({transport:f.transport,journal:f.journal,now:f.now,sleep:async()=>{}});
    await assert.rejects(restarted.reconcile());assert.equal(f.state.sent.length,1);
    assert.equal(status==='NOT_FOUND',f.journal.pending()!==null);
  }
});
test('durable new-process journal polls delayed inclusion by the exact hash without re-sending',async t=>{
  const f=await withPublisher(t);f.state.sendError=true;
  await assert.rejects(f.publisher.cycle(f.input));
  const hash=f.journal.pending().hash;
  await f.journal.close();
  const reopened=await openPublicationJournal(f.path,depthJournalScope(f.m));
  t.after(()=>reopened.close());
  let calls=0,waits=0;
  f.server.getTransaction=async requested=>{
    assert.equal(requested,hash);calls++;
    return {txHash:requested,status:calls<3?'NOT_FOUND':'SUCCESS',ledger:1001};
  };
  const restarted=createDepthPublisher({transport:f.transport,journal:reopened,now:f.now,
    sleep:async ms=>{assert.equal(ms,1000);waits++;}});
  await restarted.reconcile();
  assert.equal(calls,3);assert.equal(waits,2);assert.equal(reopened.pending(),null);
  assert.equal(f.state.sent.length,1);
  await restarted.reconcile();assert.equal(calls,3);
  await reopened.close();
});
test('confirmation polling is bounded and poisons timeout, mismatch and RPC error without another send',async t=>{
  for(const mode of ['timeout','mismatch','rpc_error','failed']) {
    const f=await withPublisher(t);f.state.sendError=true;
    await assert.rejects(f.publisher.cycle(f.input));
    const hash=f.journal.pending().hash;let calls=0,waits=0;
    f.server.getTransaction=async requested=>{
      assert.equal(requested,hash);calls++;
      if(mode==='rpc_error')throw Error('private transport details');
      return {txHash:mode==='mismatch'?'0'.repeat(64):hash,
        status:mode==='failed'&&calls===3?'FAILED':'NOT_FOUND',ledger:1001};
    };
    const restarted=createDepthPublisher({transport:f.transport,journal:f.journal,now:f.now,
      sleep:async()=>{waits++;}});
    await assert.rejects(restarted.reconcile());
    assert.equal(calls,mode==='timeout'?20:mode==='failed'?3:1);
    assert.equal(waits,mode==='timeout'?19:mode==='failed'?2:0);
    assert.equal(f.journal.pending()!==null,mode!=='failed');
    await assert.rejects(restarted.reconcile(),/stopped/);
    assert.equal(f.state.sent.length,1);
  }
});
test('journal exclusive lock, durable restart, scope mismatch and truncation guards',async t=>{
  const f=await withPublisher(t),scope=depthJournalScope(f.m);
  await assert.rejects(openPublicationJournal(f.path,scope),/EEXIST/);
  await f.journal.intent('d'.repeat(64),'publish_observation');await f.journal.close();
  await assert.rejects(openPublicationJournal(f.path,'e'.repeat(64)),/scope/);
  const reopened=await openPublicationJournal(f.path,scope);
  assert.equal(reopened.pending().hash,'d'.repeat(64));await reopened.close();
  await appendFile(f.path,'{"state":');
  await assert.rejects(openPublicationJournal(f.path,scope),/partial/);
});
test('slow confirmation reads stop at the elapsed-time bound with intent intact',async t=>{
  const f=await withPublisher(t);f.state.sendError=true;
  await assert.rejects(f.publisher.cycle(f.input));
  let elapsed=0,calls=0;
  f.server.getTransaction=async hash=>{elapsed+=10_000;calls++;return {txHash:hash,status:'NOT_FOUND'};};
  const restarted=createDepthPublisher({transport:f.transport,journal:f.journal,now:f.now,
    monotonic:()=>elapsed,sleep:async ms=>{elapsed+=ms;}});
  await assert.rejects(restarted.reconcile(),/unresolved/);
  assert.equal(calls,3);assert.equal(elapsed,32_000);
  assert(f.journal.pending());assert.equal(f.state.sent.length,1);
});
test('journal failure before send prevents broadcast; concurrent cycles are forbidden',async t=>{
  const f=await withPublisher(t);await f.journal.close();
  await assert.rejects(f.publisher.cycle(f.input));assert.equal(f.state.sent.length,0);
  const g=await withPublisher(t),first=g.publisher.cycle(g.input);
  await assert.rejects(g.publisher.cycle(g.input),/concurrent/);await first;assert.equal(g.state.sent.length,1);
});
test('manifest cannot opt into Mainnet, choose wrong quote, omit pinned code or borrow deployer identity',()=>{
  for(const edit of [m=>m.network=Networks.PUBLIC,m=>m.quote=Asset.native().contractId(Networks.PUBLIC),
    m=>delete m.routerWasmHash,m=>m.reporter='GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL']){
    const f=fixture();edit(f.m);assert.throws(()=>validateDepthManifest(f.m));
  }
});
test('collector to signed SDK replay: outage invalidates, then requires thirty new healthy minutes',async t=>{
  const f=await withPublisher(t),controller=new AbortController(),samples=history(72),submissions=[];
  let ms=0,index=0;f.time.value=start;
  const originalSend=f.server.sendTransaction;
  f.server.sendTransaction=async tx=>{
    const result=await originalSend(tx),call=tx.operations[0].func.invokeContract(),method=call.functionName().toString();
    submissions.push({method,index:index-1});
    if(method==='publish_observation') {
      const args=call.args().map(scValToNative);
      f.state.previous={ratio:args[2],start:args[3],end:args[4],valid:true,invalidated_at:f.state.previous.invalidated_at};
    }else {f.state.previous.valid=false;f.state.previous.invalidated_at=BigInt(f.now());}
    return result;
  };
  await runObserver({runId:'end-to-end-offline',signal:controller.signal,now:f.now,monotonic:()=>ms,
    sleep:async n=>{ms+=n;f.time.value=start+Math.floor(ms/1000);},emit:()=>{},
    collect:async()=>{const i=index++;ms+=2000;f.time.value=start+Math.floor(ms/1000);
      if(i===35)throw Error('controlled collector outage');return samples[i].sample;},
    afterSample:async input=>{await f.publisher.cycle(input);if(index===72)controller.abort();}});
  assert.deepEqual(submissions.filter(r=>r.method==='invalidate_observation'),[{method:'invalidate_observation',index:35}]);
  assert(submissions.some(r=>r.method==='publish_observation'&&r.index<35));
  assert(submissions.some(r=>r.method==='publish_observation'&&r.index>=66));
  assert(!submissions.some(r=>r.method==='publish_observation'&&r.index>=35&&r.index<66));
  assert.equal(f.state.previous.valid,true);assert.equal(f.journal.pending(),null);
});
function approveRecovery(f) {
  const candidate=depthCandidate(f.records,f.now(),start);
  f.state.previous={ratio:1_000_000_000_000n,start:BigInt(start-2100),end:BigInt(start-300),invalidated_at:BigInt(start),valid:false};
  f.state.recovery={admin:contract(4),reference_ratio:BigInt(candidate.ratio),proposed_at:BigInt(start),expires_at:BigInt(start+7200),recovered_end:0n,cancelled:false};
}
test('approved recovery signs only the reporter method; keeper never signs admin recovery actions',async t=>{
  const f=await withPublisher(t);approveRecovery(f);
  const result=await f.publisher.cycle(f.input);
  assert.equal(result.method,'publish_recovery_observation');assert.equal(f.state.sent.length,1);
  const call=f.state.sent[0].operations[0].func.invokeContract();
  assert.equal(call.functionName().toString(),'publish_recovery_observation');
  const lines=(await readFile(f.path,'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(lines[0].method,'publish_recovery_observation');
  for(const method of ['begin_observation_recovery','cancel_observation_recovery','finish_observation_recovery'])
    await assert.rejects(f.transport.prepare(method,{}),/unexpected publication method/);
});
test('cancelled, changed, mismatched-policy or forged recovery state prevents signing',async t=>{
  for(const mutation of [f=>{f.state.recovery.cancelled=true;},f=>{f.state.recovery.reference_ratio='bogus';},
    f=>{f.state.recovery.config={};},f=>{f.state.onPrepare=()=>{f.state.recovery.reference_ratio-=1n;};}]) {
    const f=await withPublisher(t);approveRecovery(f);mutation(f);
    const outcome=await f.publisher.cycle(f.input).then(result=>({result}),error=>({error}));
    if(outcome.result)assert.equal(outcome.result.action,'hold');
    else assert(outcome.error instanceof Error);
    assert.equal(f.state.sent.length,0);assert.equal(f.journal.pending(),null);
  }
});
test('missing recovery getter and wrong RPC source fail closed before signature',async t=>{
  const f=await withPublisher(t),original=f.server.simulateTransaction;
  f.server.simulateTransaction=async tx=>tx.operations[0].func.invokeContract().functionName().toString()==='get_observation_recovery'
    ?{error:'missing getter'}:original(tx);
  await assert.rejects(f.publisher.cycle(f.input));assert.equal(f.state.sent.length,0);
  const g=await withPublisher(t);g.server.getAccount=async()=>new Account(Keypair.random().publicKey(),'1');
  await assert.rejects(g.publisher.cycle(g.input),/stage failed: verify/);assert.equal(g.state.sent.length,0);
});

// Public confirmed Testnet anchor, signatures removed. No reusable signed XDR.
const anchorUnsigned='AAAAAgAAAACgVJuJq9HJLC4B2Qc7X0mB5CPNzYVaY0Hq/TUso3ubswAB0ocASSo4AAAACwAAAAEAAAAAAAAAAAAAAABquCoSAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAAB50BSSo3pRw+ctmok0yeoKkO8wPvKaPFWGh3kS0D6m8cAAAATcHVibGlzaF9vYnNlcnZhdGlvbgAAAAAFAAAAEgAAAAAAAAAAoFSbiavRySwuAdkHO19JgeQjzc2FWmNB6v01LKN7m7MAAAASAAAAAecC7y2pqX336Lp7eE9DvPyKfIQcugfAFiiOeC2QGe/dAAAACQAAAAAAAAAAAAAA6K0CexcAAAAFAAAAAGq4IuMAAAAFAAAAAGq4KesAAAABAAAAAAAAAAAAAAAB50BSSo3pRw+ctmok0yeoKkO8wPvKaPFWGh3kS0D6m8cAAAATcHVibGlzaF9vYnNlcnZhdGlvbgAAAAAFAAAAEgAAAAAAAAAAoFSbiavRySwuAdkHO19JgeQjzc2FWmNB6v01LKN7m7MAAAASAAAAAecC7y2pqX336Lp7eE9DvPyKfIQcugfAFiiOeC2QGe/dAAAACQAAAAAAAAAAAAAA6K0CexcAAAAFAAAAAGq4IuMAAAAFAAAAAGq4KesAAAAAAAAAAQAAAAAAAAAEAAAABgAAAAHnQFJKjelHD5y2aiTTJ6gqQ7zA+8po8VYaHeRLQPqbxwAAABAAAAABAAAAAgAAAA8AAAAGU291cmNlAAAAAAASAAAAAecC7y2pqX336Lp7eE9DvPyKfIQcugfAFiiOeC2QGe/dAAAAAQAAAAYAAAAB/xM2p7DG+tlxmY01f1YqcYXbx+9YyZECwxw8lO/W6rIAAAAUAAAAAQAAAAc7vVjp1XGrX+T6JbmsyubQDurHwcQq8RDNu7Fk47CAmAAAAAeIzzsEK0R9pyKnPqyUVhVCZiZhQzm3EO+LODl9KkE/4wAAAAEAAAAGAAAAAedAUkqN6UcPnLZqJNMnqCpDvMD7ymjxVhod5EtA+pvHAAAAFAAAAAEAIsjpAAAAAAAAAwAAAAAAAABL5wAAAAA=';
async function recoveryFixture(t) {
  const f=await withPublisher(t,fixture(true)),L=LEGACY_RECOVERY;
  let seq=L.sequence,keyReads=0,sends=0,included=null;
  await f.journal.intent(L.hash,'publish_observation');
  f.server.getAccount=async()=>new Account(L.source,seq);
  const history={network:async()=>Networks.TESTNET,lookup:async hash=>hash===L.anchor
    ?{hash,successful:true,envelope_xdr:anchorUnsigned}:hash===L.hash?null:included};
  f.server.sendTransaction=async tx=>{
    sends++;assert.equal(tx.signatures.length,1);
    const hash=tx.hash().toString('hex');
    seq=L.bumpTo;included={hash,successful:true,envelope_xdr:tx.toXDR()};
    return {hash,status:'PENDING'};
  };
  // Controlled fake signer for fixed public identity; never submitted to a network.
  const key={publicKey:()=>L.source,signDecorated:()=>new xdr.DecoratedSignature({hint:Buffer.alloc(4),signature:Buffer.alloc(64)})};
  const args={manifest:f.m,journal:f.journal,server:f.server,history,
    loadKey:async()=>{keyReads++;return key;},now:f.now,sleep:async()=>{}};
  return {...f,args,history,get keyReads(){return keyReads;},get sends(){return sends;},setSeq:v=>{seq=v;}};
}
test('operator Testnet fence retires only reviewed legacy intent and preserves append-only history',async t=>{
  const f=await recoveryFixture(t),result=await recoverLegacyIntent(f.args);
  assert.equal(result.hash,LEGACY_RECOVERY.hash);assert.equal(f.sends,1);assert.equal(f.keyReads,1);
  assert.equal(f.journal.pending(),null);assert.equal(f.journal.fence(),null);
  const rows=(await readFile(f.path,'utf8')).trim().split('\n').map(JSON.parse);
  assert.deepEqual(rows.map(r=>r.state),['intent','fence_intent','RETIRED']);
  assert.equal(validateFence(rows[1].fence).signatures.length,0);
  await f.journal.close();const reopened=await openPublicationJournal(f.path,depthJournalScope(f.m));
  assert.equal(reopened.pending(),null);await reopened.close();
  await assert.rejects(recoverLegacyIntent(f.args));assert.equal(f.sends,1);
});
test('lost fence response reconciles after reopening journal without another signature or send',async t=>{
  const f=await recoveryFixture(t),send=f.server.sendTransaction;
  f.server.sendTransaction=async tx=>{await send(tx);throw Error('response lost with private details');};
  await assert.rejects(recoverLegacyIntent(f.args));assert(f.journal.fence());
  await f.journal.close();const reopened=await openPublicationJournal(f.path,depthJournalScope(f.m));
  t.after(()=>reopened.close());
  await assert.rejects(createDepthPublisher({transport:f.transport,journal:reopened,now:f.now,sleep:async()=>{}}).reconcile(),/fence/);
  await recoverLegacyIntent({...f.args,journal:reopened});
  assert.equal(f.sends,1);assert.equal(f.keyReads,1);assert.equal(reopened.pending(),null);
  await reopened.close();
});
test('unknown, failed and mismatched fence confirmations never retire or resend',async t=>{
  for(const mode of ['unknown','failed','mismatch']) {
    const f=await recoveryFixture(t),lookup=f.history.lookup;
    f.history.lookup=async hash=>{
      const r=await lookup(hash);
      if(hash===LEGACY_RECOVERY.anchor||hash===LEGACY_RECOVERY.hash)return r;
      return mode==='unknown'?null:mode==='failed'?{...r,successful:false}:{...r,hash:'a'.repeat(64)};
    };
    await assert.rejects(recoverLegacyIntent(f.args));assert(f.journal.pending());assert(f.journal.fence());
    await assert.rejects(recoverLegacyIntent(f.args));assert.equal(f.sends,1);assert.equal(f.keyReads,1);
  }
});
test('wrong network, source sequence, included original, anchor, stale ledger and changed code block before key access',async t=>{
  for(const change of [f=>f.state.network=Networks.PUBLIC,f=>f.history.network=async()=>Networks.PUBLIC,
    f=>f.setSeq('20594093306413068'),f=>f.state.code=false,f=>f.state.age=61,
    f=>{const old=f.history.lookup;f.history.lookup=async h=>h===LEGACY_RECOVERY.hash?{successful:true}:old(h);},
    f=>{f.history.lookup=async()=>null;},f=>f.m.reporter=Keypair.random().publicKey()]) {
    const f=await recoveryFixture(t);change(f);
    await assert.rejects(recoverLegacyIntent(f.args));assert.equal(f.keyReads,0);assert.equal(f.sends,0);
  }
});
test('journal durability and pre-signing sequence recheck prevent unsafe fence broadcast',async t=>{
  const f=await recoveryFixture(t);await f.journal.close();
  await assert.rejects(recoverLegacyIntent(f.args));assert.equal(f.keyReads,0);assert.equal(f.sends,0);
  const g=await recoveryFixture(t),write=g.journal.fenceIntent;
  g.journal.fenceIntent=async(...a)=>{await write(...a);g.setSeq(LEGACY_RECOVERY.bumpTo);};
  await assert.rejects(recoverLegacyIntent(g.args));assert.equal(g.keyReads,0);assert.equal(g.sends,0);
});
test('future publication intents record sequence and expiry without signed envelope',async t=>{
  const f=await withPublisher(t);f.state.sendError=true;
  await assert.rejects(f.publisher.cycle(f.input));
  const p=f.journal.pending();assert.deepEqual(p.details,{source:f.m.reporter,sequence:'2',maxTime:f.now()+30});
  assert.deepEqual(Object.keys(p).sort(),['details','hash','method','scope','state']);
});
test('submission diagnostics never expose arbitrary exception fields or XDR',async()=>{
  const events=[];
  await assert.rejects(depthStage(r=>events.push(r),'send_rpc',async()=>{throw Error('SECRET signed_xdr');}),/stage failed/);
  assert(!JSON.stringify(events).includes('SECRET'));assert.equal(events[0].stage,'send_rpc');
  const result=depthDiagnostic('send_rejected',{errorResult:{result:()=>({switch:()=>({name:'txBadSeq'})})}});
  assert.equal(result.resultCode,'txBadSeq');
  assert.equal(depthDiagnostic('send_rejected',{errorResult:{result:()=>({switch:()=>({name:'SECRET/xdr'})})}}).resultCode,undefined);
});
test('retirement without a fence and malformed future metadata cannot clear pending intent',async t=>{
  const f=await recoveryFixture(t);
  await assert.rejects(f.journal.retire(LEGACY_RECOVERY.hash,'a'.repeat(64)));
  assert(f.journal.pending());
  const g=await withPublisher(t);
  for(const metadata of [{source:g.m.reporter,sequence:'-1',maxTime:g.now()},
    {source:g.m.reporter,sequence:'2',maxTime:Infinity},
    {source:'bad',sequence:'2',maxTime:g.now()}]) {
    await assert.rejects(g.journal.intent('a'.repeat(64),'publish_observation',metadata));
    assert.equal(g.journal.pending(),null);
  }
});
test('altered or signed fence records cannot be accepted as recovery evidence',async t=>{
  const f=await recoveryFixture(t);await recoverLegacyIntent(f.args);
  const rows=(await readFile(f.path,'utf8')).trim().split('\n').map(JSON.parse),r=rows[1].fence;
  for(const edit of [r=>r.hash='a'.repeat(64),r=>r.bumpTo='20594093306413070',r=>r.unsignedXdr='invalid']) {
    const copy={...r};edit(copy);assert.throws(()=>validateFence(copy));
  }
  const signed=TransactionBuilder.fromXDR(r.unsignedXdr,Networks.TESTNET);signed.sign(Keypair.random());
  assert.throws(()=>validateFence({...r,unsignedXdr:signed.toXDR()}));
});
