'use strict';
// Builds an execution-ready configuration, an in-memory test approval and injected dependencies for keeper v4/v5 on the
// offline world (chain-stub-v5.cjs). Public identifiers only; the "signer" returns the unsigned transaction unchanged
// (the stub never checks signatures). 'OPERATOR_APPROVED' appears only in these in-memory test approvals.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const S = require('../../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {makeWorld} = require('./chain-stub-v5.cjs');
const W5 = require('../../keeper-worker-v5.cjs'), W4 = require('../../keeper-worker-v4.cjs');
const L5 = require('../../keeper-loop-v5.cjs'), L4 = require('../../keeper-loop-v4.cjs');
const I4 = require('../../keeper-inspect-v4.cjs');
const publicKey = byte => S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, byte));
const contract = byte => S.StrKey.encodeContract(Buffer.alloc(32, byte));
const hash = s => crypto.createHash('sha256').update(s).digest('hex');
const STREAK = {maxObservationsPerAttempt: 6, maxTransactions: 30, maxFeePerTxStroops: '600000', totalFeeCapStroops: '15000000', balanceFloorStroops: '120000000', inclusionBidCapStroops: '10000'};

function tempDir(t) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'keeper-v5-test-')));
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
    stateDirectory: path.join(dir, 'state'), protocolVersion: 29,
    costModelXdrSha256: {cpu: hash(names[0]), memory: hash(names[1])},
    feeBudgetStroops: '1000000', maxTransactionFeeStroops: '10000', feeFloorXlm: '2.0000000',
    inclusionFeeStroops: '1000', inclusionFeeCapStroops: '5000', maxSnapshotAgeSeconds: 30, transactionTimeoutSeconds: 60};
}
function harness(t, worldOptions = {}, {version = 5} = {}) {
  const dir = tempDir(t), config = configFor(dir);
  const world = makeWorld({config, ...worldOptions});
  const W = version === 5 ? W5 : W4;
  const calls = {sign: 0, submit: 0};
  const base = W.productionDependencies(config, {rpc: world.rpc, monotonicMs: () => world.clock.now(), cli: () => { throw Error('CLI must not run'); }});
  const deps = {...base, now: () => world.baseTime + Math.floor(world.clock.now() / 1000),
    runtimeBindings: () => ({testOnly: 'injected public runtime provenance; no real CLI calls'}),
    publicAddress: async () => config.keeperPublicAddress,
    sign: async prepared => { calls.sign++; return S.TransactionBuilder.fromXDR(prepared.unsignedXdr, config.network); },
    submit: async signed => { calls.submit++; return base.submit(signed); }};
  if (version === 4) deps.inspect = c => I4.inspect(c, world.rpc);
  const approval = (schedule = null) => ({status: 'OPERATOR_APPROVED', scope: version === 5 ? 'cl-vault-b-pilot-keeper-v5' : 'cl-vault-b-pilot-keeper-v4',
    configDigestSha256: W.configDigest(config), sourceSha256: W.sourceBindings(), runtimeProvenance: deps.runtimeBindings(),
    expiresAt: world.baseTime + 7200, secureStoreProvisioning: {alias: config.signer.alias, publicAddress: config.keeperPublicAddress,
      configDirectory: config.signer.configDirectory, reviewed: true},
    ...(schedule ? {supervisor: (version === 5 ? L5 : L4).supervisorBinding(schedule), streak: STREAK} : {})});
  const journalFile = path.join(config.stateDirectory, 'journal.json');
  const journal = () => fs.existsSync(journalFile) ? JSON.parse(fs.readFileSync(journalFile)) : null;
  const step = (options = {}) => world.clock.drive(W.runSingleStep(config, {execute: true, approval: approval(), deps, ...options}));
  // The foreground supervisor on the same world; v5 uses its own head source, v4 getLatestLedger (as in production).
  const loop = (schedule, {streak = null} = {}) => {
    const events = [];
    const L = version === 5 ? L5 : L4;
    const loopDeps = {monotonicMs: () => world.clock.now(), epochSeconds: () => world.baseTime + Math.floor(world.clock.now() / 1000),
      sleep: ms => world.clock.sleep(ms), shouldStop: () => false,
      ledger: version === 5 ? () => base.headLedger() : async () => (await world.rpc.getLatestLedger()).sequence,
      invoke: options => W.runSingleStep(config, {...options, deps}), event: e => events.push({...e, at: world.ledgerNow()}),
      ...(version === 5 ? {warm: () => base.warmNetworkParameters()} : {}), ...(streak ? {streak} : {})};
    return world.clock.drive(L.runSupervisor(config, {execute: true, approval: approval(schedule), schedule, deps: loopDeps})).then(result => ({result, events}));
  };
  return {dir, config, world, deps, base, calls, approval, step, loop, journal, W};
}
module.exports = {harness, configFor, publicKey, contract, STREAK, tempDir};
