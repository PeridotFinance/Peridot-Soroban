// Deployed entrypoint: no contract writes, no key loading, no publish mode.
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {setTimeout as sleep} from 'node:timers/promises';
import {runObserver} from './price-observer-runtime.mjs';
import {collectorOutput,diagnosticError} from './price-observer-errors.mjs';
const controller=new AbortController();
process.once('SIGINT',()=>controller.abort());
process.once('SIGTERM',()=>controller.abort());
const log=record=>console.log(JSON.stringify(record));
try {
  assert(process.argv.length===2,'observer accepts no arguments');
  assert(!process.env.PRICE_MODE||process.env.PRICE_MODE==='observe','publication forbidden');
  assert(!Object.keys(process.env).some(k=>/(SECRET|PRIVATE_KEY|MNEMONIC|SEED|ACCESS_TOKEN|API_KEY)/i.test(k)),
    'observer must not receive credentials');
  const run=promisify(execFile);
  await runObserver({signal:controller.signal,runId:randomUUID(),
    now:()=>Math.floor(Date.now()/1000),monotonic:()=>performance.now(),
    sleep:(ms,signal)=>sleep(ms,undefined,{signal}),emit:log,
    collect:async signal=>{
      let result;
      try {
      result=await run(process.execPath,[fileURLToPath(new URL('./price-shadow.mjs',import.meta.url))],
        {timeout:45000,killSignal:'SIGKILL',maxBuffer:65536,env:{PATH:process.env.PATH??''},signal});
      } catch(error) {
        if(error?.killed)throw diagnosticError({stage:'subprocess',reason:'process_timeout'});
        // An unsuccessful child may supply ONLY an allowlisted error envelope;
        // never accept a sample from a failed process or print its output.
        try {collectorOutput(error?.stdout);}catch(diagnostic){
          if(diagnostic?.diagnostic)throw diagnostic;
        }
        throw diagnosticError({stage:'subprocess',reason:'process_failed'});
      }
      return collectorOutput(result.stdout);
    }});
} catch {
  // Never include arbitrary SDK/subprocess/environment content in errors.
  log({kind:'observer_fatal',reason:'configuration_or_runtime_guard',publicationEligible:false});
  process.exitCode=1;
}
