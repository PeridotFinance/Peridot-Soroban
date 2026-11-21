'use strict';
// v6: the job-2 latency regression of keeper-v5-job2-regression.test.cjs, run against keeper v6 (the v4 half that reproduces
// the incident stays in the v5 suite, which this branch does not carry). v6 must prepare and submit stage_convert.
// Regression for the 17 Nov 2026 Mainnet incident, pinned to job 2's recorded on-chain state
// (/Users/joshua/Peridot/keeper-pyusd-vaultb/retry-20261116/b7-read.json): command id 2, kind 0 (deposit), phase 1
// (funded, awaiting stage_convert), deadline 1794924517. Latency = MEASURED profile (17 Nov Alchemy:
// getLatestLedger ~6 s, getLedgers ~6.5 s, other calls ~0.22 s). The stub models command id/kind/phase/owner/deadline only;
// amounts (a 8000000000, min_shares 7980439306, ...) do not affect keeper timing and are not modelled.
// (v5 suite: v4 reproduces the incident, v5 prepares it.)
const test = require('node:test');
const assert = require('node:assert/strict');
const {harness} = require('./support/keeper-v6-harness.cjs');
const DEADLINE = 1794924517, BASE = DEADLINE - 3600, START = 65527103;
// The harness config pins a synthetic pilot owner; the job is attributed to it (owner identity does not affect timing).
const JOB2 = {id: 2, kind: 0, phase: 1, deadline: DEADLINE};
const schedule = {pollMs: 1000, maxRunSeconds: 952, pendingTimeoutSeconds: 120, maxAttempts: 1};

async function run(t, seed) {
  const h = harness(t, {profile: 'MEASURED', seed, startLedger: START, baseTime: BASE, commands: [JOB2]});
  const {result, events} = await h.loop(schedule);
  return {result, submitted: events.find(e => e.status === 'SUBMITTED_RECONCILE_NEXT_INVOCATION'),
    readAgain: events.filter(e => /READ_AGAIN/.test(e.status)).length};
}

test('job 2 (17 Nov): v6 prepares and submits stage_convert for command 2', async t => {
  for (const seed of [1, 2, 3, 4, 5, 6, 7, 8]) {
    const r = await run(t, seed);
    assert(r.submitted, `seed ${seed}: v6 did not submit (status ${r.result.status})`);
    assert.notEqual(r.result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED');
  }
});

test('job 2 (17 Nov) under v6 with a pending upgrade: nothing is prepared, journaled or signed; refusal PENDING_UPGRADE', async t => {
  const {contract} = require('./support/keeper-v6-harness.cjs');
  const h2 = harness(t, {profile: 'MEASURED', seed: 1, startLedger: START, baseTime: BASE, commands: [JOB2], pendingUpgrade: [contract(2)]});   // contract(2) = config.vault
  assert.equal(h2.config.vault, contract(2));
  const {result, events} = await h2.loop(schedule);
  assert.equal(result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED');
  assert.equal(result.refusalCode, 'PENDING_UPGRADE');
  assert.equal(events.find(e => e.status === 'SUBMITTED_RECONCILE_NEXT_INVOCATION'), undefined);
  assert.equal(h2.calls.sign, 0); assert.equal(h2.journal(), null);
});
