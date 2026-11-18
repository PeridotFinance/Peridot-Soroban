// Public read-only worker preflight. This program cannot sign or submit.
// v2 (2026-11-14). Supersedes prototypes/cl-pyusd-vault-b/keeper-inspect.cjs; v1 reviews do not transfer.
// Added: the reward-interval state after an emergency exit, read through the frozen ABI only
// (reward_boundary, reward_interval) plus one read-only simulation of checkpoint_rewards().
const fs = require('node:fs');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {planNext, lastBoundaryId} = require('./keeper-plan-v2.cjs');
const encode = x => JSON.stringify(x, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
async function inspect(config, rpc = null) { // rpc is injectable for offline tests only; the CLI never passes it
  assert.equal(config.network, S.Networks.PUBLIC);
  assert.equal(config.readOnly, true);
  assert(config.receipt !== config.vault);
  rpc ??= new S.rpc.Server(config.rpcUrl, {timeout: 30});
  assert.equal((await rpc.getNetwork()).passphrase, config.network);
  const result = {status: 'CHECKING', signing: false, submission: false, plans: [], reads: {}};
  const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
    contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(),
    durability: S.xdr.ContractDataDurability.persistent(),
  }));
  const start = await rpc.getLatestLedger();
  const instances = await rpc.getLedgerEntries(instanceKey(config.receipt), instanceKey(config.vault));
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
  const source = await rpc.getAccount(config.admin);
  const read = async (label, contract, method, args = []) => {
    const tx = new S.TransactionBuilder(new S.Account(config.admin, source.sequenceNumber()),
      {fee: '1000', networkPassphrase: config.network})
      .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(60).build();
    const q = await rpc.simulateTransaction(tx);
    assert(S.rpc.Api.isSimulationSuccess(q), `${label}: ${q.error || 'simulation failed'}`);
    assert(!q.restorePreamble, `${label}: restore required; no action planned`);
    const value = S.scValToNative(q.result.retval);
    result.reads[label] = {ledger: q.latestLedger, value};
    return value;
  };
  const [receiptConfig, boundReceipt, admin, guardian, keeper, guard, funding] = await Promise.all([
    read('receiptConfig', config.receipt, 'stage_config'),
    read('boundReceipt', config.vault, 'stage_receipt'),
    read('vaultAdmin', config.vault, 'get_admin'),
    read('vaultGuardian', config.vault, 'stage_guardian'),
    read('keeper', config.vault, 'stage_investment_keeper'),
    read('guard', config.vault, 'stage_investment_guard'),
    read('fundingAsset', config.vault, 'stage_funding_asset'),
  ]);
  assert.equal(receiptConfig.vault, config.vault); assert.equal(boundReceipt, config.receipt);
  assert.equal(admin, config.admin); assert.equal(guardian, config.admin);
  assert.equal(receiptConfig.guardian, config.admin); assert.equal(funding, config.fundingAsset);
  const commands = (await Promise.all(config.pilotOwners.map(owner =>
    read('commands:' + owner, config.receipt, 'stage_commands', [new S.Address(owner).toScVal()])))).flat();
  result.rewards = await rewardState(rpc, config, source, read);
  const end = await rpc.getLatestLedger();
  const ledgers = [start.sequence, instances.latestLedger, end.sequence, ...(result.rewards.simulationLedger ? [result.rewards.simulationLedger] : []),
    ...Object.values(result.reads).map(x => x.ledger)];
  if (!ledgers.every(x => x === start.sequence)) {
    result.status = 'LEDGER_CHANGED_READ_AGAIN';
    return result;
  }
  if (!config.keeperPublicAddress) {result.status = 'KEEPER_CONFIGURATION_PENDING'; return result;}
  result.plan = planNext({ledger: end.sequence, timestamp: Number(end.closeTime),
    keeper: config.keeperPublicAddress, configuredKeeper: keeper, receipt: config.receipt,
    vault: config.vault, guard: guard || null, commands, rewards: result.rewards});
  result.status = result.plan.plans.length ? 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION' : 'IDLE';
  result.note = 'Pilot-owner command discovery only. Public rollout requires durable event/index discovery. No signer or submission loop is enabled.';
  return result;
}
// Is a reward interval open? The frozen ABI has no view of the open flag, the current interval id or the
// boundary count, so: find the newest boundary, then read the interval it belongs to. Every emergency exit
// (any holder) writes a boundary and an unsealed interval record; the sealing claim sets sealed = true.
async function rewardState(rpc, config, source, read) {
  const u64 = id => S.nativeToScVal(BigInt(id), {type: 'u64'});
  const boundaries = new Map();
  const boundary = async id => {const key = String(id);
    if (!boundaries.has(key)) boundaries.set(key, await read('reward_boundary:' + key, config.vault, 'reward_boundary', [u64(id)]));
    return boundaries.get(key);};
  const last = await lastBoundaryId(async id => (await boundary(id)) != null);
  if (last === null) return {open: false, lastBoundaryId: null, interval: null, checkpointClaimable: null};
  const b = await boundary(last), interval = await read('reward_interval:' + b.interval, config.vault, 'reward_interval', [u64(b.interval)]);
  assert(interval, 'boundary without an interval record: incoherent reward history, no action planned');
  const state = {open: interval.sealed !== true, lastBoundaryId: String(last), interval: String(b.interval), boundaryHolder: b.holder,
    boundaryWaived: b.waived, intervalSegments: interval.segments, checkpointClaimable: null};
  if (!state.open) return state;
  // Read-only simulation. A failed simulation or a false return both mean "cannot seal now".
  const tx = new S.TransactionBuilder(new S.Account(config.admin, source.sequenceNumber()), {fee: '1000', networkPassphrase: config.network})
    .addOperation(new S.Contract(config.vault).call('checkpoint_rewards')).setTimeout(60).build();
  const sim = await rpc.simulateTransaction(tx); state.simulationLedger = sim.latestLedger;
  const ok = S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble && !!sim.result;
  state.checkpointClaimable = ok && sim.result.retval.switch().name === 'scvBool' && sim.result.retval.b() === true;
  if (ok) state.checkpointInstructions = Number(sim.transactionData.build().resources().instructions());
  else state.checkpointSimulation = sim.restorePreamble ? 'RESTORE_REQUIRED' : 'FAILED';
  return state;
}
if (require.main === module) {
  assert(process.argv[2] && process.argv[3], 'usage: keeper-inspect-v2.cjs <public-config.json> <new-report.json>');
  assert(!fs.existsSync(process.argv[3]), 'preserve previous reports');
  inspect(JSON.parse(fs.readFileSync(process.argv[2]))).then(report => {
    fs.writeFileSync(process.argv[3], encode(report), {flag: 'wx'});
    console.log(encode({status: report.status, ledger: report.ledger, signing: false, submission: false}));
  }).catch(e => {console.error(String(e)); process.exitCode = 1;});
}
module.exports = {inspect, rewardState};
