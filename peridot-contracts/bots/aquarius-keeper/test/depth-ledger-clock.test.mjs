import test from 'node:test';
import assert from 'node:assert/strict';
import {requireClosedLedger} from '../src/depth-ledger-clock.mjs';
test('slightly ahead ledger is waited for, never accepted while future',async()=>{
  let now=1000,delay=0;
  await requireClosedLedger({closeTime:'1002'},()=>now,async ms=>{delay=ms;now+=ms/1000;});
  assert.equal(delay,2000);assert.equal(now,1002);
});
test('fresh ledger has no delay; future, stale and malformed clocks fail closed',async()=>{
  await requireClosedLedger({closeTime:'1000'},()=>1000,()=>assert.fail('unexpected wait'));
  for(const closeTime of ['1006','939','NaN','0'])await assert.rejects(requireClosedLedger({closeTime},()=>1000,()=>assert.fail('unexpected wait')));
  await assert.rejects(requireClosedLedger({closeTime:'1002'},()=>1000,async()=>{}));
  let now=1000;
  await assert.rejects(requireClosedLedger({closeTime:'1002'},()=>now,async()=>{now=1100;}));
});
