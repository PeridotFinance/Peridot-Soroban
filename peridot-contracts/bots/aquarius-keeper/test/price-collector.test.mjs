import test from 'node:test';
import assert from 'node:assert/strict';
import {collectWindow} from '../src/price-collector.mjs';
const end=1_800_000_000;
function setup(){const urls=[];return {urls,args:{horizonUrl:'https://horizon.example',networkPassphrase:'expected',code:'yXLM',issuer:'public-issuer',now:()=>end+20,
  get:async url=>{urls.push(url);if(url.pathname==='/')return {network_passphrase:'expected'};
    if(url.pathname==='/ledgers')return {_embedded:{records:[{closed_at:new Date((end+15)*1000).toISOString()}]}};
    return {_embedded:{records:Array.from({length:6},(_,i)=>({timestamp:String((end-1800+i*300)*1000),trade_count:10,base_volume:'200',counter_volume:'200'}))}};
  }}};}
test('collector pins asset orientation, window alignment and bounded page',async()=>{
  const f=setup(),r=await collectWindow(f.args),p=f.urls[2].searchParams;
  assert.equal(r.ratio,1_000_000_000_000n);assert.equal(p.get('base_asset_code'),'yXLM');assert.equal(p.get('counter_asset_type'),'native');
  assert.equal(p.get('start_time'),String((end-1800)*1000));assert.equal(p.get('limit'),'200');
});
test('collector rejects wrong network, stale ledger, unavailable and oversized windows',async()=>{
  const wrong=setup();wrong.args.get=async()=>({network_passphrase:'wrong'});await assert.rejects(collectWindow(wrong.args),/network/);
  const stale=setup(),original=stale.args.get;stale.args.get=async url=>url.pathname==='/ledgers'?{_embedded:{records:[{closed_at:new Date((end-100)*1000).toISOString()}]}}:original(url);
  await assert.rejects(collectWindow(stale.args),/stale/);
  const down=setup();down.args.get=async()=>{throw Error('unavailable');};await assert.rejects(collectWindow(down.args),/unavailable/);
  const extra=setup(),get=extra.args.get;extra.args.get=async url=>{const r=await get(url);if(url.pathname==='/trade_aggregations')r._embedded.records.push(r._embedded.records[0]);return r;};
  await assert.rejects(collectWindow(extra.args),/incomplete/);
});
