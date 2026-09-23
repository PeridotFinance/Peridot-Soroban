// Explicit isolated TESTNET harness. No Mainnet secrets, writes or cloud changes.
// Modes: deploy, calibrate, run, audit. Public evidence is resumable; unknown hashes stop.
import assert from 'node:assert/strict';
import {execFileSync,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {readFileSync,writeFileSync,existsSync,mkdirSync,openSync,writeSync,fsyncSync,closeSync,unlinkSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {setTimeout as sleep} from 'node:timers/promises';
import * as S from '@stellar/stellar-sdk';
import {runObserver} from '../src/price-observer-runtime.mjs';
import {collectorOutput,diagnosticError} from '../src/price-observer-errors.mjs';
import {validateSample} from '../src/price-shadow-soak.mjs';
import {requireClosedLedger} from '../src/depth-ledger-clock.mjs';
import {createDepthTransport,createDepthPublisher,depthJournalScope,validateDepthManifest} from '../src/depth-publisher.mjs';
import {openPublicationJournal} from '../src/depth-publication-journal.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
const dir=resolve(root,'target/depth-testnet-replay');
const rpcUrl='https://soroban-testnet.stellar.org';
const admin='GCRV2PW4LGWTOGUGQVVQJBRRJZICGPRSKMDFLBP6G2IKCFU4MUTTRT3G';
const reporter='GCQFJG4JVPI4SLBOAHMQOO27JGA6II6NZWCVUY2B5L6TKLFDPON3HND7';
const network=S.Networks.TESTNET,quote=S.Asset.native().contractId(network);
const server=new S.rpc.Server(rpcUrl,{timeout:10000});
const mode=process.argv[2];
assert(['deploy','calibrate','run','audit'].includes(mode)&&process.argv.length===3,'choose deploy/calibrate/run/audit');
if(mode!=='audit')assert.equal(process.env.CONFIRM_DEPTH_TESTNET,'ISOLATED_DEPTH_REPLAY');
const encode=v=>JSON.stringify(v,(_,x)=>typeof x==='bigint'?x.toString():x);
const now=()=>Math.floor(Date.now()/1000);
mkdirSync(dir,{recursive:true});
const statePath=resolve(dir,'state.json');
const state=existsSync(statePath)?JSON.parse(readFileSync(statePath,'utf8')):
  {network,admin,reporter,ids:{},hashes:{},done:{},pending:null,phase:'fresh'};
assert(state.network===network&&state.admin===admin&&state.reporter===reporter);
const save=()=>{const fd=openSync(statePath,'w',0o600);try{writeSync(fd,encode(state)+'\n');fsyncSync(fd);}finally{closeSync(fd);}};
const emit=value=>{
  const line=encode({time:new Date().toISOString(),...value})+'\n';
  const fd=openSync(resolve(dir,'evidence.jsonl'),'a',0o600);
  try{writeSync(fd,line);fsyncSync(fd);}finally{closeSync(fd);}
  process.stdout.write(line);
};
const keys=new Map(),specs=new Map();
function key(role) {
  if(!keys.has(role)) {
    assert(['admin','reporter'].includes(role));
    const value=S.Keypair.fromSecret(execFileSync('stellar',['keys','secret',`peridot-depth-20260921-${role}`,
      '--config-dir',resolve(dir,'stellar')],{encoding:'utf8',stdio:['ignore','pipe','pipe']}).trim());
    assert.equal(value.publicKey(),role==='admin'?admin:reporter);
    keys.set(role,value);
  }
  return keys.get(role);
}
async function checkNetwork() {
  assert.equal((await server.getNetwork()).passphrase,network);
  const l=await server.getLatestLedger();
  await requireClosedLedger(l,now);
  return l;
}
async function build(op,role='admin') {
  const who=role==='admin'?admin:reporter;
  const account=await server.getAccount(who);assert.equal(account.accountId(),who);
  return new S.TransactionBuilder(account,{networkPassphrase:network,fee:'100000'}).addOperation(op).setTimeout(60).build();
}
async function reconcile() {
  if(!state.pending)return;
  const r=await server.getTransaction(state.pending.hash);
  assert.equal(r.txHash,state.pending.hash);
  assert.equal(r.status,'SUCCESS','pending fixture transaction requires manual reconciliation');
  state.done[state.pending.label]={hash:state.pending.hash,ledger:r.ledger,value:S.scValToNative(r.returnValue)};
  emit({kind:'fixture_reconciled',...state.pending,ledger:r.ledger});state.pending=null;save();
}
async function submit(label,op,role='admin') {
  await checkNetwork();await reconcile();
  if(state.done[label])return state.done[label].value;
  const tx=await build(op,role),sim=await server.simulateTransaction(tx);
  if(!S.rpc.Api.isSimulationSuccess(sim)||sim.restorePreamble||!sim.result) {
    emit({kind:'fixture_simulation_failed',label,error:sim.error??'restoration_or_missing_result'});
    throw Error('fixture simulation failed');
  }
  for(const auth of sim.result.auth??[])assert.equal(auth.credentials().switch().name,'sorobanCredentialsSourceAccount');
  const prepared=S.rpc.assembleTransaction(tx,sim).build();
  emit({kind:'fixture_simulated',label,fee:prepared.fee});
  // Long initial storage TTL can cost tens of faucet XLM. This fixture cap is
  // unrelated to Mainnet's budget; the actual publisher retains its0.1XLM cap.
  const cap=10_000_000_000n; //1000 faucet XLM, fixed Testnet endpoint/network only.
  assert(BigInt(prepared.fee)<=cap,'Testnet fee guard');
  const hash=prepared.hash().toString('hex');state.pending={label,hash,role};save();
  emit({kind:'fixture_intent',label,hash,fee:prepared.fee});
  prepared.sign(key(role));
  const sent=await server.sendTransaction(prepared);
  assert.equal(sent.hash,hash);assert(['PENDING','DUPLICATE'].includes(sent.status),'unresolved submission');
  for(let i=0;i<40;i++) {
    const r=await server.getTransaction(hash);
    if(r.status!=='NOT_FOUND') {await reconcile();return state.done[label].value;}
    await sleep(1000);
  }
  throw Error('unresolved fixture transaction');
}
async function abi(id) {
  assert(Object.values(state.ids).includes(id),'outside isolated fixture');
  if(!specs.has(id))specs.set(id,S.contract.Spec.fromWasm(await server.getContractWasmByContractId(id)));
  return specs.get(id);
}
async function op(id,method,args) {return new S.Contract(id).call(method,...(await abi(id)).funcArgsToScVals(method,args));}
async function read(id,method,args={}) {
  const sim=await server.simulateTransaction(await build(await op(id,method,args)));
  assert(S.rpc.Api.isSimulationSuccess(sim)&&!sim.restorePreamble&&sim.result,`read ${method} failed`);
  return S.scValToNative(sim.result.retval);
}
async function write(label,id,method,args={},role='admin') {return submit(label,await op(id,method,args),role);}
async function upload(name) {
  const wasm=readFileSync(resolve(dir,'artifacts',`${name}.wasm`));
  const hash=S.hash(wasm).toString('hex');
  if(state.hashes[name])assert.equal(state.hashes[name],hash,'artifact changed');
  state.hashes[name]=hash;save();
  await submit(`upload:${name}`,S.Operation.uploadContractWasm({wasm}));return hash;
}
async function deploy(name,hash,args=[]) {
  const salt=S.hash(Buffer.from(`peridot-depth-testnet-20260921:${name}`));
  const preimage=S.xdr.ContractIdPreimage.contractIdPreimageFromAddress(
    new S.xdr.ContractIdPreimageFromAddress({address:new S.Address(admin).toScAddress(),salt}));
  const func=S.xdr.HostFunction.hostFunctionTypeCreateContractV2(new S.xdr.CreateContractArgsV2({
    contractIdPreimage:preimage,executable:S.xdr.ContractExecutable.contractExecutableWasm(Buffer.from(hash,'hex')),constructorArgs:args}));
  const id=await submit(`deploy:${name}`,S.Operation.invokeHostFunction({func,auth:[]}));
  if(state.ids[name])assert.equal(state.ids[name],id);state.ids[name]=id;save();return id;
}
const addr=a=>new S.Address(a).toScVal();
function manifest() {return validateDepthManifest({kind:'isolated-depth-price-replay-v1',network,reporter,
  router:state.ids.router,asset:state.ids.asset,quote,pool:state.ids.pool,inIndex:0,
  routerWasmHash:state.hashes.price_router,poolWasmHash:state.hashes.mock_depth_fixture});}
async function setup() {
  for(const address of [admin,reporter]) {
    const account=await fetch(`https://horizon-testnet.stellar.org/accounts/${address}`);
    if(account.status===404)assert((await fetch(`https://friendbot.stellar.org?addr=${address}`)).ok,'Friendbot failed');
    else assert(account.ok,'account preflight failed');
  }
  const mockHash=await upload('mock_depth_fixture'),routerHash=await upload('price_router');
  await deploy('asset',mockHash,[addr(admin),addr(quote),addr(quote)]);
  await deploy('pool',mockHash,[addr(admin),addr(state.ids.asset),addr(quote)]);
  await deploy('router',routerHash);
  await write('initialize',state.ids.router,'initialize',{admin,upstream:state.ids.pool,resolution:60});
  await write('configure',state.ids.router,'set_source',{caller:admin,asset:state.ids.asset,source:{tag:'Observed',values:[{
    reporter,quote_to:quote,pool:state.ids.pool,in_idx:0,out_idx:1,probe_amount:1_000_000_000n,
    window_secs:1800n,max_age_secs:300n,min_interval_secs:120n,max_deviation_bps:100,max_step_bps:100,
    min_ratio:800_000_000_000n,max_ratio:1_050_000_000_000n}]}});
  await write('dependency',state.ids.router,'set_required_observation',{caller:admin,asset:quote,required:state.ids.asset});
  writeFileSync(resolve(dir,'manifest.json'),encode(manifest())+'\n',{mode:0o600});
  emit({kind:'fixture_ready',manifest:manifest(),admin,mocks:'quotes/upstream/asset metadata; no real Testnet yXLM market'});
}
async function prices() {
  return {observed:await read(state.ids.router,'get_observation',{asset:state.ids.asset}),
    recovery:await read(state.ids.router,'get_observation_recovery',{asset:state.ids.asset}),
    dependent:await read(state.ids.router,'price_snapshot',{asset:{tag:'Stellar',values:[quote]}})};
}
async function audit() {
  for(const [name,id] of Object.entries(state.ids)) {
    const hash=S.hash(await server.getContractWasmByContractId(id)).toString('hex');
    assert.equal(hash,state.hashes[name==='router'?'price_router':'mock_depth_fixture'],'deployed code mismatch');
  }
  const transactions=[];
  for(const [label,v] of Object.entries(state.done)) {
    const r=await server.getTransaction(v.hash);assert.equal(r.status,'SUCCESS');assert.equal(r.ledger,v.ledger);
    transactions.push({label,hash:v.hash,ledger:r.ledger});
  }
  const path=resolve(dir,'publisher.jsonl');
  if(existsSync(path))for(const r of readFileSync(path,'utf8').trim().split('\n').filter(Boolean).map(JSON.parse)) {
    if(r.state!=='intent')continue;
    const result=await server.getTransaction(r.hash);assert.equal(result.status,'SUCCESS');
    transactions.push({method:r.method,hash:r.hash,ledger:result.ledger});
  }
  emit({kind:'independent_audit',phase:state.phase,transactions,postState:await prices(),pending:state.pending});
}
async function calibrate() {
  // Explicit disposable-fixture setup only, before ANY published history/debt.
  // Never automatically follow the market during a replay to hide divergence.
  assert.equal(state.phase,'fresh');
  assert.equal(state.ids.pool,'CD7RGNVHWDDPVWLRTGGTK72WFJYYLW6H55MMTEICYMODZFHP23VLESWL');
  await audit();
  const before=await prices();
  assert.equal(before.observed.end,0n);assert.equal(before.recovery,null);
  const result=await promisify(execFile)(process.execPath,[resolve(root,'bots/aquarius-keeper/src/price-shadow.mjs')],
    {timeout:45000,killSignal:'SIGKILL',maxBuffer:65536,env:{PATH:process.env.PATH??''}});
  const sample=collectorOutput(result.stdout),comparison=validateSample(sample,now());
  assert(comparison.agrees,'real venue disagreement');
  const ratio=BigInt(comparison.referenceRatio);
  emit({kind:'testnet_mock_calibration',ratio,sample,limitation:'controlled quote stub, NOT independent market validation'});
  await write(`calibrate:${sample.point.timestamp}`,state.ids.pool,'set_ratio',{ratio});
  const sold=await read(state.ids.pool,'estimate_swap',{in_idx:0,out_idx:1,in_amount:1_000_000_000n});
  assert.equal(sold,ratio/1000n);
  emit({kind:'testnet_mock_calibrated',ratio,sold});
}
async function runReplay() {
  assert(['fresh','response_lost','recovery'].includes(state.phase),'completed/unknown run requires operator review');
  const m=manifest();
  const journal=await openPublicationJournal(resolve(dir,'publisher.jsonl'),depthJournalScope(m));
  let publisher,dropResponse=state.phase==='fresh',count=0;
  const transport=createDepthTransport({manifest:m,server,key:key('reporter'),now});
  // Fault injection only at the response boundary. Real server receives the tx
  // once; the publisher sees a lost response and must reconcile its public hash.
  const actualSend=server.sendTransaction.bind(server);
  server.sendTransaction=async tx=>{const r=await actualSend(tx);if(dropResponse){dropResponse=false;throw Error('controlled response loss');}return r;};
  const makePublisher=()=>createDepthPublisher({transport,journal,now,sleep,emit});
  const run=promisify(execFile),started=now();
  try {
    publisher=makePublisher();await publisher.reconcile();
    if(state.phase==='response_lost') {
      assert.equal(journal.pending(),null);
      const transaction=await server.getTransaction(state.lostHash);
      assert.equal(transaction.status,'SUCCESS');
      const good=await prices();assert(good.dependent,'first publication expired before restart verification');
      emit({kind:'response_loss_reconciled_after_process_restart',hash:state.lostHash,postState:good});
      // Deliberately unavailable collector input exercises actual invalidation.
      await publisher.cycle({records:[],startedAt:now()});
      const outage=await prices();assert.equal(outage.dependent,null);assert.equal(outage.observed.valid,false);
      await write('begin_recovery',state.ids.router,'begin_observation_recovery',
        {caller:admin,asset:state.ids.asset,reference_ratio:good.observed.ratio});
      state.phase='recovery';save();emit({kind:'outage_and_restart',postState:await prices(),freshWindowRequired:true});
    }
    while(now()-started<7200) {
      const controller=new AbortController();
      const stop=()=>controller.abort();process.once('SIGINT',stop);process.once('SIGTERM',stop);
      try {
        await runObserver({signal:controller.signal,runId:randomUUID(),now,monotonic:()=>performance.now(),emit,
          sleep:(ms,signal)=>sleep(ms,undefined,{signal}),
          collect:async signal=>{
            let result;
            try {
              result=await run(process.execPath,[resolve(root,'bots/aquarius-keeper/src/price-shadow.mjs')],
                {timeout:45000,killSignal:'SIGKILL',maxBuffer:65536,env:{PATH:process.env.PATH??''},signal});
            } catch(error) {
              if(error?.killed)throw diagnosticError({stage:'subprocess',reason:'process_timeout'});
              try {collectorOutput(error?.stdout);}catch(diagnostic){if(diagnostic?.diagnostic)throw diagnostic;}
              throw diagnosticError({stage:'subprocess',reason:'process_failed'});
            }
            return collectorOutput(result.stdout);
          },
          afterSample:async input=>{
            if(now()-started>=7200){controller.abort();return;}
            let result;
            try {result=await publisher.cycle(input);}
            catch(error) {
              if(journal.pending()&&!dropResponse&&state.phase==='fresh') {
                state.lostHash=journal.pending().hash;state.phase='response_lost';save();controller.abort();return;
              }
              throw error;
            }
            emit({kind:'live_depth_cycle',...result,phase:state.phase});
            if(result.action!=='confirmed')return;
            const post=await prices();emit({kind:'live_depth_poststate',...post});
            if(state.phase==='recovery'&&post.recovery?.recovered_end>0n&&post.observed.end>=post.recovery.recovered_end+300n) {
              assert.equal(post.dependent,null,'recovery must stay locked');
              await write('finish_recovery',state.ids.router,'finish_observation_recovery',{caller:admin,asset:state.ids.asset});
              const final=await prices();assert(final.dependent,'completed recovery must resume pricing');
              state.phase='complete';save();emit({kind:'live_recovery_complete',...final});controller.abort();
            }
            if(state.phase==='response_lost')throw Error('restart preparation missing');
            count++;
          }});
      } finally {process.removeListener('SIGINT',stop);process.removeListener('SIGTERM',stop);}
      if(state.phase==='complete')break;
      // Leave the unresolved public hash durable; restart is a NEW Node process,
      // not just a new object reusing the previous observer's memory/history.
      if(state.phase==='response_lost')break;
      break; // operator stop or bounded timeout, never pretend completion.
    }
  } finally {server.sendTransaction=actualSend;await journal.close();}
  emit({kind:'rehearsal_stopped',phase:state.phase,confirmedCycles:count});
}
let lock;
try {
  if(mode!=='audit')lock=openSync(resolve(dir,'harness.lock'),'wx',0o600);
  await checkNetwork();
  if(mode==='deploy'){await reconcile();await setup();}
  if(mode==='calibrate'){await reconcile();await calibrate();}
  if(mode==='run'){await reconcile();await runReplay();}
  if(mode==='audit')await audit();
}catch(error){emit({kind:'rehearsal_fatal',category:error instanceof assert.AssertionError?'assertion':'operation',
  instruction:'Inspect public state/pending hashes; do not resubmit uncertain transactions.'});process.exitCode=1;}
finally{if(lock!==undefined){closeSync(lock);unlinkSync(resolve(dir,'harness.lock'));}}
