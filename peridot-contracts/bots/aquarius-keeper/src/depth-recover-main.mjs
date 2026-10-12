// Explicit operator command, separate from the continuous publisher entrypoint.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {setTimeout as sleep} from 'node:timers/promises';
import {Keypair,rpc} from '@stellar/stellar-sdk';
import {validateDepthManifest,depthJournalScope} from './depth-publisher.mjs';
import {openPublicationJournal} from './depth-publication-journal.mjs';
import {LEGACY_RECOVERY,recoverLegacyIntent} from './depth-intent-recovery.mjs';
let journal;
try {
  assert.equal(process.argv.length,2);
  assert.equal(process.env.CONFIRM_DEPTH_TESTNET,'ISOLATED_DEPTH_REPLAY');
  assert.equal(process.env.CONFIRM_DEPTH_RECOVERY_HASH,LEGACY_RECOVERY.hash);
  const manifest=validateDepthManifest(JSON.parse(await readFile(process.env.DEPTH_TESTNET_MANIFEST,'utf8')));
  const server=new rpc.Server('https://soroban-testnet.stellar.org',{timeout:10000});
  const get=async(path,missing=false)=>{
    const r=await fetch(`https://horizon-testnet.stellar.org/${path}`,{signal:AbortSignal.timeout(10000),redirect:'error'});
    if(missing&&r.status===404)return null;
    assert.equal(r.status,200,'Testnet history unavailable');return r.json();
  };
  journal=await openPublicationJournal(process.env.DEPTH_TESTNET_JOURNAL,depthJournalScope(manifest));
  await recoverLegacyIntent({manifest,journal,server,
    history:{network:async()=>(await get('')).network_passphrase,lookup:hash=>get(`transactions/${hash}`,true)},
    loadKey:async()=>Keypair.fromSecret(process.env.DEPTH_TESTNET_REPORTER_SECRET??''),
    now:()=>Math.floor(Date.now()/1000),sleep,emit:r=>console.log(JSON.stringify(r))});
}catch {
  console.log(JSON.stringify({kind:'depth_recovery_stopped',reason:'guard_or_unresolved_fence',network:'testnet'}));
  process.exitCode=1;
}finally {await journal?.close();}
