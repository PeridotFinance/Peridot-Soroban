// Public read-only worker preflight. This program cannot sign or submit.
// v2 (2026-11-14). Supersedes prototypes/cl-pyusd-vault-b/keeper-inspect.cjs; v1 reviews do not transfer.
// Added: the reward-interval state after an emergency exit, read through the frozen ABI only
// (reward_boundary, reward_interval) plus one read-only simulation of checkpoint_rewards().
//
// v5 (2026-11-17). Successor of keeper-inspect-v4.cjs (unchanged; its approvals do not transfer). Liveness fix, no new
// action. 17 Nov on Alchemy: getLatestLedger and getLedgers took 2–21 s each (they carry the ledger's full metadata),
// while getLedgerEntries / simulateTransaction / getEvents / getNetwork took 0.1–0.5 s. v4 required every read,
// including two getLatestLedger calls, to report one ledger, so a snapshot almost never formed.
//  (1) No getLatestLedger. Reads run in two parallel bursts; the close time comes from getEvents.latestLedgerCloseTime.
//  (2) The snapshot ledger R is the highest ledger any read reported. All decision inputs (owner commands, the investment
//      guard, the reward boundary probe and interval, and the checkpoint_rewards result when an interval is open) must
//      be known AT R: either read at R, or re-read once at >= R with an identical value (CONFIRMED_STABLE). Every one of
//      the guard / reward values only moves one way (see DECISION INPUTS below), so equal values at l1 <= R <= l2 prove the
//      value at R. checkpoint_rewards and owner commands count only when read at exactly R.
//  (3) Anything else (a value changed, a read behind R) is LEDGER_CHANGED_READ_AGAIN, as before. Pinned identities
//      (code hashes, bindings, admin, guardian, keeper, funding asset) are still asserted at whatever ledger they were read.
//  (4) The report carries `decisionLabels`, `viewSource` and `planInput` so that the v5 worker can re-read exactly these
//      inputs next to its simulation and re-plan at the simulation ledger (planAt). Planner logic is keeper-plan-v4.cjs.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {planNext, lastBoundaryId} = require('./keeper-plan-v4.cjs');
const encode = x => JSON.stringify(x, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
// Canonical form for equality of decoded contract values (bigint-safe, key-order independent).
const canonical = value => JSON.stringify(sortKeys(value), (_, v) => typeof v === 'bigint' ? 'n:' + String(v) : v);
function sortKeys(v) {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === 'object' && !(v instanceof Uint8Array)) return Object.fromEntries(Object.keys(v).sort().map(k => [k, sortKeys(v[k])]));
  return v;
}
// v4: the dedicated RPC (operator's Alchemy endpoint) is configured as rpcEndpointFile (mode 600, one https URL
// including its key) plus the pinned rpcHost. The URL is read only here, at connection time, and never stored or returned.
function rpcUrlOf(config) {
  if (config.rpcUrl !== undefined) return config.rpcUrl;
  const st = fs.lstatSync(config.rpcEndpointFile);
  assert(st.isFile() && (st.mode & 0o077) === 0, 'RPC endpoint file must be mode 600');
  const text = fs.readFileSync(config.rpcEndpointFile, 'utf8').trim();
  let url; try { url = new URL(text); } catch (_) { throw Error('RPC endpoint file does not hold one https URL'); }
  assert(url.protocol === 'https:' && !url.username && !url.password && !url.hash && !/\s/.test(text), 'RPC endpoint file does not hold one https URL');
  assert.equal(url.host, config.rpcHost, 'RPC endpoint host differs from the configured host');
  return text;
}

