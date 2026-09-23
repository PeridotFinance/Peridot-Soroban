// Attach to ONE already running, fixed isolated Testnet rehearsal. Never restart
// a failed run or unknown phase. No secret reads, signing, Mainnet/cloud access.
import assert from 'node:assert/strict';
import {readFileSync,existsSync,openSync,closeSync,unlinkSync} from 'node:fs';
import {spawn} from 'node:child_process';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {setTimeout as sleep} from 'node:timers/promises';
import {waitForReplayRestart} from '../src/depth-replay-supervision.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
const dir=resolve(root,'target/depth-testnet-replay');
const statePath=resolve(dir,'state.json'),lockPath=resolve(dir,'supervisor.lock');
assert.equal(process.argv.length,2);
assert.equal(process.env.CONFIRM_DEPTH_TESTNET,'ISOLATED_DEPTH_REPLAY');
const readState=()=>JSON.parse(readFileSync(statePath,'utf8'));
const expected=readState();
assert.equal(expected.network,'Test SDF Network ; September 2015');
assert.equal(expected.admin,'GCRV2PW4LGWTOGUGQVVQJBRRJZICGPRSKMDFLBP6G2IKCFU4MUTTRT3G');
assert.equal(expected.reporter,'GCQFJG4JVPI4SLBOAHMQOO27JGA6II6NZWCVUY2B5L6TKLFDPON3HND7');
assert.deepEqual(expected.ids,{
  asset:'CDTQF3ZNVGUX357IXJ5XQT2DXT6IU7EEDS5APQAWFCHHQLMQDHX53R2J',
  pool:'CD7RGNVHWDDPVWLRTGGTK72WFJYYLW6H55MMTEICYMODZFHP23VLESWL',
  router:'CDTUAUSKRXUUOD44WZVCJUZHVAVEHPGA7PFGR4KWDIO6IS2A7KN4P6KZ',
});
assert.deepEqual(expected.hashes,{
  mock_depth_fixture:'88cf3b042b447da722a73eac945615426626614339b710ef8b38397d2a413fe3',
  price_router:'3bbd58e9d571ab5fe4fa25b9accae6d00eeac7c1c42af110cdbbb164e3b08098',
});
const now=()=>Math.floor(Date.now()/1000);
const emit=x=>console.log(JSON.stringify({time:new Date().toISOString(),...x}));
const run=mode=>new Promise((resolveRun,reject)=>{
  const child=spawn(process.execPath,[resolve(root,'bots/aquarius-keeper/scripts/depth-testnet-rehearsal.mjs'),mode],{
    cwd:root,stdio:['ignore','inherit','inherit'],
    env:{PATH:process.env.PATH??'',CONFIRM_DEPTH_TESTNET:'ISOLATED_DEPTH_REPLAY'},
  });
  child.once('error',reject);
  child.once('exit',(code,signal)=>code===0?resolveRun():reject(Error(`child stopped ${code??signal}`)));
});
let lock;
try {
  lock=openSync(lockPath,'wx',0o600);
  emit({kind:'supervisor_attached',phase:expected.phase});
  const hash=await waitForReplayRestart({readState,locked:()=>existsSync(resolve(dir,'harness.lock')),now,sleep,expected});
  emit({kind:'supervisor_starting_new_process',hash});
  await run('run');
  assert.equal(readState().phase,'complete','recovery did not complete; no retry');
  await run('audit');
  emit({kind:'supervisor_complete'});
} catch {
  emit({kind:'supervisor_stopped',instruction:'Inspect public evidence. No automatic retry or journal reset.'});
  process.exitCode=1;
} finally {
  if(lock!==undefined){closeSync(lock);unlinkSync(lockPath);}
}
