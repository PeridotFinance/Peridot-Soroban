'use strict';
// v6: the v5 loop suite against keeper-loop-v6.cjs (scope cl-vault-b-keeper-v6). The fixed config no longer names the
// retired pilot pair (v6 refuses it) and carries a state-v6 directory; v6 tests are appended.
// v5: the v4 loop suite against keeper-loop-v6.cjs (scope v5). Changed on purpose: a lower head ledger waits instead of
// refusing; the streak is triggered by the worker's OBSERVATION_STREAK_REQUIRED (deferObservation) instead of a separate
// read-only inspection; refusals carry a fixed refusalCode.
const test = require('node:test');
const assert = require('node:assert/strict');
const {runSupervisor, supervisorBinding} = require('../keeper-loop-v6.cjs');
const {StrKey} = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const config = {network: 'Public Global Stellar Network ; September 2015', rpcUrl: 'https://example.invalid',
  admin: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP',
  receipt: StrKey.encodeContract(Buffer.alloc(32, 0x31)), vault: StrKey.encodeContract(Buffer.alloc(32, 0x32)),
  stateDirectory: '/var/empty/keeper-v6-test/state-v6',
  fundingAsset: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2',
  wasmSha256: {receipt: 'a'.repeat(64), vault: 'b'.repeat(64)},
  pilotOwners: ['GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP']};
