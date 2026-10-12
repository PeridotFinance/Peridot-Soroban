import test from 'node:test';
import assert from 'node:assert/strict';
import {depthPoint,depthWindow,compareAquarius,SHADOW_POLICY} from '../src/price-depth-shadow.mjs';
import {YXLM_ISSUER} from '../src/price-diagnostics.mjs';
const end=1_800_000_000;
const points=()=>Array.from({length:31},(_,i)=>({policy:SHADOW_POLICY,timestamp:end-1800+i*60,
  ledgerBefore:1000+i*12,ledgerAfter:1001+i*12,depth:[1000n,10000n].map(n=>({probeRaw:String(n*10000000n),soldForRaw:String(n*9800000n),boughtRaw:String(n*10180000n)}))}));
test('complete depth window is time weighted, fresh and explicitly non-publishable',()=>{
  const result=depthWindow(points(),end+10);assert.equal(result.publicationEligible,false);
  assert.equal(result.start,end-1800);assert.equal(result.end,end);assert(BigInt(result.ratio)<1_000_000_000_000n);
});
test('Aquarius agreement checks both directions and cannot make a shadow point publishable',()=>{
  const point=points()[0],q={probeRaw:'1000000000',soldForRaw:'980000000',boughtRaw:'1018000000'};
  assert.equal(compareAquarius(point,q).agrees,true);assert.equal(compareAquarius(point,q).publicationEligible,false);
  assert.equal(compareAquarius(point,{...q,boughtRaw:'950000000'}).agrees,false);
  assert.throws(()=>compareAquarius(point,{...q,probeRaw:'1'}),/probe/);
  assert.throws(()=>compareAquarius(point,{...q,boughtRaw:'0'}),/output/);
});
test('depth window rejects missing time, replayed ledger, stale samples and wrong probe identity',()=>{
  assert.throws(()=>depthWindow(points().slice(1),end),/observations/);
  const gap=points();gap[5].timestamp+=40;assert.throws(()=>depthWindow(gap,end),/missing/);
  const replay=points();replay[5].ledgerBefore=replay[4].ledgerBefore;replay[5].ledgerAfter=replay[4].ledgerAfter;
  assert.throws(()=>depthWindow(replay,end),/replayed/);
  assert.throws(()=>depthWindow(points(),end+61),/stale/);
  const probe=points();probe[2].depth[0].probeRaw='1';assert.throws(()=>depthWindow(probe,end),/identity/);
});
test('empty or one-sided liquidity and excessive spread never manufacture a shadow price',()=>{
  const wide=points();wide[2].depth[0].soldForRaw='9500000000';assert.throws(()=>depthWindow(wide,end),/spread/);
  const zero=points();zero[0].depth[0].boughtRaw='0';assert.throws(()=>depthWindow(zero,end),/units/);
});
test('TWAP clips an extra boundary sample without double counting and rejects large drift',()=>{
  const original=points(),baseline=depthWindow(original,end);
  const extra={...structuredClone(original[0]),timestamp:original[0].timestamp-30,
    ledgerBefore:994,ledgerAfter:995};
  assert.equal(depthWindow([extra,...original],end).ratio,baseline.ratio);
  const drifting=points();
  drifting[15].depth=drifting[15].depth.map(d=>({...d,
    soldForRaw:String(BigInt(d.probeRaw)*95n/100n),boughtRaw:String(BigInt(d.probeRaw)*105n/100n)}));
  assert.throws(()=>depthWindow(drifting,end),/unstable/);
  const invalid=points();invalid[0].ledgerBefore=0;
  assert.throws(()=>depthWindow(invalid,end),/ledger/);
});
function fakeGet(change=r=>r) {
  return async url=>{
    if(url.pathname==='/')return {network_passphrase:'Public Global Stellar Network ; September 2015'};
    if(url.pathname==='/ledgers')return {_embedded:{records:[{sequence:1000,closed_at:new Date((end-1)*1000).toISOString()}]}};
    const p=url.searchParams,sell=p.get('source_asset_type')!=='native',amount=p.get('source_amount');
    const row={path:[],source_amount:amount,destination_amount:String(Number(amount)*(sell?0.98:1.018)),
      ...(sell?{source_asset_type:'credit_alphanum4',source_asset_code:'yXLM',source_asset_issuer:YXLM_ISSUER,destination_asset_type:'native'}
        :{source_asset_type:'native',destination_asset_type:'credit_alphanum4',destination_asset_code:'yXLM',destination_asset_issuer:YXLM_ISSUER})};
    return {_embedded:{records:[change(row)]}};
  };
}
test('collector checks exact source/destination assets and refuses multihop substitutes',async()=>{
  const args={now:()=>end,get:fakeGet()};const point=await depthPoint(args);assert.equal(point.timestamp,end-1);
  await assert.rejects(depthPoint({...args,get:fakeGet(r=>({...r,path:[{asset_type:'native'}]}))}),/direct/);
  await assert.rejects(depthPoint({...args,get:fakeGet(r=>({...r,source_amount:'1'}))}),/amount/);
  await assert.rejects(depthPoint({...args,get:fakeGet(r=>({...r,destination_asset_issuer:'wrong'}))}),/asset/);
});
