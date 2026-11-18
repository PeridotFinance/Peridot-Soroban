'use strict';
// v4: the v3 suite against keeper-worker-v4.cjs (scope v4; windows: observation 11, steps 21). Was: v2 suite run against keeper-worker-v3.cjs (source binding renamed); the 97M host-preflight tests are appended.
// Every signer, CLI and RPC is an injected fake. 'OPERATOR_APPROVED' appears only in in-memory test approvals.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const W = require('../keeper-worker-v4.cjs');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const publicKey = byte => S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, byte));
const contract = byte => S.StrKey.encodeContract(Buffer.alloc(32, byte));

// Public identifiers only. No signing keys are generated or used by these tests.
function fixture(t) {
  const temp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'vault-keeper-worker-test-')));
  t.after(() => fs.rmSync(temp, {recursive: true, force: true}));
  const config = {status: 'CONFIGURED_FOR_PILOT_EXECUTION', readOnly: false, network: S.Networks.PUBLIC,
    rpcUrl: 'https://rpc.invalid', admin: publicKey(1), keeperPublicAddress: publicKey(2),
    receipt: contract(1), vault: contract(2), fundingAsset: contract(3),
    wasmSha256: {receipt: '1'.repeat(64), vault: '2'.repeat(64)}, pilotOwners: [publicKey(1)],
    signer: {alias: 'pilot-keeper-test', storage: 'os-secure-store', configDirectory: path.join(temp, 'identities')},
    stateDirectory: path.join(temp, 'state'), protocolVersion: 29,
    costModelXdrSha256: {cpu: '3'.repeat(64), memory: '4'.repeat(64)},
    feeBudgetStroops: '100000', maxTransactionFeeStroops: '10000', feeFloorXlm: '2.0000000',
    inclusionFeeStroops: '1000', inclusionFeeCapStroops: '5000', maxSnapshotAgeSeconds: 15, transactionTimeoutSeconds: 30};
  const report = {status: 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION', ledger: 100,
    rewards: {open: false, lastBoundaryId: null, interval: null, checkpointClaimable: null},
    plan: {snapshotLedger: 100, executeAtMostOneThenRefresh: true,
      plans: [{target: config.receipt, method: 'stage_convert', args: ['5']}]},
    reads: {guard: {value: {inside: true, healthy_count: 3, observed_ledger: 100}},
      ['commands:' + config.admin]: {value: [{id: '5', job: {deadline: '2000'}}]}}};
  const prepared = {ledger: 100, closeTime: 1000, source: config.keeperPublicAddress,
    accountSequence: '7', sequence: '8', hash: hash('UNSIGNED_FAKE_TRANSACTION'),
    unsignedXdr: 'UNSIGNED_FAKE_TRANSACTION', minLedger: 101, maxLedgerExclusive: 121, maxTime: 1030,
    maxFeeStroops: '1500', inclusionFeeStroops: '1000', balanceStroops: '100000000',
    sellingLiabilitiesStroops: '0', minimumReserveStroops: '10000000', memoryAndEventsEnforcedByNetwork: true,
    metrics: {instructions: 1000000, keys: 1, writes: 0, readBytes: 100, writeBytes: 0, signedEnvelopeBytes: 400}};
  const calls = {inspect: 0, identity: 0, prepare: 0, sign: 0, submit: 0, reconcile: 0, fresh: 0};
  const deps = {
    now: () => 1001,
    runtimeBindings: () => ({testOnly: 'injected public runtime provenance; no real CLI calls'}),
    assertNetwork: async () => {},
    inspect: async c => { calls.inspect++; assert.equal(c.readOnly, true); return structuredClone(report); },
    publicAddress: async () => { calls.identity++; return config.keeperPublicAddress; },
    prepare: async () => { calls.prepare++; return structuredClone(prepared); },
    assertFresh: async () => { calls.fresh++; },
    sign: async () => { calls.sign++; return {fakeOnly: 'FAKE_SIGNED_ENVELOPE_DO_NOT_LOG'}; },
    submit: async () => {
      calls.submit++;
      const journal = JSON.parse(fs.readFileSync(path.join(config.stateDirectory, 'journal.json')));
      assert.equal(journal.entries[journal.active].hash, prepared.hash);
      assert.equal(journal.entries[journal.active].sequence, prepared.sequence);
      assert.equal(journal.entries[journal.active].status, 'SUBMITTING');
      return {status: 'PENDING', hash: prepared.hash};
    },
    reconcile: async () => { calls.reconcile++; return {status: 'UNKNOWN'}; },
  };
  const approval = () => ({status: 'OPERATOR_APPROVED', scope: 'cl-vault-b-pilot-keeper-v4',
    configDigestSha256: W.configDigest(config), sourceSha256: W.sourceBindings(), runtimeProvenance: deps.runtimeBindings(), expiresAt: 2000,
    secureStoreProvisioning: {alias: config.signer.alias, publicAddress: config.keeperPublicAddress,
      configDirectory: config.signer.configDirectory, reviewed: true}});
  const run = () => W.runSingleStep(config, {execute: true, approval: approval(), deps});
  const journal = () => JSON.parse(fs.readFileSync(path.join(config.stateDirectory, 'journal.json')));
  return {temp, config, report, prepared, calls, deps, approval, run, journal};
}