const STREAK = {maxObservationsPerAttempt: 6, maxTransactions: 30, maxFeePerTxStroops: '600000', totalFeeCapStroops: '15000000', balanceFloorStroops: '120000000', inclusionBidCapStroops: '10000'};
const schedule = {pollMs: 500, maxRunSeconds: 20, pendingTimeoutSeconds: 3, maxAttempts: 4};
function setup(options = {}) {
  let time = 0, active = options.existing ? {hash: 'f'.repeat(64), at: 0} : null, inFlight = 0;
  const calls = [], executions = [], events = [];
  const settings = {...schedule, ...options.schedule};
  const approval = {status: 'OPERATOR_APPROVED', scope: 'cl-vault-b-keeper-v6',
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
test('v5 a lower head ledger (lagging backend) waits and never acts on it; an invalid ledger still refuses', async () => {
  const f = setup({ledger: time => time === 0 ? 100 : time < 3000 ? 99 : 101, schedule: {maxRunSeconds: 4}}); const result = await f.run(false);
  assert.equal(result.status, 'SESSION_LIMIT_REACHED'); assert.equal(result.inspections, 2);
  assert(f.events.some(e => e.status === 'LEDGER_REGRESSED_WAIT'));
  const g = setup({ledger: () => -1}); const r = await g.run(false);
  assert.equal(r.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); assert.equal(r.refusalCode, 'INVALID_PUBLIC_LEDGER');
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

test('v5 an observation plan (OBSERVATION_STREAK_REQUIRED) runs one streak; on PASS the worker is invoked at once, on a miss nothing else is sent', async () => {
  for (const [outcome, expectInvoke] of [['PASS', true], ['FAIL_NO_STREAK', false]]) {
    const f = setup({schedule: {maxRunSeconds: 2, maxAttempts: 1}}); let streaks = 0, deferred = 0;
    const base = f.deps.invoke;
    f.deps.invoke = async args => {
      if (args.execute) { assert.equal(args.deferObservation, true); if (streaks === 0 || outcome !== 'PASS') { deferred++; return {status: 'OBSERVATION_STREAK_REQUIRED'}; } }
      return base(args);
    };
    f.deps.streak = async approval => { streaks++; assert.deepEqual(approval.streak, STREAK); return outcome; };
    await f.run();
    assert(streaks >= 1 && deferred >= 1); assert.equal(f.executions.length > 0, expectInvoke, outcome);
    assert(f.calls.every(args => args.reconcileOnly || args.execute), 'no separate read-only inspection in execution mode');
  }
});
test('v4 approval without streak caps, or with caps above the limits, is refused', async () => {
  for (const bad of [undefined, {...STREAK, totalFeeCapStroops: '15000001'}, {...STREAK, maxObservationsPerAttempt: 7}, {...STREAK, balanceFloorStroops: '1'}]) {
    const f = setup(); f.approval.streak = bad; await assert.rejects(f.run());
  }
});

test('v5 PASS -> invest handoff: IDLE in the observation ledger, a soft read-again, then invest next ledger; one streak only, no refusal', async () => {
  let streaks = 0, passLedger = null, phase = 3, invested = null, movedOnce = false, pending = false;
  const f = setup({schedule: {maxRunSeconds: 8, maxAttempts: 2}, invoke: (args, {time}) => {
    const L = 100 + Math.floor(time / 1000);
    if (args.reconcileOnly) { if (pending) { pending = false; return {status: 'RECONCILED_SUCCESS_REFRESH_REQUIRED', hash: 'a'.repeat(64)}; } return {status: 'NOTHING_TO_RECONCILE'}; }
    assert(args.execute && args.deferObservation, 'execution with deferObservation only');
    const plan = phase !== 3 ? [] : passLedger === null ? [{method: 'stage_checkpoint_investment'}] : L === passLedger ? [] : [{method: 'stage_invest'}];
    if (!plan.length) return {status: 'IDLE'};
    if (plan[0].method === 'stage_checkpoint_investment') return {status: 'OBSERVATION_STREAK_REQUIRED'};
    if (!movedOnce) { movedOnce = true; return {status: 'LEDGER_CHANGED_READ_AGAIN', reason: 'CONFIRMATION_BEHIND_REFERENCE'}; }
    invested = L; phase = 5; pending = true; return {status: 'SUBMITTED_RECONCILE_NEXT_INVOCATION', hash: 'a'.repeat(64)};
  }});
  f.deps.streak = async () => { streaks++; passLedger = 100 + Math.floor(f.deps.monotonicMs() / 1000); return 'PASS'; };
  const r = await f.run();
  assert.notEqual(r.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED');
  assert.equal(streaks, 1); assert(invested > passLedger, `invest prepared at ${invested}, observation ledger ${passLedger}`);
  assert(invested - passLedger <= 2, 'invest must still fit [O+1, O+4)');
  assert(f.events.some(e => e.status === 'LEDGER_CHANGED_READ_AGAIN' && e.reason === 'CONFIRMATION_BEHIND_REFERENCE'));
});
test('v5 soft statuses never stop the loop; refusals carry a fixed code and no exception text', async () => {
  for (const status of ['LEDGER_REGRESSED_READ_AGAIN', 'LEDGER_CHANGED_READ_AGAIN']) {
    const f = setup({workerStatus: status, schedule: {maxRunSeconds: 3}}); const r = await f.run();
    assert.equal(r.status, 'SESSION_LIMIT_REACHED'); assert.equal(r.executionCalls, 3);
  }
  const g = setup({invoke: args => { if (args.reconcileOnly) return {status: 'NOTHING_TO_RECONCILE'}; throw Error('keeper simulation failed https://secret.example/key'); }});
  const r = await g.run(); assert.equal(r.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); assert.equal(r.refusalCode, 'SIMULATION_FAILED');
  assert(!JSON.stringify(r).includes('secret'));
  const {refusalCode} = require('../keeper-loop-v6.cjs');
  assert.equal(refusalCode({response: {status: 429}, message: 'https://x/key'}), 'UNCLASSIFIED'); // not an Error: nothing inspected
  assert.equal(refusalCode(Object.assign(Error('Request failed https://x/key'), {response: {status: 429}})), 'RPC_HTTP_429');
});
test('v5 streakRpc: the unchanged probe gets the same RPC methods; getLatestLedger answers from getLedgerEntries (no ledger metadata)', async () => {
  const {streakRpc} = require('../keeper-loop-v6.cjs'); const seen = [];
  const fake = new Proxy({}, {get: (_, m) => async (...a) => { seen.push(m); return m === 'getLedgerEntries' ? {latestLedger: 777, entries: []} : {m, a}; }});
  const r = streakRpc(fake, config);
  assert.deepEqual(await r.getLatestLedger(), {sequence: 777}); assert.deepEqual(seen, ['getLedgerEntries']);
  for (const m of ['getNetwork', 'getFeeStats', 'simulateTransaction', 'sendTransaction', 'getTransaction']) assert.equal((await r[m]('x')).m, m);
  assert.equal(r.getLedgers, undefined, 'nothing else is exposed');
  const probe = require('../keeper-streak-probe-v6.cjs'); const chain = probe.makeChain(r);
  assert.equal((await chain.latest()).sequence, 777);
});


// ---- v6 ----
test('v6 the supervisor refuses the retired pilot pair, placeholders and a v5 state directory before any call', async () => {
  const deps = new Proxy({}, {get: () => () => { throw Error('no dependency may be used'); }});
  for (const over of [{receipt: 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI'}, {vault: 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63'},
    {vault: 'PLACEHOLDER_vault_after_G7'}, {stateDirectory: '/Users/joshua/Peridot/keeper-pyusd-vaultb/state-v5'}])
    await assert.rejects(runSupervisor({...config, ...over}, {deps}));
});
test('v6 refusal codes: pending upgrade and v6 config refusals map to fixed codes', () => {
  const {refusalCode} = require('../keeper-loop-v6.cjs');
  const A = m => Object.assign(Error(m), {code: 'ERR_ASSERTION'});
  assert.equal(refusalCode(A('pending upgrade on the vault or receipt: refuse')), 'PENDING_UPGRADE');
  assert.equal(refusalCode(A('placeholder left in config.vault')), 'PLACEHOLDER_CONFIG');
  assert.equal(refusalCode(A('retired pilot vault: keeper v6 targets the fresh stack only')), 'RETIRED_PILOT_TARGET');
  assert.equal(refusalCode(A('v6 state directory must be absolute and named state-v6*')), 'STATE_DIRECTORY_REFUSED');
});
test('v6 a streak that refuses with a pending upgrade stops the session (no observation is sent)', async () => {
  const f = setup({workerStatus: 'OBSERVATION_STREAK_REQUIRED'});
  f.deps.streak = async () => { throw Object.assign(Error('pending upgrade on the vault or receipt: refuse'), {code: 'ERR_ASSERTION'}); };
  const r = await f.run();
  assert.equal(r.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); assert.equal(r.refusalCode, 'PENDING_UPGRADE');
});
