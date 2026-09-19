import test from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {runObserver} from '../src/price-observer-runtime.mjs';
import {SHADOW_POLICY} from '../src/price-depth-shadow.mjs';
const start=1_800_000_000;
const sample=seconds=>({publicationEligible:false,point:{policy:SHADOW_POLICY,timestamp:seconds,
  ledgerBefore:1000+(seconds-start),ledgerAfter:1001+(seconds-start),depth:[1000n,10000n].map(n=>({
    probeRaw:String(n*10000000n),soldForRaw:String(n*9800000n),boughtRaw:String(n*10180000n)}))},
  aquarius:{pool:'CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F',
    ledgerMin:1000+(seconds-start),ledgerMax:1001+(seconds-start),probeRaw:'1000000000',soldForRaw:'980000000',boughtRaw:'1018000000'}});
async function fixture({cycles=180,failAt=-1,pauseAt=-1,pauseMs=0,clockJump=false}={}){
  let ms=0,calls=0;const events=[],controller=new AbortController();
  const stats=await runObserver({runId:'test',signal:controller.signal,now:()=>start+Math.floor(ms/1000)+(clockJump&&ms>0?100:0),
    monotonic:()=>ms,sleep:async n=>{ms+=n;if(ms===pauseAt*60000)ms+=pauseMs;},
    collect:async()=>{const value=sample(start+Math.floor(ms/1000));const index=calls++;ms+=1000;if(index===failAt)throw Error('private content');return value;},
    emit:r=>{events.push(r);if(r.kind==='observer_sample'&&r.index>=cycles-1)controller.abort();}});
  return {events,stats,calls};
}
test('continuous observer retains bounded history beyond two hours and never publishes',async()=>{
  const {events,stats}=await fixture();assert.equal(stats.scheduled,180);assert.equal(stats.collected,180);
  assert.equal(stats.healthyWindows,150);assert.equal(stats.matureAttempts,150);
  assert(events.every(r=>r.publicationEligible===false));
  assert(events.filter(r=>r.kind==='observer_sample').every(r=>r.historySize<=64));
  assert.equal(events.at(-1).kind,'observer_stop');
});
test('failed sample blocks every overlapping window and then recovers',async()=>{
  const {events,stats}=await fixture({cycles:65,failAt:15});assert.equal(stats.collectionFailures,1);
  assert.equal(events.find(r=>r.kind==='observer_sample'&&r.index===31).window.reason,'failed_sample_in_window');
  assert.equal(events.find(r=>r.kind==='observer_sample'&&r.index===46).window.state,'healthy');
  assert(!JSON.stringify(events).includes('private content'));
});
test('large scheduling gaps are coalesced, counted, and never backfilled',async()=>{
  const {events,stats,calls}=await fixture({cycles:150,pauseAt:8,pauseMs:120*60000});
  const gap=events.find(r=>r.kind==='observer_gap');assert.equal(gap.count,120);
  assert.equal(gap.firstIndex,8);assert.equal(gap.index,127);assert.equal(stats.missed,120);
  assert.equal(calls,30);assert.equal(stats.scheduled,150);assert.equal(stats.matureAttempts,120);
  assert.equal(stats.healthyWindows,0);
});
test('new process starts a fresh warmup, and a wall-clock jump stops the observer',async()=>{
  const first=await fixture({cycles:40});assert(first.stats.healthyWindows>0);
  const restarted=await fixture({cycles:10});assert.equal(restarted.stats.healthyWindows,0);
  await assert.rejects(fixture({clockJump:true}),/clock jump/);
});
test('deployment image allowlist excludes signing and publisher entrypoints',()=>{
  const docker=readFileSync(new URL('../Dockerfile.observer',import.meta.url),'utf8');
  assert(!docker.includes('COPY src ./src'));
  assert(!docker.includes('src/main.mjs'));assert(!docker.includes('src/price-main.mjs'));
  assert(docker.includes('--ignore-scripts'));assert(docker.includes('USER node'));
  const spec=readFileSync(new URL('../../../.do/aquarius-price-observer.yaml',import.meta.url),'utf8');
  assert(!/^\s*envs:/m.test(spec));assert(!spec.includes('aquarius-keeper-v0.4.1'));
  assert(spec.includes('instance_count: 1'));
});
test('entrypoint refuses publishing and credential injection before any network call',()=>{
  const script=fileURLToPath(new URL('../src/price-observer-main.mjs',import.meta.url));
  for(const env of [{PRICE_MODE:'publish'},{KEEPER_SECRET:'not-a-real-secret'}]){
    assert.throws(()=>execFileSync(process.execPath,[script],{env,timeout:5000,stdio:'pipe'}),error=>{
      assert.equal(error.status,1);const result=JSON.parse(String(error.stdout));
      assert.equal(result.kind,'observer_fatal');assert.equal(result.publicationEligible,false);return true;
    });
  }
});
