import test from 'node:test';
import assert from 'node:assert/strict';
import {waitForReplayRestart} from '../src/depth-replay-supervision.mjs';
const expected={network:'testnet',admin:'admin',reporter:'reporter',ids:{router:'router'},hashes:{router:'hash'},pending:null,phase:'fresh'};
const hash='a'.repeat(64);
test('restart waits for response loss AND released process lock, never carries history',async()=>{
  let t=1000,n=0;
  const result=await waitForReplayRestart({expected,now:()=>t,sleep:async()=>{t++;n++;},
    readState:()=>({...expected,...(n>0?{phase:'response_lost',lostHash:hash}:{})}),locked:()=>n<2});
  assert.equal(result,hash);assert.equal(n,2);
});
test('unexpected state, changed fixture, stopped run and missing hash reject without restart',async()=>{
  for(const changes of [{phase:'complete'},{phase:'recovery'},{network:'mainnet'},
    {ids:{router:'other'}},{hashes:{router:'other'}},{pending:{hash}},
    {},{phase:'response_lost'},{phase:'response_lost',lostHash:'invalid'}]) {
    await assert.rejects(waitForReplayRestart({expected,now:()=>1000,sleep:()=>assert.fail('must not wait'),
      readState:()=>({...expected,...changes}),locked:()=>false}));
  }
});
test('bounded supervisor times out and rejects clock regression',async()=>{
  for(const step of [1,-1]) {
    let t=1000;
    await assert.rejects(waitForReplayRestart({expected,now:()=>t,timeout:2,sleep:async()=>{t+=step;},
      readState:()=>({...expected}),locked:()=>true}));
  }
});
