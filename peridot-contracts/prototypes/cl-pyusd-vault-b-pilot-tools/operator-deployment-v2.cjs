// Unsigned-only ceremony preparation. There is deliberately no signer, key
// lookup, sendTransaction, shell execution, retry, or cash-market upgrade path.
//
// v2 (2026-11-14). Supersedes prototypes/cl-pyusd-vault-b/operator-deployment.cjs.
// Reviews, test evidence and helper hashes recorded for v1 do NOT transfer to this file.
// Second change: an activation plan whose backing cap differs from the planned seed total is refused by name.
// First change: USER DECISION 14 Nov 2026 - up to 110,000,000 declared CPU instructions
// for the two exact code-upload transactions of the frozen pair (hash-bound, below).
// Every other transaction keeps 100,000,000. Declared resources are never clamped or rewritten.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const ROOT = path.resolve(__dirname, '../..');
const LIMITS = Object.freeze({instructions: 100000000, memory: 41943040, keys: 100,
  writes: 50, entries: 100, readBytes: 200000, writeBytes: 132096, events: 16384, transactionBytes: 132096});
// Frozen candidate pair. Hard-coded here on purpose: neither the plan nor the
// config can name other bytes for the upload allowance.
const PAIR = Object.freeze({vault: 'e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9',
  receipt: '140dcaab08680be910f9ea65f6682b72c1c4029bd991bca9397b516af9b66823'});
const UPLOAD_CPU_POLICY = 'upload-only-110M-exact-frozen-pair-user-decision-20261114';
const UPLOAD_ALLOWANCE = Object.freeze({instructions: 110000000, otherTransactions: LIMITS.instructions,
  wasmSha256: PAIR, steps: Object.freeze({'upload-vault': PAIR.vault, 'upload-receipt': PAIR.receipt})});
// Limit for ONE actual operation. 110M only when (a) the host function is an upload,
// (b) the bytes inside the transaction hash to a frozen hash, and (c) the ceremony
// step is the matching upload step. Anything else: 100M.
function instructionAllowance(op, step) {
  const normal = reason => ({limit: LIMITS.instructions, upload: false, reason});
  if (op.body().switch().name !== 'invokeHostFunction') return normal('not a host-function operation');
  const fn = op.body().invokeHostFunctionOp().hostFunction();
  if (fn.switch().name !== 'hostFunctionTypeUploadContractWasm') return normal('not a code upload: 100M applies');
  const hash = sha(fn.wasm());
  if (UPLOAD_ALLOWANCE.steps[step] !== hash) return normal(`upload of bytes ${hash} is not the frozen artifact for step ${step}: 100M applies`);
  return {limit: UPLOAD_ALLOWANCE.instructions, upload: true, reason: `exact frozen upload ${hash}`};
}
const ORDER = ['upload-vault', 'upload-receipt', 'create-vault', 'create-receipt',
  'initialize-vault', 'vault-reward-token', 'vault-command-cap', 'initialize-receipt',
  'controller-list-new-market', 'set_pause_deposit', 'set_pause_redeem', 'set_pause_borrow',
  'bind-controller', 'cash-reserve', 'floor-only-supply-cap', 'permanent-floor',
  'bind-custody', 'vault-guardian', 'observation-keeper', 'activate-receipt'];
const CALLS = {
  'initialize-vault': ['vault', 'initialize'], 'vault-reward-token': ['vault', 'set_primary_reward_token'],
  'vault-command-cap': ['vault', 'set_max_deploy'], 'initialize-receipt': ['receipt', 'initialize'],
  'controller-list-new-market': ['controller', 'add_market'],
  set_pause_deposit: ['controller', 'set_pause_deposit'], set_pause_redeem: ['controller', 'set_pause_redeem'],
  set_pause_borrow: ['controller', 'set_pause_borrow'], 'bind-controller': ['receipt', 'set_peridottroller'],
  'cash-reserve': ['receipt', 'set_min_cash_reserve_bps'], 'floor-only-supply-cap': ['receipt', 'set_supply_cap'],
  'permanent-floor': ['receipt', 'seed_minimum_supply'], 'bind-custody': ['vault', 'stage_bind'],
  'vault-guardian': ['vault', 'stage_configure_guardian'], 'observation-keeper': ['vault', 'stage_set_invest_keeper'],
  'activate-receipt': ['receipt', 'stage_activate'],
};
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const stringify = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
const sorted = v => Array.isArray(v) ? v.map(sorted) : v && typeof v === 'object'
  ? Object.fromEntries(Object.keys(v).sort().map(k => [k, sorted(v[k])])) : v;
