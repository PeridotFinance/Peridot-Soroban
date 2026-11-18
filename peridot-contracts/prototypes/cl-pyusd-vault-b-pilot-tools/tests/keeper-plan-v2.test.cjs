'use strict';
// keeper-plan-v2.cjs: the v1 planner tests (with the now-mandatory reward state) plus the checkpoint_rewards rule.
const test = require('node:test');
const assert = require('node:assert/strict');
const {planNext, lastBoundaryId, RELEASE} = require('../keeper-plan-v2.cjs');
const v1 = require('../../cl-pyusd-vault-b/keeper-plan.cjs'); // read-only reference for the regression comparison
const CLOSED = {open: false, lastBoundaryId: null, interval: null, checkpointClaimable: null};
const OPEN = {open: true, lastBoundaryId: '3', interval: '2', checkpointClaimable: true};
const DOWN = {...OPEN, checkpointClaimable: false};
const snapshot = (commands, guard = {observed_ledger: 100, healthy_count: 3, inside: true}, rewards = CLOSED) =>
  ({ledger: 100, timestamp: 500, keeper: 'keeper', configuredKeeper: 'keeper', receipt: 'receipt', vault: 'vault', guard, commands, rewards});
const command = (kind, phase, deadline = '900', id = '0') => ({id, job: {kind, phase, deadline}});
const CHECKPOINT = {target: 'vault', method: 'checkpoint_rewards', args: [], reason: 'emergency exit left the reward interval open; seal it before anything else'};
const busy = () => [command(0, 1, '900', '0'), command(0, 2, '900', '1'), command(0, 3, '900', '2'), command(0, 5, '1', '3'), command(1, 10, '900', '4'),
  command(1, 11, '900', '5'), command(2, 21, '0', '6'), command(0, 1, '400', '7'), command(0, 4, '0', '8'), command(2, 22, '0', '9')];
