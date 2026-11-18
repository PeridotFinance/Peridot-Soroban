'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {runSupervisor, supervisorBinding} = require('../keeper-loop-v4.cjs');
const config = {network: 'Public Global Stellar Network ; September 2015', rpcUrl: 'https://example.invalid',
  admin: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP',
  receipt: 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI',
  vault: 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63',
  fundingAsset: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2',
  wasmSha256: {receipt: 'a'.repeat(64), vault: 'b'.repeat(64)},
  pilotOwners: ['GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP']};
const STREAK = {maxObservationsPerAttempt: 6, maxTransactions: 30, maxFeePerTxStroops: '600000', totalFeeCapStroops: '15000000', balanceFloorStroops: '120000000', inclusionBidCapStroops: '10000'};
const schedule = {pollMs: 500, maxRunSeconds: 20, pendingTimeoutSeconds: 3, maxAttempts: 4};
function setup(options = {}) {
  let time = 0, active = options.existing ? {hash: 'f'.repeat(64), at: 0} : null, inFlight = 0;
  const calls = [], executions = [], events = [];
  const settings = {...schedule, ...options.schedule};
  const approval = {status: 'OPERATOR_APPROVED', scope: 'cl-vault-b-pilot-keeper-v4',
    expiresAt: options.expiresAt ?? 2000, supervisor: supervisorBinding(settings), streak: STREAK};
  const deps = {
    monotonicMs: () => time, epochSeconds: () => 1000 + Math.floor(time / 1000),
    shouldStop: () => Boolean(options.stop?.(time)), event: event => events.push(event),
    sleep: async ms => {time += ms;},
    ledger: async () => options.ledger ? options.ledger(time) : 100 + Math.floor(time / 1000),
    invoke: async args => {
      inFlight++; assert.equal(inFlight, 1, 'worker operations overlapped');
      try {
        calls.push(args); await Promise.resolve();
        if (options.invoke) return options.invoke(args, {time, executions, calls});
        if (args.reconcileOnly) {
          if (!active) return {status: 'NOTHING_TO_RECONCILE', signing: false};
          if (options.neverConfirm || time - active.at < (options.confirmMs ?? 1000)) {
            return {status: 'HALTED_UNKNOWN', hash: options.changeHash && time > 0 ? 'e'.repeat(64) : active.hash};
          }
          const hash = active.hash; active = null;
          return {status: options.fail ? 'HALTED_CONFIRMED_FAILURE' : 'RECONCILED_SUCCESS_REFRESH_REQUIRED', hash};
        }
        if (!args.execute) return {status: 'IDLE', signing: false};
        if (options.refusal) throw Error('secret-shaped RPC details must never be returned');
        if (options.workerStatus) return {status: options.workerStatus};
        assert.equal(active, null, 'attempted duplicate signing while pending');
        executions.push({closedLedger: 100 + Math.floor(time / 1000), time});
        active = {hash: String(executions.length).padStart(64, '0'), at: time};
        return {status: options.submitUnknown ? 'HALTED_UNKNOWN' : 'SUBMITTED_RECONCILE_NEXT_INVOCATION', hash: active.hash, signing: true};
      } finally {inFlight--;}
    },
  };
  return {deps, approval, settings, calls, executions, events, active: () => active,
    run: (execute = true) => runSupervisor(config, {execute, approval: execute ? approval : null, schedule: settings, deps})};
}

