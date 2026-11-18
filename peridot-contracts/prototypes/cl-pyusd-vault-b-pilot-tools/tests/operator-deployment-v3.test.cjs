'use strict';
// v3 tests for operator-deployment-v3.cjs and operator-deployment-draft-v3.cjs. Offline only: every RPC is an
// injected fake; nothing is signed or sent.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const D = require('../operator-deployment-v3.cjs');
const G = require('../operator-deployment-draft-v3.cjs');
const ROOT = path.resolve(__dirname, '../../..');
const draft = JSON.parse(fs.readFileSync(path.join(ROOT, 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/deploy/mainnet-deployment-plan-v3-seed800.json')));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const account = byte => S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, byte));
const KEEPER = 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4';
const PYUSD = {code: 'PYUSD', issuer: 'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5'};
const USDC = {code: 'USDC', issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'};
const AQUA = {code: 'AQUA', issuer: 'GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA'};
const sac = asset => new S.Asset(asset.code, asset.issuer).contractId(S.Networks.PUBLIC);
const CONTROLLER = {path: 'target/lending-medium-final-20261111/artifacts/simple_peridottroller.wasm', sha256: '847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902'};
const declared = row => S.TransactionBuilder.fromXDR(row.unsignedXdr, draft.network).toEnvelope().v1().tx().ext().sorobanData().resources().instructions();

function baseConfig(plan, stage, keeper) {
  const readbacks = {}, authRoots = {};
  for (const step of D.ORDER) { authRoots[step] = []; readbacks[step] = {before: [{test: 'injected adapter only'}], after: [{test: 'injected adapter only'}]}; }
  const common = {planDigestSha256: D.digest(plan),
    helperSha256: sha(fs.readFileSync(path.join(__dirname, '../operator-deployment-v3.cjs'))), runtimePins: D.runtimePins(),
    expiresAt: 2000, rpcUrl: 'https://rpc.invalid', protocolVersion: 29, source: plan.admin, initialSourceSequence: '8',
    keeperPublicAddress: keeper, aquaToken: sac(AQUA), controllerArtifact: CONTROLLER,
    costModelXdrSha256: {cpu: 'a'.repeat(64), memory: 'b'.repeat(64)}, uploadCpuPolicy: D.UPLOAD_CPU_POLICY, uploadCpuException: JSON.parse(JSON.stringify(D.UPLOAD_ALLOWANCE)),
    feeBudgetStroops: '100000', maxUploadTransactionFeeStroops: '20000', maxTransactionFeeStroops: '10000', inclusionFeeStroops: '1000', inclusionFeeCapStroops: '2000',
    balanceFloorStroops: '50000000', timeoutSeconds: 180, readbacks, authRoots,
    dependencies: {pool: {contract: plan.steps.find(s => s.step === 'initialize-vault').args[1], wasmSha256: D.PAIR.vault},
      oracle: {contract: plan.steps.find(s => s.step === 'initialize-vault').args[3], wasmSha256: D.PAIR.vault},
      fundingToken: {contract: sac(PYUSD), asset: PYUSD}, otherToken: {contract: sac(USDC), asset: USDC}, rewardToken: {contract: sac(AQUA), asset: AQUA}}};
  return stage === D.STAGES.PRE
    ? {status: D.PRE_RELEASE_STATUS, releaseStage: D.STAGES.PRE, userDecision: {date: '2026-11-15', text: D.USER_DECISION_20261115},
      releaseApproval: {status: 'NOT_REQUIRED_IN_PRE_RELEASE_STAGE', independentReview: null, signedCore: null}, ...common}
    : {status: 'REVIEWED_UNSIGNED_CEREMONY', releaseStage: D.STAGES.FULL, ...common};
}
function fixture(t, {stage = D.STAGES.PRE, keeper = null} = {}) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'unsigned-vault-ceremony-v3-')));
  t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  const evidence = path.join(dir, 'fake-reviewed-evidence.json'); fs.writeFileSync(evidence, '{"TEST_ONLY":true}');
  const pin = {path: evidence, sha256: sha(fs.readFileSync(evidence))};
  const plan = structuredClone(draft), config = baseConfig(plan, stage, keeper);
  if (stage === D.STAGES.FULL) config.releaseApproval = {status: 'APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE',
    wasmSha256: Object.fromEntries(plan.candidateArtifacts.map(a => [a.name, a.sha256])), independentReview: pin, signedCore: pin};
  function prepared(step = 'upload-vault', sequence = '9', cpu = 10000000, replaceOperation, cfg = config, resourceFee = '500') {
    const data = new S.SorobanDataBuilder().setResources(cpu, 0, 123532).setResourceFee(resourceFee).build();
    const op = replaceOperation || D.operation(plan, cfg, step);
    const tx = new S.TransactionBuilder(new S.Account(plan.admin, String(BigInt(sequence) - 1n)), {fee: cfg.inclusionFeeStroops, networkPassphrase: plan.network})
      .addOperation(op).setSorobanData(data).setLedgerbounds(101, 120).setTimebounds(1000, 1180).build();
    return {step, sequence, unsignedXdr: tx.toXDR(), hash: tx.hash().toString('hex'), maxFeeStroops: tx.fee,
      ledger: 100, closeTime: 1000, balanceStroops: '5000000000', sellingLiabilities: '0', networkMemoryAndEventCapsVerified: true};
  }
  let count = 0;
  const depsFor = cfg => ({now: () => 1001, prepare: async (step, sequence) => { count++; return prepared(step, sequence, 10000000, undefined, cfg); },
    transaction: async row => ({status: 'SUCCESS', hash: row.hash, sequence: row.sequence, source: plan.admin, feeCharged: '1200', ledger: 101}),
    readback: async () => ({ledger: 102, executableAndCodePinsVerified: true})});
  const state = path.join(dir, 'state');
  const writeConfig = (name, value) => { const file = path.join(dir, name); fs.writeFileSync(file, JSON.stringify(value)); return {path: file, sha256: sha(fs.readFileSync(file))}; };
  return {dir, plan, config, pin, prepared, depsFor, state, writeConfig, count: () => count,
    journal: () => JSON.parse(fs.readFileSync(path.join(state, 'journal.json'))),
    step: (cfg = config) => D.prepare(plan, cfg, state, depsFor(cfg)), reconcile: (cfg = config) => D.reconcile(plan, cfg, state, depsFor(cfg))};
}
async function runSteps(f, n, cfg = f.config) { for (let i = 0; i < n; i++) { await f.step(cfg); await f.reconcile(cfg); } }

