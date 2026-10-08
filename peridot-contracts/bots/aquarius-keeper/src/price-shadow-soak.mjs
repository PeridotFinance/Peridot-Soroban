// Bounded, read-only research. Never imported by a transaction publisher.
import assert from 'node:assert/strict';
import {compareAquarius,depthWindow} from './price-depth-shadow.mjs';
import {near} from './observation.mjs';
const POOL='CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F';

export function validateSample(sample, now) {
  assert.equal(sample.publicationEligible,false,'shadow-only sample required');
  const {point,aquarius}=sample;
  assert.equal(aquarius.pool,POOL,'wrong Aquarius pool');
  assert(Number.isSafeInteger(now)&&point.timestamp<=now&&now-point.timestamp<=60,'stale/future point');
  assert([aquarius.ledgerMin,aquarius.ledgerMax].every(Number.isSafeInteger)
    && aquarius.ledgerMin>=point.ledgerBefore && aquarius.ledgerMax>=aquarius.ledgerMin
    && aquarius.ledgerMax-aquarius.ledgerMin<=6
    && aquarius.ledgerMax-point.ledgerBefore<=12,'incoherent RPC sample');
  // Recompute; never trust a stored comparison.agrees flag.
  return compareAquarius(point,aquarius);
}

export function assessWindow(records, now) {
  const unavailable=reason=>({state:'unavailable',reason,publicationEligible:false});
  const last=records.at(-1);
  if(!last || last.state!=='ok')return unavailable('latest_sample_unavailable');
  const start=last.sample.point.timestamp-1800;
  // Keep the boundary sample and EVERY subsequent attempted slot, including
  // failures. Removing bad rows must never manufacture uninterrupted coverage.
  let first=-1;
  for(let i=0;i<records.length;i++) {
    if(records[i].state==='ok'&&records[i].sample.point.timestamp<=start)first=i;
  }
  if(first<0)return {state:'warming',publicationEligible:false};
  const selected=records.slice(first);
  if(selected.some(r=>r.state!=='ok'))return unavailable('failed_sample_in_window');
  if(selected.some((r,i)=>i>0&&r.index!==selected[i-1].index+1))return unavailable('missing_attempt');
  try {
    const window=depthWindow(selected.map(r=>r.sample.point),now);
    for(const row of selected) {
      // Validate historical points at their recorded collection time, not now.
      const comparison=validateSample(row.sample,row.finishedAt);
      if(!comparison.agrees)return unavailable('venue_disagreement');
      if(!near(BigInt(comparison.aquariusBid),BigInt(window.ratio),100n)
        || !near(BigInt(comparison.aquariusAsk),BigInt(window.ratio),100n))
        return unavailable('aquarius_window_deviation');
    }
    return {...window,state:'healthy',samples:selected.length,publicationEligible:false,
      limitation:'Observed shadow window only; no economic safety or production availability approval.'};
  } catch {
    return unavailable('window_guard_failed');
  }
}

export async function runSoak({minutes=35,collect,now,monotonic,sleep,emit}) {
  assert(Number.isSafeInteger(minutes)&&minutes>=32&&minutes<=120,'duration must be32–120minutes');
  const began=monotonic(),wallStart=now(),records=[];
  emit({kind:'start',minutes,cadenceSeconds:60,startedAt:wallStart,publicationEligible:false});
  for(let index=0;index<=minutes;index++) {
    const due=began+index*60000;
    while(monotonic()<due)await sleep(Math.min(60000,due-monotonic()));
    const row={kind:'sample',index,startedAt:now(),state:'unavailable',publicationEligible:false};
    if(Math.abs((now()-wallStart)*1000-(monotonic()-began))>5000) {
      emit({...row,reason:'clock_jump'});
      throw Error('clock jump: stopped without a completed summary');
    }
    if(monotonic()-due>5000)row.reason='missed_slot';
    else {
      try {
        const sample=await collect();
        const comparison=validateSample(sample,now());
        row.state='ok';row.sample={...sample,comparison};
      } catch {
        // Do not serialize SDK/HTTP error objects or subprocess output.
        row.reason='collection_or_validation_failed';
      }
    }
    row.finishedAt=now();records.push(row);
    row.window=assessWindow(records,now());
    emit(row);
  }
  const matured=records.filter(r=>r.index>=30);
  const summary={kind:'summary',startedAt:wallStart,finishedAt:now(),attempts:records.length,
    collected:records.filter(r=>r.state==='ok').length,
    agreeingSamples:records.filter(r=>r.state==='ok'&&r.sample.comparison.agrees).length,
    matureAttempts:matured.length,healthyWindows:matured.filter(r=>r.window.state==='healthy').length,
    windowStates:matured.reduce((out,r)=>{const key=r.window.reason??r.window.state;out[key]=(out[key]??0)+1;return out;},{}),
    publicationEligible:false,limitation:'Short bounded shadow run; overlapping windows are not independent trials. No manipulation-cost lower bound or lending approval.'};
  emit(summary);return summary;
}
