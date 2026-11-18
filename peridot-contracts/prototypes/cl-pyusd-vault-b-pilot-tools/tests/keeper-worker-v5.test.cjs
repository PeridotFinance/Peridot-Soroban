'use strict';
// v5: the v4 suite against keeper-worker-v5.cjs (scope v5; prepared.simulationLedger; production-adapter tests moved onto
// the offline world stub because v5 reads ledger/account/config/instances in one call and needs getEvents). Was v4: the v3 suite against keeper-worker-v4.cjs (scope v4; windows: observation 11, steps 21). Was: v2 suite run against keeper-worker-v3.cjs (source binding renamed); the 97M host-preflight tests are appended.
// Every signer, CLI and RPC is an injected fake. 'OPERATOR_APPROVED' appears only in in-memory test approvals.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const W = require('../keeper-worker-v5.cjs');
const {harness} = require('./support/keeper-v5-harness.cjs');
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
  const prepared = {ledger: 100, simulationLedger: 100, closeTime: 1000, source: config.keeperPublicAddress,
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
  const approval = () => ({status: 'OPERATOR_APPROVED', scope: 'cl-vault-b-pilot-keeper-v5',
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
  const changed = f.approval(); changed.sourceSha256['keeper-worker-v5.cjs'] = '0'.repeat(64);
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
  assert.deepEqual(Object.keys(result.sourceSha256), ['keeper-worker-v5.cjs', 'keeper-plan-v4.cjs', 'keeper-inspect-v5.cjs']);
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


// ---- v5: production adapter on the offline world stub (tests/support/chain-stub-v5.cjs); no keys, no network ----
const START = 1000, BASE = 1794920000, DL = BASE + 3600;
const cmd = (kind, phase, extra = {}) => ({id: 5, kind, phase, deadline: DL, ...extra});
const fixed = (t, options = {}, version = 5) => harness(t, {profile: 'FIXED', jitterMs: 0, startLedger: START, baseTime: BASE, ...options}, {version});
const txOf = h => { const [r] = [...h.world.landed.values()]; return r && S.TransactionBuilder.fromXDR(r.envelope.toXDR('base64'), h.config.network); };
const nothingSigned = h => { assert.equal(h.calls.sign + h.calls.submit, 0); const j = h.journal(); assert(!j || (j.entries.length === 0 && j.active === null && j.halted === false)); };
// Delay every call matching `when` by `ms` once the snapshot exists (after inspect).
function afterInspect(h, when, ms = 6000) {
  const state = {inspected: false, prepared: false, signed: false};
  const inspect = h.deps.inspect, prepare = h.deps.prepare, sign = h.deps.sign;
  h.deps.inspect = async c => { const r = await inspect(c); state.inspected = true; return r; };
  h.deps.prepare = async (p, r) => { const x = await prepare(p, r); state.prepared = true; return x; };
  h.deps.sign = async p => { const x = await sign(p); state.signed = true; return x; };
  h.hook = async (method, payload) => { if (when(method, payload, state)) await h.world.clock.sleep(ms); };
  return state;
}
const hooked = (t, options = {}, version = 5) => { let h; const hookRef = (m, p) => h.hook ? h.hook(m, p) : undefined; h = fixed(t, {...options, hook: hookRef}, version); return h; };
const isStepSim = (h, tx) => tx && tx.source === h.config.keeperPublicAddress;

test('v5 production adapter: unsigned observation with exact source auth, bounds anchored at the snapshot ledger', async t => {
  const h = fixed(t, {commands: [cmd(0, 3)]});
  const r = await h.step(); assert.equal(r.status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  const rec = h.journal().entries[0], tx = txOf(h);
  assert.equal(rec.method, 'stage_checkpoint_investment'); assert.equal(rec.snapshotLedger, START); assert.equal(rec.simulationLedger, START);
  assert.deepEqual(tx.ledgerBounds, {minLedger: START + 1, maxLedger: START + 11}); assert.equal(tx.operations[0].auth.length, 1);
  assert.equal(tx.fee, '1500'); assert.equal(h.calls.sign, 1);
});
test('v5 production simulation refuses restore, foreign or nested auth and an enlarged memory limit (nothing signed)', async t => {
  const foreign = (m, r, {root}) => { r.result.auth = [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsAddress(new S.xdr.SorobanAddressCredentials({
    address: new S.Address(publicKey(1)).toScAddress(), nonce: S.xdr.Int64.fromString('0'), signatureExpirationLedger: 200, signature: S.xdr.ScVal.scvVoid()})), rootInvocation: root})]; return r; };
  const nested = (m, r, {root}) => { root.subInvocations([new S.xdr.SorobanAuthorizedInvocation({function: root.function(), subInvocations: []})]); return r; };
  for (const options of [{tweak: (m, r) => Object.assign(r, {restorePreamble: {}})}, {tweak: foreign}, {tweak: nested}, {memoryLimit: 41943041}]) {
    const h = fixed(t, {commands: [cmd(0, 3)], ...options});
    await assert.rejects(h.step()); nothingSigned(h);
  }
});
test('v5 permissionless production step has no auth and uses the owner command deadline', async t => {
  const h = fixed(t, {commands: [cmd(0, 1, {deadline: BASE + 10})]});
  assert.equal((await h.step()).status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  assert.equal(h.journal().entries[0].maxTime, BASE + 10); assert.equal(txOf(h).operations[0].auth.length, 0);
  assert.deepEqual(txOf(h).ledgerBounds, {minLedger: START + 1, maxLedger: START + 21});
  const hostile = fixed(t, {commands: [cmd(0, 1)], tweak: (m, r, {invoke, root}) => { r.result.auth = [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(), rootInvocation: root})]; return r; }});
  await assert.rejects(hostile.step(), /unexpectedly requires/); nothingSigned(hostile);
});
test('v5 checkpoint_rewards: argument-free, authorization-free, vault target; refused when the step simulation would not claim', async t => {
  const h = fixed(t, {boundaries: [0], commands: []});
  assert.equal((await h.step()).status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  const tx = txOf(h), fn = tx.operations[0].func.invokeContract();
  assert.equal(fn.functionName().toString(), 'checkpoint_rewards'); assert.equal(fn.args().length, 0); assert.equal(tx.operations[0].auth.length, 0);
  assert.equal(S.Address.fromScAddress(fn.contractAddress()).toString(), h.config.vault);
  const g = fixed(t, {boundaries: [0], commands: [], tweak: (m, r, {tx: sim}) => m === 'checkpoint_rewards' && isStepSim(g, sim) ? Object.assign(r, {result: {...r.result, retval: S.xdr.ScVal.scvBool(false)}}) : r});
  await assert.rejects(g.step(), /would not claim/); nothingSigned(g);
  const f = fixture(t);
  for (const instructions of [100000001, 104870950, 110000000]) {
    const prepared = structuredClone(f.prepared); prepared.metrics.instructions = instructions;
    assert.throws(() => W.validatePrepared(f.config, f.report, prepared, 0n, 1001), /resource ceiling: instructions/);
  }
});

// ---- v5 adversarial: the ledger moves in a way that changes the decision -> nothing journaled or signed ----
test('v5 NEGATIVE job phase changes between the snapshot and the simulation burst: read again, nothing signed; next call plans the new phase', async t => {
  const h = hooked(t, {commands: [cmd(0, 1)]});
  const st = afterInspect(h, (m, p, s) => s.inspected && !s.prepared && m === 'simulateTransaction');
  h.world.mutate(START + 1, s => { s.commands[0].phase = 2; }); // a third party converted (permissionless)
  const r = await h.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'DECISION_INPUT_CHANGED'); nothingSigned(h);
  st.inspected = false; h.hook = null;
  const again = await h.step(); assert.equal(again.status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION'); assert.equal(h.journal().entries[0].method, 'stage_mint');
});
test('v5 NEGATIVE the guard observation changes between the snapshot and the simulation burst: read again, nothing signed', async t => {
  const h = hooked(t, {commands: [cmd(0, 3)], guard: {observed_ledger: START - 1, healthy_count: 3, inside: true}});
  afterInspect(h, (m, p, s) => s.inspected && !s.prepared && m === 'simulateTransaction');
  h.world.mutate(START + 1, s => { s.guard = {observed_ledger: START + 1, healthy_count: 1, inside: true}; });
  const r = await h.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'DECISION_INPUT_CHANGED'); nothingSigned(h);
});
test('v5 NEGATIVE invest window: simulation at O+3 is refused (planner no longer chooses invest); O+1..O+2 required by validatePrepared', async t => {
  const O = START - 2;
  const h = hooked(t, {commands: [cmd(0, 3)], guard: {observed_ledger: O, healthy_count: 3, inside: true}});
  afterInspect(h, (m, p, s) => s.inspected && !s.prepared && m === 'simulateTransaction');
  const r = await h.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'PLAN_CHANGED_AT_SIMULATION_LEDGER'); nothingSigned(h);
  const f = fixture(t); const p = f.report.plan.plans[0];
  // O = 96, snapshot R = 97 (O+1): accepted simulation ledgers are exactly O+1 and O+2.
  Object.assign(p, {method: 'stage_invest', minLedger: 97, maxLedgerExclusive: 100}); f.report.reads.guard.value.observed_ledger = 96; f.report.ledger = 97; f.report.plan.snapshotLedger = 97;
  for (const [sim, ok] of [[96, false], [97, true], [98, true], [99, false]]) {
    const q = {...structuredClone(f.prepared), ledger: 97, minLedger: 98, maxLedgerExclusive: 100, simulationLedger: sim};
    if (ok) W.validatePrepared(f.config, f.report, q, 0n, 1001); else assert.throws(() => W.validatePrepared(f.config, f.report, q, 0n, 1001), String(sim));
  }
  // and the live path: invest simulated at O+2 is prepared and lands at O+3
  const ok = fixed(t, {commands: [cmd(0, 3)], guard: {observed_ledger: START - 2, healthy_count: 3, inside: true}});
  assert.equal((await ok.step()).status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  assert.deepEqual(txOf(ok).ledgerBounds, {minLedger: START + 1, maxLedger: START + 2});
});
test('v5 NEGATIVE a simulation (or any burst read) older than the snapshot ledger is LEDGER_REGRESSED_READ_AGAIN', async t => {
  for (const which of ['simulateTransaction', 'getLedgerEntries', 'getEvents']) {
    let inspected = false;
    const h = fixed(t, {commands: [cmd(0, 1)], lag: method => inspected && method === which ? 1 : 0});
    const inspect = h.deps.inspect; h.deps.inspect = async c => { const r = await inspect(c); inspected = true; return r; };
    const r = await h.step(); assert.equal(r.status, 'LEDGER_REGRESSED_READ_AGAIN', which); assert.equal(r.reason, 'LEDGER_REGRESSED'); nothingSigned(h);
  }
});
test('v5 NEGATIVE a reward interval opens (emergency exit by any holder) between snapshot and simulation: read again', async t => {
  const h = hooked(t, {commands: [cmd(0, 1)]});
  afterInspect(h, (m, p, s) => s.inspected && !s.prepared && m === 'simulateTransaction');
  h.world.mutate(START + 1, s => { s.boundaries.push(0); });
  const r = await h.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'DECISION_INPUT_CHANGED'); nothingSigned(h);
  h.hook = null; const again = await h.step(); assert.equal(again.status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION'); assert.equal(h.journal().entries[0].method, 'checkpoint_rewards');
});
test('v5 NEGATIVE checkpoint_rewards availability counts only when read at exactly the simulation ledger', async t => {
  const h = hooked(t, {boundaries: [0], commands: [cmd(0, 5)]});
  // Delay every burst-2 simulation except the checkpoint re-read: all one-way inputs are proven at L+1, checkpoint is not.
  afterInspect(h, (m, p, s) => s.inspected && !s.prepared && m === 'simulateTransaction' && !(p.source === h.config.admin && p.operations[0].func.invokeContract().functionName().toString() === 'checkpoint_rewards'));
  const r = await h.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'CHECKPOINT_NOT_AT_REFERENCE_LEDGER'); nothingSigned(h);
});
test('v5 NEGATIVE keeper account sequence changes before journaling: hard refusal, nothing signed', async t => {
  const h = hooked(t, {commands: [cmd(0, 1)]});
  afterInspect(h, (m, p, s) => { if (s.prepared && m === 'getLedgerEntries') h.world.setKeeperSequence(8n); return false; });
  await assert.rejects(h.step()); nothingSigned(h);
});
test('v5 window closing before journaling is a soft read-again; after signing it halts as in v4 (signed, never submitted)', async t => {
  const O = START - 2; // invest simulated at O+2; one ledger left
  const pre = hooked(t, {commands: [cmd(0, 3)], guard: {observed_ledger: O, healthy_count: 3, inside: true}});
  afterInspect(pre, (m, p, s) => s.prepared && m === 'getLedgerEntries');
  const r = await pre.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'WINDOW_CLOSING'); nothingSigned(pre);
  const post = hooked(t, {commands: [cmd(0, 3)], guard: {observed_ledger: O, healthy_count: 3, inside: true}});
  afterInspect(post, (m, p, s) => s.signed && m === 'getLedgerEntries');
  assert.equal((await post.step()).status, 'HALTED_UNKNOWN'); assert.equal(post.calls.sign, 1); assert.equal(post.calls.submit, 0);
  assert.equal(post.journal().active, 0); assert.deepEqual(post.journal().entries[0].events, ['PREPARED_BEFORE_SIGNING', 'HALTED_AFTER_PREPARATION']);
});
test('v5 a ledger closing during the freshness check is harmless (v4 threw LEDGER_MOVED outside its catch: the 17 Nov refusal path)', async t => {
  const h = hooked(t, {commands: [cmd(0, 1)]});
  afterInspect(h, (m, p, s) => s.prepared && m === 'getLedgerEntries');
  assert.equal((await h.step()).status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  const v4 = hooked(t, {commands: [cmd(0, 1)]}, 4);
  afterInspect(v4, (m, p, s) => s.prepared && m === 'getLedgerEntries');
  await assert.rejects(v4.step(), /ledger moved during preparation/); assert.equal(v4.calls.sign, 0);
});
test('v5 NEGATIVE a failed simulation in a proven snapshot is still a hard refusal (not read-again)', async t => {
  const h = fixed(t, {commands: [cmd(0, 1)], failMethods: ['stage_convert']});
  await assert.rejects(h.step(), /keeper simulation failed/); nothingSigned(h);
});
test('v5 deferObservation returns the observation plan to the loop before anything is journaled; soft codes map to fixed statuses', async t => {
  const f = fixture(t); Object.assign(f.report.plan.plans[0], {target: f.config.vault, method: 'stage_checkpoint_investment', args: [f.config.keeperPublicAddress], minLedger: 101, maxLedgerExclusive: 111});
  const r = await W.runSingleStep(f.config, {execute: true, approval: f.approval(), deps: f.deps, deferObservation: true});
  assert.equal(r.status, 'OBSERVATION_STREAK_REQUIRED'); assert.equal(f.calls.prepare + f.calls.sign + f.calls.submit, 0);
  await assert.rejects(W.runSingleStep(f.config, {deferObservation: true, deps: f.deps}), /execution only/);
  for (const code of W.SOFT_CODES) assert.equal(W.softStatus(W.soft(code)).status, code === 'LEDGER_REGRESSED' ? 'LEDGER_REGRESSED_READ_AGAIN' : 'LEDGER_CHANGED_READ_AGAIN');
  assert.equal(W.softStatus(Error('keeper simulation failed')), null); assert.throws(() => W.soft('ANY_OTHER'));
  f.deps.prepare = async () => { throw W.soft('DECISION_INPUT_CHANGED'); };
  const s = await f.run(); assert.equal(s.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(f.calls.sign, 0);
});
test('v5 header cache: fetched once, reused up to 120 ledgers, refreshed in the background after 60, refetched when too old', async t => {
  const h = fixed(t, {commands: [cmd(0, 1, {deadline: BASE + 7200})]});
  const getLedgers = () => h.world.stats.calls.getLedgers || 0;
  await h.world.clock.drive(h.base.warmNetworkParameters()); assert.equal(getLedgers(), 1);
  await h.world.clock.drive(h.base.warmNetworkParameters()); assert.equal(getLedgers(), 1, 'warm-up skips a fresh header');
  const at = async ledgers => { await h.world.clock.drive(h.world.clock.sleep(ledgers * 5500)); return h.world.clock.drive(h.base.prepare(...await planFor(h))); };
  await at(10); assert.equal(getLedgers(), 1);
  await at(55); await h.world.clock.drive(h.world.clock.sleep(1000)); assert.equal(getLedgers(), 2, 'background refresh past half age');
  await at(200); assert.equal(getLedgers(), 3, 'too old: fetched before preparing');
  assert.equal(W.HEADER_MAX_AGE_LEDGERS, 120);
});
async function planFor(h) { const r = await h.world.clock.drive(h.deps.inspect({...h.config, readOnly: true})); return [r.plan.plans[0], r]; }
test('v5 invest with only its last ledger left is journaled only if the freshness check answered within 1.5 s', async t => {
  const O = START - 2;
  const slow = hooked(t, {commands: [cmd(0, 3)], guard: {observed_ledger: O, healthy_count: 3, inside: true}});
  afterInspect(slow, (m, p, s) => s.prepared && m === 'getLedgerEntries', 2000); // slow RPC, same ledger
  const r = await slow.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'WINDOW_CLOSING'); nothingSigned(slow);
  const fast = hooked(t, {commands: [cmd(0, 3)], guard: {observed_ledger: O, healthy_count: 3, inside: true}});
  afterInspect(fast, (m, p, s) => s.prepared && m === 'getLedgerEntries', 1000);
  assert.equal((await fast.step()).status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  const other = hooked(t, {commands: [cmd(0, 1)]}); afterInspect(other, (m, p, s) => s.prepared && m === 'getLedgerEntries', 2000);
  assert.equal((await other.step()).status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION', 'command steps (20-ledger window) are not affected');
  assert.equal(W.LAST_LEDGER_MAX_FRESH_MS, 1500);
});
test('v5 NEGATIVE (lead decision) owner commands re-read one ledger after the simulation, unchanged, are still not accepted', async t => {
  // Snapshot at R; step simulation and guard/reward re-reads land in L_sim = R+1; the commands re-read lands in R+2 (unchanged).
  const h = hooked(t, {commands: [cmd(0, 1)]});
  const st = afterInspect(h, () => false);
  h.hook = async (m, p) => { if (!(st.inspected && !st.prepared && m === 'simulateTransaction')) return;
    const commands = p.source === h.config.admin && p.operations[0].func.invokeContract().functionName().toString() === 'stage_commands';
    await h.world.clock.sleep(commands ? 12000 : 6000); };
  const r = await h.step(); assert.equal(r.status, 'LEDGER_CHANGED_READ_AGAIN'); assert.equal(r.reason, 'COMMANDS_NOT_AT_REFERENCE_LEDGER'); nothingSigned(h);
  assert(W.SOFT_CODES.has('COMMANDS_NOT_AT_REFERENCE_LEDGER'));
  h.hook = null; assert.equal((await h.step()).status, 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
});