test('default is read-only even with execution-ready configuration; no signer or journal access', async t => {
  const f = fixture(t);
  const result = await W.runSingleStep(f.config, {deps: f.deps});
  assert.equal(result.signing, false); assert.equal(result.submission, false);
  assert.equal(f.calls.inspect, 1); assert.equal(f.calls.identity + f.calls.sign + f.calls.submit, 0);
  assert(!fs.existsSync(f.config.stateDirectory));
});
test('explicit execution also requires exact, unexpired, separately reviewed configuration and sources', async t => {
  const f = fixture(t);
  await assert.rejects(W.runSingleStep(f.config, {execute: true, deps: f.deps}));
  const approval = f.approval(); f.config.inclusionFeeStroops = '1001';
  await assert.rejects(W.runSingleStep(f.config, {execute: true, approval, deps: f.deps}), /configuration differs/);
  const expired = f.approval(); expired.expiresAt = 1000;
  await assert.rejects(W.runSingleStep(f.config, {execute: true, approval: expired, deps: f.deps}), /expired/);
  const changed = f.approval(); changed.sourceSha256['keeper-worker-v4.cjs'] = '0'.repeat(64);
  await assert.rejects(W.runSingleStep(f.config, {execute: true, approval: changed, deps: f.deps}), /changed after approval/);
  const runtime = f.approval(); runtime.runtimeProvenance = {unexpected: 'runtime'};
  await assert.rejects(W.runSingleStep(f.config, {execute: true, approval: runtime, deps: f.deps}), /runtime differs/);
  assert.equal(f.calls.sign + f.calls.submit, 0);
});
test('draft, Ledger keeper, unreviewed key storage, and secret-like aliases are refused', async t => {
  const f = fixture(t); const original = structuredClone(f.config);
  for (const change of [c => c.status = 'DRAFT_NO_SIGNER_NO_SUBMISSION',
    c => c.keeperPublicAddress = c.admin, c => c.signer.storage = 'plaintext',
    c => c.signer.alias = 'word one word two']) {
    Object.assign(f.config, structuredClone(original)); change(f.config);
    await assert.rejects(f.run());
  }
  assert.equal(f.calls.sign, 0);
});
test('wrong alias public address and forbidden owner/admin operations never reach signing', async t => {
  const f = fixture(t); f.deps.publicAddress = async () => publicKey(9);
  await assert.rejects(f.run(), /public key differs/);
  f.deps.publicAddress = async () => f.config.keeperPublicAddress;
  for (const method of ['stage_open', 'stage_prepare_close', 'stage_emergency_exit', 'stage_cancel', 'stage_pause_deposits', 'upgrade_wasm']) {
    f.report.plan.plans[0].method = method;
    await assert.rejects(f.run(), /allowlist/);
  }
  assert.equal(f.calls.sign + f.calls.submit, 0);
});
test('coherent snapshot, CPU/key/write/entry caps, fee budget and balance floor fail closed', async t => {
  const f = fixture(t); const original = structuredClone(f.prepared);
  for (const change of [p => p.ledger++, p => p.closeTime = 900,
    p => p.metrics.instructions = 100000001, p => p.metrics.keys = 101,
    p => p.metrics.writes = 51, p => { p.metrics.keys = 70; p.metrics.writes = 40; },
    p => p.metrics.signedEnvelopeBytes = 132097, p => p.memoryAndEventsEnforcedByNetwork = false,
    p => p.maxFeeStroops = '10001', p => p.inclusionFeeStroops = '5001',
    p => p.balanceStroops = '20000000', p => p.maxLedgerExclusive = 999,
    p => p.maxTime = 2000]) {
    Object.assign(f.prepared, structuredClone(original)); change(f.prepared);
    await assert.rejects(f.run());
  }
  assert.equal(f.calls.sign + f.calls.submit, 0);
});
test('checkpoint is bounded to next ledger and investment to its observation freshness window', t => {
  const f = fixture(t); const p = f.report.plan.plans[0];
  Object.assign(p, {target: f.config.vault, method: 'stage_checkpoint_investment', args: [f.config.keeperPublicAddress], minLedger: 101, maxLedgerExclusive: 111});
  W.validatePlan(f.config, f.report, p); p.maxLedgerExclusive = 112;
  assert.throws(() => W.validatePlan(f.config, f.report, p));
  Object.assign(p, {target: f.config.receipt, method: 'stage_invest', args: ['5'], minLedger: 101, maxLedgerExclusive: 104});
  W.validatePlan(f.config, f.report, p); f.report.reads.guard.value.observed_ledger = 96;
  assert.throws(() => W.validatePlan(f.config, f.report, p));
});
test('single-step journals hash and sequence before submission and never stores signed XDR', async t => {
  const f = fixture(t); const result = await f.run();
  assert.equal(result.status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  assert.equal(f.calls.sign, 1); assert.equal(f.calls.submit, 1);
  const text = fs.readFileSync(path.join(f.config.stateDirectory, 'journal.json'), 'utf8');
  assert(!text.includes('FAKE_SIGNED_ENVELOPE')); assert(!text.includes('unsignedXdr'));
  assert.equal(f.journal().entries[0].status, 'SUBMITTED');
});
test('submission timeout stays locked to the original hash; subsequent calls only reconcile', async t => {
  const f = fixture(t);
  f.deps.submit = async () => { f.calls.submit++; throw Error('FAKE_SIGNED_ENVELOPE_DO_NOT_LOG'); };
  assert.equal((await f.run()).status, 'HALTED_UNKNOWN');
  assert.equal((await f.run()).status, 'HALTED_UNKNOWN');
  assert.equal(f.calls.sign, 1); assert.equal(f.calls.submit, 1); assert.equal(f.calls.reconcile, 1);
  assert(!JSON.stringify(f.journal()).includes('FAKE_SIGNED_ENVELOPE'));
});
test('stale snapshot after signing halts without submitting or automatically resigning', async t => {
  const f = fixture(t);
  f.deps.assertFresh = async () => { f.calls.fresh++; if (f.calls.fresh === 2) throw Error('ledger advanced'); };
  assert.equal((await f.run()).status, 'HALTED_UNKNOWN');
  assert.equal(f.calls.sign, 1); assert.equal(f.calls.submit, 0);
  await f.run(); assert.equal(f.calls.sign, 1);
});
test('confirmed failure halts and confirmed success only reconciles this invocation', async t => {
  for (const status of ['SUCCESS', 'FAILED']) {
    const f = fixture(t); await f.run();
    f.deps.reconcile = async record => ({status, hash: record.hash, source: record.source,
      sequence: record.sequence, ledger: 101, feeChargedStroops: '1234'});
    const result = await f.run();
    assert.equal(result.status, status === 'SUCCESS' ? 'RECONCILED_SUCCESS_REFRESH_REQUIRED' : 'HALTED_CONFIRMED_FAILURE');
    assert.equal(f.calls.sign, 1); assert.equal(f.journal().entries[0].actualFeeStroops, '1234');
    if (status === 'FAILED') { assert.equal((await f.run()).status, 'HALTED_REQUIRES_OPERATOR_REVIEW'); assert.equal(f.calls.sign, 1); }
  }
});
test('actual reconciled fees count toward the persistent cumulative budget', async t => {
  const f = fixture(t);
  Object.assign(f.config, {feeBudgetStroops: '1000', maxTransactionFeeStroops: '800', inclusionFeeStroops: '200', inclusionFeeCapStroops: '500'});
  Object.assign(f.prepared, {maxFeeStroops: '600', inclusionFeeStroops: '200'});
  await f.run();
  f.deps.reconcile = async record => ({status: 'SUCCESS', hash: record.hash, source: record.source,
    sequence: record.sequence, ledger: 101, feeChargedStroops: '600'});
  await f.run(); f.prepared.accountSequence = '8'; f.prepared.sequence = '9';
  await assert.rejects(f.run(), /cumulative approved fee budget/);
  assert.equal(f.calls.sign, 1);
});
test('single-worker lock is never stolen', async t => {
  const f = fixture(t); fs.mkdirSync(f.config.stateDirectory);
  const lock = path.join(f.config.stateDirectory, 'worker.lock'); fs.writeFileSync(lock, '{"token":"another-worker"}');
  await assert.rejects(f.run(), /EEXIST/); assert(fs.existsSync(lock)); assert.equal(f.calls.sign, 0);
});
test('corrupt journal cannot discard an unknown reservation or bypass a confirmed failure halt', async t => {
  const f = fixture(t); await f.run();
  const file = path.join(f.config.stateDirectory, 'journal.json');
  const journal = f.journal(); journal.active = null; fs.writeFileSync(file, JSON.stringify(journal));
  await assert.rejects(f.run(), /unresolved journal entry/); assert.equal(f.calls.sign, 1);
  journal.entries[0].status = 'FAILED'; journal.entries[0].actualFeeStroops = '1';
  fs.writeFileSync(file, JSON.stringify(journal));
  await assert.rejects(f.run(), /must remain halted/); assert.equal(f.calls.sign, 1);
});
test('confirmation with another hash, excessive fee or out-of-window ledger does not release reservation', async t => {
  const f = fixture(t); await f.run();
  for (const change of [row => row.hash = '0'.repeat(64), row => row.feeChargedStroops = '1501', row => row.ledger = 121]) {
    f.deps.reconcile = async record => { const row = {status: 'SUCCESS', hash: record.hash, source: record.source,
      sequence: record.sequence, ledger: 101, feeChargedStroops: '1234'}; change(row); return row; };
    await assert.rejects(f.run()); assert.equal(f.journal().active, 0); assert.equal(f.calls.sign, 1);
  }
});
test('explicit reconciliation works without execution approval or access to any signer', async t => {
  const f = fixture(t); await f.run();
  f.deps.publicAddress = f.deps.sign = f.deps.runtimeBindings = async () => { throw Error('must not access signer/runtime'); };
  f.deps.reconcile = async record => ({status: 'SUCCESS', hash: record.hash, source: record.source,
    sequence: record.sequence, ledger: 101, feeChargedStroops: '1234'});
  assert.equal((await W.runSingleStep(f.config, {reconcileOnly: true, deps: f.deps})).status, 'RECONCILED_SUCCESS_REFRESH_REQUIRED');
  assert.equal(f.calls.sign, 1); assert.equal(f.calls.submit, 1);
  await assert.rejects(W.runSingleStep(f.config, {execute: true, reconcileOnly: true, deps: f.deps}), /cannot enable execution/);
});

function unsignedRpcFixture(f, options = {}) {
  const names = ['configSettingContractComputeV0', 'configSettingContractEventsV0',
    'configSettingContractCostParamsCpuInstructions', 'configSettingContractCostParamsMemoryBytes'];
  f.config.costModelXdrSha256 = {cpu: hash(names[2]), memory: hash(names[3])};
  const value = {accountId: () => S.xdr.AccountId.publicKeyTypeEd25519(S.StrKey.decodeEd25519PublicKey(f.config.keeperPublicAddress)),
    numSubEntries: () => 0, signers: () => [], thresholds: () => Buffer.from([1, 0, 0, 0]),
    ext: () => ({switch: () => 0}), seqNum: () => 7n, balance: () => 100000000n};
  let simulations = 0;
  const rpc = {
    getNetwork: async () => ({passphrase: f.config.network, protocolVersion: 29}),
    getLatestLedger: async () => ({sequence: 100, closeTime: '1000'}),
    getLedgerEntries: async (...keys) => ({latestLedger: 100, entries: keys[0].switch().name === 'account'
      ? [{val: {account: () => value}}]
      : names.map(name => ({val: {configSetting: () => ({switch: () => ({name}), toXDR: () => Buffer.from(name),
        contractCompute: () => ({txMemoryLimit: () => options.memoryLimit || 41943040}),
        contractEvents: () => ({txMaxContractEventsSizeBytes: () => 16384})})}}))}),
    getLedgers: async () => ({latestLedger: 100, ledgers: [{headerXdr: {header: () => ({ledgerSeq: () => 100,
      scpValue: () => ({closeTime: () => 1000n}), baseFee: () => 100, baseReserve: () => 5000000})}}]}),
    simulateTransaction: async tx => {
      simulations++; assert.equal(tx.signatures.length, 0);
      const invoke = tx.operations[0].func.invokeContract();
      const root = new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(invoke), subInvocations: []});
      if (options.nestedAuth) root.subInvocations([new S.xdr.SorobanAuthorizedInvocation({function: root.function(), subInvocations: []})]);
      const credential = options.foreignAuth ? S.xdr.SorobanCredentials.sorobanCredentialsAddress(new S.xdr.SorobanAddressCredentials({
        address: new S.Address(f.config.admin).toScAddress(), nonce: S.xdr.Int64.fromString('0'), signatureExpirationLedger: 200, signature: S.xdr.ScVal.scvVoid()}))
        : S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount();
      const auth = invoke.functionName().toString() === 'stage_checkpoint_investment' || options.unexpectedAuth
        ? [new S.xdr.SorobanAuthorizationEntry({credentials: credential, rootInvocation: root})] : [];
      const result = {latestLedger: options.staleSimulation ? 101 : 100, _parsed: true, events: [], minResourceFee: '500',
        transactionData: new S.SorobanDataBuilder().setResources(1000000, 100, 0).setResourceFee('500'),
        result: {auth, retval: invoke.functionName().toString() === 'checkpoint_rewards' ? S.xdr.ScVal.scvBool(options.claimFails ? false : true) : S.xdr.ScVal.scvVoid()}};
      if (options.restore) result.restorePreamble = {};
      return result;
    },
  };
  return {rpc, simulations: () => simulations};
}
test('production adapter constructs an unsigned checkpoint with exact source auth and next-ledger bounds', async t => {
  const f = fixture(t); Object.assign(f.report.plan.plans[0], {target: f.config.vault, method: 'stage_checkpoint_investment',
    args: [f.config.keeperPublicAddress], minLedger: 101, maxLedgerExclusive: 111});
  const fake = unsignedRpcFixture(f);
  const deps = W.productionDependencies(f.config, {rpc: fake.rpc, cli: () => { throw Error('CLI must not run'); }});
  const prepared = await deps.prepare(f.report.plan.plans[0], f.report);
  W.validatePrepared(f.config, f.report, prepared, 0n, 1001);
  const tx = S.TransactionBuilder.fromXDR(prepared.unsignedXdr, f.config.network);
  assert.equal(tx.signatures.length, 0); assert.equal(tx.fee, '1500');
  assert.deepEqual(tx.ledgerBounds, {minLedger: 101, maxLedger: 111});
  assert.equal(tx.operations[0].auth.length, 1); assert.equal(fake.simulations(), 1);
});
test('production simulation refuses restore, foreign/nested auth, stale ledger and enlarged memory limits', async t => {
  for (const options of [{restore: true}, {foreignAuth: true}, {nestedAuth: true}, {staleSimulation: true}, {memoryLimit: 41943041}]) {
    const f = fixture(t); Object.assign(f.report.plan.plans[0], {target: f.config.vault, method: 'stage_checkpoint_investment',
      args: [f.config.keeperPublicAddress], minLedger: 101, maxLedgerExclusive: 111});
    const fake = unsignedRpcFixture(f, options);
    const deps = W.productionDependencies(f.config, {rpc: fake.rpc, cli: () => { throw Error('CLI must not run'); }});
    await assert.rejects(deps.prepare(f.report.plan.plans[0], f.report));
  }
});
test('permissionless production step has no auth and uses the owner command deadline', async t => {
  const f = fixture(t); f.report.reads['commands:' + f.config.admin].value[0].job.deadline = '1010';
  const fake = unsignedRpcFixture(f);
  const deps = W.productionDependencies(f.config, {rpc: fake.rpc});
  const prepared = await deps.prepare(f.report.plan.plans[0], f.report);
  assert.equal(prepared.maxTime, 1010);
  assert.equal(S.TransactionBuilder.fromXDR(prepared.unsignedXdr, f.config.network).operations[0].auth.length, 0);
  const hostile = unsignedRpcFixture(f, {unexpectedAuth: true});
  await assert.rejects(W.productionDependencies(f.config, {rpc: hostile.rpc}).prepare(f.report.plan.plans[0], f.report), /unexpectedly requires/);
});