test('stage values are exactly two; the pre-release stage needs the quoted user decision, its status and a 5 XLM floor', t => {
  assert.deepEqual(Object.values(D.STAGES), ['PRE_RELEASE_DEPLOYMENT_STEPS_1_TO_18', 'FULL_RELEASE_STEPS_1_TO_20']);
  const f = fixture(t); D.validateCeremony(f.plan, f.config, 1001);
  for (const change of [c => c.releaseStage = 'PRE_RELEASE', c => delete c.releaseStage, c => c.status = 'REVIEWED_UNSIGNED_CEREMONY',
    c => c.userDecision.text = 'deploy now', c => c.balanceFloorStroops = '49999999', c => c.keeperPublicAddress = f.plan.admin,
    c => c.helperSha256 = '0'.repeat(64), c => c.expiresAt = 1000, c => c.readbacks['bind-custody'].before = [], c => c.authRoots['vault-guardian'] = null]) {
    const c = structuredClone(f.config); change(c); assert.throws(() => D.validateCeremony(f.plan, c, 1001));
  }
  // A config still cannot claim the full-release status without the full-release pins.
  const c = structuredClone(f.config); c.releaseStage = D.STAGES.FULL; c.status = 'REVIEWED_UNSIGNED_CEREMONY';
  assert.throws(() => D.validateCeremony(f.plan, c, 1001));
});

test('steps 1-18 are preparable without release approval and without a keeper; 19 and 20 are refused as HELD', async t => {
  const f = fixture(t);
  assert.equal(f.config.keeperPublicAddress, null); assert.equal(f.config.releaseApproval.independentReview, null);
  // Keeper-dependent readbacks/auth may stay unresolved in this stage.
  f.config.authRoots['observation-keeper'] = null; f.config.readbacks['observation-keeper'].after = [{unresolved: 'keeper'}];
  f.config.readbacks['activate-receipt'].before = [{unresolved: 'keeper'}];
  D.validateCeremony(f.plan, f.config, 1001);
  await runSteps(f, 18);
  const j = f.journal(); assert.equal(j.entries.length, 18); assert(j.entries.every(r => r.status === 'CONFIRMED'));
  assert.deepEqual(j.entries.map(r => r.step), D.PRE_RELEASE_STEPS);
  assert.deepEqual(j.entries.map(r => r.sequence), Array.from({length: 18}, (_, i) => String(9 + i)));
  await assert.rejects(f.step(), /HELD: step 19 observation-keeper/);
  assert.equal(f.count(), 18); assert.equal(f.journal().entries.length, 18); assert.equal(f.journal().active, null);
  // Step 20 is refused the same way even if the journal were at 19 (see the keeper test).
  const review = D.review(f.plan, f.config);
  assert.deepEqual(review.heldSteps.map(h => h.step), ['observation-keeper', 'activate-receipt']);
  assert.equal(review.preparableSteps.length, 18);
});

