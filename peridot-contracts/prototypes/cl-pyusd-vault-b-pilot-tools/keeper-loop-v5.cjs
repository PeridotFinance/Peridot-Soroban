// Bounded foreground supervisor. No service installation or key provisioning.
// v3 (2026-11-15): identical to keeper-loop-v2.cjs except that it drives keeper-worker-v3.cjs (97M host-preflight stop,
// AT-LIMIT label). v2 reviews and approvals do not transfer.
// v4 (2026-11-16): drives keeper-worker-v4.cjs. ONLY addition: when the plan's first step is an investment observation,
// the loop runs one consecutive-ledger streak (keeper-streak-probe.cjs `attempt`: pre-signed observations, each sent the
// moment the previous one is seen confirmed, 11-ledger windows, at most approval.streak.maxObservationsPerAttempt) and,
// on PASS, immediately invokes the worker once so `stage_invest` lands within [O+1, O+4). Streak transactions are
// journaled in <stateDirectory>/streak/ with their own caps (approval.streak); the worker journal is unchanged.
// v5 (2026-11-17): drives keeper-worker-v5.cjs (scope cl-vault-b-pilot-keeper-v5). Liveness changes only:
//  (1) the head ledger comes from getLedgerEntries (0.1-0.5 s on Alchemy) instead of getLatestLedger (2-11 s);
//  (2) one worker dependency object per session, so the cached base-fee header and the RPC connection are reused;
//  (3) no separate read-only inspection per ledger: execution calls the worker with deferObservation, which returns
//      OBSERVATION_STREAK_REQUIRED (nothing journaled) when the next step is an observation; then the streak runs as in v4;
//  (4) LEDGER_REGRESSED_READ_AGAIN / OBSERVATION_STREAK_REQUIRED are soft, and a lower head ledger from a lagging
//      backend waits instead of refusing (it never drives an action);
//  (5) a refusal carries `refusalCode` from a fixed list (never an exception message);
//  (6) the streak probe (source unchanged) is given streakRpc(): identical RPC methods, except that getLatestLedger
//      answers {sequence} from getLedgerEntries' latestLedger. The probe calls getLatestLedger in every confirmation poll
//      (follow), so a 2-11 s call there delays each next observation past the consecutive ledger it must land in.
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const {performance} = require('node:perf_hooks');
const worker = require('./keeper-worker-v5.cjs');
const probe = require('./keeper-streak-probe.cjs');
const path = require('node:path');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');