test('default read-only inspects once per new ledger and never reconciles/signs', async () => {
  const f = setup({schedule: {maxRunSeconds: 3}}); const result = await f.run(false);
  assert.equal(result.status, 'SESSION_LIMIT_REACHED'); assert.equal(result.inspections, 3);
  assert(f.calls.every(args => Object.keys(args).length === 0)); assert.equal(result.signingMayHaveOccurred, false);
});
test('supervisor serially confirms three observations then investment across ledger advances', async () => {
  const f = setup(); const result = await f.run();
  assert.equal(result.status, 'ATTEMPT_LIMIT_REACHED'); assert.equal(result.attempts, 4); assert.equal(result.confirmed, 4);
  assert.deepEqual(f.executions.map(x => x.closedLedger), [100, 101, 102, 103]);
  assert(f.calls.filter(x => x.execute).every(x => x.approval === f.approval)); assert.equal(f.active(), null);
});
test('unresolved result only polls the same hash then stops without re-signing', async () => {
  const f = setup({neverConfirm: true, schedule: {pendingTimeoutSeconds: 2}}); const result = await f.run();
  assert.equal(result.status, 'UNRESOLVED_TIMEOUT'); assert.equal(f.executions.length, 1);
  assert.equal(result.pendingHash, f.active().hash); assert(f.calls.slice(2).every(x => x.reconcileOnly));
});
test('preexisting pending journal is reconciled without a new execution', async () => {
  const f = setup({existing: true, neverConfirm: true, schedule: {pendingTimeoutSeconds: 1}}); const result = await f.run();
  assert.equal(result.status, 'UNRESOLVED_TIMEOUT'); assert.equal(result.executionCalls, 0); assert.equal(f.executions.length, 0);
});
test('confirmed failure halts without another transaction', async () => {
  const f = setup({fail: true}); const result = await f.run();
  assert.equal(result.status, 'HALTED_CONFIRMED_FAILURE'); assert.equal(f.executions.length, 1);
  assert.equal(result.pendingHash, null); assert.equal(f.active(), null);
});
test('expired execution approval still permits pending public reconciliation', async () => {
  const f = setup({expiresAt: 1001, confirmMs: 2000}); const result = await f.run();
  assert.equal(result.status, 'APPROVAL_EXPIRED'); assert.equal(result.confirmed, 1); assert.equal(f.executions.length, 1);
});
test('approval already expired never starts an execution', async () => {
  const f = setup({expiresAt: 1000}); const result = await f.run();
  assert.equal(result.status, 'APPROVAL_EXPIRED'); assert.equal(result.executionCalls, 0);
});
test('worker safety refusal stops and redacts exception content', async () => {
  const f = setup({refusal: true}); const result = await f.run();
  assert.equal(result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); assert.equal(result.executionCalls, 1);
  assert(!JSON.stringify(result).includes('secret-shaped')); assert.equal(f.calls.filter(x => x.execute).length, 1);
});
test('owner recovery is not converted into cancellation or another operation', async () => {
  const f = setup({workerStatus: 'OWNER_RECOVERY_REQUIRED'}); const result = await f.run();
  assert.equal(result.status, 'OWNER_RECOVERY_REQUIRED'); assert.equal(result.executionCalls, 1);
});
test('changed pending hash fails closed', async () => {
  const f = setup({neverConfirm: true, changeHash: true}); const result = await f.run();
  assert.equal(result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); assert.equal(f.executions.length, 1);
});
test('disappearing pending journal reservation fails closed', async () => {
  let signed = false;
  const f = setup({invoke: async args => {
    if (args.reconcileOnly) return {status: 'NOTHING_TO_RECONCILE'};
    assert(!signed); signed = true; return {status: 'HALTED_UNKNOWN', hash: 'a'.repeat(64)};
  }}); const result = await f.run();
  assert.equal(result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); assert.equal(result.attempts, 1);
});
test('public ledger regression stops the foreground loop', async () => {
  const f = setup({ledger: time => time === 0 ? 100 : 99}); const result = await f.run(false);
  assert.equal(result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); assert.equal(result.inspections, 1);
});
test('source/cadence approval mismatch rejects before invoking worker or RPC', async () => {
  const f = setup(); f.approval.supervisor.sha256 = '0'.repeat(64);
  await assert.rejects(f.run(), /supervisor source or cadence/); assert.equal(f.calls.length, 0);
  const other = setup(); other.approval.supervisor.schedule.pollMs = 1000;
  await assert.rejects(other.run(), /supervisor source or cadence/); assert.equal(other.calls.length, 0);
});
test('session timeout leaves unresolved journal reservation intact', async () => {
  const f = setup({neverConfirm: true, schedule: {maxRunSeconds: 1, pendingTimeoutSeconds: 3}}); const result = await f.run();
  assert.equal(result.status, 'SESSION_LIMIT_REACHED'); assert.equal(result.pendingHash, f.active().hash);
  assert.equal(f.executions.length, 1);
});
test('requested stop does not start a worker call', async () => {
  const f = setup({stop: () => true}); const result = await f.run();
  assert.equal(result.status, 'STOP_REQUESTED'); assert.equal(f.calls.length, 0);
});
test('unknown submission result is reconciled, never immediately retried', async () => {
  const f = setup({submitUnknown: true, schedule: {maxAttempts: 1}}); const result = await f.run();
  assert.equal(result.status, 'ATTEMPT_LIMIT_REACHED'); assert.equal(result.confirmed, 1); assert.equal(f.executions.length, 1);
});