test('with the keeper address the pre-release stage prepares step 19 after 18 and still refuses step 20', async t => {
  const f = fixture(t, {keeper: KEEPER}); D.validateCeremony(f.plan, f.config, 1001);
  await runSteps(f, 19);
  assert.equal(f.journal().entries.at(-1).step, 'observation-keeper');
  await assert.rejects(f.step(), /HELD: step 20 activate-receipt opens deposits to anyone/);
  assert.equal(f.count(), 19);
  assert.deepEqual(D.review(f.plan, f.config).heldSteps.map(h => h.step), ['activate-receipt']);
  // With a keeper, the keeper-dependent readbacks and the step-19 auth root become mandatory.
  const c = structuredClone(f.config); c.authRoots['observation-keeper'] = null; assert.throws(() => D.validateCeremony(f.plan, c, 1001));
  const d = structuredClone(f.config); d.readbacks['observation-keeper'].after = [{unresolved: 'x'}]; assert.throws(() => D.validateCeremony(f.plan, d, 1001), /keeper-dependent/);
});

test('step 20 needs a FULL_RELEASE config with the exact release-approval pins', t => {
  const f = fixture(t, {stage: D.STAGES.FULL, keeper: KEEPER}); D.validateCeremony(f.plan, f.config, 1001);
  for (const change of [c => c.releaseApproval.status = 'NOT_REQUIRED_IN_PRE_RELEASE_STAGE', c => c.releaseApproval.independentReview = null,
    c => c.releaseApproval.signedCore = null, c => delete c.releaseApproval, c => c.releaseApproval.signedCore = {path: f.pin.path, sha256: '0'.repeat(64)},
    c => c.releaseApproval.wasmSha256.vault = '0'.repeat(64), c => c.keeperPublicAddress = null, c => c.status = D.PRE_RELEASE_STATUS]) {
    const c = structuredClone(f.config); change(c); assert.throws(() => D.validateCeremony(f.plan, c, 1001));
  }
});

test('successor configs continue the journal: pre-release 1-18, then keeper successor for 19, then full release for 20', async t => {
  const f = fixture(t); await runSteps(f, 18);
  const first = f.writeConfig('pre-1.json', f.config);
  const second = baseConfig(f.plan, D.STAGES.PRE, KEEPER); second.initialSourceSequence = '200'; second.predecessorConfig = first;
  // The old config cannot be used once the journal is continued; the successor cannot skip a confirmation.
  const r19 = await f.step(second); assert.equal(r19.step, 'observation-keeper'); assert.equal(r19.sequence, '201');
  await assert.rejects(f.step(f.config), /continued by a newer successor/);
  await assert.rejects(f.reconcile(f.config), /continued by a newer successor/);
  await f.reconcile(second);
  const j = f.journal(); assert.equal(j.segments.length, 2); assert.equal(j.segments[1].firstEntry, 18); assert.equal(j.entries[18].segment, 1);
  await assert.rejects(f.step(second), /HELD: step 20/);
  const secondPin = f.writeConfig('pre-2.json', second);
  const full = fixture(t, {stage: D.STAGES.FULL, keeper: KEEPER}).config; // reuse a full-release shape
  full.releaseApproval.independentReview = full.releaseApproval.signedCore = f.pin;
  full.initialSourceSequence = '300'; full.predecessorConfig = secondPin;
  const r20 = await f.step(full); assert.equal(r20.step, 'activate-receipt'); assert.equal(r20.sequence, '301');
  await f.reconcile(full); assert.equal(f.journal().entries.length, 20);
  await assert.rejects(f.step(full), /already complete/);
});

test('a successor is refused while a step is outstanding, if it goes backwards in sequence, or full -> pre', async t => {
  const f = fixture(t); await runSteps(f, 3); await f.step();
  const pin = f.writeConfig('pre-1.json', f.config);
  const next = baseConfig(f.plan, D.STAGES.PRE, null); next.initialSourceSequence = '500'; next.predecessorConfig = pin;
  await assert.rejects(f.step(next), /must be reconciled/);
  await f.reconcile(); const pin2 = f.writeConfig('pre-1b.json', f.config);
  const back = baseConfig(f.plan, D.STAGES.PRE, null); back.initialSourceSequence = '5'; back.predecessorConfig = pin2;
  await assert.rejects(f.step(back), /successor sequence precedes/);
  const g = fixture(t, {stage: D.STAGES.FULL, keeper: KEEPER}); const fullPin = g.writeConfig('full.json', g.config);
  const pre = baseConfig(g.plan, D.STAGES.PRE, null); pre.predecessorConfig = fullPin;
  assert.throws(() => D.configChain(g.plan, pre), /cannot succeed a full-release/);
  // A successor cannot open a fresh journal.
  const h = fixture(t); const hp = h.writeConfig('p.json', h.config); const orphan = baseConfig(h.plan, D.STAGES.PRE, null); orphan.predecessorConfig = hp;
  await assert.rejects(h.step(orphan), /only continue an existing journal/);
});

