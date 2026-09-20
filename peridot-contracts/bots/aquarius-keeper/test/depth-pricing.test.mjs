import test from 'node:test';
import assert from 'node:assert/strict';
import {depthCandidate,planDepthPublication,DEPTH_PRICING_POLICY} from '../src/depth-pricing.mjs';
import {SHADOW_POLICY} from '../src/price-depth-shadow.mjs';
const startedAt=1800000000;
function history(count=32){return Array.from({length:count},(_,index)=>{
  const t=startedAt+index*60;
  return {index,state:'ok',startedAt:t,finishedAt:t+2,sample:{publicationEligible:false,
    point:{policy:SHADOW_POLICY,timestamp:t,ledgerBefore:1000+index*12,ledgerAfter:1001+index*12,
      depth:[1000n,10000n].map(n=>({probeRaw:String(n*10000000n),soldForRaw:String(n*9800000n),boughtRaw:String(n*10180000n)}))},
    aquarius:{pool:'CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F',
      ledgerMin:1000+index*12,ledgerMax:1001+index*12,probeRaw:'1000000000',soldForRaw:'980000000',boughtRaw:'1018000000'}}};
});}
const records=history(),now=records.at(-1).finishedAt;
const build=(rs=records,time=now,start=startedAt)=>depthCandidate(rs,time,start);
const candidate=build();
const previous=(overrides={})=>({ratio:BigInt(candidate.ratio),start:BigInt(candidate.start-300),
  end:BigInt(candidate.end-300),invalidated_at:0n,valid:true,...overrides});
const initial={ratio:0n,start:0n,end:0n,valid:false,invalidated_at:BigInt(startedAt)};
const plan=(p=initial,rs=records,time=now)=>planDepthPublication({records:rs,now:time,startedAt,previous:p});
test('full depth history yields a reproducible method-tagged unsigned candidate',()=>{
  assert.equal(candidate.state,'candidate');assert.equal(candidate.policy,DEPTH_PRICING_POLICY.id);
  assert.equal(candidate.end-candidate.start,1800);assert.equal(candidate.samples,31);
  assert.match(candidate.evidenceSha256,/^[a-f0-9]{64}$/);
  assert.deepEqual(build(structuredClone(records)),candidate);
  assert.equal(candidate.publicationEligible,false);assert.equal(plan().action,'publish_candidate');
  assert.equal(plan().publicationEligible,false);
});
test('restart cannot adopt pre-process history or skip the fresh thirty minute warmup',()=>{
  assert.equal(build(records,now,now-60).state,'warming');
  assert.equal(build(history(30),startedAt+29*60+2).state,'warming');
  const prior=history().map(r=>({...r,startedAt:r.startedAt-1800}));
  assert.notEqual(build(prior).state,'candidate');
});
test('failed, omitted, repeated and reordered slots cannot manufacture coverage',()=>{
  for(const mutate of [r=>{r[15]={...r[15],state:'unavailable'};},r=>r.splice(15,1),
    r=>{r[15]=structuredClone(r[14]);},r=>{[r[15],r[16]]=[r[16],r[15]];}]){
    const rs=structuredClone(records);mutate(rs);assert.notEqual(build(rs).state,'candidate');
    assert.equal(plan(previous(),rs).action,'invalidate');
    assert.equal(plan(previous({valid:false}),rs).action,'hold');
  }
});
test('forged healthy flags cannot override spread, depth or Aquarius guards',()=>{
  for(const mutate of [r=>{r[15].sample.point.depth[1].boughtRaw='110000000000';},
    r=>{r[15].sample.aquarius.soldForRaw='800000000';},
    r=>{r[15].sample.aquarius.pool='wrong';}]){
    const rs=structuredClone(records);mutate(rs);
    for(const r of rs){r.window={state:'healthy'};r.sample.comparison={agrees:true};}
    assert.notEqual(build(rs).state,'candidate');
  }
});
test('history stays bounded; slow, future and overlapping attempts are rejected',()=>{
  assert.notEqual(build(history(65),startedAt+64*60+2).state,'candidate');
  for(const mutate of [r=>{r[15].finishedAt=r[15].startedAt+46;},r=>{r[15].finishedAt=now+1;},
    r=>{r[15].startedAt=r[14].finishedAt;},r=>{r[15].finishedAt=r[15].startedAt-1;}]){
    const rs=structuredClone(records);mutate(rs);assert.notEqual(build(rs).state,'candidate');
  }
});
test('fresh on-chain state controls replay, invalidation and minimum update interval',()=>{
  assert.equal(plan(previous()).action,'publish_candidate');
  assert.equal(plan(previous({end:BigInt(candidate.end),start:BigInt(candidate.start)})).reason,'newer_window_required');
  assert.equal(plan(previous({valid:false,invalidated_at:BigInt(candidate.end)})).reason,'newer_window_required');
  assert.equal(plan(previous({end:BigInt(candidate.end-299),start:BigInt(candidate.start-299)})).reason,'minimum_interval');
});
test('step violations are not rounded away, ramped synthetically or reset after invalidation',()=>{
  const ratio=BigInt(candidate.ratio)*98n/100n;
  assert.equal(plan(previous({ratio})).action,'invalidate');
  assert.equal(plan(previous({ratio,valid:false})).reason,'ratio_step');
  assert.equal(plan(previous({ratio,valid:false})).action,'hold');
  assert.equal(plan(previous({ratio,end:BigInt(candidate.end-60),start:BigInt(candidate.start-60)})).action,'invalidate');
});
test('stale/future inputs and malformed chain state fail closed without coercion',()=>{
  assert.notEqual(build(records,now+61).state,'candidate');
  assert.notEqual(build(records,startedAt-1).state,'candidate');
  for(const p of [null,{},previous({ratio:'980000000000'}),previous({end:BigInt(now+1)}),
    previous({invalidated_at:BigInt(now+1)}),previous({end:0n}),previous({ratio:0n})])
    assert.equal(plan(p).action,'halt');
  assert.equal(plan(previous(),records,NaN).action,'halt');
});
test('one failed sample requires a newly complete healthy window, not last-good reuse',()=>{
  const rs=history(64);rs[30]={...rs[30],state:'unavailable'};
  assert.notEqual(build(rs.slice(0,61),rs[60].finishedAt).state,'candidate');
  assert.equal(build(rs,rs.at(-1).finishedAt).state,'candidate');
});
