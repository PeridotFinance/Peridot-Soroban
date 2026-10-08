import test from 'node:test';
import assert from 'node:assert/strict';
import {availability,history} from '../src/price-diagnostics.mjs';
const end=1_800_000_000_000,start=end-3600000;
const rows=()=>Array.from({length:12},(_,i)=>({timestamp:String(start+i*300000),trade_count:3,base_volume:'200',counter_volume:'200'}));
test('historical availability counts complete windows, volume and missing intervals',()=>{
  const good=availability(rows(),start,end);assert.equal(good.windows,7);assert.equal(good.passed,7);assert.equal(good.totalYxlmRaw,'24000000000');
  const gap=rows();gap.splice(5,1);const bad=availability(gap,start,end);assert.equal(bad.passed,1);assert.equal(bad.failures['incomplete window'],6);assert.equal(bad.longestFailedWindowRunSeconds,1800);
});
test('all-empty history is unavailable, never zero-price or forward-filled',()=>{
  const r=availability([],start,end);assert.equal(r.passed,0);assert.equal(r.failures['incomplete window'],7);
  assert.throws(()=>availability([],end-8*86400000,end),/seven days/);
});
test('duplicate, unordered and malformed data do not masquerade as liquidity gaps',()=>{
  const duplicate=rows();duplicate[1]=duplicate[0];assert.throws(()=>availability(duplicate,start,end),/duplicate/);
  const bad=rows();bad[0].base_volume='NaN';assert.throws(()=>availability(bad,start,end),/decimal/);
  assert.throws(()=>availability(rows().reverse(),start,end),/unordered/);
});
test('history is network-bound and rejects truncated or out-of-range pages',async()=>{
  const args={root:'https://example.com',networkPassphrase:'network',startMs:start,endMs:end};
  await assert.rejects(history({...args,get:async()=>({network_passphrase:'wrong'})}),/network/);
  const get=async url=>url.pathname==='/'?{network_passphrase:'network'}:{_embedded:{records:rows()}};
  assert.equal((await history({...args,get})).length,12);
  await assert.rejects(history({...args,get:async url=>url.pathname==='/'?get(url):{_embedded:{records:Array(200).fill(rows()[0])}}}),/truncated/);
  await assert.rejects(history({...args,get:async url=>url.pathname==='/'?get(url):{_embedded:{records:[{...rows()[0],timestamp:String(end)}]}}}),/outside/);
});