test('cash-market targets and arguments are refused; controller steps may only target the fresh controller and name the new receipt', t => {
  assert.equal(D.FRESH_CONTROLLER, 'CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK');
  D.validatePlan(structuredClone(draft));
  const cash = Object.values(D.CASH_MARKETS);
  assert.equal(cash.length, 4);
  for (const market of cash) {
    for (const change of [p => p.steps.find(x => x.step === 'controller-list-new-market').args[0] = market,
      ...['set_pause_deposit', 'set_pause_redeem', 'set_pause_borrow'].map(n => p => p.steps.find(x => x.step === n).args[0] = market),
      ...['controller-list-new-market', 'set_pause_deposit', 'set_pause_redeem', 'set_pause_borrow', 'bind-controller', 'cash-reserve'].map(n => p => p.steps.find(x => x.step === n).target = market),
      p => p.controller = market]) {
      const p = structuredClone(draft); change(p); assert.throws(() => D.validatePlan(p), /cash market|fresh controller|REFUSED/);
    }
  }
  for (const change of [p => p.controller = 'CA' + p.controller.slice(2), p => delete p.unchangedCashMarkets.EURC]) {
    const p = structuredClone(draft); change(p); assert.throws(() => D.validatePlan(p));
  }
  // operation() repeats the guard even if a caller skipped validatePlan.
  const f = fixture(t); const p = structuredClone(draft); p.steps.find(x => x.step === 'set_pause_borrow').args[0] = D.CASH_MARKETS.PYUSD;
  assert.throws(() => D.operation(p, f.config, 'set_pause_borrow'), /live cash market/);
  const q = structuredClone(draft); q.steps.find(x => x.step === 'set_pause_deposit').target = D.CASH_MARKETS.XLM;
  assert.throws(() => D.operation(q, f.config, 'set_pause_deposit'), /live cash market/);
});

