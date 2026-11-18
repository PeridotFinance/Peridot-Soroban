'use strict';
// v2 tests for operator-deployment-v2.cjs. Copied from the v1 tests; the upload-policy tests are replaced
// and extended for the hash-bound 110M upload allowance (user decision 14 Nov 2026). Offline only.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const D = require('../operator-deployment-v2.cjs');
const ROOT = path.resolve(__dirname, '../../..');
const draft = JSON.parse(fs.readFileSync(path.join(ROOT, 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/mainnet-deployment-plan-DRAFT.json')));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const contract = byte => S.StrKey.encodeContract(Buffer.alloc(32, byte));
const account = byte => S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, byte));
function fixture(t) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'unsigned-vault-ceremony-')));
  t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  const evidence = path.join(dir, 'fake-reviewed-evidence.json');
  fs.writeFileSync(evidence, '{"TEST_ONLY":true}');
  const pin = {path: evidence, sha256: sha(fs.readFileSync(evidence))};
  const plan = structuredClone(draft), readbacks = {}, authRoots = {};
  for (const step of D.ORDER) { authRoots[step] = []; readbacks[step] = {before: [{test: 'injected adapter only'}], after: [{test: 'injected adapter only'}]}; }
  const config = {status: 'REVIEWED_UNSIGNED_CEREMONY', planDigestSha256: D.digest(plan),
    helperSha256: sha(fs.readFileSync(path.join(__dirname, '../operator-deployment-v2.cjs'))), runtimePins: D.runtimePins(),
    expiresAt: 2000, rpcUrl: 'https://rpc.invalid', protocolVersion: 29, source: plan.admin, initialSourceSequence: '8',
    keeperPublicAddress: account(12), aquaToken: contract(13), controllerArtifact: plan.candidateArtifacts[0],
    releaseApproval: {status: 'APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE',
      wasmSha256: Object.fromEntries(plan.candidateArtifacts.map(a => [a.name, a.sha256])), independentReview: pin, signedCore: pin},
    costModelXdrSha256: {cpu: 'a'.repeat(64), memory: 'b'.repeat(64)}, uploadCpuPolicy: D.UPLOAD_CPU_POLICY, uploadCpuException: JSON.parse(JSON.stringify(D.UPLOAD_ALLOWANCE)),
    feeBudgetStroops: '100000', maxTransactionFeeStroops: '10000', inclusionFeeStroops: '1000', inclusionFeeCapStroops: '2000',
    balanceFloorStroops: '10000000', seedExecutionReserveRaw: '1', timeoutSeconds: 180, readbacks, authRoots,
    dependencies: {pool: {contract: plan.steps.find(s => s.step === 'initialize-vault').args[1], wasmSha256: '1'.repeat(64)},
      oracle: {contract: plan.steps.find(s => s.step === 'initialize-vault').args[3], wasmSha256: '2'.repeat(64)},
      fundingToken: {contract: plan.steps.find(s => s.step === 'initialize-receipt').args[0], wasmSha256: '3'.repeat(64)},
      otherToken: {contract: contract(14), wasmSha256: '5'.repeat(64)},
      rewardToken: {contract: contract(13), wasmSha256: '4'.repeat(64)}}};
  function prepared(step = 'upload-vault', sequence = '9', cpu = 10000000, replaceOperation) {
    const data = new S.SorobanDataBuilder().setResources(cpu, 0, 123532).setResourceFee('500').build();
    const op = replaceOperation || D.operation(plan, config, step);
    const tx = new S.TransactionBuilder(new S.Account(plan.admin, String(BigInt(sequence) - 1n)), {fee: '1000', networkPassphrase: plan.network})
      .addOperation(op).setSorobanData(data).setLedgerbounds(101, 120).setTimebounds(1000, 1180).build();
    return {step, sequence, unsignedXdr: tx.toXDR(), hash: tx.hash().toString('hex'), maxFeeStroops: tx.fee,
      ledger: 100, closeTime: 1000, balanceStroops: '5000000000', sellingLiabilities: '0', networkMemoryAndEventCapsVerified: true};
  }
  let count = 0;
  const deps = {now: () => 1001, prepare: async (step, sequence) => { count++; return prepared(step, sequence); },
    transaction: async row => ({status: 'SUCCESS', hash: row.hash, sequence: row.sequence, source: plan.admin, feeCharged: '1200', ledger: 101}),
    readback: async () => ({ledger: 102, executableAndCodePinsVerified: true})};
  const state = path.join(dir, 'state');
  return {dir, plan, config, prepared, deps, state, count: () => count,
    journal: () => JSON.parse(fs.readFileSync(path.join(state, 'journal.json'))),
    run: () => D.prepare(plan, config, state, deps), reconcile: () => D.reconcile(plan, config, state, deps)};
}
test('offline review derives the pinned fresh identities and refuses unknown configuration inputs', () => {
  const report = D.review(draft);
  assert.equal(report.signing, false); assert.equal(report.submission, false);
  const vaultUpload = report.uploads.find(a => a.role === 'vault');
  assert.equal(vaultUpload.savedDeclaredInstructions, 104870950); assert.equal(vaultUpload.limit, 110000000); assert.equal(vaultUpload.policyPass, true);
  assert.equal(report.uploads.find(a => a.role === 'receipt').limit, 110000000); assert.equal(report.limits.instructions, 100000000);
  assert.equal(report.steps.find(s => s.name === 'observation-keeper').status, 'UNRESOLVED');
  assert.equal(report.steps.find(s => s.name === 'controller-list-new-market').status, 'UNRESOLVED');
  assert.equal(report.steps.find(s => s.name === 'activate-receipt').status, 'OPERATION_ONLY');
  assert.throws(() => D.operation(draft, {}, 'upgrade_wasm'));
  assert.throws(() => D.operation(draft, {}, 'fund-pilot-command'));
});
test('wrong network, salt, code bytes, cash target, cap, and floor are refused before preparation', () => {
  for (const change of [p => p.network = S.Networks.TESTNET,
    p => p.proposedInstances.vault.salt = '0'.repeat(64),
    p => p.candidateArtifacts[0].sha256 = '0'.repeat(64),
    p => p.steps.find(x => x.step === 'bind-controller').target = p.unchangedCashMarkets.PYUSD,
    p => p.steps.find(x => x.step === 'activate-receipt').args[3] = '10000000001',
    p => p.steps.find(x => x.step === 'permanent-floor').args[1] = '1']) {
    const p = structuredClone(draft); change(p); assert.throws(() => D.validatePlan(p));
  }
});
test('NEGATIVE activation plan whose backing cap differs from the planned seed total is refused', () => {
  for (const change of [p => p.steps.find(x => x.step === 'activate-receipt').args[3] = '10000000001', p => p.steps.find(x => x.step === 'activate-receipt').args[3] = '20000000000',
    p => p.selected.backingCapRaw = '10000000001', p => p.selected.backingCapRaw = '5000000000', p => p.selected.seedPrincipalRaw = '9999999999',
    p => {p.selected.backingCapRaw = '20000000000'; p.steps.find(x => x.step === 'activate-receipt').args[3] = '20000000000';}]) {
    const p = structuredClone(draft); change(p); assert.throws(() => D.validatePlan(p), /activation backing cap must equal the planned seed total exactly/);
  }
  // Cap and seed moved together still fail: the pilot amount itself is pinned.
  const both = structuredClone(draft); both.selected.backingCapRaw = both.selected.seedPrincipalRaw = '20000000000'; both.steps.find(x => x.step === 'activate-receipt').args[3] = '20000000000';
  assert.throws(() => D.validatePlan(both)); D.validatePlan(structuredClone(draft));
});
test('exact release evidence, runtime/source/configuration, and separate keeper are mandatory', t => {
  const f = fixture(t); D.validateCeremony(f.plan, f.config, 1001);
  for (const change of [c => c.status = 'DRAFT', c => c.releaseApproval.status = 'UNSIGNED_ONLY',
    c => c.runtimePins.sdkEntrySha256 = '0'.repeat(64), c => c.helperSha256 = '0'.repeat(64),
    c => c.planDigestSha256 = '0'.repeat(64), c => c.keeperPublicAddress = f.plan.admin,
    c => c.readbacks['bind-custody'].before = [], c => c.authRoots['activate-receipt'] = null,
    c => c.uploadCpuPolicy = 'network-400M', c => c.expiresAt = 1000]) {
    const c = structuredClone(f.config); change(c); assert.throws(() => D.validateCeremony(f.plan, c, 1001));
  }
});
const declared = row => S.TransactionBuilder.fromXDR(row.unsignedXdr, draft.network).toEnvelope().v1().tx().ext().sorobanData().resources().instructions();
test('frozen vault upload at its saved 104,870,950 CPU is accepted under the hash-bound allowance; declaration untouched', t => {
  const f = fixture(t), row = f.prepared('upload-vault', '9', 104870950), original = row.unsignedXdr;
  D.validatePrepared(f.plan, f.config, row, '9', '0');
  assert.equal(row.unsignedXdr, original); assert.equal(declared(row), 104870950);
});
test('allowance ends at exactly 110,000,000 for both frozen uploads; one instruction more is refused and nothing is clamped', t => {
  const f = fixture(t);
  for (const [step, sequence] of [['upload-vault', '9'], ['upload-receipt', '9']]) {
    D.validatePrepared(f.plan, f.config, f.prepared(step, sequence, 110000000), sequence, '0');
    const over = f.prepared(step, sequence, 110000001), original = over.unsignedXdr;
    assert.throws(() => D.validatePrepared(f.plan, f.config, over, sequence, '0'), /declared CPU 110000001 exceeds 110000000/);
    assert.equal(over.unsignedXdr, original); assert.equal(declared(over), 110000001);
  }
});
test('every non-upload operation keeps 100,000,000: creation and configuration above it are refused', t => {
  const f = fixture(t);
  for (const step of ['create-vault', 'create-receipt', 'vault-command-cap', 'permanent-floor', 'bind-custody', 'activate-receipt']) {
    D.validatePrepared(f.plan, f.config, f.prepared(step, '9', 100000000), '9', '0');
    for (const cpu of [100000001, 104870950, 110000000]) {
      const row = f.prepared(step, '9', cpu), original = row.unsignedXdr;
      assert.throws(() => D.validatePrepared(f.plan, f.config, row, '9', '0'), /exceeds 100000000 \(not a code upload: 100M applies\)/);
      assert.equal(row.unsignedXdr, original);
    }
  }
});
test('upload of any other bytes never receives the allowance: wrong bytes, wrong step and a re-pointed plan are all refused', t => {
  const f = fixture(t);
  const other = S.Operation.uploadContractWasm({wasm: Buffer.concat([fs.readFileSync(path.join(ROOT, draft.candidateArtifacts[0].path)), Buffer.from([0])])});
  const a = D.instructionAllowance(other, 'upload-vault'); assert.equal(a.limit, 100000000); assert.equal(a.upload, false); assert.match(a.reason, /not the frozen artifact/);
  // Frozen receipt bytes presented under the vault step (and vice versa) are not the matching upload either.
  assert.equal(D.instructionAllowance(D.operation(f.plan, f.config, 'upload-receipt'), 'upload-vault').limit, 100000000);
  assert.equal(D.instructionAllowance(D.operation(f.plan, f.config, 'upload-vault'), 'upload-receipt').limit, 100000000);
  assert.equal(D.instructionAllowance(D.operation(f.plan, f.config, 'upload-vault'), 'create-vault').limit, 100000000);
  assert.equal(D.instructionAllowance(D.operation(f.plan, f.config, 'upload-vault'), 'upload-vault').limit, 110000000);
  assert.equal(D.instructionAllowance(D.operation(f.plan, f.config, 'create-vault'), 'upload-vault').limit, 100000000);
  // A prepared row carrying other bytes at 104.87M is refused before any journal entry can exist.
  const row = f.prepared('upload-vault', '9', 104870950, other), original = row.unsignedXdr;
  assert.throws(() => D.validatePrepared(f.plan, f.config, row, '9', '0'), /prepared operation changed/); assert.equal(row.unsignedXdr, original);
  // A plan re-pointed at other bytes (file, size and hash all consistent) is refused by the hard-coded pair.
  const file = path.join(f.dir, 'other.wasm'); fs.writeFileSync(file, Buffer.from('not the frozen candidate'));
  const plan = structuredClone(draft), art = plan.candidateArtifacts.find(x => x.name === 'vault');
  art.path = file; art.sha256 = sha(fs.readFileSync(file)); art.size = fs.statSync(file).size;
  plan.proposedInstances.vault.wasmSha256 = art.sha256; plan.steps.find(x => x.step === 'upload-vault').args.sha256 = art.sha256;
  assert.throws(() => D.validatePlan(plan), /not the frozen candidate/);
});
test('allowance is hard-coded and frozen: a reviewed config cannot widen it, add hashes or restore the old policy string', t => {
  const f = fixture(t); D.validateCeremony(f.plan, f.config, 1001);
  assert.equal(D.LIMITS.instructions, 100000000); assert.equal(D.UPLOAD_ALLOWANCE.instructions, 110000000);
  assert(Object.isFrozen(D.UPLOAD_ALLOWANCE) && Object.isFrozen(D.PAIR) && Object.isFrozen(D.UPLOAD_ALLOWANCE.steps));
  assert.deepEqual(D.PAIR, {vault: 'e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9', receipt: '140dcaab08680be910f9ea65f6682b72c1c4029bd991bca9397b516af9b66823'});
  for (const change of [c => c.uploadCpuException.instructions = 400000000, c => c.uploadCpuException.instructions = 110000001,
    c => c.uploadCpuException.otherTransactions = 110000000, c => c.uploadCpuException.steps['create-vault'] = D.PAIR.vault,
    c => c.uploadCpuException.wasmSha256.vault = '0'.repeat(64), c => delete c.uploadCpuException,
    c => c.uploadCpuPolicy = 'same-100M-no-exception', c => c.uploadCpuPolicy = 'network-400M']) {
    const c = structuredClone(f.config); change(c); assert.throws(() => D.validateCeremony(f.plan, c, 1001));
  }
});
test('prepared body, sequence, fees, spend floor, and bounds are checked from real unsigned XDR', t => {
  const f = fixture(t); D.validatePrepared(f.plan, f.config, f.prepared(), '9', '0');
  for (const change of [r => r.sequence = '10', r => r.hash = '0'.repeat(64),
    r => r.balanceStroops = '10000000', r => r.closeTime = 999,
    r => r.maxFeeStroops = '1499', r => r.networkMemoryAndEventCapsVerified = false]) {
    const r = f.prepared(); change(r); assert.throws(() => D.validatePrepared(f.plan, f.config, r, '9', '0'));
  }
  const wrong = f.prepared('upload-vault', '9', 10, D.operation(f.plan, f.config, 'upload-receipt'));
  assert.throws(() => D.validatePrepared(f.plan, f.config, wrong, '9', '0'), /operation changed/);
  assert.throws(() => D.validatePrepared(f.plan, f.config, f.prepared(), '9', '99000'));
});
test('durable unsigned reservation blocks duplicate preparation; unknown hash never releases it', async t => {
  const f = fixture(t); const result = await f.run();
  assert.equal(result.status, 'UNSIGNED_PREPARED_OPERATOR_REVIEW_REQUIRED');
  assert.equal(f.journal().entries[0].hash, result.hash); assert.equal(f.journal().active, 'upload-vault');
  await assert.rejects(f.run(), /must be reconciled/); assert.equal(f.count(), 1);
  f.deps.transaction = async () => ({status: 'NOT_FOUND'});
  assert.equal((await f.reconcile()).status, 'UNKNOWN_NO_RETRY');
  await assert.rejects(f.run(), /must be reconciled/);
});
test('confirmed transaction must match pinned hash/source/sequence before any advancement', async t => {
  const f = fixture(t); await f.run();
  for (const field of ['hash', 'source', 'sequence']) {
    f.deps.transaction = async row => ({status: 'SUCCESS', hash: row.hash, sequence: row.sequence,
      source: f.plan.admin, feeCharged: '1200', ledger: 101, [field]: 'wrong'});
    await assert.rejects(f.reconcile()); assert.equal(f.journal().entries[0].status, 'PREPARED');
  }
});
test('confirmed success with failed readback persists its fee once and blocks the next operation', async t => {
  const f = fixture(t); await f.run();
  f.deps.readback = async () => { throw Error('actual executable differs'); };
  await assert.rejects(f.reconcile(), /executable differs/);
  assert.equal(f.journal().entries[0].status, 'APPLIED_PENDING_READBACK'); assert.equal(f.journal().feesCharged, '1200');
  await assert.rejects(f.run(), /must be reconciled/);
  f.deps.transaction = async () => { throw Error('must not double-reconcile confirmed fee'); };
  f.deps.readback = async () => ({ledger: 102, executableAndCodePinsVerified: true});
  await f.reconcile(); assert.equal(f.journal().feesCharged, '1200');
  const next = await f.run(); assert.equal(next.step, 'upload-receipt'); assert.equal(next.sequence, '10');
});
test('failed confirmed transaction is charged and halted without retry', async t => {
  const f = fixture(t); await f.run();
  f.deps.transaction = async row => ({status: 'FAILED', hash: row.hash, sequence: row.sequence, source: f.plan.admin, feeCharged: '1300', ledger: 101});
  await assert.rejects(f.reconcile(), /transaction failed/); assert.equal(f.journal().feesCharged, '1300');
  assert.equal(f.journal().halted, true); await assert.rejects(f.run(), /confirmed failure/);
});
test('journal tampering, changed plan, and concurrent lock fail closed', async t => {
  const f = fixture(t); await f.run(); const j = f.journal(); j.active = null;
  fs.writeFileSync(path.join(f.state, 'journal.json'), JSON.stringify(j)); await assert.rejects(f.run());
  fs.writeFileSync(path.join(f.state, 'lock'), 'other worker'); await assert.rejects(f.run(), /EEXIST/);
});
test('configuration authorization insertion preserves exact host function and reviewed auth tree', t => {
  const f = fixture(t); const step = 'vault-command-cap';
  const op = D.operation(f.plan, f.config, step), invoke = op.body().invokeHostFunctionOp();
  const root = new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(invoke.hostFunction().invokeContract()), subInvocations: []});
  const auth = new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(), rootInvocation: root});
  invoke.auth([auth]); f.config.authRoots[step] = [root.toXDR('base64')];
  const row = f.prepared(step, '9', 1000, op); D.validatePrepared(f.plan, f.config, row, '9', '0');
  f.config.authRoots[step] = []; assert.throws(() => D.validatePrepared(f.plan, f.config, row, '9', '0'));
});
function rpcFixture(f, options = {}) {
  const captured = JSON.parse(fs.readFileSync(path.join(ROOT, 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/network-config-20261114.json')));
  const settings = Object.fromEntries(Object.entries(captured.settings).map(([name, row]) => [name, S.xdr.LedgerEntryData.fromXDR(row.xdr, 'base64').configSetting()]));
  f.config.costModelXdrSha256 = {cpu: sha(settings.configSettingContractCostParamsCpuInstructions.toXDR()), memory: sha(settings.configSettingContractCostParamsMemoryBytes.toXDR())};
  f.config.readbacks['upload-vault'] = {before: [], after: []};
  let simulations = 0;
  const rpc = {
    getNetwork: async () => ({passphrase: options.wrongNetwork ? S.Networks.TESTNET : f.plan.network}),
    getLatestLedger: async () => ({sequence: 100, closeTime: '1000', protocolVersion: options.changedProtocol ? 30 : 29}),
    getLedgerEntries: async (...keys) => ({latestLedger: 100, entries: keys[0].switch().name === 'account'
      ? [{val: {account: () => ({signers: () => [], thresholds: () => Buffer.from([1, 0, 0, 0]),
        ext: () => ({switch: () => 0}), numSubEntries: () => 0, seqNum: () => options.changedSequence ? 9n : 8n, balance: () => 5000000000n})}}]
      : keys[0].switch().name === 'configSetting'
        ? keys.map(key => ({val: {configSetting: () => settings[key.configSetting().configSettingId().name]}}))
        : options.existingInstance && keys[0].switch().name === 'contractData' ? [{unexpectedExistingInstance: true}] : []}),
    getLedgers: async () => ({latestLedger: 100, ledgers: [{headerXdr: {header: () => ({ledgerSeq: () => 100,
      scpValue: () => ({closeTime: () => 1000n}), baseFee: () => 100, baseReserve: () => 5000000})}}]}),
    simulateTransaction: async tx => {
      simulations++; assert.equal(tx.signatures.length, 0);
      assert.equal(tx.operations[0].func.switch().name, 'hostFunctionTypeUploadContractWasm');
      const result = {latestLedger: options.staleSimulation ? 101 : 100, _parsed: true, events: [], minResourceFee: '500',
        transactionData: new S.SorobanDataBuilder().setResources(options.cpu ?? 99000000, 0, 123532).setResourceFee('500'),
        result: {auth: [], retval: S.xdr.ScVal.scvBytes(Buffer.from(f.plan.proposedInstances.vault.wasmSha256, 'hex'))}};
      if (options.restore) result.restorePreamble = {};
      return result;
    },
  };
  return {rpc, simulations: () => simulations};
}
test('production adapter builds only unsigned upload from coherent public reads and exact captured costs', async t => {
  const f = fixture(t), r = rpcFixture(f);
  const row = await D.adapter(f.plan, f.config, r.rpc).prepare('upload-vault', '9');
  D.validatePrepared(f.plan, f.config, row, '9', '0');
  assert.equal(row.maxFeeStroops, '1500'); assert.equal(r.simulations(), 1);
  assert.equal(S.TransactionBuilder.fromXDR(row.unsignedXdr, f.plan.network).signatures.length, 0);
});
test('production adapter rejects wrong network/protocol/sequence, preexisting instance, restore and incoherent simulation', async t => {
  for (const options of [{wrongNetwork: true}, {changedProtocol: true}, {changedSequence: true},
    {existingInstance: true}, {restore: true}, {staleSimulation: true}]) {
    const f = fixture(t), r = rpcFixture(f, options);
    await assert.rejects(D.adapter(f.plan, f.config, r.rpc).prepare('upload-vault', '9'));
  }
});
test('production adapter keeps the simulated 104,870,950 declaration verbatim in the unsigned upload and journals it', async t => {
  const f = fixture(t), r = rpcFixture(f, {cpu: 104870950});
  const deps = D.adapter(f.plan, f.config, r.rpc); deps.now = () => 1001;
  const result = await D.prepare(f.plan, f.config, f.state, deps);
  assert.equal(result.step, 'upload-vault'); assert.equal(result.signing, false); assert.equal(result.submission, false);
  assert.equal(declared(result), 104870950); assert.equal(S.TransactionBuilder.fromXDR(result.unsignedXdr, f.plan.network).signatures.length, 0);
});
test('production result above 110M remains unsigned and cannot enter the durable preparation journal', async t => {
  const f = fixture(t), r = rpcFixture(f, {cpu: 110000001});
  const deps = D.adapter(f.plan, f.config, r.rpc); deps.now = () => 1001;
  await assert.rejects(D.prepare(f.plan, f.config, f.state, deps), /declared CPU 110000001 exceeds 110000000/);
  assert(!fs.existsSync(path.join(f.state, 'journal.json')));
});
test('concrete draft derives every known operation, authorization root and before/after invariant from exact ABIs', () => {
  const G = require('../operator-deployment-draft-v2.cjs'); const config = G.build(account(20));
  const report = D.review(draft, config);
  assert(report.steps.every(s => s.status === 'OPERATION_ONLY'));
  assert.equal(config.controllerArtifact.sha256, '847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902');
  assert.equal(config.dependencies.rewardToken.asset.code, 'AQUA');
  for (const name of D.ORDER) {
    assert(Array.isArray(config.authRoots[name]));
    for (const encoded of config.authRoots[name]) S.xdr.SorobanAuthorizedInvocation.fromXDR(encoded, 'base64');
    if (!name.startsWith('upload-') && !name.startsWith('create-')) {
      assert(config.readbacks[name].before.length > 0); assert(config.readbacks[name].after.length > 0);
      for (const r of [...config.readbacks[name].before, ...config.readbacks[name].after]) {
        assert(!r.unresolved); if (r.expectedResultXdr !== null && r.expectedResultXdr !== undefined) S.xdr.ScVal.fromXDR(r.expectedResultXdr, 'base64');
      }
    }
  }
  const seed = S.xdr.SorobanAuthorizedInvocation.fromXDR(config.authRoots['permanent-floor'][0], 'base64');
  assert.equal(seed.subInvocations().length, 1);
  const transfer = seed.subInvocations()[0].function().contractFn();
  assert.equal(transfer.functionName().toString(), 'transfer');
  assert.equal(S.scValToNative(transfer.args()[2]), 10000000n);
  assert(config.readbacks['bind-custody'].before.some(r => r.method === 'get_params'));
  assert(config.readbacks['activate-receipt'].after.some(r => r.method === 'stage_state'));
  assert.equal(config.status, 'DRAFT_NOT_APPROVED_UNSIGNED_ONLY'); assert.equal(config.keeperPublicAddress, account(20));
  assert.equal(config.uploadCpuPolicy, D.UPLOAD_CPU_POLICY); assert.equal(config.uploadCpuException.instructions, 110000000);
  assert.throws(() => D.validateCeremony(draft, config, 1001), /DRAFT_NOT_APPROVED/);
  // Without the keeper public address the keeper binding stays unresolved and the draft stays a draft.
  const pending = G.build(null); assert.equal(pending.keeperPublicAddress, null); assert.equal(pending.status, 'DRAFT_NOT_APPROVED_UNSIGNED_ONLY');
  assert.equal(D.review(draft, pending).steps.find(s => s.name === 'observation-keeper').status, 'UNRESOLVED');
});
