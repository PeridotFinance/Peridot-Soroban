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
function fixture() {
  const key=Keypair.random(),records=history(),time={value:records.at(-1).finishedAt};
  const m={kind:'isolated-depth-price-replay-v1',network:Networks.TESTNET,reporter:key.publicKey(),
    router:contract(1),asset:contract(2),pool:contract(3),quote:Asset.native().contractId(Networks.TESTNET),
    routerWasmHash:'a'.repeat(64),poolWasmHash:'b'.repeat(64),inIndex:0};
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
  const transport=createDepthTransport({manifest:m,server,key,now:()=>time.value});
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
  await assert.rejects(g.publisher.cycle(g.input),/source account mismatch/);assert.equal(g.state.sent.length,0);
});