test('v4 an observation plan runs one streak; on PASS the worker is invoked at once (invest), on a miss nothing else is sent', async () => {
  for (const [outcome, expectInvoke] of [['PASS', true], ['FAIL_NO_STREAK', false]]) {
    const f = setup({schedule: {maxRunSeconds: 2, maxAttempts: 1}}); let streaks = 0;
    const base = f.deps.invoke;
    f.deps.invoke = async args => (!args.execute && !args.reconcileOnly) ? {status: 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION', plan: {plans: [{method: 'stage_checkpoint_investment'}]}} : base(args);
    f.deps.streak = async approval => { streaks++; assert.deepEqual(approval.streak, STREAK); return outcome; };
    await f.run();
    assert(streaks >= 1); assert.equal(f.executions.length > 0, expectInvoke, outcome);
  }
});
test('v4 approval without streak caps, or with caps above the limits, is refused', async () => {
  for (const bad of [undefined, {...STREAK, totalFeeCapStroops: '15000001'}, {...STREAK, maxObservationsPerAttempt: 7}, {...STREAK, balanceFloorStroops: '1'}]) {
    const f = setup(); f.approval.streak = bad; await assert.rejects(f.run());
  }
});

test('v4 PASS -> invest handoff: IDLE in the observation ledger, a ledger change during prepare, then invest next ledger; one streak only, no refusal', async () => {
  let streaks = 0, passLedger = null, phase = 3, invested = null, movedOnce = false, pending = false;
  const f = setup({schedule: {maxRunSeconds: 8, maxAttempts: 2}, invoke: (args, {time}) => {
    const L = 100 + Math.floor(time / 1000);
    if (args.reconcileOnly) { if (pending) { pending = false; return {status: 'RECONCILED_SUCCESS_REFRESH_REQUIRED', hash: 'a'.repeat(64)}; } return {status: 'NOTHING_TO_RECONCILE'}; }
    // Planner semantics: phase 3 with count 3 observed in L itself -> nothing planned; L > O -> stage_invest; before PASS -> observation.
    const plan = phase !== 3 ? [] : passLedger === null ? [{method: 'stage_checkpoint_investment'}] : L === passLedger ? [] : [{method: 'stage_invest'}];
    if (!args.execute) return plan.length ? {status: 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION', plan: {plans: plan}} : {status: 'IDLE'};
    if (!plan.length) return {status: 'IDLE'};
    assert.equal(plan[0].method, 'stage_invest', 'the worker must never be asked to execute an observation here');
    if (!movedOnce) { movedOnce = true; return {status: 'LEDGER_CHANGED_READ_AGAIN'}; }
    invested = L; phase = 5; pending = true; return {status: 'SUBMITTED_RECONCILE_NEXT_INVOCATION', hash: 'a'.repeat(64)};
  }});
  f.deps.streak = async () => { streaks++; passLedger = 100 + Math.floor(f.deps.monotonicMs() / 1000); return 'PASS'; };
  const r = await f.run();
  assert.notEqual(r.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED');
  assert.equal(streaks, 1); assert(invested > passLedger, `invest prepared at ${invested}, observation ledger ${passLedger}`);
  assert(invested - passLedger <= 2, 'invest must still fit [O+1, O+4)');
});
