'use strict';
// keeper-inspect-v5.cjs on the offline world stub (support/chain-stub-v6.cjs). Read-only; public identifiers only.
// Ports the v4 inspector tests (reward boundary search, checkpoint-first, venue unavailable, fail-closed cases) and adds
// the v5 snapshot rule: mixed read ledgers are accepted only when every decision input is proven at the snapshot ledger.
const test = require('node:test');
const assert = require('node:assert/strict');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {inspect, knownAt} = require('../keeper-inspect-v5.cjs');
const {makeWorld} = require('./support/chain-stub-v6.cjs');
const {configFor, publicKey, tempDir} = require('./support/keeper-v6-harness.cjs');
const START = 1000, BASE = 1794920000;
function fixture(t, options = {}) {
  const config = {...configFor(tempDir(t)), readOnly: true};
  if ('keeper' in options) { if (options.keeper === null) delete config.keeperPublicAddress; }
  const calls = [];
  const world = makeWorld({config: {...config, keeperPublicAddress: config.keeperPublicAddress ?? publicKey(2)}, profile: 'FIXED', jitterMs: 0, startLedger: START, baseTime: BASE,
    hook: async (method, payload) => { if (method === 'simulateTransaction') calls.push(payload.operations[0].func.invokeContract().functionName().toString()); if (options.hook) await options.hook(method, payload, world); },
    ...options.world});
  return {config, world, calls, run: () => world.clock.drive(inspect(config, world.rpc))};
}
const command = (id, kind, phase, deadline = BASE + 3600) => ({id, kind, phase, deadline});
test('v5 no boundary: nothing to checkpoint, ordinary plan, no checkpoint simulated, no getLatestLedger', async t => {
  const f = fixture(t); const r = await f.run();
  assert.equal(r.status, 'IDLE'); assert.deepEqual(r.rewards, {open: false, lastBoundaryId: null, interval: null, checkpointClaimable: null});
  assert(!f.calls.includes('checkpoint_rewards')); assert(!f.calls.includes('reward_interval')); assert.equal(r.signing, false); assert.equal(r.submission, false);
  assert.equal(f.world.stats.calls.getLatestLedger, undefined); assert.equal(f.world.stats.calls.getLedgers, undefined);
  assert.equal(r.coherence.mode, 'SINGLE_LEDGER'); assert.equal(r.ledger, START); assert.deepEqual(r.decisionLabels, ['commands:' + f.config.admin, 'guard', 'reward_boundary:0']);
});
test('v5 emergency exit by ANY holder leaves an unsealed interval: checkpoint_rewards is planned first, also with no listed command', async t => {
  const f = fixture(t, {world: {boundaries: [0]}}); const r = await f.run();
  assert.equal(r.status, 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION'); assert.equal(r.rewards.open, true); assert.equal(r.rewards.checkpointClaimable, true);
  assert.equal(r.rewards.lastBoundaryId, '0'); assert.equal(r.rewards.interval, '0');
  assert.deepEqual(r.plan.plans.map(p => p.method), ['checkpoint_rewards']); assert.equal(r.plan.rewardCheckpoint.status, 'PLANNED_FIRST');
  assert.deepEqual(r.decisionLabels.slice(2), ['reward_boundary:0', 'reward_boundary:1', 'reward_interval:0', 'checkpoint_rewards']);
  const g = fixture(t, {world: {boundaries: [0], commands: [command(4, 2, 21, 0), command(5, 1, 10)]}});
  assert.deepEqual((await g.run()).plan.plans.map(p => p.method), ['checkpoint_rewards', 'stage_emergency_pay', 'stage_withdraw']);
});
test('v5 the newest boundary decides: sealed intervals need nothing, a later exit reopens; bounded search', async t => {
  let f = fixture(t, {world: {boundaries: [0, 0, 1, 1, 2], sealed: [0, 1, 2]}}); let r = await f.run();
  assert.equal(r.rewards.open, false); assert.equal(r.rewards.lastBoundaryId, '4'); assert.equal(r.status, 'IDLE');
  f = fixture(t, {world: {boundaries: [0, 0, 1, 1, 2, 3], sealed: [0, 1, 2]}}); r = await f.run();
  assert.equal(r.rewards.open, true); assert.equal(r.rewards.interval, '3'); assert.deepEqual(r.plan.plans.map(p => p.method), ['checkpoint_rewards']);
  const many = fixture(t, {world: {boundaries: Array.from({length: 41}, (_, i) => i), sealed: Array.from({length: 40}, (_, i) => i)}}); r = await many.run();
  assert.equal(r.rewards.lastBoundaryId, '40'); assert(many.calls.filter(m => m === 'reward_boundary').length <= 14);
});
test('v5 NEGATIVE venue claim unavailable: no checkpoint is planned, only release steps', async t => {
  const f = fixture(t, {world: {boundaries: [0], claim: false, commands: [command(4, 2, 21, 0), command(5, 1, 10)]}}); const r = await f.run();
  assert.equal(r.rewards.checkpointClaimable, false); assert.deepEqual(r.plan.plans.map(p => p.method), ['stage_emergency_pay']);
  assert.deepEqual(r.plan.rewardCheckpoint.held, [{method: 'stage_withdraw', args: ['5']}]);
});
test('v5 NEGATIVE fail closed: missing interval record, undeployed pair, wrong code hash; keeper address pending plans nothing', async t => {
  await assert.rejects(fixture(t, {world: {boundaries: [0], missingInterval: true}}).run(), /boundary without an interval record/);
  assert.equal((await fixture(t, {world: {absentInstances: true}}).run()).status, 'NOT_DEPLOYED_OR_ARCHIVED');
  const bad = fixture(t); bad.config.wasmSha256 = {...bad.config.wasmSha256, vault: '9'.repeat(64)}; await assert.rejects(bad.run(), /executable identity mismatch/);
  const k = fixture(t, {keeper: null, world: {boundaries: [0]}}); const r = await k.run();
  assert.equal(r.status, 'KEEPER_CONFIGURATION_PENDING'); assert.equal(r.plan, undefined);
  const g = fixture(t); await assert.rejects(g.world.clock.drive(inspect({...g.config, readOnly: false}, g.world.rpc)));
});
test('v5 mixed read ledgers with unchanged one-way inputs: CONFIRMED_STABLE plan at the highest ledger', async t => {
  // The guard read lands one ledger later than the others; the confirmation burst proves commands/boundary unchanged there.
  let n = 0;
  const f = fixture(t, {world: {commands: [command(5, 0, 1)]}, hook: async (method, p, world) => {
    if (method === 'simulateTransaction' && p.operations[0].func.invokeContract().functionName().toString() === 'stage_investment_guard' && n++ === 0) await world.clock.sleep(6000); }});
  const r = await f.run();
  assert.equal(r.status, 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION'); assert.equal(r.coherence.mode, 'CONFIRMED_STABLE');
  assert.equal(r.ledger, START + 1); assert.equal(r.reads.guard.ledger, START + 1); assert.equal(r.reads['commands:' + f.config.admin].ledger, START);
  assert.deepEqual(r.plan.plans.map(p => p.method), ['stage_convert']); assert.equal(r.plan.snapshotLedger, START + 1);
});
test('v5 NEGATIVE mixed read ledgers with a changed input: LEDGER_CHANGED_READ_AGAIN (no plan)', async t => {
  let n = 0;
  const f = fixture(t, {world: {commands: [command(5, 0, 1)]}, hook: async (method, p, world) => {
    if (method === 'simulateTransaction' && p.operations[0].func.invokeContract().functionName().toString() === 'stage_investment_guard' && n++ === 0) await world.clock.sleep(6000); }});
  f.world.mutate(START + 1, s => { s.commands[0].phase = 2; });
  const r = await f.run(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'DECISION_INPUT_CHANGED'); assert.equal(r.plan, undefined);
});
test('v5 knownAt: one-way inputs need equal reads bracketing the reference; checkpoint needs a read at exactly the reference', () => {
  const v = (ledger, value) => ({ledger, value});
  assert.equal(knownAt(['guard'], {guard: v(10, 1)}, null, 10), null);
  assert.equal(knownAt(['guard'], {guard: v(9, 1)}, null, 10), 'NOT_AT_REFERENCE_LEDGER');
  assert.equal(knownAt(['guard'], {guard: v(9, 1)}, {guard: v(11, 1)}, 10), null);
  assert.equal(knownAt(['guard'], {guard: v(9, 1)}, {guard: v(9, 1)}, 10), 'CONFIRMATION_BEHIND_REFERENCE');
  assert.equal(knownAt(['guard'], {guard: v(9, 1)}, {guard: v(11, 2)}, 10), 'DECISION_INPUT_CHANGED');
  assert.equal(knownAt(['guard'], {guard: v(10, 1)}, {guard: v(11, 2)}, 10), 'DECISION_INPUT_CHANGED');
  assert.equal(knownAt(['checkpoint_rewards'], {checkpoint_rewards: v(9, true)}, {checkpoint_rewards: v(11, true)}, 10), 'CHECKPOINT_NOT_AT_REFERENCE_LEDGER');
  assert.equal(knownAt(['checkpoint_rewards'], {checkpoint_rewards: v(9, true)}, {checkpoint_rewards: v(10, true)}, 10), null);
  // Lead decision 17 Nov: owner commands are not treated as one-way; equal reads bracketing the reference are not enough.
  assert.equal(knownAt(['commands:x'], {'commands:x': v(9, [{id: 1n, job: {phase: 1}}])}, {'commands:x': v(12, [{job: {phase: 1}, id: 1n}])}, 10), 'COMMANDS_NOT_AT_REFERENCE_LEDGER');
  assert.equal(knownAt(['commands:x'], {'commands:x': v(9, [{id: 1n, job: {phase: 1}}])}, {'commands:x': v(10, [{job: {phase: 1}, id: 1n}])}, 10), null);
  assert.equal(knownAt(['commands:x'], {'commands:x': v(10, [1])}, null, 10), null);
  assert.equal(knownAt(['commands:x'], {'commands:x': v(10, [1])}, {'commands:x': v(10, [2])}, 10), 'DECISION_INPUT_CHANGED');
  assert.throws(() => knownAt(['guard'], {}, null, 10));
});