const digest = v => sha(JSON.stringify(sorted(v)));
const read = file => JSON.parse(fs.readFileSync(file));
function uint(v) { assert(/^(0|[1-9][0-9]*)$/.test(String(v)), 'exact unsigned integer required'); return BigInt(v); }
function filePin(pin) {
  assert(pin && typeof pin.path === 'string' && /^[a-f0-9]{64}$/.test(pin.sha256), 'file path/hash required');
  const bytes = fs.readFileSync(path.resolve(ROOT, pin.path));
  assert.equal(sha(bytes), pin.sha256, `file changed: ${pin.path}`);
  if (pin.size !== undefined) assert.equal(bytes.length, pin.size);
  return bytes;
}
function derivedId(network, account, salt) {
  assert(/^[a-f0-9]{64}$/.test(salt));
  const preimage = S.xdr.ContractIdPreimage.contractIdPreimageFromAddress(new S.xdr.ContractIdPreimageFromAddress({
    address: new S.Address(account).toScAddress(), salt: Buffer.from(salt, 'hex')}));
  return S.StrKey.encodeContract(S.hash(S.xdr.HashIdPreimage.envelopeTypeContractId(new S.xdr.HashIdPreimageContractId({
    networkId: S.hash(Buffer.from(network)), contractIdPreimage: preimage})).toXDR()));
}
function validatePlan(plan) {
  assert.equal(plan.network, S.Networks.PUBLIC);
  assert(S.StrKey.isValidEd25519PublicKey(plan.admin)); assert.equal(plan.guardian, plan.admin);
  assert(S.StrKey.isValidContract(plan.controller));
  assert.deepEqual(plan.steps.slice(0, ORDER.length).map(row => row.step), ORDER, 'ceremony order changed; review helper first');
  // Cap = exact seed total is the ONLY atomic guard for the window between activation and the seed: the seed
  // then reverts if any foreign principal exists, and the cap is full once it is funded. Refuse anything else.
  const activation = plan.steps.find(x => x.step === 'activate-receipt');
  assert(plan.selected?.backingCapRaw === plan.selected?.seedPrincipalRaw && activation?.args?.[3] === plan.selected?.seedPrincipalRaw,
    'activation backing cap must equal the planned seed total exactly (the only atomic guard for the pre-seed window)');
  assert.equal(plan.selected.receiptFloorRaw, '10000000'); assert.equal(plan.selected.seedPrincipalRaw, '10000000000');
  assert.equal(plan.selected.maxDeployRaw, '10000000000'); assert.equal(plan.selected.backingCapRaw, '10000000000');
  assert.deepEqual(plan.selected.ticks, [-20, 20]); assert.equal(plan.selected.fundingIndex, 0);
  assert.equal(plan.selected.quoteToleranceBps, 20); assert.equal(plan.selected.quoteReserveMarginBps, 5000);
  assert.equal(plan.candidateArtifacts.length, 2);
  for (const role of ['vault', 'receipt']) assert.equal(plan.candidateArtifacts.find(x => x.name === role)?.sha256, PAIR[role], `plan ${role} artifact is not the frozen candidate`);
  const ids = [plan.controller, ...Object.values(plan.unchangedCashMarkets)];
  for (const role of ['vault', 'receipt']) {
    const p = plan.proposedInstances[role], a = plan.candidateArtifacts.find(x => x.name === role);
    assert(a); assert.equal(p.wasmSha256, a.sha256); filePin(a);
    assert.equal(p.contract, derivedId(plan.network, plan.admin, p.salt));
    assert(!ids.includes(p.contract), 'fresh instances must differ from controller and every cash market'); ids.push(p.contract);
    assert.equal(plan.steps.find(x => x.step === `upload-${role}`).args.sha256, a.sha256);
    const creation = plan.steps.find(x => x.step === `create-${role}`);
    assert.deepEqual(creation.args, {source: plan.admin, salt: p.salt, candidateWasm: role, expectedContract: p.contract});
  }
  for (const [name, [role, method]] of Object.entries(CALLS)) {
    const step = plan.steps.find(x => x.step === name);
    assert.equal(step.method, method); assert.equal(step.target, role === 'controller' ? plan.controller : plan.proposedInstances[role].contract);
  }
  const args = name => plan.steps.find(x => x.step === name).args;
  const receipt = plan.proposedInstances.receipt.contract, vault = plan.proposedInstances.vault.contract;
  assert.deepEqual(args('vault-command-cap'), [plan.admin, '10000000000']);
  assert.deepEqual(args('floor-only-supply-cap'), ['10000000']);
  assert.deepEqual(args('permanent-floor'), [plan.admin, '10000000']);
  assert.deepEqual(args('cash-reserve'), [plan.admin, 10000]);
  assert.deepEqual(args('bind-custody'), [plan.admin, receipt]);
  assert.deepEqual(args('vault-guardian'), [plan.admin, plan.guardian]);
  assert.deepEqual(args('observation-keeper'), [plan.admin, '$approvedKeeperAddress']);
  assert.deepEqual(args('activate-receipt'), [plan.admin, vault, plan.guardian, '10000000000']);
  assert.deepEqual(args('controller-list-new-market'), [receipt]);
  assert.deepEqual(args('bind-controller'), [plan.controller]);
  for (const [name, paused] of [['set_pause_deposit', false], ['set_pause_redeem', false], ['set_pause_borrow', true]])
    assert.deepEqual(args(name), [receipt, paused]);
}
function runtimePins() {
  const sdk = path.resolve(ROOT, 'bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
  return {nodeVersion: process.version, nodeExecutableSha256: sha(fs.readFileSync(process.execPath)),
    sdkVersion: read(path.join(sdk, 'package.json')).version,
    sdkEntrySha256: sha(fs.readFileSync(require.resolve('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk'))),
    packageLockSha256: sha(fs.readFileSync(path.join(ROOT, 'bots/margin-liquidator/package-lock.json'))),
    trustBoundary: 'Operator-controlled installed dependencies remain immutable; package-lock is not recursive installed-file attestation.'};
}
function validateCeremony(plan, config, now) {
  validatePlan(plan);
  assert.equal(config.status, 'REVIEWED_UNSIGNED_CEREMONY');
  assert.equal(config.planDigestSha256, digest(plan));
  assert.equal(config.helperSha256, sha(fs.readFileSync(__filename)));
  assert.deepEqual(config.runtimePins, runtimePins());
  assert(Number.isSafeInteger(config.expiresAt) && config.expiresAt > now);
  const url = new URL(config.rpcUrl); assert.equal(url.protocol, 'https:'); assert(!url.username && !url.password);
  assert.equal(config.protocolVersion, 29);
  assert.equal(config.source, plan.admin, 'source must be the pinned Ledger admin'); uint(config.initialSourceSequence);
  assert(S.StrKey.isValidEd25519PublicKey(config.keeperPublicAddress)); assert.notEqual(config.keeperPublicAddress, plan.admin);
  assert(S.StrKey.isValidContract(config.aquaToken));
  assert.equal(config.releaseApproval.status, 'APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE');
  assert.deepEqual(config.releaseApproval.wasmSha256, Object.fromEntries(plan.candidateArtifacts.map(a => [a.name, a.sha256])));
  // These are separately reviewed evidence files, never manufactured by this tool.
  for (const name of ['independentReview', 'signedCore']) filePin(config.releaseApproval[name]);
  filePin(config.controllerArtifact);
  for (const name of ['cpu', 'memory']) assert(/^[a-f0-9]{64}$/.test(config.costModelXdrSha256[name]));
  assert.equal(config.uploadCpuPolicy, UPLOAD_CPU_POLICY, 'reviewed config must state the exact upload-only policy');
  assert.deepEqual(config.uploadCpuException, JSON.parse(JSON.stringify(UPLOAD_ALLOWANCE)), 'upload allowance must equal the hard-coded hash-bound allowance exactly; it cannot be widened by config');
  assert(uint(config.feeBudgetStroops) > 0n && uint(config.maxTransactionFeeStroops) > 0n && uint(config.maxTransactionFeeStroops) <= uint(config.feeBudgetStroops));
  assert(uint(config.inclusionFeeStroops) >= 100n && uint(config.inclusionFeeStroops) <= uint(config.inclusionFeeCapStroops));
  assert(uint(config.inclusionFeeCapStroops) <= uint(config.maxTransactionFeeStroops)); assert(uint(config.balanceFloorStroops) > 0n);
  assert(Number.isSafeInteger(config.timeoutSeconds) && config.timeoutSeconds > 0 && config.timeoutSeconds <= 300);
  for (const name of Object.keys(CALLS)) {
    for (const phase of ['before', 'after']) {
      assert(Array.isArray(config.readbacks?.[name]?.[phase]) && config.readbacks[name][phase].length > 0, `explicit reviewed ${phase} readbacks required for ${name}`);
      for (const rule of config.readbacks[name][phase]) assert(!rule.unresolved, 'keeper-dependent readback must be regenerated with the chosen public address');
    }
  }
  for (const name of ORDER) assert(Array.isArray(config.authRoots?.[name]), `explicit source authorization roots required for ${name}`);
  for (const role of ['pool', 'oracle', 'fundingToken', 'otherToken', 'rewardToken']) {
    const p = config.dependencies?.[role]; assert(p && S.StrKey.isValidContract(p.contract));
    if (p.asset) assert.equal(new S.Asset(p.asset.code, p.asset.issuer).contractId(plan.network), p.contract);
    else assert(/^[a-f0-9]{64}$/.test(p.wasmSha256));
  }
  assert.equal(config.dependencies.pool.contract, plan.steps.find(x => x.step === 'initialize-vault').args[1]);
  assert.equal(config.dependencies.oracle.contract, plan.steps.find(x => x.step === 'initialize-vault').args[3]);
  assert.equal(config.dependencies.fundingToken.contract, plan.steps.find(x => x.step === 'initialize-receipt').args[0]);
  assert.equal(config.dependencies.rewardToken.contract, config.aquaToken);
}
function resolve(value, config) {
  if (value === '$approvedKeeperAddress') { assert(config.keeperPublicAddress, 'keeper public address unresolved'); return config.keeperPublicAddress; }
  if (value === '$capturedAquaToken') { assert(config.aquaToken, 'reward token unresolved'); return config.aquaToken; }
  if (typeof value === 'string') { assert(!value.includes('$'), 'unresolved parameter'); return value; }
  if (Array.isArray(value)) return value.map(v => resolve(v, config));
  if (value && typeof value === 'object') {
    assert.deepEqual(Object.keys(value), ['Some'], 'unsupported structured argument'); return resolve(value.Some, config);
  }
  return value;
}
function operation(plan, config, name) {
  assert(ORDER.includes(name), 'outside upload/create/configuration ceremony; pilot lifecycle is separate');
  const step = plan.steps.find(x => x.step === name);
  if (name.startsWith('upload-')) return S.Operation.uploadContractWasm({wasm: filePin(plan.candidateArtifacts.find(a => a.name === name.slice(7)))});
  if (name.startsWith('create-')) {
    const p = plan.proposedInstances[name.slice(7)];
    return S.Operation.createCustomContract({address: new S.Address(plan.admin), wasmHash: Buffer.from(p.wasmSha256, 'hex'), salt: Buffer.from(p.salt, 'hex'), constructorArgs: []});
  }
  const [role] = CALLS[name];
  const artifact = role === 'controller' ? config.controllerArtifact : plan.candidateArtifacts.find(a => a.name === role);
  const spec = S.contract.Spec.fromWasm(filePin(artifact));
  const inputs = spec.getFunc(step.method).inputs(), args = resolve(step.args, config);
  assert.equal(args.length, inputs.length, 'argument count differs from exact artifact ABI');
  const named = Object.fromEntries(inputs.map((input, i) => [input.name().toString(), args[i]]));
  return new S.Contract(step.target).call(step.method, ...spec.funcArgsToScVals(step.method, named));
}
function review(plan, config = {}) {
  validatePlan(plan);
  return {status: 'OFFLINE_REVIEW_ONLY_NOT_RELEASE_APPROVAL', signing: false, submission: false,
    planDigestSha256: digest(plan), helperSha256: sha(fs.readFileSync(__filename)), runtimePins: runtimePins(),
    limits: LIMITS, uploadCpuPolicy: UPLOAD_CPU_POLICY, uploadCpuException: UPLOAD_ALLOWANCE,
    uploads: plan.candidateArtifacts.map(a => ({role: a.name, sha256: a.sha256, size: a.size,
      savedDeclaredInstructions: a.cpu, limit: instructionAllowance(operation(plan, config, `upload-${a.name}`), `upload-${a.name}`).limit,
      policyPass: a.cpu <= instructionAllowance(operation(plan, config, `upload-${a.name}`), `upload-${a.name}`).limit,
      caution: 'Saved quote is historical (capture ledger in the plan); prepare always simulates again. RPC success is not internal-policy acceptance.'})),
    steps: ORDER.map(name => { try { const op = operation(plan, config, name); return {name, status: 'OPERATION_ONLY', operationXdr: op.toXDR('base64')}; }
      catch (e) { return {name, status: 'UNRESOLVED', reason: e.message}; } }),
    requiredPreparationInputs: ['Exact-pair signed/Core approval and independent review evidence',
      'Dedicated keeper public address, captured AQUA address, exact controller artifact and external executable pins',
      'Fresh source sequence, explicit fee budget/transaction fee cap/inclusion cap/balance floor',
      'Current protocol/cost model pins, expiry, Node/SDK/helper fingerprints',
      'Reviewed before/after readbacks for every configuration step; permanent floor requires additional funding'],
    limitations: ['No signatures or submission are implemented.', 'No automatic retry or expired-operation release. Unknown hashes block the ceremony.',
      'Upload allowance is 110M for the two frozen hashes only (user decision 14 Nov 2026); every other transaction is 100M. No resource declarations are reduced.',
      'Seed/convert/mint/invest/finalise/pause and TTL are separate ceremonies; this ends after stage_activate.',
      'Readback assertions are approval inputs and must cover all deployment-plan invariants; this helper cannot infer business approval from JSON.']};
}
function atomic(file, value) {
  const tmp = `${file}.tmp-${process.pid}`; const fd = fs.openSync(tmp, 'wx', 0o600);
  try { fs.writeFileSync(fd, stringify(value)); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  fs.renameSync(tmp, file); const directory = fs.openSync(path.dirname(file), 'r');
  try { fs.fsyncSync(directory); } finally { fs.closeSync(directory); }
}
function locked(directory, run) {
  fs.mkdirSync(directory, {recursive: true, mode: 0o700});
  assert.equal(fs.realpathSync(directory), path.resolve(directory), 'journal path must be canonical');
  const lock = path.join(directory, 'lock'); const fd = fs.openSync(lock, 'wx', 0o600);
  return Promise.resolve().then(run).finally(() => { fs.closeSync(fd); fs.unlinkSync(lock); });
}
function loadJournal(directory, plan, config) {
  const file = path.join(directory, 'journal.json');
  const journal = fs.existsSync(file) ? read(file) : {schema: 'unsigned-vault-deployment-v1',
    planDigestSha256: digest(plan), configDigestSha256: digest(config), entries: [], active: null, feesCharged: '0', halted: false};
  assert.equal(journal.schema, 'unsigned-vault-deployment-v1');
  assert.equal(journal.planDigestSha256, digest(plan)); assert.equal(journal.configDigestSha256, digest(config));
  assert(journal.entries.length <= ORDER.length);
  let fees = 0n;
  for (let i = 0; i < journal.entries.length; i++) {
    const row = journal.entries[i]; assert.equal(row.step, ORDER[i]);
    assert.equal(row.sequence, String(uint(config.initialSourceSequence) + BigInt(i + 1)));
    assert(['PREPARED', 'APPLIED_PENDING_READBACK', 'CONFIRMED', 'FAILED'].includes(row.status));
    const tx = S.TransactionBuilder.fromXDR(row.unsignedXdr, plan.network);
    assert.equal(tx.signatures.length, 0); assert.equal(tx.hash().toString('hex'), row.hash);
    assert.equal(tx.source, plan.admin); assert.equal(tx.sequence, row.sequence);
    validatePrepared(plan, config, row, row.sequence, String(fees));
    assert.equal(row.feeCharged !== undefined, row.status !== 'PREPARED');
    if (row.feeCharged !== undefined) assert(uint(row.feeCharged) <= uint(row.maxFeeStroops));
    if (row.status === 'CONFIRMED') assert(row.readback?.executableAndCodePinsVerified === true);
    if (row.feeCharged !== undefined) fees += uint(row.feeCharged);
    if (i < journal.entries.length - 1) assert.equal(row.status, 'CONFIRMED');
  }
  assert.equal(String(fees), journal.feesCharged);
  const tail = journal.entries.at(-1);
  assert.equal(journal.active, tail && tail.status !== 'CONFIRMED' ? tail.step : null);
  assert.equal(journal.halted, tail?.status === 'FAILED');
  return journal;
}
function validatePrepared(plan, config, row, expectedSequence, spent) {
  const tx = S.TransactionBuilder.fromXDR(row.unsignedXdr, plan.network);
  assert.equal(tx.signatures.length, 0); assert.equal(tx.source, plan.admin); assert.equal(tx.sequence, expectedSequence);
  assert.equal(row.sequence, expectedSequence);
  assert.equal(tx.hash().toString('hex'), row.hash); assert.equal(tx.operations.length, 1);
  const actual = tx.toEnvelope().v1().tx().operations()[0], expected = operation(plan, config, row.step);
  assert(actual.sourceAccount() == null, 'unexpected operation source');
  assert.equal(actual.body().switch().name, 'invokeHostFunction');
  assert.equal(actual.body().invokeHostFunctionOp().hostFunction().toXDR('base64'), expected.body().invokeHostFunctionOp().hostFunction().toXDR('base64'), 'prepared operation changed');
  const auth = actual.body().invokeHostFunctionOp().auth();
  assert(auth.every(a => a.credentials().switch().name === 'sorobanCredentialsSourceAccount'));
  assert.deepEqual(auth.map(a => a.rootInvocation().toXDR('base64')), config.authRoots[row.step]);
  assert.equal(tx.fee, row.maxFeeStroops);
  const data = tx.toEnvelope().v1().tx().ext().sorobanData(), r = data.resources(), fp = r.footprint();
  assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'automatic restoration forbidden');
  const keys = [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64'));
  assert.equal(new Set(keys).size, keys.length);
  const allowance = instructionAllowance(actual, row.step);
  assert(r.instructions() <= allowance.limit, `declared CPU ${r.instructions()} exceeds ${allowance.limit} (${allowance.reason}); nothing is clamped`);
  assert(keys.length <= LIMITS.keys && fp.readWrite().length <= LIMITS.writes && keys.length + fp.readWrite().length <= LIMITS.entries);
  assert(r.diskReadBytes() <= LIMITS.readBytes && r.writeBytes() <= LIMITS.writeBytes);
  assert(Buffer.from(tx.toXDR(), 'base64').length + 72 <= LIMITS.transactionBytes);
  assert.equal(uint(tx.fee), uint(config.inclusionFeeStroops) + uint(data.resourceFee().toString()));
  assert(uint(tx.fee) <= uint(config.maxTransactionFeeStroops)); assert(uint(spent) + uint(tx.fee) <= uint(config.feeBudgetStroops));
  assert(uint(row.balanceStroops) - uint(row.sellingLiabilities) - uint(tx.fee) >= uint(config.balanceFloorStroops));
  assert(row.networkMemoryAndEventCapsVerified === true);
  assert(Number.isSafeInteger(row.ledger) && row.ledger > 0);
  assert.equal(Number(tx.ledgerBounds.minLedger), row.ledger + 1); assert.equal(Number(tx.ledgerBounds.maxLedger), row.ledger + 20);
  assert.equal(Number(tx.timeBounds.maxTime), row.closeTime + config.timeoutSeconds);
}
async function prepare(plan, config, directory, deps = adapter(plan, config)) {
  validateCeremony(plan, config, deps.now());
  return locked(directory, async () => {
    const journal = loadJournal(directory, plan, config); assert(!journal.halted, 'confirmed failure requires operator investigation');
    assert.equal(journal.active, null, 'existing prepared/unknown operation must be reconciled; never blindly prepare again');
    const step = ORDER[journal.entries.length]; assert(step, 'configuration ceremony already complete');
    const sequence = String(uint(config.initialSourceSequence) + BigInt(journal.entries.length + 1));
    const row = await deps.prepare(step, sequence);
    assert.equal(row.step, step); validatePrepared(plan, config, row, sequence, journal.feesCharged);
    row.status = 'PREPARED'; journal.entries.push(row); journal.active = step;
    // Durable hash/sequence/unsigned envelope precede any operator handoff.
    atomic(path.join(directory, 'journal.json'), journal);
    return {status: 'UNSIGNED_PREPARED_OPERATOR_REVIEW_REQUIRED', step, hash: row.hash, sequence,
      unsignedXdr: row.unsignedXdr, maxFeeStroops: row.maxFeeStroops, signing: false, submission: false};
  });
}
async function reconcile(plan, config, directory, deps = adapter(plan, config)) {
  // Reconciliation remains possible after expiry. No signer is ever consulted.
  validatePlan(plan);
  return locked(directory, async () => {
    const journal = loadJournal(directory, plan, config); assert(journal.active, 'no outstanding operation');
    const row = journal.entries.at(-1); assert.notEqual(row.status, 'FAILED', 'confirmed failure is halted');
    if (row.status === 'PREPARED') {
      const result = await deps.transaction(row);
      if (!['SUCCESS', 'FAILED'].includes(result.status)) return {status: 'UNKNOWN_NO_RETRY', hash: row.hash, signing: false, submission: false};
      assert.equal(result.hash, row.hash); assert.equal(result.sequence, row.sequence); assert.equal(result.source, plan.admin);
      assert(uint(result.feeCharged) <= uint(row.maxFeeStroops)); assert(Number.isSafeInteger(result.ledger) && result.ledger > row.ledger);
      row.feeCharged = String(result.feeCharged); row.appliedLedger = result.ledger;
      journal.feesCharged = String(uint(journal.feesCharged) + uint(result.feeCharged));
      row.status = result.status === 'SUCCESS' ? 'APPLIED_PENDING_READBACK' : 'FAILED'; journal.halted = result.status === 'FAILED';
      atomic(path.join(directory, 'journal.json'), journal);
    }
    assert(!journal.halted, 'operator transaction failed; halt, do not retry');
    row.readback = await deps.readback(row.step); row.status = 'CONFIRMED'; journal.active = null;
    atomic(path.join(directory, 'journal.json'), journal);
    return {status: 'CONFIRMED_WITH_READBACK', step: row.step, hash: row.hash, readback: row.readback, signing: false, submission: false};
  });
}
function adapter(plan, config, rpc = new S.rpc.Server(config.rpcUrl, {timeout: 45})) {
  const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
    contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
  const codeKey = hash => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(hash, 'hex')}));
  const checkLedger = (result, ledger) => assert.equal(result.latestLedger, ledger, 'ledger changed; no unsigned transaction prepared');
  async function network() { const n = await rpc.getNetwork(); assert.equal(n.passphrase, plan.network); }
  async function entries(keys, ledger) { const r = await rpc.getLedgerEntries(...keys); checkLedger(r, ledger); return r.entries; }
  async function code(hash, ledger, present = true) {
    const rows = await entries([codeKey(hash)], ledger); assert.equal(rows.length, present ? 1 : 0, present ? 'required code absent' : 'code already present; reconcile original upload instead');
    if (present) assert.equal(sha(rows[0].val.contractCode().code()), hash, 'actual code bytes differ');
  }
  async function instance(pin, ledger, present = true) {
    const rows = await entries([instanceKey(pin.contract)], ledger); assert.equal(rows.length, present ? 1 : 0, 'instance presence differs from ceremony');
    if (!present) return;
    const executable = rows[0].val.contractData().val().instance().executable();
    if (pin.asset) {
      assert.equal(executable.switch().name, 'contractExecutableStellarAsset');
      assert.equal(new S.Asset(pin.asset.code, pin.asset.issuer).contractId(plan.network), pin.contract);
    } else {
      assert.equal(executable.switch().name, 'contractExecutableWasm'); assert.equal(executable.wasmHash().toString('hex'), pin.wasmSha256);
      await code(pin.wasmSha256, ledger);
    }
  }
  async function account(ledger) {
    const key = S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: new S.Address(plan.admin).toScAddress().accountId()}));
    const rows = await entries([key], ledger); assert.equal(rows.length, 1);
    const a = rows[0].val.account();
    assert.equal(a.signers().length, 0); assert.equal(a.thresholds()[0], 1);
    assert([...a.thresholds().slice(1)].every(n => n <= 1));
    const v1 = a.ext().switch() === 1 ? a.ext().v1() : null;
    const v2 = v1 && v1.ext().switch() === 2 ? v1.ext().v2() : null;
    return {sequence: a.seqNum().toString(), balance: a.balance().toString(),
      selling: v1 ? v1.liabilities().selling().toString() : '0',
      reserveUnits: 2 + a.numSubEntries() + (v2 ? v2.numSponsoring() - v2.numSponsored() : 0)};
  }
  function transaction(a, op, latest) {
    return new S.TransactionBuilder(new S.Account(plan.admin, a.sequence), {fee: String(config.inclusionFeeStroops), networkPassphrase: plan.network})
      .addOperation(op).setLedgerbounds(latest.sequence + 1, latest.sequence + 20)
      .setTimebounds(Number(latest.closeTime), Number(latest.closeTime) + config.timeoutSeconds).build();
  }
  async function views(rules, a, latest) {
    const cache = new Map();
    const get = key => { const id = key.toXDR('base64'); if (!cache.has(id)) cache.set(id, entries([key], latest.sequence)); return cache.get(id); };
    return Promise.all(rules.map(async rule => {
      assert(S.StrKey.isValidContract(rule.contract));
      if (['instanceValue', 'persistentValue', 'instanceEmpty'].includes(rule.kind)) {
        if (rule.kind === 'instanceEmpty') {
          const rows = await get(instanceKey(rule.contract)); assert.equal(rows.length, 1);
          assert.equal((rows[0].val.contractData().val().instance().storage() || []).length, 0, 'new instance already initialized');
          return {kind: rule.kind, contract: rule.contract, empty: true};
        }
        const key = S.xdr.ScVal.fromXDR(rule.keyXdr, 'base64'); let actual;
        if (rule.kind === 'instanceValue') {
          const rows = await get(instanceKey(rule.contract)); assert.equal(rows.length, 1);
          const found = (rows[0].val.contractData().val().instance().storage() || []).filter(e => e.key().toXDR('base64') === rule.keyXdr);
          assert(found.length <= 1); actual = found.length ? found[0].val().toXDR('base64') : null;
        } else {
          const rows = await get(S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
            contract: new S.Address(rule.contract).toScAddress(), key, durability: S.xdr.ContractDataDurability.persistent()})));
          assert(rows.length <= 1); actual = rows.length ? rows[0].val.contractData().val().toXDR('base64') : null;
        }
        assert.equal(actual, rule.expectedResultXdr, `storage readback failed: ${rule.contract}.${rule.label || rule.keyXdr}`);
        return {kind: rule.kind, contract: rule.contract, keyXdr: rule.keyXdr, resultXdr: actual};
      }
      assert.equal(rule.kind || 'view', 'view'); assert(typeof rule.method === 'string');
      assert(Array.isArray(rule.argsXdr) && typeof rule.expectedResultXdr === 'string', 'typed readback args and exact expected result required');
      const op = new S.Contract(rule.contract).call(rule.method, ...rule.argsXdr.map(x => S.xdr.ScVal.fromXDR(x, 'base64')));
      const sim = await rpc.simulateTransaction(transaction(a, op, latest)); checkLedger(sim, latest.sequence);
      assert(S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble && sim.result, 'readback simulation failed or requires restore');
      assert.equal(sim.result.auth.length, 0, 'readbacks must not require authorization');
      assert.equal(sim.result.retval.toXDR('base64'), rule.expectedResultXdr, `readback failed: ${rule.contract}.${rule.method}`);
      return {contract: rule.contract, method: rule.method, argsXdr: rule.argsXdr, resultXdr: sim.result.retval.toXDR('base64')};
    }));
  }
  async function identities(step, latest, after = false) {
    const index = ORDER.indexOf(step), ledger = latest.sequence;
    const pins = [], hashes = new Set();
    for (const role of ['vault', 'receipt']) {
      const created = index > ORDER.indexOf(`create-${role}`) || (after && step === `create-${role}`);
      pins.push({...plan.proposedInstances[role], present: created});
      const uploaded = index > ORDER.indexOf(`upload-${role}`) || (after && step === `upload-${role}`);
      if (uploaded) hashes.add(plan.proposedInstances[role].wasmSha256);
    }
    if (CALLS[step]) {
      pins.push({contract: plan.controller, wasmSha256: config.controllerArtifact.sha256, present: true});
      for (const pin of Object.values(config.dependencies)) pins.push({...pin, present: true});
    }
    const rows = await entries(pins.map(p => instanceKey(p.contract)), ledger);
    const byKey = new Map(rows.map(row => [row.key.toXDR('base64'), row])); assert.equal(byKey.size, rows.length);
    for (const pin of pins) {
      const row = byKey.get(instanceKey(pin.contract).toXDR('base64'));
      assert.equal(!!row, pin.present, 'instance presence differs from ceremony'); if (!row) continue;
      const executable = row.val.contractData().val().instance().executable();
      if (pin.asset) {
        assert.equal(executable.switch().name, 'contractExecutableStellarAsset');
        assert.equal(new S.Asset(pin.asset.code, pin.asset.issuer).contractId(plan.network), pin.contract);
      } else {
        assert.equal(executable.switch().name, 'contractExecutableWasm'); assert.equal(executable.wasmHash().toString('hex'), pin.wasmSha256);
        hashes.add(pin.wasmSha256);
      }
    }
    if (hashes.size) {
      const codeRows = await entries([...hashes].map(codeKey), ledger); assert.equal(codeRows.length, hashes.size);
      for (const row of codeRows) assert(hashes.delete(sha(row.val.contractCode().code())), 'actual code bytes differ');
      assert.equal(hashes.size, 0);
    }
  }
  return {now: () => Math.floor(Date.now() / 1000),
    prepare: async (step, sequence) => {
      await network(); const latest = await rpc.getLatestLedger(); assert.equal(latest.protocolVersion, config.protocolVersion);
      const a = await account(latest.sequence); assert.equal(String(uint(a.sequence) + 1n), sequence, 'source sequence changed; stop ceremony');
      await identities(step, latest);
      if (step.startsWith('upload-')) await code(plan.proposedInstances[step.slice(7)].wasmSha256, latest.sequence, false);
      const headers = await rpc.getLedgers({startLedger: latest.sequence, pagination: {limit: 1}}); checkLedger(headers, latest.sequence);
      assert.equal(headers.ledgers.length, 1); const header = headers.ledgers[0].headerXdr.header();
      assert.equal(header.ledgerSeq(), latest.sequence); assert.equal(Number(header.scpValue().closeTime().toString()), Number(latest.closeTime));
      assert(uint(config.inclusionFeeStroops) >= BigInt(header.baseFee()));
      const minimumReserve = BigInt(header.baseReserve()) * BigInt(a.reserveUnits);
      assert(uint(config.balanceFloorStroops) >= minimumReserve, 'approved balance floor below current account reserve');
      const names = ['configSettingContractComputeV0', 'configSettingContractEventsV0',
        'configSettingContractCostParamsCpuInstructions', 'configSettingContractCostParamsMemoryBytes'];
      const rows = await entries(names.map(name => S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[name]()}))), latest.sequence);
      assert.equal(rows.length, names.length); const settings = new Map(rows.map(row => { const v = row.val.configSetting(); return [v.switch().name, v]; }));
      assert(settings.get(names[0]).contractCompute().txMemoryLimit() <= LIMITS.memory);
      assert(settings.get(names[1]).contractEvents().txMaxContractEventsSizeBytes() <= LIMITS.events);
      assert.equal(sha(settings.get(names[2]).toXDR()), config.costModelXdrSha256.cpu);
      assert.equal(sha(settings.get(names[3]).toXDR()), config.costModelXdrSha256.memory);
      const before = await views(config.readbacks?.[step]?.before || [], a, latest);
      if (step === 'permanent-floor') {
        const token = config.dependencies.fundingToken.contract;
        const balance = await rpc.simulateTransaction(transaction(a, new S.Contract(token).call('balance', new S.Address(plan.admin).toScVal()), latest));
        checkLedger(balance, latest.sequence); assert(S.rpc.Api.isSimulationSuccess(balance) && !balance.restorePreamble && balance.result);
        assert.equal(balance.result.auth.length, 0);
        assert(uint(S.scValToNative(balance.result.retval)) >= 10000000000n + 10000000n,
          'permanent floor must not consume approved1000PYUSD principal; actual quoted execution reserve is additional before later stage_open');
      }
      const unsigned = transaction(a, operation(plan, config, step), latest);
      const sim = await rpc.simulateTransaction(unsigned); checkLedger(sim, latest.sequence);
      assert(S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble && sim.result, 'simulation failed or requires restoration');
      assert(sim.result.auth.every(auth => auth.credentials().switch().name === 'sorobanCredentialsSourceAccount'), 'unexpected separate address authorization');
      assert.deepEqual(sim.result.auth.map(auth => auth.rootInvocation().toXDR('base64')), config.authRoots[step], 'authorization tree differs from operator review');
      const assembled = S.rpc.assembleTransaction(unsigned, sim).build();
      assert(uint(assembled.toEnvelope().v1().tx().ext().sorobanData().resourceFee().toString()) >= uint(sim.minResourceFee));
      assert.equal((await rpc.getLatestLedger()).sequence, latest.sequence, 'ledger advanced before preparation finished');
      return {step, hash: assembled.hash().toString('hex'), sequence: assembled.sequence, unsignedXdr: assembled.toXDR(),
        ledger: latest.sequence, closeTime: Number(latest.closeTime), maxFeeStroops: assembled.fee,
        balanceStroops: a.balance, sellingLiabilities: a.selling, networkMemoryAndEventCapsVerified: true,
        minimumReserveStroops: String(minimumReserve), before, simulationResourceFeeStroops: String(sim.minResourceFee)};
    },
    transaction: async row => {
      await network(); const result = await rpc.getTransaction(row.hash);
      if (!['SUCCESS', 'FAILED'].includes(result.status)) return {status: 'UNKNOWN'};
      assert(result.envelopeXdr && result.resultXdr);
      const tx = S.TransactionBuilder.fromXDR(result.envelopeXdr.toXDR('base64'), plan.network);
      assert.equal(tx.hash().toString('hex'), row.hash, 'confirmed transaction differs from prepared body');
      assert.equal(result.resultXdr.result().switch().name, result.status === 'SUCCESS' ? 'txSuccess' : 'txFailed');
      // Signed envelope is inspected in memory, never returned or persisted.
      return {status: result.status, hash: row.hash, source: tx.source, sequence: tx.sequence,
        ledger: result.ledger, feeCharged: result.resultXdr.feeCharged().toString()};
    },
    readback: async step => {
      await network(); const latest = await rpc.getLatestLedger(); const a = await account(latest.sequence);
      await identities(step, latest, true);
      const checks = await views(config.readbacks?.[step]?.after || [], a, latest);
      assert.equal((await rpc.getLatestLedger()).sequence, latest.sequence);
      return {ledger: latest.sequence, executableAndCodePinsVerified: true, checks};
    }};
}
async function main() {
  const [mode, planFile, configFile, directory] = process.argv.slice(2);
  if (mode === '--help' || !mode) {
    console.log('Unsigned only (v2; v1 reviews do not transfer): review PLAN [CONFIG] | prepare PLAN CONFIG STATE_DIRECTORY | reconcile PLAN CONFIG STATE_DIRECTORY\nReview emits operation-only XDR and blockers. Preparation requires REVIEWED_UNSIGNED_CEREMONY with exact plan/helper/runtime pins, signed/Core and independent-review approval evidence, controller artifact, dependencies, dedicated keeper/AQUA, fee/cost/sequence pins, and per-step typed readbacks/auth roots. No signing/submission mode exists. Configuration ends at stage_activate; lifecycle/TTL are separate. Unknown or expired prepared transactions remain blocked for operator investigation.'); return;
  }
  assert(['review', 'prepare', 'reconcile'].includes(mode)); assert(planFile);
  const plan = read(planFile), config = configFile ? read(configFile) : {};
  assert(mode === 'review' || directory, 'state directory required');
  const result = mode === 'review' ? review(plan, config) : await (mode === 'prepare' ? prepare : reconcile)(plan, config, path.resolve(directory));
  console.log(stringify(result));
}
module.exports = {LIMITS, PAIR, UPLOAD_ALLOWANCE, UPLOAD_CPU_POLICY, instructionAllowance, ORDER, derivedId, digest, validatePlan, validateCeremony, operation, review,
  runtimePins, validatePrepared, loadJournal, prepare, reconcile, adapter};
if (require.main === module) main().catch(e => { console.error(e.message); process.exitCode = 1; });