// DECISION INPUTS. Each is one-way on the frozen code, so two equal reads bracket an unchanged value:
//  commands:<owner>  NOT relied on as one-way (lead decision 17 Nov: staged.rs not proven to be the source of receipt
//                    140dcaab): counts only when read at exactly the reference ledger, like checkpoint_rewards.
//  guard             every observation writes observed_ledger = current ledger and asserts ledger > previous (vault.rs:147).
//  reward_boundary:k written once per id (rewards.rs:225-235), ids contiguous from 0.
//  reward_interval:i created unsealed, sealed only false -> true (reward_history.rs:244, 366).
//  checkpoint_rewards NOT one-way (venue claim availability): counts only when read at exactly the reference ledger.
const u64 = id => S.nativeToScVal(BigInt(id), {type: 'u64'});
function decisionCall(config, label) {
  if (label === 'guard') return [config.vault, 'stage_investment_guard', []];
  if (label.startsWith('commands:')) return [config.receipt, 'stage_commands', [new S.Address(label.slice(9)).toScVal()]];
  const m = /^(reward_boundary|reward_interval):(0|[1-9][0-9]*)$/.exec(label);
  if (m) return [config.vault, m[1], [u64(m[2])]];
  if (label === 'checkpoint_rewards') return [config.vault, 'checkpoint_rewards', []];
  throw Error('unknown decision input');
}
function makeViewer(rpc, config, source) {
  const view = async (contract, method, args = []) => {
    const tx = new S.TransactionBuilder(new S.Account(source.address, source.sequence), {fee: '1000', networkPassphrase: config.network})
      .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(60).build();
    return rpc.simulateTransaction(tx);
  };
  const read = async (label, contract, method, args = []) => {
    const q = await view(contract, method, args);
    assert(S.rpc.Api.isSimulationSuccess(q), `${label}: ${q.error || 'simulation failed'}`);
    assert(!q.restorePreamble, `${label}: restore required; no action planned`);
    return {ledger: q.latestLedger, value: S.scValToNative(q.result.retval)};
  };
  // Read-only simulation. A failed simulation or a false return both mean "cannot seal now".
  const checkpoint = async () => {
    const sim = await view(config.vault, 'checkpoint_rewards');
    const ok = S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble && !!sim.result;
    const value = ok && sim.result.retval.switch().name === 'scvBool' && sim.result.retval.b() === true;
    return {ledger: sim.latestLedger, value, ok, sim};
  };
  return {read, checkpoint};
}
// Re-read every decision input in one parallel burst. Used by inspect's confirmation and by the v5 worker next to its
// step simulation. Returns {label: {ledger, value}}.
async function rereadDecisions(rpc, config, labels, source) {
  const viewer = makeViewer(rpc, config, source);
  const rows = await Promise.all(labels.map(async label => {
    if (label === 'checkpoint_rewards') { const c = await viewer.checkpoint(); return [label, {ledger: c.ledger, value: c.value}]; }
    const [contract, method, args] = decisionCall(config, label);
    return [label, await viewer.read(label, contract, method, args)];
  }));
  return Object.fromEntries(rows);
}
// Is every decision input known at `reference`? first/second: {label: {ledger, value}}; second may be null.
// Returns null when proven, else a reason code (never a value).
function knownAt(labels, first, second, reference) {
  for (const label of labels) {
    const a = first[label], b = second?.[label];
    assert(a, 'decision input missing from snapshot');
    if (b && canonical(a.value) !== canonical(b.value)) return 'DECISION_INPUT_CHANGED';
    if (label === 'checkpoint_rewards') {
      if (a.ledger !== reference && b?.ledger !== reference) return 'CHECKPOINT_NOT_AT_REFERENCE_LEDGER';
      continue;
    }
    // Lead decision 17 Nov: staged.rs is not proven to be the source of the deployed receipt 140dcaab, so the one-way
    // argument is not relied on for owner commands: they count only when read at exactly the reference ledger.
    if (label.startsWith('commands:')) {
      if (a.ledger !== reference && b?.ledger !== reference) return 'COMMANDS_NOT_AT_REFERENCE_LEDGER';
      continue;
    }
    if (a.ledger === reference) continue;
    if (a.ledger > reference) return 'READ_AFTER_REFERENCE';
    if (!b) return 'NOT_AT_REFERENCE_LEDGER';
    if (b.ledger < reference) return 'CONFIRMATION_BEHIND_REFERENCE';
  }
  return null;
}
// The planner, re-run from the report's own decision values at another ledger/time (the worker uses the simulation ledger).
function planAt(report, ledger, timestamp) {
  const p = report.planInput;
  return planNext({ledger, timestamp, keeper: p.keeper, configuredKeeper: p.configuredKeeper, receipt: p.receipt,
    vault: p.vault, guard: p.guard, commands: p.commands, rewards: report.rewards});
}