// ---- v2 additions: checkpoint_rewards as a planned, allowlisted step (user decision 14 Nov 2026) ----
function checkpointFixture(t, rewards = {open: true, lastBoundaryId: '0', interval: '0', checkpointClaimable: true}) {
  const f = fixture(t); f.report.rewards = rewards;
  f.report.plan.plans = [{target: f.config.vault, method: 'checkpoint_rewards', args: []}];
  return f;
}
test('v2 checkpoint_rewards is accepted only for the vault, without arguments, while an interval is open and claimable', t => {
  const f = checkpointFixture(t), plan = f.report.plan.plans[0]; W.validatePlan(f.config, f.report, plan);
  for (const change of [p => p.target = f.config.receipt, p => p.args = ['0'], p => p.args = [f.config.keeperPublicAddress], p => p.minLedger = 101, p => p.maxLedgerExclusive = 105])
    {const p = structuredClone(plan); change(p); assert.throws(() => W.validatePlan(f.config, f.report, p));}
  for (const rewards of [{open: false}, {open: true, checkpointClaimable: false}, {open: true, checkpointClaimable: null}, {open: true}, null, undefined, {}])
    assert.throws(() => W.validatePlan(f.config, {...f.report, rewards}, plan));
  assert(W.METHODS.has('checkpoint_rewards')); assert.equal(W.LIMITS.instructions, 100000000);
});
test('v2 NEGATIVE while a reward interval is open nothing else may be prepared before checkpoint_rewards', t => {
  const f = checkpointFixture(t), other = method => method === 'stage_checkpoint_investment'
    ? {target: f.config.vault, method, args: [f.config.keeperPublicAddress], minLedger: 101, maxLedgerExclusive: 111}
    : {target: f.config.receipt, method, args: ['5'], ...(method === 'stage_invest' ? {minLedger: 101, maxLedgerExclusive: 104} : {})};
  const all = ['stage_checkpoint_investment', 'stage_convert', 'stage_mint', 'stage_invest', 'stage_finalise', 'stage_withdraw', 'stage_close_convert', 'stage_emergency_pay'];
  for (const method of all) assert.throws(() => W.validatePlan(f.config, f.report, other(method)), /checkpoint_rewards goes before anything else/);
  // Venue claim unavailable: only the two release steps with no reward dependency may proceed.
  f.report.rewards = {open: true, lastBoundaryId: '0', interval: '0', checkpointClaimable: false};
  for (const method of all) {
    if (W.RELEASE_METHODS.has(method)) W.validatePlan(f.config, f.report, other(method));
    else assert.throws(() => W.validatePlan(f.config, f.report, other(method)), /checkpoint_rewards goes before anything else/);
  }
  assert.deepEqual([...W.RELEASE_METHODS], ['stage_emergency_pay', 'stage_finalise']);
  f.report.rewards = {open: false}; for (const method of all) W.validatePlan(f.config, f.report, other(method));
});
test('v2 NEGATIVE enable_verified_reward_view and every other method outside the allowlist is refused', t => {
  const f = fixture(t);
  for (const method of ['enable_verified_reward_view', 'claim_rewards', 'propose_reward_segment', 'attribute_reward_segment', 'attribute_rewards', 'settle_reward_segment',
    'stage_open', 'stage_pause_deposits', 'stage_emergency_exit', 'stage_cancel', 'upgrade']) {
    assert(!W.METHODS.has(method));
    assert.throws(() => W.validatePlan(f.config, f.report, {target: f.config.vault, method, args: []}), /outside the keeper allowlist/);
  }
  assert.equal(W.METHODS.size, 9);
});
test('v2 production adapter builds an unsigned, argument-free, authorization-free checkpoint_rewards for the vault', async t => {
  const f = checkpointFixture(t), fake = unsignedRpcFixture(f);
  const deps = W.productionDependencies(f.config, {rpc: fake.rpc, cli: () => { throw Error('CLI must not run'); }});
  W.validatePlan(f.config, f.report, f.report.plan.plans[0]);
  const prepared = await deps.prepare(f.report.plan.plans[0], f.report); W.validatePrepared(f.config, f.report, prepared, 0n, 1001);
  const tx = S.TransactionBuilder.fromXDR(prepared.unsignedXdr, f.config.network), fn = tx.operations[0].func.invokeContract();
  assert.equal(tx.signatures.length, 0); assert.equal(tx.source, f.config.keeperPublicAddress); assert.equal(tx.operations.length, 1);
  assert.equal(fn.functionName().toString(), 'checkpoint_rewards'); assert.equal(fn.args().length, 0);
  assert.equal(S.Address.fromScAddress(fn.contractAddress()).toString(), f.config.vault); assert.equal(tx.operations[0].auth.length, 0);
  assert.deepEqual(tx.ledgerBounds, {minLedger: 101, maxLedger: 121}); assert.equal(prepared.maxTime, 1030); assert.equal(fake.simulations(), 1);
});
test('v2 NEGATIVE checkpoint_rewards is not prepared when the venue claim would fail, needs auth, needs a restore or exceeds 100M', async t => {
  for (const options of [{claimFails: true}, {unexpectedAuth: true}, {restore: true}, {staleSimulation: true}]) {
    const f = checkpointFixture(t), fake = unsignedRpcFixture(f, options);
    const deps = W.productionDependencies(f.config, {rpc: fake.rpc, cli: () => { throw Error('CLI must not run'); }});
    await assert.rejects(deps.prepare(f.report.plan.plans[0], f.report), options.claimFails ? /would not claim \(venue claim unavailable\); nothing is prepared/ : undefined);
  }
  const f = checkpointFixture(t);
  for (const instructions of [100000001, 104870950, 110000000]) {
    const prepared = structuredClone(f.prepared); prepared.metrics.instructions = instructions;
    assert.throws(() => W.validatePrepared(f.config, f.report, prepared, 0n, 1001), /resource ceiling: instructions/);
  }
});
test('v2 single step with injected fakes journals checkpoint_rewards (no arguments, vault target) and reconciles it', async t => {
  const f = checkpointFixture(t); const result = await f.run();
  assert.equal(result.status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION'); // fake signer and fake RPC only; see fixture()
  const record = f.journal().entries[0]; assert.equal(record.method, 'checkpoint_rewards'); assert.deepEqual(record.args, []); assert.equal(record.target, f.config.vault);
  assert(!JSON.stringify(f.journal()).includes('FAKE_SIGNED')); assert.equal(f.calls.sign, 1); assert.equal(f.calls.submit, 1);
  f.deps.reconcile = async r => ({status: 'SUCCESS', hash: r.hash, source: r.source, sequence: r.sequence, ledger: 101, feeChargedStroops: '1234'});
  assert.equal((await W.runSingleStep(f.config, {reconcileOnly: true, deps: f.deps})).status, 'RECONCILED_SUCCESS_REFRESH_REQUIRED');
  // Open interval but the snapshot offers another step first: refused before any signer is consulted.
  const g = checkpointFixture(t); g.report.plan.plans = [{target: g.config.receipt, method: 'stage_emergency_pay', args: ['5']}];
  await assert.rejects(g.run(), /checkpoint_rewards goes before anything else/); assert.equal(g.calls.sign + g.calls.submit, 0);
});
test('v2 default mode stays read-only and reports the v2 source bindings', async t => {
  const f = checkpointFixture(t), result = await W.runSingleStep(f.config, {deps: f.deps});
  assert.equal(result.executionEnabled, false); assert.equal(result.signing, false); assert.equal(result.submission, false);
  assert.deepEqual(Object.keys(result.sourceSha256), ['keeper-worker-v4.cjs', 'keeper-plan-v4.cjs', 'keeper-inspect-v4.cjs']);
  assert.equal(f.calls.identity + f.calls.sign + f.calls.submit, 0); assert(!fs.existsSync(f.config.stateDirectory));
});

// ---- v3 additions: host-preflight stop 97,000,000 and AT-LIMIT label (user rules 15 Nov 2026) ----
test('v3 NEGATIVE a keeper step above 97M host preflight is refused before journaling or signing; 97M exactly passes; labels', async t => {
  for (const [cpu, label] of [[97000001, 'ABOVE_97M_STOP'], [98000000, 'AT-LIMIT'], [99932089, 'AT-LIMIT'], [100000000, 'AT-LIMIT']]) {
    const f = fixture(t), p = structuredClone(f.prepared); p.metrics.instructions = cpu;
    assert.throws(() => W.validatePrepared(f.config, f.report, p, 0n, 1001), new RegExp(`${label}: host preflight ${cpu} > 97000000`));
    f.deps.prepare = async () => p; await assert.rejects(f.run(), new RegExp(label));
    assert.equal(f.calls.sign + f.calls.submit, 0); assert(!fs.existsSync(path.join(f.config.stateDirectory, 'journal.json')) || f.journal().entries.length === 0);
  }
  const ok = fixture(t), p = structuredClone(ok.prepared); p.metrics.instructions = 97000000; ok.deps.prepare = async () => p;
  const result = await ok.run(); assert.equal(result.status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION'); assert.equal(result.cpuLabel, 'WITHIN_POLICY'); assert.equal(result.declaredInstructions, 97000000);
  assert.equal(ok.journal().entries[0].declaredInstructions, 97000000);
  assert.equal(W.cpuLabel(94153726), 'WITHIN_POLICY'); assert.equal(W.HOST_STOP, 97000000); assert.equal(W.AT_LIMIT, 98000000); assert.equal(W.LIMITS.instructions, 100000000);
});
test('v3 checkpoint_rewards is also subject to the 97M stop (the controlled checkpoint was ~15.2M; a close above 97M must wait for it)', t => {
  const f = fixture(t); f.report.rewards = {open: true, checkpointClaimable: true, lastBoundaryId: '0', interval: '0'};
  f.report.plan.plans = [{target: f.config.vault, method: 'checkpoint_rewards', args: []}];
  W.validatePlan(f.config, f.report, f.report.plan.plans[0]);
  const p = structuredClone(f.prepared); p.metrics.instructions = 15200000; W.validatePrepared(f.config, f.report, p, 0n, 1001);
  p.metrics.instructions = 97500000; assert.throws(() => W.validatePrepared(f.config, f.report, p, 0n, 1001), /ABOVE_97M_STOP/);
});
test('v4 dedicated RPC: endpoint file + pinned host; exactly one of rpcUrl or rpcEndpointFile; URL never in the config digest', t => {
  const f = fixture(t), file = path.join(f.config.stateDirectory + '-rpc'); fs.writeFileSync(file, 'https://stellar-mainnet.g.alchemy.com/v2/secretpart', {mode: 0o600});
  const {rpcUrl, ...rest} = f.config, cfg = {...rest, rpcEndpointFile: file, rpcHost: 'stellar-mainnet.g.alchemy.com'};
  W.validateConfig(cfg); assert.equal(W.rpcUrlOf(cfg), 'https://stellar-mainnet.g.alchemy.com/v2/secretpart');
  assert(!JSON.stringify(cfg).includes('secretpart'));
  assert.throws(() => W.validateConfig({...cfg, rpcUrl: 'https://x.invalid'}), /exactly one/);
  assert.throws(() => W.rpcUrlOf({...cfg, rpcHost: 'other.example'}), /host differs/);
  fs.chmodSync(file, 0o640); assert.throws(() => W.rpcUrlOf(cfg), /mode 600/);
});

test('a ledger closing during preparation is LEDGER_CHANGED_READ_AGAIN before anything is journaled or signed; other prepare failures still refuse', async t => {
  const f = fixture(t);
  f.deps.prepare = async () => { f.calls.prepare++; W.sameLedger(101, 100); };
  const r = await f.run();
  assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.signing, false); assert.equal(r.submission, false);
  assert.equal(f.calls.sign + f.calls.submit + f.calls.fresh, 0);
  const jf = path.join(f.config.stateDirectory, 'journal.json');
  if (fs.existsSync(jf)) { const j = f.journal(); assert.equal(j.entries.length, 0); assert.equal(j.active, null); assert.equal(j.halted, false); }
  f.deps.prepare = async () => { throw Error('keeper simulation failed'); };
  await assert.rejects(f.run(), /keeper simulation failed/);
  assert.doesNotThrow(() => W.sameLedger(100, 100));
});
