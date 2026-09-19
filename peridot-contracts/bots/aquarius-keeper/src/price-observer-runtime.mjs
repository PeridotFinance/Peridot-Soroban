// Continuous research observer. No signer, storage of keys, or publishing path.
import assert from 'node:assert/strict';
import {assessWindow,validateSample} from './price-shadow-soak.mjs';
export async function runObserver({collect,now,monotonic,sleep,emit,signal,runId}) {
  assert(typeof runId==='string'&&runId.length>0);
  const began=monotonic(),startedAt=now(),records=[];
  const stats={scheduled:0,collected:0,agreeing:0,missed:0,collectionFailures:0,matureAttempts:0,healthyWindows:0};
  let index=0;
  const log=record=>emit({...record,runId,publicationEligible:false});
  const remember=row=>{records.push(row);if(records.length>64)records.shift();};
  log({kind:'observer_start',startedAt,cadenceSeconds:60,historyLimit:64,restartPolicy:'fresh_window_required'});
  while(!signal.aborted) {
    const due=began+index*60000;
    while(monotonic()<due&&!signal.aborted) {
      try {await sleep(Math.min(60000,due-monotonic()),signal);}
      catch(error){if(!signal.aborted)throw error;}
    }
    if(signal.aborted)break;
    assert(Math.abs((now()-startedAt)*1000-(monotonic()-began))<=5000,'clock jump');
    const current=Math.floor((monotonic()-began)/60000);
    assert(Number.isSafeInteger(current)&&current>=index,'invalid scheduler clock');
    if(current>index) {
      // One bounded gap marker accounts for EVERY missed scheduled slot. Do not
      // burst old requests or retain an unbounded backlog after a long outage.
      const count=current-index;
      stats.scheduled+=count;stats.missed+=count;
      stats.matureAttempts+=Math.max(0,current-Math.max(30,index));
      const gap={kind:'observer_gap',index:current-1,firstIndex:index,count,
        state:'unavailable',reason:'missed_slot',finishedAt:now()};
      remember(gap);log({...gap,stats:{...stats}});index=current;
    }
    const row={kind:'observer_sample',index,startedAt:now(),state:'unavailable'};
    stats.scheduled++;
    if(monotonic()-(began+index*60000)>5000){row.reason='missed_slot';stats.missed++;}
    else {
      try {
        const sample=await collect(signal);
        assert(!signal.aborted,'stopping');
        const comparison=validateSample(sample,now());
        row.state='ok';row.sample={...sample,comparison};stats.collected++;
        if(comparison.agrees)stats.agreeing++;
      } catch {
        row.reason=signal.aborted?'shutdown':'collection_or_validation_failed';stats.collectionFailures++;
      }
    }
    row.finishedAt=now();remember(row);
    row.window=assessWindow(records,now());
    if(index>=30){stats.matureAttempts++;if(row.window.state==='healthy')stats.healthyWindows++;}
    // Stats in every heartbeat preserve aggregate evidence in a bounded log tail.
    // They are per-process only; never combine separate runs as continuous history.
    log({...row,historySize:records.length,stats:{...stats}});index++;
  }
  log({kind:'observer_stop',startedAt,finishedAt:now(),stats:{...stats}});
  return stats;
}