const DEFAULT_SCHEDULE = Object.freeze({pollMs: 1000, maxRunSeconds: 900, pendingTimeoutSeconds: 120, maxAttempts: 100});
const sourceHash = () => crypto.createHash('sha256').update(fs.readFileSync(__filename)).digest('hex');
function validateSchedule(schedule) {
  assert.deepEqual(Object.keys(schedule).sort(), Object.keys(DEFAULT_SCHEDULE).sort());
  for (const [field, min, max] of [['pollMs', 500, 10000], ['maxRunSeconds', 1, 3600],
    ['pendingTimeoutSeconds', 1, 300], ['maxAttempts', 1, 1000]]) {
    assert(Number.isSafeInteger(schedule[field]) && schedule[field] >= min && schedule[field] <= max, 'invalid supervisor schedule: ' + field);
  }
}
function validateStreak(s) {
  assert(s && typeof s === 'object', 'approval.streak caps required');
  for (const [f, min, max] of [['maxObservationsPerAttempt', 3, 6], ['maxTransactions', 1, 30]]) assert(Number.isSafeInteger(s[f]) && s[f] >= min && s[f] <= max, 'streak ' + f);
  for (const f of ['maxFeePerTxStroops', 'totalFeeCapStroops', 'balanceFloorStroops', 'inclusionBidCapStroops']) assert(/^[1-9][0-9]*$/.test(String(s[f])), 'streak ' + f);
  assert(BigInt(s.totalFeeCapStroops) <= 15000000n && BigInt(s.maxFeePerTxStroops) <= 1000000n && BigInt(s.balanceFloorStroops) >= 50000000n);
}
function supervisorBinding(schedule) {
  validateSchedule(schedule);
  return {scope: 'cl-vault-b-pilot-foreground-supervisor', sha256: sourceHash(), streakProbeSha256: probe.sourceSha256(), schedule: {...schedule}};
}
function validateApproval(approval, schedule) {
  assert.equal(approval?.status, 'OPERATOR_APPROVED');
  assert.equal(approval.scope, 'cl-vault-b-pilot-keeper-v5');
  validateStreak(approval.streak);
  assert.deepEqual(approval.supervisor, supervisorBinding(schedule), 'supervisor source or cadence differs from approval');
  assert(Number.isSafeInteger(approval.expiresAt), 'missing execution approval expiry');
}
// The probe uses getLatestLedger only for `.sequence` (attempt start, expiry checks, band wait); its expiry proof already
// takes the ledger from getLedgerEntries. Every other method is passed through unchanged.
function streakRpc(rpc, config) {
  const head = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(config.vault).toScAddress(),
    key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
  return {getNetwork: () => rpc.getNetwork(), getFeeStats: () => rpc.getFeeStats(), getLedgerEntries: (...keys) => rpc.getLedgerEntries(...keys),
    simulateTransaction: tx => rpc.simulateTransaction(tx), sendTransaction: tx => rpc.sendTransaction(tx), getTransaction: hash => rpc.getTransaction(hash),
    getLatestLedger: async () => { const sequence = (await rpc.getLedgerEntries(head)).latestLedger; assert(Number.isSafeInteger(sequence) && sequence > 0); return {sequence}; }};
}
function productionDependencies(config, shouldStop = () => false) {
  const rpc = new S.rpc.Server(worker.rpcUrlOf(config), {timeout: 30});
  const streakStore = probe.journal;
  const workerDeps = worker.productionDependencies(config, {rpc});
  return {
    monotonicMs: () => performance.now(), epochSeconds: () => Math.floor(Date.now() / 1000),
    sleep: ms => new Promise(resolve => setTimeout(resolve, ms)), shouldStop,
    ledger: () => workerDeps.headLedger(),
    warm: () => workerDeps.warmNetworkParameters(),
    invoke: options => worker.runSingleStep(config, {...options, deps: workerDeps}),
    // One streak attempt. Signs only stage_checkpoint_investment(keeper) through the secure-store alias.
    streak: async approval => {
      const store = streakStore(path.join(config.stateDirectory, 'streak'));
      try {
        assert(!store.j.halted, 'streak journal halted; operator review');
        const caps = {...approval.streak, expiresAt: approval.expiresAt}, chain = probe.makeChain(streakRpc(rpc, config));
        if (!(await probe.inBand(chain)).inside) return 'OUT_OF_BAND_WAIT';   // read-only check; no fee while out of band
        const a = await probe.attempt({chain, store, approval: caps, sign: probe.cliSigner(config.signer, config.network), pollMs: 200}, store.j.attempts.length + 1);
        return a.result;
      } finally { store.release(); }
    },
    event: value => process.stdout.write(JSON.stringify(value) + '\n'),
  };
}

// A fixed category only. Assertion messages written in these tools are matched against fixed prefixes; nothing from the
// exception is copied into the output.
const REFUSAL_PATTERNS = [['keeper simulation failed', 'SIMULATION_FAILED'], ['host preflight', 'HOST_PREFLIGHT_ABOVE_97M'],
  ['transaction fee exceeds ceiling', 'FEE_CEILING'], ['cumulative approved fee budget', 'FEE_BUDGET'], ['keeper balance floor', 'BALANCE_FLOOR'],
  ['submission window nearly closed', 'WINDOW_CLOSED_AFTER_SIGNING'], ['stale ledger clock', 'STALE_LEDGER_CLOCK'], ['executable identity mismatch', 'CODE_HASH_MISMATCH'],
  ['streak journal halted', 'STREAK_JOURNAL_HALTED'], ['unrecognized worker result', 'UNRECOGNIZED_WORKER_RESULT'], ['active journal hash changed', 'PENDING_HASH_CHANGED'],
  ['pending journal reservation disappeared', 'PENDING_RESERVATION_DISAPPEARED'], ['invalid public ledger', 'INVALID_PUBLIC_LEDGER'], ['EEXIST', 'WORKER_LOCK_HELD']];