test('CPU: non-upload above 100M refused; frozen uploads up to 110M allowed, above refused; nothing clamped', t => {
  const f = fixture(t);
  for (const [step, sequence] of [['upload-vault', '9'], ['upload-receipt', '9']]) {
    for (const cpu of [104870950, 110000000]) D.validatePrepared(f.plan, f.config, f.prepared(step, sequence, cpu), sequence, '0');
    const over = f.prepared(step, sequence, 110000001), original = over.unsignedXdr;
    assert.throws(() => D.validatePrepared(f.plan, f.config, over, sequence, '0'), /declared CPU 110000001 exceeds 110000000/);
    assert.equal(over.unsignedXdr, original); assert.equal(declared(over), 110000001);
  }
  for (const step of ['create-vault', 'initialize-vault', 'controller-list-new-market', 'permanent-floor', 'bind-custody', 'vault-guardian']) {
    D.validatePrepared(f.plan, f.config, f.prepared(step, '9', 100000000), '9', '0');
    for (const cpu of [100000001, 110000000]) assert.throws(() => D.validatePrepared(f.plan, f.config, f.prepared(step, '9', cpu), '9', '0'), /exceeds 100000000 \(not a code upload/);
  }
  const other = S.Operation.uploadContractWasm({wasm: Buffer.from('not frozen')});
  assert.equal(D.instructionAllowance(other, 'upload-vault').limit, 100000000);
});

test('fee caps: the large cap applies only to the two frozen uploads; budget and floor are enforced', t => {
  const f = fixture(t);
  // 15,000 resource + 1,000 inclusion = 16,000: above the 10,000 non-upload cap, below the 20,000 upload cap.
  D.validatePrepared(f.plan, f.config, f.prepared('upload-vault', '9', 1000, undefined, f.config, '15000'), '9', '0');
  assert.throws(() => D.validatePrepared(f.plan, f.config, f.prepared('create-vault', '9', 1000, undefined, f.config, '15000'), '9', '0'), /non-upload cap/);
  assert.throws(() => D.validatePrepared(f.plan, f.config, f.prepared('upload-vault', '9', 1000, undefined, f.config, '19001'), '9', '0'), /upload cap/);
  assert.throws(() => D.validatePrepared(f.plan, f.config, f.prepared('upload-vault', '9', 1000), '9', '99000'), /budget/);
  const low = f.prepared('create-vault', '9'); low.balanceStroops = '50001000';
  assert.throws(() => D.validatePrepared(f.plan, f.config, low, '9', '0'), /balance floor/);
  const c = structuredClone(f.config); c.maxTransactionFeeStroops = '30000'; assert.throws(() => D.validateCeremony(f.plan, c, 1001), /fee caps/);
});

test('durable reservation, unknown status and failures keep v2 behaviour', async t => {
  const f = fixture(t); const r = await f.step();
  assert.equal(r.status, 'UNSIGNED_PREPARED_OPERATOR_REVIEW_REQUIRED'); assert.equal(r.signing, false); assert.equal(r.submission, false);
  assert.deepEqual(r.validLedgers, {min: 101, max: 120}); assert.equal(r.validUntilUnixTime, 1180);
  await assert.rejects(f.step(), /must be reconciled/);
  const deps = f.depsFor(f.config); deps.transaction = async () => ({status: 'NOT_FOUND'});
  assert.equal((await D.reconcile(f.plan, f.config, f.state, deps)).status, 'UNKNOWN_NO_RETRY');
  await assert.rejects(f.step(), /must be reconciled/);
  deps.transaction = async row => ({status: 'FAILED', hash: row.hash, sequence: row.sequence, source: f.plan.admin, feeCharged: '1300', ledger: 101});
  await assert.rejects(D.reconcile(f.plan, f.config, f.state, deps), /transaction failed/);
  assert.equal(f.journal().halted, true); await assert.rejects(f.step(), /confirmed failure/);
});

// Minimal fake RPC for the production adapter: answers instance/code/account/config reads coherently on one ledger.
function fakeRpc(f, {keeperExists = true, p99 = '200', cpu = 99000000, instancesPresent = false} = {}) {
  const bytes = {[CONTROLLER.sha256]: fs.readFileSync(path.join(ROOT, CONTROLLER.path)), [D.PAIR.vault]: fs.readFileSync(path.join(ROOT, draft.candidateArtifacts[0].path)), [D.PAIR.receipt]: fs.readFileSync(path.join(ROOT, draft.candidateArtifacts[1].path))};
  const captured = JSON.parse(fs.readFileSync(path.join(ROOT, 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/network-config-20261114.json')));
  const settings = Object.fromEntries(Object.entries(captured.settings).map(([name, row]) => [name, S.xdr.LedgerEntryData.fromXDR(row.xdr, 'base64').configSetting()]));
  f.config.costModelXdrSha256 = {cpu: sha(settings.configSettingContractCostParamsCpuInstructions.toXDR()), memory: sha(settings.configSettingContractCostParamsMemoryBytes.toXDR())};
  const assets = new Map([[sac(PYUSD), PYUSD], [sac(USDC), USDC], [sac(AQUA), AQUA]]);
  const proposed = new Set([f.plan.proposedInstances.vault.contract, f.plan.proposedInstances.receipt.contract]);
  const wasmOf = id => id === f.plan.proposedInstances.receipt.contract ? D.PAIR.receipt : id === f.plan.controller ? CONTROLLER.sha256 : D.PAIR.vault;
  const calls = {getLedgers: 0, keeperChecked: 0};
  const rpc = {
    getNetwork: async () => ({passphrase: f.plan.network}),
    getLatestLedger: async () => ({sequence: 100, closeTime: '1000', protocolVersion: 29}),
    getFeeStats: async () => ({sorobanInclusionFee: {p99}}),
    getLedgerEntries: async (...keys) => ({latestLedger: 100, entries: keys.flatMap(k => {
      const type = k.switch().name;
      if (type === 'account') {
        const id = S.StrKey.encodeEd25519PublicKey(k.account().accountId().ed25519());
        if (id === f.config.keeperPublicAddress) { calls.keeperChecked++; return keeperExists ? [{key: k, val: {}}] : []; }
        return [{key: k, val: {account: () => ({signers: () => [], thresholds: () => Buffer.from([1, 0, 0, 0]), ext: () => ({switch: () => 0}),
          numSubEntries: () => 0, seqNum: () => 26n, balance: () => 5000000000n})}}];
      }
      if (type === 'configSetting') return [{key: k, val: {configSetting: () => settings[k.configSetting().configSettingId().name]}}];
      if (type === 'contractCode' && !instancesPresent && Object.values(D.PAIR).includes(k.contractCode().hash().toString('hex'))) return [];
      if (type === 'contractCode') return [{key: k, val: {contractCode: () => ({code: () => bytes[k.contractCode().hash().toString('hex')]})}}];
      const id = S.Address.fromScAddress(k.contractData().contract()).toString();
      if (proposed.has(id) && !instancesPresent) return [];
      const executable = assets.has(id) ? {switch: () => ({name: 'contractExecutableStellarAsset'})}
        : {switch: () => ({name: 'contractExecutableWasm'}), wasmHash: () => Buffer.from(wasmOf(id), 'hex')};
      return [{key: k, val: {contractData: () => ({val: () => ({instance: () => ({executable: () => executable, storage: () => []})})})}}];
    })}),
    getLedgers: async () => { calls.getLedgers++; return {latestLedger: 100, ledgers: [{headerXdr: {header: () => ({ledgerSeq: () => 100,
      scpValue: () => ({closeTime: () => 1000n}), baseFee: () => 100, baseReserve: () => 5000000})}}]}; },
    simulateTransaction: async () => ({latestLedger: 100, _parsed: true, events: [], minResourceFee: '500',
      transactionData: new S.SorobanDataBuilder().setResources(cpu, 0, 123532).setResourceFee('500'), result: {auth: [], retval: S.xdr.ScVal.scvVoid()}}),
  };
  return {rpc, calls};
}

test('production adapter refuses step 19 until the keeper account exists on chain', async t => {
  const f = fixture(t, {keeper: KEEPER}); f.config.readbacks['observation-keeper'] = {before: [], after: []};
  const absent = fakeRpc(f, {keeperExists: false, instancesPresent: true});
  await assert.rejects(D.adapter(f.plan, f.config, absent.rpc).prepare('observation-keeper', '27'), /keeper account GBHWPV6Q.* does not exist on chain/);
  assert.equal(absent.calls.keeperChecked, 1); assert.equal(absent.calls.getLedgers, 0);
  const present = fakeRpc(f, {keeperExists: true, instancesPresent: true});
  const row = await D.adapter(f.plan, f.config, present.rpc).prepare('observation-keeper', '27');
  assert.equal(present.calls.keeperChecked, 1); assert.equal(row.step, 'observation-keeper');
  D.validatePrepared(f.plan, f.config, row, '27', '0');
});

test('production adapter: fresh upload kept verbatim up to 110M, refused above; live p99 above the bid refuses', async t => {
  const f = fixture(t); f.config.readbacks['upload-vault'] = {before: [], after: []};
  const ok = fakeRpc(f, {cpu: 104870950});
  const row = await D.adapter(f.plan, f.config, ok.rpc).prepare('upload-vault', '27'); D.validatePrepared(f.plan, f.config, row, '27', '0');
  assert.equal(declared(row), 104870950); assert.equal(S.TransactionBuilder.fromXDR(row.unsignedXdr, f.plan.network).signatures.length, 0);
  const over = fakeRpc(f, {cpu: 110000001}); const r2 = await D.adapter(f.plan, f.config, over.rpc).prepare('upload-vault', '27');
  assert.throws(() => D.validatePrepared(f.plan, f.config, r2, '27', '0'), /exceeds 110000000/);
  const surge = fakeRpc(f, {p99: '1001'});
  await assert.rejects(D.adapter(f.plan, f.config, surge.rpc).prepare('upload-vault', '27'), /p99 1001 exceeds the reviewed bid 1000/);
});

test('fresh generator: bid rule, steps 3-18 estimate, and a generated pre-release config validates with and without keeper', t => {
  assert.equal(G.inclusionBid('200'), 1000n); assert.equal(G.inclusionBid('100'), 1000n); assert.equal(G.inclusionBid('5000'), 25000n);
  assert.throws(() => G.inclusionBid('20001'), /refused/);
  assert.throws(() => G.build({}, 'GBHWIIIGBBEPRJSJIDSPWZE5USLY7TPUTHROK3VZ3ZTJ2GEKTBOPZON4'), /lookalike/); // the 15 Nov dust sender
  const est = G.steps3to18Estimate(); assert.equal(Object.keys(est.perStep).length, 16); assert.match(est.label, /ESTIMATE/);
  assert.equal(est.totalStroops, String(2n * 178317n + 2n * 4139106n + 11n * 219880n + 831022n));
  const f = fixture(t);
  const live = {observedAt: '2026-11-15T13:00:00.000Z', sequence: '246894391124295826', costModelXdrSha256: {cpu: 'a'.repeat(64), memory: 'b'.repeat(64)},
    dependencies: {pool: f.config.dependencies.pool, oracle: f.config.dependencies.oracle, fundingToken: f.config.dependencies.fundingToken,
      otherToken: f.config.dependencies.otherToken, rewardToken: f.config.dependencies.rewardToken},
    fees: {p99: '200', inclusion: '1000', uploadQuotes: {vault: '1648264116', receipt: '1451849565'},
      uploadDetails: {vault: {totalWithInclusionStroops: '1648265116'}, receipt: {totalWithInclusionStroops: '1451850565'}}}, provenance: {}};
  const now = Date.parse(live.observedAt) / 1000;
  for (const keeper of [null, KEEPER]) {
    const config = G.build(live, keeper); D.validateCeremony(f.plan, config, now);
    assert.equal(config.status, D.PRE_RELEASE_STATUS); assert.equal(config.releaseStage, D.STAGES.PRE);
    assert.equal(config.maxTransactionFeeStroops, '50000000'); assert.equal(config.balanceFloorStroops, '50000000');
    assert.equal(config.maxUploadTransactionFeeStroops, String((1648264116n * 110n + 99n) / 100n + 100000n));
    assert.equal(BigInt(config.feeBudgetStroops), (1648264116n * 110n + 99n) / 100n + 1000n + (1451849565n * 110n + 99n) / 100n + 1000n + 100000000n);
    assert.equal(D.stageSteps(config).length, keeper ? 19 : 18);
    if (keeper) {
      const root = S.xdr.SorobanAuthorizedInvocation.fromXDR(config.authRoots['observation-keeper'][0], 'base64');
      assert.equal(S.Address.fromScVal(root.function().contractFn().args()[1]).toString(), KEEPER);
      assert.equal(config.readbacks['observation-keeper'].after[0].expectedNative, KEEPER);
    }
    const req = G.fundingRequirements(config, live);
    assert(BigInt(req.xlm.requiredBeforeStep1Stroops) >= BigInt(config.feeBudgetStroops) + 50000000n + 50000000n);
    assert(BigInt(req.xlm.requiredBeforeStep1Stroops) >= BigInt(req.xlm.leadFormulaStroops));
  }
});

test('800 PYUSD seed (user decision 15 Nov): seed = cap = 8,000,000,000 raw, fallback 2 x 400, step-7 cap unchanged at 1,000', () => {
  const p = structuredClone(draft); D.validatePlan(p);
  assert.equal(p.selected.seedPrincipalRaw, '8000000000'); assert.equal(p.selected.backingCapRaw, '8000000000'); assert.equal(p.selected.maxDeployRaw, '10000000000');
  assert.equal(p.steps.find(x => x.step === 'activate-receipt').args[3], '8000000000');
  for (const change of [q => q.steps.find(x => x.step === 'activate-receipt').args[3] = '10000000000',
    q => { q.selected.backingCapRaw = q.selected.seedPrincipalRaw = '10000000000'; q.steps.find(x => x.step === 'activate-receipt').args[3] = '10000000000'; },
    q => q.selected.backingCapRaw = '8000000001', q => q.selected.maxDeployRaw = '8000000000', q => q.steps.find(x => x.step === 'vault-command-cap').args[1] = '8000000000',
    q => q.selected.fallbackSplitRaw = ['5000000000', '3000000000'], q => q.selected.quoteToleranceBps = 30, q => q.selected.quoteReserveMarginBps = 4000]) {
    const q = structuredClone(draft); change(q); assert.throws(() => D.validatePlan(q));
  }
});

// Fake RPC for the fresh generator, backed by the saved 13 Nov capture's real ledger entries (offline).
function generatorRpc({uploaded = [], created = [], cpu = 104870950} = {}) {
  const st = JSON.parse(fs.readFileSync(path.join(ROOT, 'research/cl-pyusd-tight-pilot-20261111/live-pool-state-20261113-v1/state.json')));
  const captured = JSON.parse(fs.readFileSync(path.join(ROOT, 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/network-config-20261114.json')));
  const byKey = new Map(st.entries.map(r => [r.key_xdr, S.xdr.LedgerEntryData.fromXDR(r.entry_data_xdr, 'base64')]));
  const settings = Object.fromEntries(Object.entries(captured.settings).map(([name, row]) => [name, S.xdr.LedgerEntryData.fromXDR(row.xdr, 'base64').configSetting()]));
  const ctrlBytes = fs.readFileSync(path.join(ROOT, CONTROLLER.path));
  const frozen = {[D.PAIR.vault]: fs.readFileSync(path.join(ROOT, draft.candidateArtifacts[0].path)), [D.PAIR.receipt]: fs.readFileSync(path.join(ROOT, draft.candidateArtifacts[1].path))};
  const V = draft.proposedInstances.vault, R = draft.proposedInstances.receipt;
  const wasmInstance = hash => ({contractData: () => ({val: () => ({instance: () => ({executable: () => ({switch: () => ({name: 'contractExecutableWasm'}), wasmHash: () => Buffer.from(hash, 'hex')}), storage: () => []})})})});
  const sims = [];
  const rpc = {getNetwork: async () => ({passphrase: S.Networks.PUBLIC}),
    getLatestLedger: async () => ({sequence: 100, closeTime: '1794747000', protocolVersion: 29}),
    getFeeStats: async () => ({sorobanInclusionFee: {p99: '200'}, latestLedger: 100}),
    simulateTransaction: async tx => { sims.push(tx.operations[0].type); return {latestLedger: 100, _parsed: true, events: [], minResourceFee: '1648264116',
      transactionData: new S.SorobanDataBuilder().setResources(cpu, 0, 123532).setResourceFee('1648264116'), result: {auth: [], retval: S.xdr.ScVal.scvVoid()}}; },
    getLedgerEntries: async (...keys) => ({latestLedger: 100, entries: keys.flatMap(k => {
      const type = k.switch().name;
      if (type === 'account') return [{key: k, val: {account: () => ({signers: () => [], thresholds: () => Buffer.from([1, 0, 0, 0]), seqNum: () => ({toString: () => '246894391124295829'}),
        balance: () => ({toString: () => '3862003503'}), numSubEntries: () => 3})}}];
      if (type === 'configSetting') return [{key: k, val: {configSetting: () => settings[k.configSetting().configSettingId().name]}}];
      if (type === 'contractCode') {
        const h = k.contractCode().hash().toString('hex');
        if (h === CONTROLLER.sha256) return [{key: k, val: {contractCode: () => ({code: () => ctrlBytes})}}];
        if (frozen[h]) return uploaded.includes(h) ? [{key: k, val: {contractCode: () => ({code: () => frozen[h]})}}] : [];
      }
      if (type === 'contractData') {
        const id = S.Address.fromScAddress(k.contractData().contract()).toString();
        if (id === draft.controller) return [{key: k, val: wasmInstance(CONTROLLER.sha256)}];
        if (id === V.contract || id === R.contract) return created.includes(id) ? [{key: k, val: wasmInstance(id === V.contract ? V.wasmSha256 : R.wasmSha256)}] : [];
      }
      const e = byKey.get(k.toXDR('base64')); return e ? [{key: k, val: e}] : [];
    })})};
  return {rpc, sims};
}
test('fresh generator readLive: new journal needs everything absent and simulates both uploads; successor checks confirmed presence and skips finished uploads', async () => {
  const fresh = generatorRpc(); const live = await G.readLive(fresh.rpc, 0);
  assert.deepEqual(Object.keys(live.fees.uploadDetails), ['vault', 'receipt']); assert.equal(fresh.sims.length, 2);
  assert.equal(live.sequence, '246894391124295829'); assert.equal(live.dependencies.fundingToken.asset.code, 'PYUSD');
  // Code already on chain but not confirmed in a journal: refused (reconcile instead).
  await assert.rejects(G.readLive(generatorRpc({uploaded: [D.PAIR.vault]}).rpc, 0), /already present on chain .* not confirmed/);
  // Successor after both uploads confirmed: presence required, no upload simulated, budget has no upload part.
  const after = generatorRpc({uploaded: [D.PAIR.vault, D.PAIR.receipt]}); const l2 = await G.readLive(after.rpc, 2);
  assert.deepEqual(l2.fees.uploadDetails, {}); assert.equal(after.sims.length, 0);
  const c2 = G.build(l2, KEEPER); assert.equal(c2.maxUploadTransactionFeeStroops, '50000000'); assert.equal(c2.feeBudgetStroops, '100000000');
  D.validateCeremony(l2.plan, c2, 1794747000);
  // Confirmed in the journal but absent on chain: stop.
  await assert.rejects(G.readLive(generatorRpc({uploaded: [D.PAIR.vault]}).rpc, 2), /confirmed in the journal but absent/);
  // After step 4 both instances must exist with the frozen executables.
  const l4 = await G.readLive(generatorRpc({uploaded: Object.values(D.PAIR), created: [draft.proposedInstances.vault.contract, draft.proposedInstances.receipt.contract]}).rpc, 4);
  assert.equal(l4.confirmedSteps, 4);
  // A fresh upload above 110M is a blocker, never adjusted.
  await assert.rejects(G.readLive(generatorRpc({cpu: 110000001}).rpc, 0), /BLOCKER: fresh upload-vault declares 110000001/);
});
