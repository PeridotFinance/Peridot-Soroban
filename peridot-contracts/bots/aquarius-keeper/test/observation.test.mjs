import test from 'node:test';
import assert from 'node:assert/strict';
import {raw7,windowPrice,crossCheck,runObservationCycle,SCALE} from '../src/observation.mjs';
const end=1_800_000_000;
const records=()=>Array.from({length:6},(_,i)=>({timestamp:String((end-1800+i*300)*1000),trade_count:10,base_volume:'200.0000000',counter_volume:'199.8000000'}));
test('complete equal-duration windows use exact rational volume prices',()=>{
  const r=windowPrice(records(),end,end+20);assert.equal(r.ratio,SCALE*999n/1000n);
  assert.equal(r.end,BigInt(end));assert.equal(r.start,BigInt(end-1800));
  assert.equal(raw7('0.0000001'),1n);
});
test('malformed decimal, incomplete, duplicate, stale and future observations fail closed',()=>{
  for(const x of ['1e4','-1','NaN','0.00000001',' 1','1.'])assert.throws(()=>raw7(x));
  assert.throws(()=>windowPrice(records().slice(1),end,end+20));
  const duplicate=records();duplicate[1].timestamp=duplicate[0].timestamp;
  assert.throws(()=>windowPrice(duplicate,end,end+20));
  assert.throws(()=>windowPrice(records(),end,end+301));
  assert.throws(()=>windowPrice(records(),end,end-1));
});
test('thin, discontinuous and volatile windows cannot bootstrap a price',()=>{
  const thin=records();thin[0].trade_count=1;assert.throws(()=>windowPrice(thin,end,end));
  const volume=records().map(r=>({...r,base_volume:'10',counter_volume:'10'}));assert.throws(()=>windowPrice(volume,end,end));
  const unstable=records();unstable[0].counter_volume='170';assert.throws(()=>windowPrice(unstable,end,end));
});
test('both executable directions cross-check but never change SDEX report',()=>{
  const r=windowPrice(records(),end,end);assert.equal(crossCheck(r,999n,1001n,1000n),r);
  assert.throws(()=>crossCheck(r,999n,950n,1000n));assert.throws(()=>crossCheck(r,0n,1001n,1000n));
});
function cycle(previous=null) { const calls=[];return {calls,args:{readPrevious:async()=>previous,
  collect:async()=>windowPrice(records(),end,end+20),quotes:async()=>({sell:999n,buy:1001n,probe:1000n}),
  publish:async r=>calls.push(['publish',r]),invalidate:async()=>calls.push(['invalidate']),now:()=>end+20}}; }
test('healthy publication does not require local history',async()=>{const f=cycle();assert.equal((await runObservationCycle(f.args)).state,'published');assert.equal(f.calls.length,1);});
test('failed crosscheck invalidates once; a stopped keeper relies on expiry',async()=>{
  const f=cycle({valid:true,end:BigInt(end-300),invalidated_at:0n});f.args.quotes=async()=>{throw Error('down');};
  assert.equal((await runObservationCycle(f.args)).state,'halted');assert.deepEqual(f.calls,[['invalidate']]);
  f.calls.length=0;f.args.readPrevious=async()=>({valid:false});await runObservationCycle(f.args);assert.equal(f.calls.length,0);
});
test('old reports cannot be republished after restart or invalidation',async()=>{
  for(const p of [{valid:true,end:BigInt(end),invalidated_at:0n},{valid:false,end:0n,invalidated_at:BigInt(end+1)}]) {
    const f=cycle(p);assert.equal((await runObservationCycle(f.args)).state,'waiting');assert.equal(f.calls.length,0);
  }
});
test('unknown submission is fatal and not converted into an automatic retry',async()=>{
  const f=cycle();f.args.publish=async()=>{throw Error('unknown hash');};
  await assert.rejects(runObservationCycle(f.args),/unknown hash/);assert.equal(f.calls.length,0);
});
