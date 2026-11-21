'use strict';
// Keeper v6 harness (copy of keeper-v5-harness.cjs restricted to v6): execution-ready v6 configuration, in-memory test
// approval and injected dependencies on the offline world (chain-stub-v6.cjs). Public identifiers only; the "signer"
// returns the unsigned transaction unchanged (the stub never checks signatures). 'OPERATOR_APPROVED' appears only in these
// in-memory test approvals. The v4 comparison of the v5 harness is not carried over (v4 is not part of this branch; the
// v4-vs-v5 incident reproduction stays in the v5 suite).
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const S = require('../../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {makeWorld} = require('./chain-stub-v6.cjs');
const W = require('../../keeper-worker-v6.cjs');
const L = require('../../keeper-loop-v6.cjs');
const publicKey = byte => S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, byte));
const contract = byte => S.StrKey.encodeContract(Buffer.alloc(32, byte));
const hash = s => crypto.createHash('sha256').update(s).digest('hex');
const STREAK = {maxObservationsPerAttempt: 6, maxTransactions: 30, maxFeePerTxStroops: '600000', totalFeeCapStroops: '15000000', balanceFloorStroops: '120000000', inclusionBidCapStroops: '10000'};

function tempDir(t) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'keeper-v6-test-')));
  if (t) t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  return dir;
}
function configFor(dir) {
  const names = ['configSettingContractCostParamsCpuInstructions', 'configSettingContractCostParamsMemoryBytes'];
  return {status: 'CONFIGURED_FOR_PILOT_EXECUTION', readOnly: false, network: S.Networks.PUBLIC,
    rpcUrl: 'https://rpc.invalid', admin: publicKey(1), keeperPublicAddress: publicKey(2),
    receipt: contract(1), vault: contract(2), fundingAsset: contract(3),
    wasmSha256: {receipt: '1'.repeat(64), vault: '2'.repeat(64)}, pilotOwners: [publicKey(1)],
    signer: {alias: 'pilot-keeper-test', storage: 'os-secure-store', configDirectory: path.join(dir, 'identities')},
    stateDirectory: path.join(dir, 'state-v6'), protocolVersion: 29,
    costModelXdrSha256: {cpu: hash(names[0]), memory: hash(names[1])},
    feeBudgetStroops: '1000000', maxTransactionFeeStroops: '10000', feeFloorXlm: '2.0000000',
    inclusionFeeStroops: '1000', inclusionFeeCapStroops: '5000', maxSnapshotAgeSeconds: 30, transactionTimeoutSeconds: 60};
}
function harness(t, worldOptions = {}) {
  const dir = tempDir(t), config = configFor(dir);
  const world = makeWorld({config, ...worldOptions});
  const calls = {sign: 0, submit: 0};
  const base = W.productionDependencies(config, {rpc: world.rpc, monotonicMs: () => world.clock.now(), cli: () => { throw Error('CLI must not run'); }});
  const deps = {...base, now: () => world.baseTime + Math.floor(world.clock.now() / 1000),
    runtimeBindings: () => ({testOnly: 'injected public runtime provenance; no real CLI calls'}),
    publicAddress: async () => config.keeperPublicAddress,
    sign: async prepared => { calls.sign++; return S.TransactionBuilder.fromXDR(prepared.unsignedXdr, config.network); },
    submit: async signed => { calls.submit++; return base.submit(signed); }};
  const approval = (schedule = null) => ({status: 'OPERATOR_APPROVED', scope: 'cl-vault-b-keeper-v6',
    configDigestSha256: W.configDigest(config), sourceSha256: W.sourceBindings(), runtimeProvenance: deps.runtimeBindings(),
    expiresAt: world.baseTime + 7200, secureStoreProvisioning: {alias: config.signer.alias, publicAddress: config.keeperPublicAddress,
      configDirectory: config.signer.configDirectory, reviewed: true},
    ...(schedule ? {supervisor: L.supervisorBinding(schedule), streak: STREAK} : {})});
  const journalFile = path.join(config.stateDirectory, 'journal.json');
  const journal = () => fs.existsSync(journalFile) ? JSON.parse(fs.readFileSync(journalFile)) : null;
  const step = (options = {}) => world.clock.drive(W.runSingleStep(config, {execute: true, approval: approval(), deps, ...options}));
  const loop = (schedule, {streak = null} = {}) => {
    const events = [];
    const loopDeps = {monotonicMs: () => world.clock.now(), epochSeconds: () => world.baseTime + Math.floor(world.clock.now() / 1000),
      sleep: ms => world.clock.sleep(ms), shouldStop: () => false, ledger: () => base.headLedger(),
      invoke: options => W.runSingleStep(config, {...options, deps}), event: e => events.push({...e, at: world.ledgerNow()}),
      warm: () => base.warmNetworkParameters(), ...(streak ? {streak} : {})};
    return world.clock.drive(L.runSupervisor(config, {execute: true, approval: approval(schedule), schedule, deps: loopDeps})).then(result => ({result, events}));
  };
  return {dir, config, world, deps, base, calls, approval, step, loop, journal, W};
}
module.exports = {harness, configFor, publicKey, contract, STREAK, tempDir};
