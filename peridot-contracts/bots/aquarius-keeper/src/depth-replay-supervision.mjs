import assert from 'node:assert/strict';

// Wait only for the explicitly armed response-loss experiment. This does not
// reconcile, sign, rebuild or retry a transaction; the new reporter process does
// its normal durable-hash reconciliation before taking any further action.
export async function waitForReplayRestart({readState,locked,now,sleep,expected,timeout=7500}) {
  const start=now();
  assert(Number.isSafeInteger(start)&&Number.isSafeInteger(timeout)&&timeout>0&&timeout<=7500);
  let last=start;
  for(;;) {
    const current=now();
    assert(Number.isSafeInteger(current)&&current>=last&&current-start<timeout,'supervision timeout/clock regression');
    last=current;
    const state=readState();
    for(const field of ['network','admin','reporter'])assert.equal(state[field],expected[field],'fixture identity changed');
    assert.deepEqual(state.ids,expected.ids,'fixture addresses changed');
    assert.deepEqual(state.hashes,expected.hashes,'fixture code changed');
    assert.equal(state.pending,null,'unresolved fixture transaction');
    assert(['fresh','response_lost'].includes(state.phase),'unexpected rehearsal phase');
    const active=locked();
    if(state.phase==='response_lost'&&!active) {
      assert(/^[a-f0-9]{64}$/.test(state.lostHash??''),'missing response-loss hash');
      return state.lostHash;
    }
    assert(active,'replay stopped without expected response loss');
    await sleep(1000);
  }
}
