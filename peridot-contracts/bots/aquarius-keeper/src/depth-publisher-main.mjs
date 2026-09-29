// Explicitly isolated Testnet REPLAY of Mainnet public observations. Not a
// production oracle, not included in Dockerfile.observer, no Mainnet mode.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {setTimeout as sleep} from 'node:timers/promises';
import {Keypair,rpc} from '@stellar/stellar-sdk';
import {runObserver} from './price-observer-runtime.mjs';
import {collectorOutput,diagnosticError} from './price-observer-errors.mjs';
import {validateDepthManifest,depthJournalScope,createDepthTransport,createDepthPublisher} from './depth-publisher.mjs';
import {openPublicationJournal} from './depth-publication-journal.mjs';
const controller=new AbortController();
process.once('SIGINT',()=>controller.abort());process.once('SIGTERM',()=>controller.abort());
const emit=record=>console.log(JSON.stringify(record));
let journal;
try {
  assert(process.argv.length===2&&process.env.CONFIRM_DEPTH_TESTNET==='ISOLATED_DEPTH_REPLAY','isolated Testnet confirmation required');
  const manifest=validateDepthManifest(JSON.parse(await readFile(process.env.DEPTH_TESTNET_MANIFEST,'utf8')));
  // Network/manifest fence precedes secret access. Dedicated fixture identity only.
  const server=new rpc.Server('https://soroban-testnet.stellar.org',{timeout:10_000});
  assert((await server.getNetwork()).passphrase===manifest.network,'Testnet endpoint mismatch');
  const key=Keypair.fromSecret(process.env.DEPTH_TESTNET_REPORTER_SECRET??'');
  const now=()=>Math.floor(Date.now()/1000);
  const transport=createDepthTransport({manifest,server,key,now});
  journal=await openPublicationJournal(process.env.DEPTH_TESTNET_JOURNAL,depthJournalScope(manifest));
  const publisher=createDepthPublisher({transport,journal,now,sleep,emit});
  await publisher.reconcile();
  const run=promisify(execFile);
  await runObserver({signal:controller.signal,runId:randomUUID(),now,monotonic:()=>performance.now(),emit,
    sleep:(ms,signal)=>sleep(ms,undefined,{signal}),
    afterSample:input=>publisher.cycle(input).then(result=>emit({kind:'depth_cycle',network:'testnet',...result})),
    collect:async signal=>{
      let result;
      try {
        result=await run(process.execPath,[fileURLToPath(new URL('./price-shadow.mjs',import.meta.url))],
          {timeout:45000,killSignal:'SIGKILL',maxBuffer:65536,env:{PATH:process.env.PATH??''},signal});
      }catch(error){
        if(error?.killed)throw diagnosticError({stage:'subprocess',reason:'process_timeout'});
        try {collectorOutput(error?.stdout);}catch(diagnostic){if(diagnostic?.diagnostic)throw diagnostic;}
        throw diagnosticError({stage:'subprocess',reason:'process_failed'});
      }
      return collectorOutput(result.stdout);
    }});
}catch {
  // Never log arbitrary SDK exceptions, subprocess output, manifest or env values.
  emit({kind:'depth_fatal',reason:'guard_or_submission_failure',network:'testnet',
    instruction:'Inspect public journal; unresolved hashes must be reconciled before restart.'});
  process.exitCode=1;
}finally {try {await journal?.close();}catch {process.exitCode=1;}}