function refusalCode(error) {
  if (error?.code === 'ERR_ASSERTION' || error instanceof Error) {
    const text = String(error?.message ?? '');
    for (const [needle, code] of REFUSAL_PATTERNS) if (text.includes(needle)) return code;
    if (error?.code === 'ERR_ASSERTION') return 'ASSERTION_OTHER';
    if (error?.response?.status) return 'RPC_HTTP_' + (Number.isSafeInteger(error.response.status) ? error.response.status : 'OTHER');
  }
  return 'UNCLASSIFIED';
}
async function runSupervisor(config, {execute = false, approval = null, schedule = DEFAULT_SCHEDULE, deps = null} = {}) {
  worker.validateConfig(config);
  schedule = {...schedule};
  validateSchedule(schedule);
  if (execute) validateApproval(approval, schedule);
  else assert.equal(approval, null, 'read-only supervision does not consume an execution approval');
  deps ??= productionDependencies(config);
  const start = deps.monotonicMs();
  let lastActionLedger = null, highestLedger = 0, pendingHash = null, pendingSince = null;
  let attempts = 0, confirmed = 0, executionCalls = 0, inspections = 0;
  const elapsed = () => deps.monotonicMs() - start;
  const done = reason => ({status: reason, ...(reason === 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED' ? {refusalCode: lastRefusal} : {}), mode: execute ? 'approved-foreground-execution' : 'read-only',
    attempts, confirmed, executionCalls, inspections, lastObservedLedger: highestLedger || null,
    pendingHash, elapsedMs: Math.round(elapsed()), signingMayHaveOccurred: execute && executionCalls > 0,
    note: 'No service created. In-flight outcomes require the existing worker journal and public reconciliation.'});
  const REASONS = new Set(['LEDGER_REGRESSED', 'DECISION_INPUT_CHANGED', 'NOT_AT_REFERENCE_LEDGER', 'CONFIRMATION_BEHIND_REFERENCE',
    'CHECKPOINT_NOT_AT_REFERENCE_LEDGER', 'COMMANDS_NOT_AT_REFERENCE_LEDGER', 'READ_AFTER_REFERENCE', 'PLAN_CHANGED_AT_SIMULATION_LEDGER', 'WINDOW_CLOSING']);
  const emit = (status, hash = null, reason = undefined) => {
    if (hash !== null) assert(/^[0-9a-f]{64}$/.test(hash), 'invalid public transaction hash');
    deps.event?.({status, hash, attempts, confirmed, ledger: highestLedger || null, ...(REASONS.has(reason) ? {reason} : {})});
  };
  const pending = hash => {
    assert(/^[0-9a-f]{64}$/.test(hash), 'missing pending transaction hash');
    if (pendingHash !== null) assert.equal(hash, pendingHash, 'active journal hash changed');
    else {pendingHash = hash; pendingSince = deps.monotonicMs();}
  };
  const pause = async () => deps.sleep(Math.max(1, Math.min(schedule.pollMs, schedule.maxRunSeconds * 1000 - elapsed())));
  const SOFT = ['IDLE', 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION', 'LEDGER_CHANGED_READ_AGAIN', 'LEDGER_REGRESSED_READ_AGAIN', 'OBSERVATION_STREAK_REQUIRED'];
  let lastRefusal = null;
  // v5: fetch the base-fee header once before the first ledger (read-only), so no step waits for the slow getLedgers.
  if (execute && deps.warm) {
    try { await deps.warm(); } catch (error) { lastRefusal = refusalCode(error); return done('SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED'); }
  }
  while (true) {
    if (deps.shouldStop?.()) return done('STOP_REQUESTED');
    if (elapsed() >= schedule.maxRunSeconds * 1000) return done('SESSION_LIMIT_REACHED');
    try {
      if (execute) {
        // Always reconcile first, including after restart. This path cannot sign
        // or submit and remains available after the execution approval expires.
        const reconciliation = await deps.invoke({reconcileOnly: true});
        if (reconciliation.status === 'HALTED_UNKNOWN') {
          pending(reconciliation.hash);
          emit('PENDING_PUBLIC_RECONCILIATION', pendingHash);
          if (deps.monotonicMs() - pendingSince >= schedule.pendingTimeoutSeconds * 1000) return done('UNRESOLVED_TIMEOUT');
          await pause(); continue;
        }
        if (reconciliation.status === 'HALTED_CONFIRMED_FAILURE' || reconciliation.status === 'HALTED_REQUIRES_OPERATOR_REVIEW') {
          emit(reconciliation.status, reconciliation.hash || null);
          if (reconciliation.status === 'HALTED_CONFIRMED_FAILURE') {pendingHash = null; pendingSince = null;}
          return done(reconciliation.status);
        }
        if (reconciliation.status === 'RECONCILED_SUCCESS_REFRESH_REQUIRED') {
          if (pendingHash !== null) assert.equal(reconciliation.hash, pendingHash);
          confirmed++; emit('RECONCILED_SUCCESS', reconciliation.hash);
          pendingHash = null; pendingSince = null;
        } else {
          assert.equal(reconciliation.status, 'NOTHING_TO_RECONCILE', 'unrecognized reconciliation result');
          assert.equal(pendingHash, null, 'pending journal reservation disappeared');
        }
        if (deps.epochSeconds() >= approval.expiresAt) return done('APPROVAL_EXPIRED');
        if (attempts >= schedule.maxAttempts) return done('ATTEMPT_LIMIT_REACHED');
      }
      if (deps.shouldStop?.()) return done('STOP_REQUESTED');
      if (elapsed() >= schedule.maxRunSeconds * 1000) return done('SESSION_LIMIT_REACHED');
      const ledger = await deps.ledger();
      assert(Number.isSafeInteger(ledger) && ledger > 0, 'invalid public ledger');
      if (ledger < highestLedger) {emit('LEDGER_REGRESSED_WAIT'); await pause(); continue;} // lagging backend: never acts on it
      highestLedger = ledger;
      if (ledger === lastActionLedger) {await pause(); continue;}
      if (deps.shouldStop?.()) return done('STOP_REQUESTED');
      if (elapsed() >= schedule.maxRunSeconds * 1000) return done('SESSION_LIMIT_REACHED');
      if (execute && deps.epochSeconds() >= approval.expiresAt) return done('APPROVAL_EXPIRED');
      lastActionLedger = ledger;
      if (execute) executionCalls++;
      else inspections++;
      const defer = execute && deps.streak ? {deferObservation: true} : {};
      let result = await deps.invoke(execute ? {execute: true, approval, ...defer} : {});
      if (execute && deps.streak && result.status === 'OBSERVATION_STREAK_REQUIRED') {
        emit(result.status);
        if (deps.epochSeconds() >= approval.expiresAt) return done('APPROVAL_EXPIRED');
        const outcome = await deps.streak(approval); emit('STREAK_' + outcome);
        if (outcome !== 'PASS') { if (['HALTED', 'TX_CAP', 'FEE_CAP', 'BALANCE_FLOOR', 'APPROVAL_EXPIRED'].includes(outcome)) return done('STREAK_' + outcome); await pause(); continue; }
        // PASS: invoke the worker at once; it plans and sends stage_invest once L_sim is in [O+1, O+2].
        if (deps.epochSeconds() >= approval.expiresAt) return done('APPROVAL_EXPIRED');
        executionCalls++;
        result = await deps.invoke({execute: true, approval, ...defer});
      }
      if (execute && ['SUBMITTED_RECONCILE_NEXT_INVOCATION', 'HALTED_UNKNOWN'].includes(result.status)) {
        attempts++; pending(result.hash); emit(result.status, result.hash);
      } else if (SOFT.includes(result.status)) {
        assert(!execute || result.status !== 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION', 'execution returned an unexecuted plan');
        assert(execute || result.status !== 'OBSERVATION_STREAK_REQUIRED', 'read-only inspection never defers');
        emit(result.status, null, result.reason);
      } else if (['OWNER_RECOVERY_REQUIRED', 'NOT_DEPLOYED_OR_ARCHIVED', 'KEEPER_CONFIGURATION_PENDING',
        'HALTED_CONFIRMED_FAILURE', 'HALTED_REQUIRES_OPERATOR_REVIEW'].includes(result.status)) {
        emit(result.status, result.hash || null); return done(result.status);
      } else throw Error('unrecognized worker result');
      await pause();
    } catch (error) {
      // CLI/RPC exceptions can contain envelopes or the endpoint key; never print their contents. Fixed codes only.
      lastRefusal = refusalCode(error);
      return done('SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED');
    }
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('--help')) {
    console.log('Usage: node keeper-loop-v5.cjs <public-config.json> [--execute --approval <approval.json>] [--poll-ms N --max-run-seconds N --pending-timeout-seconds N --max-attempts N]\nDefault: bounded read-only foreground inspection. --binding prints supervisor source/cadence binding only, without RPC/key access. Approval must include supervisor binding in addition to the existing worker approval. No daemon, service, automatic expiry release, retry, fee escalation, restore or owner recovery. Ctrl-C requests stop between operations; in-flight outcomes remain in the worker journal. Mainnet consecutive-ledger liveness is unproven.');
    return;
  }
  const schedule = {...DEFAULT_SCHEDULE};
  const switches = {'--poll-ms': 'pollMs', '--max-run-seconds': 'maxRunSeconds',
    '--pending-timeout-seconds': 'pendingTimeoutSeconds', '--max-attempts': 'maxAttempts'};
  let configFile = null, approvalFile = null, execute = false, binding = false;
  const seen = new Set();
  for (let i = 0; i < args.length; i++) {
    const arg = args[i]; assert(!seen.has(arg), 'duplicate option'); seen.add(arg);
    if (arg === '--execute') execute = true;
    else if (arg === '--binding') binding = true;
    else if (arg === '--approval') {assert(args[i + 1]); approvalFile = args[++i];}
    else if (switches[arg]) {assert(/^[1-9][0-9]*$/.test(args[i + 1] || '')); schedule[switches[arg]] = Number(args[++i]);}
    else {assert(!arg.startsWith('--') && configFile === null); configFile = arg;}
  }
  validateSchedule(schedule);
  if (binding) {assert(!configFile && !execute && !approvalFile); console.log(JSON.stringify(supervisorBinding(schedule), null, 2)); return;}
  assert(configFile && (execute ? approvalFile : !approvalFile), 'execution requires approval; read-only does not');
  const config = JSON.parse(fs.readFileSync(configFile));
  worker.validateConfig(config);
  const approval = approvalFile ? JSON.parse(fs.readFileSync(approvalFile)) : null;
  let stop = false;
  process.once('SIGINT', () => {stop = true;});
  process.once('SIGTERM', () => {stop = true;});
  const result = await runSupervisor(config, {execute, approval, schedule, deps: productionDependencies(config, () => stop)});
  console.log(JSON.stringify(result, null, 2));
  if (!['SESSION_LIMIT_REACHED', 'ATTEMPT_LIMIT_REACHED', 'STOP_REQUESTED'].includes(result.status)) process.exitCode = 2;
}
if (require.main === module) main().catch(() => {console.error('Keeper supervisor refused. Review the public configuration, approval and journal; sensitive error details were withheld.'); process.exitCode = 2;});
module.exports = {streakRpc, refusalCode, validateStreak, runSupervisor, supervisorBinding, validateSchedule, DEFAULT_SCHEDULE};
