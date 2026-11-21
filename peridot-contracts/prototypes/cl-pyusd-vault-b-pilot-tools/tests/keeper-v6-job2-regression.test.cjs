'use strict';
// Regression for the 17 Nov 2026 Mainnet incident, pinned to job 2's recorded on-chain state
// (/Users/joshua/Peridot/keeper-pyusd-vaultb/retry-20261116/b7-read.json): command id 2, kind 0 (deposit), phase 1
// (funded, awaiting stage_convert), deadline 1794924517. Latency = MEASURED profile (17 Nov Alchemy:
// getLatestLedger ~6 s, getLedgers ~6.5 s, other calls ~0.22 s). The stub models command id/kind/phase/owner/deadline only;
// amounts (a 8000000000, min_shares 7980439306, ...) do not affect keeper timing and are not modelled.
// v4 must reproduce the incident (no stage_convert prepared over the 952 s the live session ran); v5 must prepare it.
const test = require('node:test');
const assert = require('node:assert/strict');
const {harness} = require('./support/keeper-v5-harness.cjs');
const DEADLINE = 1794924517, BASE = DEADLINE - 3600, START = 65527103;
// The harness config pins a synthetic pilot owner; the job is attributed to it (owner identity does not affect timing).
const JOB2 = {id: 2, kind: 0, phase: 1, deadline: DEADLINE};
const schedule = {pollMs: 1000, maxRunSeconds: 952, pendingTimeoutSeconds: 120, maxAttempts: 1};

async function run(t, version, seed) {
  const h = harness(t, {profile: 'MEASURED', seed, startLedger: START, baseTime: BASE, commands: [JOB2]}, {version});
  const {result, events} = await h.loop(schedule);
  return {result, submitted: events.find(e => e.status === 'SUBMITTED_RECONCILE_NEXT_INVOCATION'),
    readAgain: events.filter(e => /READ_AGAIN/.test(e.status)).length};
}

test('job 2 (17 Nov): v4 never prepares stage_convert in 952 s under measured latency', async t => {
  for (const seed of [1, 2, 3]) {
    const r = await run(t, 4, seed);
    assert.equal(r.submitted, undefined, `seed ${seed}: v4 unexpectedly prepared a step`);
    assert(r.readAgain > 0, `seed ${seed}: expected LEDGER_CHANGED read-again loop`);
  }
});

test('job 2 (17 Nov): v5 prepares and submits stage_convert for command 2', async t => {
  for (const seed of [1, 2, 3, 4, 5, 6, 7, 8]) {
    const r = await run(t, 5, seed);
    assert(r.submitted, `seed ${seed}: v5 did not submit (status ${r.result.status})`);
    assert.notEqual(r.result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED');
  }
});
