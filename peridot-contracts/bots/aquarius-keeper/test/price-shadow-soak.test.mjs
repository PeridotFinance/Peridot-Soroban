import test from 'node:test';
import assert from 'node:assert/strict';
import {SHADOW_POLICY} from '../src/price-depth-shadow.mjs';
import {validateSample,assessWindow,runSoak} from '../src/price-shadow-soak.mjs';
const end=1_800_000_000;
const sample=i=>({publicationEligible:false,point:{policy:SHADOW_POLICY,timestamp:end+i*60,
  ledgerBefore:1000+i*12,ledgerAfter:1001+i*12,depth:[1000n,10000n].map(n=>({
    probeRaw:String(n*10000000n),soldForRaw:String(n*9800000n),boughtRaw:String(n*10180000n)}))},
  aquarius:{pool:'CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F',
    ledgerMin:1000+i*12,ledgerMax:1001+i*12,probeRaw:'1000000000',soldForRaw:'980000000',boughtRaw:'1018000000'}});
const rows=()=>Array.from({length:33},(_,index)=>({index,state:'ok',sample:sample(index),finishedAt:end+index*60+5}));
test('real window boundary keeps failed attempts and rejects deleted rows',()=>{
  const r=rows();assert.equal(assessWindow(r,end+1925).state,'healthy');
  r[15]={index:15,state:'unavailable'};
  assert.equal(assessWindow(r,end+1925).reason,'failed_sample_in_window');
  r.splice(15,1);assert.equal(assessWindow(r,end+1925).reason,'missing_attempt');
});
test('venue disagreement cannot be hidden by a forged comparison flag',()=>{
  const r=rows();r[20].sample.aquarius.boughtRaw='950000000';r[20].sample.comparison={agrees:true};
  assert.equal(assessWindow(r,end+1925).reason,'venue_disagreement');
});
test('sample validation rejects wrong pools, future/stale and incoherent RPC',()=>{
  const s=sample(0);assert.equal(validateSample(s,end+5).agrees,true);
  assert.throws(()=>validateSample(s,end+61),/stale/);
  assert.throws(()=>validateSample(s,end-1),/future/);
  s.aquarius.ledgerMax+=10;assert.throws(()=>validateSample(s,end+5),/RPC/);
  s.aquarius.pool='wrong';assert.throws(()=>validateSample(s,end+5),/pool/);
});
test('bounded scheduler records failures, recovers only after window ages out, and never publishes',async()=>{
  let ms=0,index=0;const emitted=[];
  const summary=await runSoak({minutes:65,now:()=>end+Math.floor(ms/1000),monotonic:()=>ms,
    sleep:async n=>{ms+=n;},collect:async()=>{const i=index++;ms+=1000;if(i===15)throw Error('private error text');return sample(i);},
    emit:r=>emitted.push(r)});
  assert.equal(summary.attempts,66);assert.equal(summary.collected,65);
  assert.equal(emitted.find(r=>r.kind==='sample'&&r.index===31).window.reason,'failed_sample_in_window');
  assert.equal(emitted.find(r=>r.kind==='sample'&&r.index===46).window.state,'healthy');
  assert(!JSON.stringify(emitted).includes('private error text'));
  assert(emitted.every(r=>r.publicationEligible===false));
});
test('clock jump aborts rather than reporting a complete run',async()=>{
  let ms=0,jump=0;
  await assert.rejects(runSoak({minutes:32,now:()=>end+Math.floor(ms/1000)+jump,
    monotonic:()=>ms,sleep:async n=>{ms+=n;jump=100;},collect:async()=>sample(0),emit:()=>{}}),/clock jump/);
});
test('late scheduling records missed slots instead of backfilling observations',async()=>{
  let ms=0,calls=0;const emitted=[];
  const result=await runSoak({minutes:32,now:()=>end+Math.floor(ms/1000),monotonic:()=>ms,
    sleep:async n=>{ms+=n;if(ms===60000)ms+=10000;},
    collect:async()=>{calls++;return sample(Math.floor(ms/60000));},emit:r=>emitted.push(r)});
  assert.equal(result.attempts,33);assert.equal(calls,32);
  assert.equal(emitted.find(r=>r.index===1).reason,'missed_slot');
});
test('matching fabricated venues can pass consistency guards, never establish independent truth',()=>{
  const r=rows();
  for(const row of r) {
    for(const d of row.sample.point.depth){const p=BigInt(d.probeRaw);
      d.soldForRaw=String(p*95n/100n);d.boughtRaw=String(p*100n/95n);}
    row.sample.aquarius.soldForRaw='950000000';row.sample.aquarius.boughtRaw='1052631578';
  }
  const result=assessWindow(r,end+1925);
  assert.equal(result.state,'healthy');assert.equal(result.publicationEligible,false);
  // This is a trust-model counterexample with fabricated input, not a live
  // manipulation exploit or a measurement of attacker funding requirements.
});