async function inspect(config, rpc = null) { // rpc is injectable for offline tests only; the CLI never passes it
  assert.equal(config.network, S.Networks.PUBLIC);
  assert.equal(config.readOnly, true);
  assert(config.receipt !== config.vault);
  rpc ??= new S.rpc.Server(rpcUrlOf(config), {timeout: 30});
  const result = {status: 'CHECKING', signing: false, submission: false, plans: [], reads: {}};
  const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
    contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(),
    durability: S.xdr.ContractDataDurability.persistent(),
  }));
  // Burst 0 (parallel): network, both instances, the view source account.
  const [network, instances, source] = await Promise.all([rpc.getNetwork(), rpc.getLedgerEntries(instanceKey(config.receipt), instanceKey(config.vault)), rpc.getAccount(config.admin)]);
  assert.equal(network.passphrase, config.network);
  result.ledger = instances.latestLedger;
  if (instances.entries.length !== 2) {
    result.status = 'NOT_DEPLOYED_OR_ARCHIVED';
    result.presentInstances = instances.entries.length;
    return result;
  }
  for (const row of instances.entries) {
    const id = S.Address.fromScAddress(row.val.contractData().contract()).toString();
    const role = id === config.receipt ? 'receipt' : id === config.vault ? 'vault' : null;
    assert(role, 'unexpected instance');
    const executable = row.val.contractData().val().instance().executable();
    assert.equal(executable.switch().name, 'contractExecutableWasm');
    assert.equal(executable.wasmHash().toString('hex'), config.wasmSha256[role], 'executable identity mismatch');
  }
  const viewSource = {address: config.admin, sequence: source.sequenceNumber()};
  const viewer = makeViewer(rpc, config, viewSource);
  const read = async (label, contract, method, args = []) => { const r = await viewer.read(label, contract, method, args); result.reads[label] = r; return r.value; };
  const boundaries = new Map();
  const boundary = async id => {const key = String(id);
    if (!boundaries.has(key)) boundaries.set(key, read('reward_boundary:' + key, ...decisionCall(config, 'reward_boundary:' + key)));
    return boundaries.get(key);};
  // Burst 1 (parallel): identities, guard, every owner's commands, the first boundary probe, and the close time.
  const [receiptConfig, boundReceipt, admin, guardian, keeper, guard, funding, commandRows, , events] = await Promise.all([
    read('receiptConfig', config.receipt, 'stage_config'),
    read('boundReceipt', config.vault, 'stage_receipt'),
    read('vaultAdmin', config.vault, 'get_admin'),
    read('vaultGuardian', config.vault, 'stage_guardian'),
    read('keeper', config.vault, 'stage_investment_keeper'),
    read('guard', ...decisionCall(config, 'guard')),
    read('fundingAsset', config.vault, 'stage_funding_asset'),
    Promise.all(config.pilotOwners.map(owner => read('commands:' + owner, ...decisionCall(config, 'commands:' + owner)))),
    boundary(0n),
    rpc.getEvents({startLedger: Math.max(1, instances.latestLedger - 5), filters: [{type: 'contract', contractIds: [config.vault]}], limit: 1}),
  ]);
  assert.equal(receiptConfig.vault, config.vault); assert.equal(boundReceipt, config.receipt);
  assert.equal(admin, config.admin); assert.equal(guardian, config.admin);
  assert.equal(receiptConfig.guardian, config.admin); assert.equal(funding, config.fundingAsset);
  const commands = commandRows.flat();
  assert(Number.isSafeInteger(events.latestLedger) && /^[1-9][0-9]*$/.test(String(events.latestLedgerCloseTime)), 'close time unavailable');
  result.closeTime = {ledger: events.latestLedger, time: Number(events.latestLedgerCloseTime)};
  result.rewards = await rewardState(config, viewer, boundary, result);
  const labels = [...config.pilotOwners.map(o => 'commands:' + o), 'guard', ...result.rewardLabels,
    ...(result.rewards.open ? ['checkpoint_rewards'] : [])];
  delete result.rewardLabels;
  const first = Object.fromEntries(labels.map(l => [l, l === 'checkpoint_rewards' ? {ledger: result.rewards.simulationLedger, value: result.rewards.checkpointClaimable} : result.reads[l]]));
  const all = [instances.latestLedger, events.latestLedger, ...Object.values(result.reads).map(x => x.ledger), ...(result.rewards.simulationLedger ? [result.rewards.simulationLedger] : [])];
  const reference = Math.max(...all);
  result.ledger = reference;
  result.decisionLabels = labels;
  result.decisions = first;
  result.viewSource = {address: viewSource.address, sequence: String(viewSource.sequence)};
  let reason = knownAt(labels, first, null, reference), second = null;
  if (reason) {
    // One confirmation burst; one-way values that are unchanged at >= R are known at R.
    second = await rereadDecisions(rpc, config, labels, viewSource);
    reason = knownAt(labels, first, second, reference);
  }
  result.coherence = {mode: second ? 'CONFIRMED_STABLE' : 'SINGLE_LEDGER', reference, lowestRead: Math.min(...all),
    confirmationLedgers: second ? Object.fromEntries(Object.entries(second).map(([k, v]) => [k, v.ledger])) : null};
  if (reason) {
    result.status = 'LEDGER_CHANGED_READ_AGAIN'; result.reason = reason;
    return result;
  }
  if (!config.keeperPublicAddress) {result.status = 'KEEPER_CONFIGURATION_PENDING'; return result;}
  result.planInput = {keeper: config.keeperPublicAddress, configuredKeeper: keeper, receipt: config.receipt, vault: config.vault, guard: guard || null, commands};
  // The close time is of a ledger <= R (getEvents answers fast; no getLatestLedger). It feeds only the deadline check,
  // which the contract, the transaction maxTime (<= deadline) and the worker's wall-clock checks enforce again.
  result.plan = planAt(result, reference, result.closeTime.time);
  result.status = result.plan.plans.length ? 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION' : 'IDLE';
  result.note = 'Pilot-owner command discovery only. Public rollout requires durable event/index discovery. No signer or submission loop is enabled.';
  return result;
}
// Is a reward interval open? The frozen ABI has no view of the open flag, the current interval id or the
// boundary count, so: find the newest boundary, then read the interval it belongs to. Every emergency exit
// (any holder) writes a boundary and an unsealed interval record; the sealing claim sets sealed = true.
// v5: also records which reads decide the result (result.rewardLabels): boundary 0 when none exists, else the newest
// boundary, the absent next id and the newest interval.
async function rewardState(config, viewer, boundary, result) {
  const last = await lastBoundaryId(async id => (await boundary(id)) != null);
  if (last === null) { result.rewardLabels = ['reward_boundary:0']; return {open: false, lastBoundaryId: null, interval: null, checkpointClaimable: null}; }
  await boundary(last + 1n); // already read by the search; listed so the worker can re-read it
  const b = await boundary(last);
  const intervalLabel = 'reward_interval:' + b.interval;
  const interval = (await viewer.read(intervalLabel, ...decisionCall(config, intervalLabel)));
  result.reads[intervalLabel] = interval;
  result.rewardLabels = ['reward_boundary:' + last, 'reward_boundary:' + (last + 1n), intervalLabel];
  assert(interval.value, 'boundary without an interval record: incoherent reward history, no action planned');
  const state = {open: interval.value.sealed !== true, lastBoundaryId: String(last), interval: String(b.interval), boundaryHolder: b.holder,
    boundaryWaived: b.waived, intervalSegments: interval.value.segments, checkpointClaimable: null};
  if (!state.open) return state;
  const c = await viewer.checkpoint(); state.simulationLedger = c.ledger;
  state.checkpointClaimable = c.value;
  if (c.ok) state.checkpointInstructions = Number(c.sim.transactionData.build().resources().instructions());
  else state.checkpointSimulation = c.sim.restorePreamble ? 'RESTORE_REQUIRED' : 'FAILED';
  return state;
}
if (require.main === module) {
  assert(process.argv[2] && process.argv[3], 'usage: keeper-inspect-v5.cjs <public-config.json> <new-report.json>');
  assert(!fs.existsSync(process.argv[3]), 'preserve previous reports');
  inspect(JSON.parse(fs.readFileSync(process.argv[2]))).then(report => {
    fs.writeFileSync(process.argv[3], encode(report), {flag: 'wx'});
    console.log(encode({status: report.status, ledger: report.ledger, coherence: report.coherence?.mode, signing: false, submission: false}));
  }).catch(() => {console.error('inspection failed (v5: details withheld; RPC errors can contain the endpoint key)'); process.exitCode = 1;});
}
module.exports = {inspect, rewardState, rpcUrlOf, rereadDecisions, knownAt, planAt, decisionCall, canonical};
