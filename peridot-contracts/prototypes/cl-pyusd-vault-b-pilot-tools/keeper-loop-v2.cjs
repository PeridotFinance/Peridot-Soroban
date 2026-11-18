// Bounded foreground supervisor. No service installation or key provisioning.
// v2 (2026-11-14): identical to prototypes/cl-pyusd-vault-b/keeper-loop.cjs except that it drives
// keeper-worker-v2.cjs (checkpoint_rewards first after an emergency exit). v1 reviews and approvals do not transfer.
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const {performance} = require('node:perf_hooks');
const worker = require('./keeper-worker-v2.cjs');
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
function supervisorBinding(schedule) {
  validateSchedule(schedule);
  return {scope: 'cl-vault-b-pilot-foreground-supervisor', sha256: sourceHash(), schedule: {...schedule}};
}
function validateApproval(approval, schedule) {
  assert.equal(approval?.status, 'OPERATOR_APPROVED');
  assert.equal(approval.scope, 'cl-vault-b-pilot-keeper-single-step');
  assert.deepEqual(approval.supervisor, supervisorBinding(schedule), 'supervisor source or cadence differs from approval');
  assert(Number.isSafeInteger(approval.expiresAt), 'missing execution approval expiry');
}
function productionDependencies(config, shouldStop = () => false) {
  const rpc = new S.rpc.Server(config.rpcUrl, {timeout: 30});
  return {
    monotonicMs: () => performance.now(), epochSeconds: () => Math.floor(Date.now() / 1000),
    sleep: ms => new Promise(resolve => setTimeout(resolve, ms)), shouldStop,
    ledger: async () => (await rpc.getLatestLedger()).sequence,
    invoke: options => worker.runSingleStep(config, options),
    event: value => process.stdout.write(JSON.stringify(value) + '\n'),
  };
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
  const done = reason => ({status: reason, mode: execute ? 'approved-foreground-execution' : 'read-only',
    attempts, confirmed, executionCalls, inspections, lastObservedLedger: highestLedger || null,
    pendingHash, elapsedMs: Math.round(elapsed()), signingMayHaveOccurred: execute && executionCalls > 0,
    note: 'No service created. In-flight outcomes require the existing worker journal and public reconciliation.'});
  const emit = (status, hash = null) => {
    if (hash !== null) assert(/^[0-9a-f]{64}$/.test(hash), 'invalid public transaction hash');
    deps.event?.({status, hash, attempts, confirmed, ledger: highestLedger || null});
  };
  const pending = hash => {
    assert(/^[0-9a-f]{64}$/.test(hash), 'missing pending transaction hash');
    if (pendingHash !== null) assert.equal(hash, pendingHash, 'active journal hash changed');
    else {pendingHash = hash; pendingSince = deps.monotonicMs();}
  };
  const pause = async () => deps.sleep(Math.max(1, Math.min(schedule.pollMs, schedule.maxRunSeconds * 1000 - elapsed())));
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
      assert(Number.isSafeInteger(ledger) && ledger > 0 && ledger >= highestLedger, 'invalid or regressing public ledger');
      highestLedger = ledger;
      if (ledger === lastActionLedger) {await pause(); continue;}
      if (deps.shouldStop?.()) return done('STOP_REQUESTED');
      if (elapsed() >= schedule.maxRunSeconds * 1000) return done('SESSION_LIMIT_REACHED');
      if (execute && deps.epochSeconds() >= approval.expiresAt) return done('APPROVAL_EXPIRED');
      lastActionLedger = ledger;
      if (execute) executionCalls++;
      else inspections++;
      const result = await deps.invoke(execute ? {execute: true, approval} : {});
      if (execute && ['SUBMITTED_RECONCILE_NEXT_INVOCATION', 'HALTED_UNKNOWN'].includes(result.status)) {
        attempts++; pending(result.hash); emit(result.status, result.hash);
      } else if (['IDLE', 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION', 'LEDGER_CHANGED_READ_AGAIN'].includes(result.status)) {
        assert(!execute || result.status !== 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION', 'execution returned an unexecuted plan');
        emit(result.status);
      } else if (['OWNER_RECOVERY_REQUIRED', 'NOT_DEPLOYED_OR_ARCHIVED', 'KEEPER_CONFIGURATION_PENDING',
        'HALTED_CONFIRMED_FAILURE', 'HALTED_REQUIRES_OPERATOR_REVIEW'].includes(result.status)) {
        emit(result.status, result.hash || null); return done(result.status);
      } else throw Error('unrecognized worker result');
      await pause();
    } catch (_) {
      // CLI/RPC exceptions can contain envelopes; never print their contents.
      return done('SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED');
    }
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('--help')) {
    console.log('Usage: node keeper-loop.cjs <public-config.json> [--execute --approval <approval.json>] [--poll-ms N --max-run-seconds N --pending-timeout-seconds N --max-attempts N]\nDefault: bounded read-only foreground inspection. --binding prints supervisor source/cadence binding only, without RPC/key access. Approval must include supervisor binding in addition to the existing worker approval. No daemon, service, automatic expiry release, retry, fee escalation, restore or owner recovery. Ctrl-C requests stop between operations; in-flight outcomes remain in the worker journal. Mainnet consecutive-ledger liveness is unproven.');
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
module.exports = {runSupervisor, supervisorBinding, validateSchedule, DEFAULT_SCHEDULE};