test('three prior observations permit investment only inside ledger freshness bounds', () => {
  const p = planNext(snapshot([command(0, 3)]));
  assert.deepEqual(p.plans, [{target: 'receipt', method: 'stage_invest', args: ['0'], minLedger: 101, maxLedgerExclusive: 104}]);
  assert.equal(p.signing, false); assert.equal(p.submission, false); assert.equal(p.rewardCheckpoint.status, 'NOT_REQUIRED');
});
test('insufficient, stale or unhealthy observations require next-ledger checkpoint', () => {
  for (const guard of [null, {observed_ledger: 100, healthy_count: 2, inside: true}, {observed_ledger: 97, healthy_count: 3, inside: true}, {observed_ledger: 100, healthy_count: 3, inside: false}]) {
    const p = planNext(snapshot([command(0, 3)], guard));
    assert.deepEqual(p.plans, [{target: 'vault', method: 'stage_checkpoint_investment', args: ['keeper'], minLedger: 101, maxLedgerExclusive: 102}]);
  }
});
test('unexpired owner requests are advanced without new owner choices; payouts and refunds stay available after deadline', () => {
  const p = planNext(snapshot([command(0, 1, '900', '0'), command(0, 2, '900', '1'), command(1, 10, '900', '2'), command(1, 11, '900', '3')]));
  assert.deepEqual(p.plans.map(x => x.method), ['stage_convert', 'stage_mint', 'stage_withdraw', 'stage_close_convert']);
  assert.deepEqual(planNext(snapshot([command(2, 21, '0', '0'), command(0, 5, '1', '1')])).plans.map(x => x.method), ['stage_emergency_pay', 'stage_finalise']);
  assert.deepEqual(planNext(snapshot([command(0, 3, '900', '0'), command(0, 5, '1', '1'), command(2, 21, '0', '2')], null)).plans.map(x => x.method), ['stage_emergency_pay', 'stage_finalise', 'stage_checkpoint_investment']);
});
test('expired requests require explicit recovery; idle or terminal-only pilot requires nothing', () => {
  for (const [kind, phase] of [[0, 1], [0, 2], [0, 3], [1, 10], [1, 11]]) {
    const p = planNext(snapshot([command(kind, phase, '500')])); assert.equal(p.plans[0].action, 'OWNER_RECOVERY_REQUIRED'); assert.equal(p.plans[0].method, undefined);
  }
  assert.deepEqual(planNext(snapshot([])).plans, []);
  assert.deepEqual(planNext(snapshot([command(0, 4), command(1, 12, '0', '1'), command(2, 22, '0', '2')])).plans, []);
});
test('unknown states, incoherent ledger, duplicate commands and wrong keeper fail closed', () => {
  assert.throws(() => planNext(snapshot([command(9, 8)])), /Unsupported/);
  assert.throws(() => planNext(snapshot([command(0, 3)], {observed_ledger: 101, healthy_count: 3, inside: true})), /incoherent/);
  assert.throws(() => planNext(snapshot([command(0, 1), command(0, 2)])), /duplicate/);
  assert.throws(() => planNext({...snapshot([]), configuredKeeper: 'another'}), /differs/);
});
test('with no open reward interval the v2 plans equal the v1 planner exactly', () => {
  for (const [commands, guard] of [[busy(), undefined], [busy(), null], [[], undefined], [[command(0, 3)], {observed_ledger: 98, healthy_count: 3, inside: true}], [[command(2, 21, '0', '4'), command(1, 10, '900', '2')], undefined]]) {
    const s = snapshot(commands, guard === undefined ? {observed_ledger: 100, healthy_count: 3, inside: true} : guard), {rewards, ...old} = s;
    assert.deepEqual(planNext(s).plans, v1.planNext(old).plans);
  }
});
test('NEGATIVE the reward state is mandatory: a missing or partial read never plans as if no emergency exit happened', () => {
  for (const rewards of [undefined, null, {}, {open: 'false'}, {open: 1}, {open: true}, {open: true, checkpointClaimable: null}, {open: true, checkpointClaimable: 'true'}])
    assert.throws(() => planNext({...snapshot(busy()), rewards}), /reward interval state is required|open interval requires the simulated checkpoint_rewards result/);
  const {rewards: _, ...absent} = snapshot(busy()); assert.throws(() => planNext(absent), /reward interval state is required/);
});
test('after any emergency exit checkpoint_rewards is planned first, before payouts, refunds, observations and every other step', () => {
  const p = planNext(snapshot(busy(), null, OPEN));
  assert.deepEqual(p.plans[0], CHECKPOINT); assert.equal(p.rewardCheckpoint.status, 'PLANNED_FIRST'); assert.equal(p.executeAtMostOneThenRefresh, true);
  assert.deepEqual(p.plans.slice(1), v1.planNext((({rewards, ...old}) => old)(snapshot(busy(), null))).plans, 'the rest of the queue is unchanged and follows');
  assert.equal(p.plans.filter(x => x.method === 'checkpoint_rewards').length, 1); assert.equal(p.signing, false); assert.equal(p.submission, false);
  // The emergency payout of the very exit that opened the interval comes second.
  assert.deepEqual(planNext(snapshot([command(2, 21, '0', '6')], undefined, OPEN)).plans.map(x => x.method), ['checkpoint_rewards', 'stage_emergency_pay']);
  // An exit by a holder the keeper does not list (no commands in the snapshot), or after the payout: still planned.
  const idle = planNext(snapshot([], undefined, OPEN)); assert.deepEqual(idle.plans, [CHECKPOINT]);
  assert.deepEqual(planNext(snapshot([command(2, 22, '0', '6')], undefined, OPEN)).plans, [CHECKPOINT]);
  // Once sealed it is no longer planned.
  assert.deepEqual(planNext(snapshot([command(2, 21, '0', '6')], undefined, {...OPEN, open: false})).plans.map(x => x.method), ['stage_emergency_pay']);
});
test('NEGATIVE checkpoint unavailable: only stage_emergency_pay and stage_finalise proceed; everything else is held and listed', () => {
  const p = planNext(snapshot(busy(), null, DOWN));
  assert.deepEqual(p.plans.map(x => x.method), ['stage_emergency_pay', 'stage_finalise']); assert(!p.plans.some(x => x.method === 'checkpoint_rewards'));
  assert.equal(p.rewardCheckpoint.status, 'UNAVAILABLE_HOLDING_OTHER_STEPS');
  assert.deepEqual(p.rewardCheckpoint.held.map(x => x.method ?? x.action), ['stage_checkpoint_investment', 'stage_convert', 'stage_mint', 'stage_withdraw', 'stage_close_convert', 'OWNER_RECOVERY_REQUIRED']);
  assert.deepEqual(planNext(snapshot([command(1, 10)], undefined, DOWN)).plans, []);
  assert.deepEqual([...RELEASE], ['stage_emergency_pay', 'stage_finalise']);
});
test('newest boundary is found without a count view, in O(log n) reads, for contiguous ids', async () => {
  for (const n of [0, 1, 2, 3, 4, 5, 7, 8, 9, 31, 32, 33, 41, 1000, 12345]) {
    let reads = 0; const last = await lastBoundaryId(async id => {reads++; assert.equal(typeof id, 'bigint'); return id < BigInt(n);});
    assert.equal(last, n === 0 ? null : BigInt(n - 1), 'n=' + n); assert(reads <= 2 * Math.ceil(Math.log2(n + 2)) + 3, `n=${n} reads=${reads}`);
  }
  await assert.rejects(lastBoundaryId(async () => true, 1024n), /boundary scan exceeded its bound/);
});
