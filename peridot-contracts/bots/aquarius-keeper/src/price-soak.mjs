// Public-data subprocesses only. No service deployment, keys or transactions.
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {setTimeout as sleep} from 'node:timers/promises';
import {runSoak} from './price-shadow-soak.mjs';
const args=process.argv.slice(2),minutes=Number(args[0]??35);
assert(args.length<=1&&Number.isSafeInteger(minutes)&&minutes>=32&&minutes<=120,
  'usage: node src/price-soak.mjs [minutes:32..120]');
const run=promisify(execFile),digest=createHash('sha256');
const collect=async()=>{
  // Kill a stalled attempt rather than overlap requests or stamp stale data.
  // Only PATH is inherited; keeper/deployer credentials are not forwarded.
  const {stdout}=await run(process.execPath,[fileURLToPath(new URL('./price-shadow.mjs',import.meta.url))],
    {timeout:45000,killSignal:'SIGKILL',maxBuffer:65536,env:{PATH:process.env.PATH??''}});
  return JSON.parse(stdout);
};
await runSoak({minutes,collect,now:()=>Math.floor(Date.now()/1000),
  monotonic:()=>performance.now(),sleep,emit:record=>{
    const line=JSON.stringify(record);digest.update(`${line}\n`);console.log(line);
  }});
console.log(JSON.stringify({kind:'digest',algorithm:'sha256',precedingJsonlSha256:digest.digest('hex')}));
