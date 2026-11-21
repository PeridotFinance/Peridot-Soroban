// Pure planning for an operator-run worker. No keys, signing, or submission.
// Each plan expires when its ledger/job snapshot changes; simulate again before execution.
//
// v2 (2026-11-14). Supersedes prototypes/cl-pyusd-vault-b/keeper-plan.cjs; v1 reviews do not transfer.
// USER DECISION 14 Nov 2026: after ANY emergency exit (extract) the keeper sends the permissionless
// vault call `checkpoint_rewards` before anything else for that vault. Reason (measured on the frozen
// bytes): the first claim-first call after an emergency boundary also seals the open reward interval and
// an ordinary exit can then exceed the 100M limit; a prior checkpoint (about 15.2M) removes that cost.
//
// The caller supplies `rewards`, read from the frozen ABI only:
//   {open, lastBoundaryId, interval, checkpointClaimable}
// open = the newest reward_boundary's reward_interval record is not sealed. It covers emergency exits of
// every holder, not only the pilot owners whose commands the keeper lists.
// checkpointClaimable = the simulated checkpoint_rewards() returned true in the same ledger; false means the
// venue claim is unavailable and the call would seal nothing.
const assert = require('node:assert/strict');
const TERMINAL = new Set([4, 12, 22, 99]);
// No reward, venue, oracle, deadline or pause dependency: these release value already owed to an owner
// and are never held back by an open interval that cannot be checkpointed.
const RELEASE = new Set(['stage_emergency_pay', 'stage_finalise']);
const asBig = value => {
  assert(/^(0|[1-9][0-9]*)$/.test(String(value)), 'expected unsigned integer');
  return BigInt(value);
};
const OBSERVATION_WINDOW = 11;
function planNext({ledger, timestamp, keeper, configuredKeeper, receipt, vault, guard, commands, rewards}) {
  assert(Number.isSafeInteger(ledger) && ledger > 0);
  assert(Number.isSafeInteger(timestamp) && timestamp > 0);
  assert.equal(keeper, configuredKeeper, 'worker signer differs from one-time vault keeper');
  assert(receipt && vault && receipt !== vault);
  assert(rewards && typeof rewards.open === 'boolean', 'reward interval state is required; a missing read never means "no emergency exit"');
  if (rewards.open) assert(typeof rewards.checkpointClaimable === 'boolean', 'open interval requires the simulated checkpoint_rewards result');
  guard ??= {observed_ledger: 0, healthy_count: 0, inside: false};
  const plans = [], seen = new Set();
  let observationNeeded = false;
  for (const {id, job} of commands) {
    const commandId = String(asBig(id));
    assert(!seen.has(commandId), 'duplicate command in snapshot'); seen.add(commandId);
    const {kind, phase} = job;
    if (TERMINAL.has(phase)) continue;
    const emit = (method, extra = {}) => plans.push({target: receipt, method, args: [commandId], ...extra});
    // Owner authorization created these requests. The worker never creates an
    // open/close/emergency request or chooses a waiver, amount, or tolerance.
    if (kind === 2 && phase === 21) {emit('stage_emergency_pay'); continue;}
    if (kind === 0 && phase === 5) {emit('stage_finalise'); continue;}
    if (asBig(job.deadline) <= BigInt(timestamp)) {
      plans.push({commandId, action: 'OWNER_RECOVERY_REQUIRED', phase,
        reason: 'No future execution time remains: do not silently choose cancellation or an in-kind exit.'});
      continue;
    }
    if (kind === 0 && phase === 1) emit('stage_convert');
    else if (kind === 0 && phase === 2) emit('stage_mint');
    else if (kind === 0 && phase === 3) {
      assert(Number.isInteger(guard.observed_ledger) && guard.observed_ledger >= 0);
      assert(Number.isInteger(guard.healthy_count) && guard.healthy_count >= 0 && guard.healthy_count <= 3);
      assert.equal(typeof guard.inside, 'boolean');
      assert(guard.observed_ledger <= ledger, 'incoherent snapshot; read again');
      // A transaction submitted after closed ledger L can execute no earlier
      // than L+1. The last observation must precede that execution ledger.
      const nextLedger = ledger + 1;
      // 16 Nov 2026: stellar-rpc simulates AT the snapshot ledger, and stage_invest requires observed_ledger < now.
      // With the third observation in the snapshot ledger itself the simulation would trap, so plan nothing for this
      // command now and read again next ledger, which plans stage_invest in [O+2, O+4).
      if (guard.inside && guard.healthy_count === 3 && guard.observed_ledger === ledger) continue;
      if (guard.inside && guard.healthy_count === 3 && guard.observed_ledger < nextLedger
          && nextLedger - guard.observed_ledger <= 3) {
        emit('stage_invest', {minLedger: guard.observed_ledger + 1,
          maxLedgerExclusive: guard.observed_ledger + 4});
      } else observationNeeded = true;
    } else if (kind === 1 && phase === 10) emit('stage_withdraw');
    else if (kind === 1 && phase === 11) emit('stage_close_convert');
    else throw Error(`Unsupported command kind=${kind} phase=${phase}`);
  }
  if (observationNeeded) {
    // v4 (16 Nov 2026): an 11-ledger window [L+1, L+11). The vault never fails a late observation; it restarts the
    // count at 1 (vault.rs:150-153), so a late landing costs one fee and no safety. The one-ledger window made
    // most observations expire unincluded on a public RPC. Consecutive streaks are driven by keeper-loop-v4.
    plans.unshift({target: vault, method: 'stage_checkpoint_investment', args: [keeper],
      minLedger: ledger + 1, maxLedgerExclusive: ledger + OBSERVATION_WINDOW});
  }
  // Release already-extracted payouts and invested-command refunds before
  // observations for new investment. A busy deposit queue must not hide exits.
  const priority = p => p.method === 'stage_emergency_pay' ? 0 : p.method === 'stage_finalise' ? 1 : 2;
  plans.sort((a, b) => priority(a) - priority(b));
  let rewardCheckpoint = {status: 'NOT_REQUIRED', lastBoundaryId: rewards.lastBoundaryId ?? null, interval: rewards.interval ?? null, held: []};
  let ordered = plans;
  if (rewards.open && rewards.checkpointClaimable) {
    // First, before every other step for this vault, including payouts.
    ordered = [{target: vault, method: 'checkpoint_rewards', args: [],
      reason: 'emergency exit left the reward interval open; seal it before anything else'}, ...plans];
    rewardCheckpoint = {...rewardCheckpoint, status: 'PLANNED_FIRST'};
  } else if (rewards.open) {
    // The venue claim is unavailable, so the checkpoint cannot seal. Do not act around it, except to pay out
    // what owners are already owed. Everything else waits and is listed so that nothing is silently dropped.
    ordered = plans.filter(p => RELEASE.has(p.method));
    rewardCheckpoint = {...rewardCheckpoint, status: 'UNAVAILABLE_HOLDING_OTHER_STEPS',
      held: plans.filter(p => !RELEASE.has(p.method)).map(p => p.method ? {method: p.method, args: p.args} : {action: p.action, commandId: p.commandId}),
      reason: 'checkpoint_rewards would return false (venue claim unavailable). Only stage_emergency_pay and stage_finalise proceed; read again next ledger.'};
  }
  return {schema: 'cl-vault-b-keeper-plan-v2', snapshotLedger: ledger,
    expectedExecutionLedger: ledger + 1, signing: false, submission: false,
    executeAtMostOneThenRefresh: true, rewardCheckpoint, plans: ordered};
}
// Newest boundary id by galloping then bisecting over reward_boundary(id): ids are contiguous from 0 and
// the frozen ABI has no count view. `has(id)` must answer for a single coherent ledger. Returns null when none.
async function lastBoundaryId(has, limit = 1n << 40n) {
  if (!await has(0n)) return null;
  let low = 0n, high = 1n;
  while (await has(high)) {low = high; high *= 2n; assert(high <= limit, 'boundary scan exceeded its bound');}
  while (high - low > 1n) {const mid = (low + high) / 2n; if (await has(mid)) low = mid; else high = mid;}
  return low;
}
module.exports = {OBSERVATION_WINDOW, planNext, lastBoundaryId, RELEASE};
