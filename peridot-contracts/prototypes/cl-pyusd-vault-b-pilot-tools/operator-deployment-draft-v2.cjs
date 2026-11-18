// Derive the review draft from exact release ABIs and saved public dependency
// state. Optional `capture` performs public ledger reads only. No signer exists.
// v2 (2026-11-14). Supersedes prototypes/cl-pyusd-vault-b/operator-deployment-draft.cjs for
// operator-deployment-v2.cjs. v1 reviews do not transfer. Output stays DRAFT; `capture` was
// not run by the tooling agent.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const D = require('./operator-deployment-v2.cjs');
const ROOT = path.resolve(__dirname, '../..');
const PLAN = 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/mainnet-deployment-plan-DRAFT.json';
const CAPTURE = 'research/cl-pyusd-tight-pilot-20261111/live-pool-state-20261113-v1/state.json';
const NETWORK = 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/network-config-20261114.json';
const CONTROLLER = 'target/lending-medium-final-20261111/artifacts/simple_peridottroller.wasm';
const READINESS = 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/mainnet-readiness-20261114-2/readiness.json';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const read = file => JSON.parse(fs.readFileSync(path.resolve(ROOT, file)));
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
const addr = value => new S.Address(value).toScVal();
const key = name => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(name)]);
function dependencies() {
  const bytes = fs.readFileSync(path.resolve(ROOT, CAPTURE));
  assert.equal(sha(bytes), 'b792bb3b5530328ec80715374bc03f39b8a87fd1fe307c06b131a61b01fe083c');
  const saved = JSON.parse(bytes), byId = new Map();
  for (const row of saved.entries) {
    const e = S.xdr.LedgerEntryData.fromXDR(row.entry_data_xdr, 'base64');
    if (e.switch().name !== 'contractData' || e.contractData().key().switch().name !== 'scvLedgerKeyContractInstance') continue;
    const d = e.contractData(), i = d.val().instance(), id = S.Address.fromScAddress(d.contract()).toString();
    const pin = {contract: id};
    if (i.executable().switch().name === 'contractExecutableWasm') pin.wasmSha256 = i.executable().wasmHash().toString('hex');
    else {
      const info = S.scValToNative(S.xdr.ScVal.scvMap(i.storage())).AssetInfo[1];
      pin.asset = {code: info.asset_code.replace(/\0+$/, ''), issuer: S.StrKey.encodeEd25519PublicKey(info.issuer)};
      assert.equal(new S.Asset(pin.asset.code, pin.asset.issuer).contractId(S.Networks.PUBLIC), id);
    }
    byId.set(id, pin);
  }
  const ids = {pool: saved.pool, oracle: saved.oracle, fundingToken: saved.pool_fields.Token0,
    otherToken: saved.pool_fields.Token1, rewardToken: saved.pool_fields.RewardToken,
    rewardFeed: saved.pool_fields.RewardBoostFeed, boostToken: saved.pool_fields.RewardBoostToken,
    poolVault: saved.pool_fields.VaultAddr, configStorage: saved.pool_fields.ConfigStorage,
    poolPlane: saved.pool_fields.Plane, poolRouter: saved.pool_fields.Router};
  return Object.fromEntries(Object.entries(ids).map(([role, id]) => { assert(byId.has(id), `missing captured dependency ${role}`); return [role, byId.get(id)]; }));
}
function build(keeperPublicAddress = null) {
  const plan = read(PLAN); D.validatePlan(plan);
  const dep = dependencies(), controllerBytes = fs.readFileSync(path.resolve(ROOT, CONTROLLER));
  assert.equal(sha(controllerBytes), '847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902');
  if (keeperPublicAddress !== null) { assert(S.StrKey.isValidEd25519PublicKey(keeperPublicAddress)); assert.notEqual(keeperPublicAddress, plan.admin); }
  const capturedConfig = read(NETWORK), getCost = name => S.xdr.LedgerEntryData.fromXDR(capturedConfig.settings[name].xdr, 'base64').configSetting().toXDR();
  const readiness = read(READINESS); assert.equal(readiness.source, plan.admin);
  const config = {status: 'DRAFT_NOT_APPROVED_UNSIGNED_ONLY', planDigestSha256: D.digest(plan),
    helperSha256: sha(fs.readFileSync(path.join(__dirname, 'operator-deployment-v2.cjs'))), runtimePins: D.runtimePins(),
    generatedAt: new Date().toISOString(), expiresAt: Math.floor(Date.now() / 1000) + 86400,
    rpcUrl: 'https://rpc.lightsail.network', protocolVersion: 29, source: plan.admin,
    initialSourceSequence: readiness.sourceAccount.sequence, keeperPublicAddress, aquaToken: dep.rewardToken.contract,
    controllerArtifact: {path: CONTROLLER, sha256: sha(controllerBytes), size: controllerBytes.length}, dependencies: dep,
    releaseApproval: {status: 'PENDING_SIGNED_CORE_AND_REVIEW', wasmSha256: Object.fromEntries(plan.candidateArtifacts.map(a => [a.name, a.sha256])), independentReview: null, signedCore: null},
    costModelXdrSha256: {cpu: sha(getCost('configSettingContractCostParamsCpuInstructions')), memory: sha(getCost('configSettingContractCostParamsMemoryBytes'))},
    uploadCpuPolicy: D.UPLOAD_CPU_POLICY, uploadCpuException: JSON.parse(JSON.stringify(D.UPLOAD_ALLOWANCE)), feeBudgetStroops: null, maxTransactionFeeStroops: null,
    inclusionFeeStroops: null, inclusionFeeCapStroops: null, balanceFloorStroops: null, timeoutSeconds: 180,
    authRoots: {}, authShapes: {}, readbacks: {},
    provenance: {generator: 'prototypes/cl-pyusd-vault-b-pilot-tools/operator-deployment-draft-v2.cjs', generatorSha256: sha(fs.readFileSync(__filename)),
      dependencyCapture: {path: CAPTURE, sha256: sha(fs.readFileSync(path.resolve(ROOT, CAPTURE)))}, networkConfig: {path: NETWORK, sha256: sha(fs.readFileSync(path.resolve(ROOT, NETWORK)))},
      sourceSequenceRead: {path: READINESS, sha256: sha(fs.readFileSync(path.resolve(ROOT, READINESS))), ledger: readiness.sourceAccount.ledger}},
    notes: ['No signatures/submissions. Immutable staged-vault/receipt identities; never upgrades a cash market.',
      'Keeper-dependent authorization and readback are deterministically filled by rerunning this generator with the chosen public address.',
      'Initial source sequence and expiry must be refreshed when the funded ceremony is reviewed; read-only historical values are not execution pins.',
      'USER DECISION 14 Nov 2026: up to 110M declared CPU for the two exact frozen code uploads only; every other transaction stays at 100M. Hash-bound in the helper, not configurable.',
      'The1PYUSD floor is additional to1000PYUSD principal. Actual quoted execution reserve is funded before the separate later stage_open ceremony.',
      'Expected authorization roots derive from compiled ABI/source. Fresh simulation must match them exactly; discrepancies stop preparation.']};
  const V = plan.proposedInstances.vault.contract, R = plan.proposedInstances.receipt.contract, C = plan.controller, A = plan.admin;
  const specs = {vault: S.contract.Spec.fromWasm(fs.readFileSync(path.resolve(ROOT, plan.candidateArtifacts.find(a => a.name === 'vault').path))),
    receipt: S.contract.Spec.fromWasm(fs.readFileSync(path.resolve(ROOT, plan.candidateArtifacts.find(a => a.name === 'receipt').path))), controller: S.contract.Spec.fromWasm(controllerBytes)};
  const ids = {vault: V, receipt: R, controller: C};
  function view(role, method, args, expected) {
    const spec = specs[role], fn = spec.getFunc(method), names = fn.inputs().map(i => i.name().toString()); assert.equal(args.length, names.length);
    assert.equal(fn.outputs().length, 1);
    return {kind: 'view', contract: ids[role], method, argsXdr: spec.funcArgsToScVals(method, Object.fromEntries(names.map((n, i) => [n, args[i]]))).map(v => v.toXDR('base64')),
      expectedNative: expected, expectedResultXdr: spec.nativeToScVal(expected, fn.outputs()[0]).toXDR('base64')};
  }
  const raw = (role, kind, name, value = null) => ({kind, contract: ids[role], label: name, keyXdr: key(name).toXDR('base64'), expectedResultXdr: value === null ? null : value.toXDR('base64')});
  const iv = (role, name, value) => raw(role, 'instanceValue', name, value);
  const pv = (role, name, value) => raw(role, 'persistentValue', name, value);
  const value = (v, type) => S.nativeToScVal(v, {type});
  const tokenBalance = (token, owner, amount) => ({kind: 'view', contract: token, method: 'balance', argsXdr: [addr(owner).toXDR('base64')],
    expectedNative: amount, expectedResultXdr: value(amount, 'i128').toXDR('base64')});
  const put = (step, before, after) => { config.readbacks[step] = {before, after}; };
  const adminV = view('vault', 'get_admin', [], A), adminR = view('receipt', 'get_admin', [], A), adminC = view('controller', 'get_admin', [], A);
  const emptyV = [view('vault', 'total_supply', [], '0'), view('vault', 'get_position_liquidity', [], '0'), view('vault', 'balance', [R], '0'), view('vault', 'balance', [A], '0'),
    ...['fundingToken', 'otherToken', 'rewardToken'].map(role => tokenBalance(dep[role].contract, V, '0'))];
  const configV = view('vault', 'get_config', [], {pool: dep.pool.contract, oracle: dep.oracle.contract, token0: dep.fundingToken.contract,
    token1: dep.otherToken.contract, dec0: 7, dec1: 7, underlying_index: 0, tick_lower: -20, tick_upper: 20});
  const paramsV = cap => view('vault', 'get_params', [], {max_deploy: cap, slippage_bps: 100, harvest_cooldown: '3600', oracle_max_age_mult: '2',
    nav_root_max_age: '300', max_pool_divergence_bps: 200, nav_root_max_stale: '3600', paused: false});
  const baseR = [adminR, view('receipt', 'get_underlying_token', [], dep.fundingToken.contract),
    view('receipt', 'get_cash_borrowing_enabled', [], false), view('receipt', 'get_total_borrowed', [], '0'),
    view('receipt', 'get_boosted_vault', [], null), view('receipt', 'get_margin_controller', [], null),
    view('receipt', 'get_cl_custody', [], false), view('receipt', 'get_total_reserves', [], '0'), view('receipt', 'get_total_admin_fees', [], '0'), view('receipt', 'get_total_bad_debt', [], '0'),
    pv('receipt', 'InterestModel'), pv('receipt', 'BoostedAssetCount'), pv('receipt', 'PendingAdmin'), pv('receipt', 'PendingUpgradeHash'), pv('receipt', 'PendingUpgradeEta'),
    pv('receipt', 'YearlyRateScaled', value('0', 'u128')), pv('receipt', 'BorrowYearlyRateScaled', value('0', 'u128'))];
  const emptyR = [view('receipt', 'get_total_ptokens', [], '0'), view('receipt', 'get_total_underlying', [], '0'),
    view('receipt', 'get_minimum_supply', [], '0'), tokenBalance(dep.fundingToken.contract, R, '0')];
  const controlledR = [pv('receipt', 'Peridottroller', addr(C)), view('controller', 'is_market_supported', [R], true),
    view('controller', 'get_market_cf', [R], '0'), view('controller', 'is_deposit_paused', [R], false),
    view('controller', 'is_redeem_paused', [R], false), view('controller', 'is_borrow_paused', [R], true)];
  const floorR = [view('receipt', 'get_total_ptokens', [], '10000000'), view('receipt', 'get_minimum_supply', [], '10000000'),
    view('receipt', 'get_total_underlying', [], '10000000'), view('receipt', 'get_ptoken_balance', [R], '10000000'),
    view('receipt', 'get_ptoken_balance', [A], '0'), view('receipt', 'get_exchange_rate', [], '1000000'), tokenBalance(dep.fundingToken.contract, R, '10000000')];
  const finalV = [adminV, configV, paramsV('10000000000'), ...emptyV, view('vault', 'get_primary_reward_token', [], config.aquaToken),
    pv('vault', 'PendingAdmin'), pv('vault', 'PendingUpgradeHash'), pv('vault', 'PendingUpgradeEta')];
  put('initialize-vault', [{kind: 'instanceEmpty', contract: V}], [adminV, configV, paramsV('0'), ...emptyV]);
  put('vault-reward-token', [adminV, ...emptyV, view('vault', 'get_primary_reward_token', [], null)], [adminV, view('vault', 'get_primary_reward_token', [], config.aquaToken)]);
  put('vault-command-cap', [adminV, paramsV('0'), ...emptyV], finalV);
  put('initialize-receipt', [{kind: 'instanceEmpty', contract: R}, pv('receipt', 'Initialized'), pv('receipt', 'Admin'), pv('receipt', 'UnderlyingToken')], [...baseR, ...emptyR]);
  put('controller-list-new-market', [adminC, ...baseR, ...emptyR, view('controller', 'is_market_supported', [R], false)],
    [adminC, view('controller', 'is_market_supported', [R], true), view('controller', 'get_market_cf', [R], '0')]);
  for (const [step, getter, expected] of [['set_pause_deposit', 'is_deposit_paused', false], ['set_pause_redeem', 'is_redeem_paused', false], ['set_pause_borrow', 'is_borrow_paused', true]])
    put(step, [adminC, view('controller', 'is_market_supported', [R], true)], [adminC, view('controller', getter, [R], expected)]);
  put('bind-controller', [...baseR, pv('receipt', 'Peridottroller'), ...controlledR.filter(r => r.kind === 'view')], [...baseR, ...controlledR]);
  put('cash-reserve', [...baseR, ...emptyR, ...controlledR, view('receipt', 'get_min_cash_reserve_bps', [], 0)],
    [...baseR, view('receipt', 'get_min_cash_reserve_bps', [], 10000), ...controlledR]);
  put('floor-only-supply-cap', [...baseR, ...emptyR, pv('receipt', 'SupplyCap', value('0', 'u128'))],
    [...baseR, pv('receipt', 'SupplyCap', value('10000000', 'u128'))]);
  put('permanent-floor', [...baseR, ...emptyR, ...controlledR, view('receipt', 'get_min_cash_reserve_bps', [], 10000),
    pv('receipt', 'SupplyCap', value('10000000', 'u128'))], [...baseR, ...floorR]);
  put('bind-custody', [...finalV, iv('vault', 'StgV1Receipt'), ...baseR, ...floorR, ...controlledR],
    [...finalV, view('vault', 'stage_receipt', [], R), view('vault', 'stage_funding_asset', [], dep.fundingToken.contract)]);
  put('vault-guardian', [...finalV, view('vault', 'stage_receipt', [], R), iv('vault', 'StgV1Guardian')],
    [view('vault', 'stage_guardian', [], A), view('vault', 'stage_deposits_paused', [], false)]);
  put('observation-keeper', [...finalV, view('vault', 'stage_receipt', [], R), iv('vault', 'StgV1InvestKeeper')], keeperPublicAddress
    ? [view('vault', 'stage_investment_keeper', [], keeperPublicAddress)]
    : [{kind: 'view', contract: V, method: 'stage_investment_keeper', argsXdr: [], expectedResultXdr: null,
      unresolved: 'Deterministically derive address ScVal from chosen keeperPublicAddress by rerunning generator.'}]);
  const stg = specs.receipt.entries.filter(e => e.switch().name === 'scSpecEntryUdtStructV0').map(e => e.udtStructV0()).find(t => t.name().toString() === 'StgState');
  assert(stg); const zeroState = Object.fromEntries(stg.fields().map(f => [f.name().toString(), f.type().switch().name === 'scSpecTypeU32' ? 0 : '0']));
  put('activate-receipt', [...finalV, ...baseR, ...floorR, ...controlledR, view('receipt', 'get_min_cash_reserve_bps', [], 10000),
    view('vault', 'stage_receipt', [], R), view('vault', 'stage_guardian', [], A), view('vault', 'stage_deposits_paused', [], false),
    ...config.readbacks['observation-keeper'].after, view('receipt', 'stage_is_active', [], false)],
    [...baseR, ...floorR, ...controlledR, view('receipt', 'stage_config', [], {version: 1, vault: V, guardian: A, backing_cap: '10000000000'}),
      view('receipt', 'stage_state', [], zeroState), view('receipt', 'stage_is_active', [], true), view('receipt', 'stage_deposits_paused', [], false), ...emptyV]);
  for (const step of D.ORDER) {
    config.readbacks[step] ||= {before: [], after: []};
    if (!step.startsWith('upload-') && !step.startsWith('create-')) for (const phase of ['before', 'after'])
      config.readbacks[step][phase].push(adminC, pv('controller', 'PauseGuardian', addr(A)), pv('controller', 'PendingAdmin'), pv('controller', 'PendingUpgradeHash'), pv('controller', 'PendingUpgradeEta'));
    if (step.startsWith('upload-')) { config.authRoots[step] = []; config.authShapes[step] = []; continue; }
    if (step === 'observation-keeper' && !keeperPublicAddress) {
      config.authRoots[step] = null; config.authShapes[step] = [{source: A, target: V, method: 'stage_set_invest_keeper', args: [A, '$keeperPublicAddress'], children: []}]; continue;
    }
    const op = D.operation(plan, config, step).body().invokeHostFunctionOp().hostFunction();
    const fn = step.startsWith('create-') ? S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeCreateContractV2HostFn(op.createContractV2())
      : S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(op.invokeContract());
    const children = [];
    if (step === 'permanent-floor') children.push(new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(
      new S.xdr.InvokeContractArgs({contractAddress: new S.Address(dep.fundingToken.contract).toScAddress(), functionName: 'transfer', args: [addr(A), addr(R), value('10000000', 'i128')]})), subInvocations: []}));
    const root = new S.xdr.SorobanAuthorizedInvocation({function: fn, subInvocations: children});
    config.authRoots[step] = [root.toXDR('base64')];
    config.authShapes[step] = [{source: A, target: plan.steps.find(s => s.step === step).target,
      method: plan.steps.find(s => s.step === step).method, children: step === 'permanent-floor' ? [{contract: dep.fundingToken.contract, method: 'transfer', from: A, to: R, amountRaw: '10000000'}] : []}];
  }
  return config;
}
async function capture(output) {
  assert(!fs.existsSync(output), 'preserve previous dependency capture');
  const config = build(), plan = read(PLAN), rpc = new S.rpc.Server(config.rpcUrl, {timeout: 45});
  assert.equal((await rpc.getNetwork()).passphrase, plan.network);
  const pins = {...config.dependencies, controller: {contract: plan.controller, wasmSha256: config.controllerArtifact.sha256}};
  const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
  const got = await rpc.getLedgerEntries(...Object.values(pins).map(p => instanceKey(p.contract)));
  const map = new Map(got.entries.map(row => [row.key.toXDR('base64'), row]));
  assert.equal(map.size, Object.keys(pins).length);
  for (const pin of Object.values(pins)) {
    const row = map.get(instanceKey(pin.contract).toXDR('base64')); assert(row);
    const executable = row.val.contractData().val().instance().executable();
    if (pin.asset) { assert.equal(executable.switch().name, 'contractExecutableStellarAsset'); assert.equal(new S.Asset(pin.asset.code, pin.asset.issuer).contractId(plan.network), pin.contract); }
    else { assert.equal(executable.switch().name, 'contractExecutableWasm'); assert.equal(executable.wasmHash().toString('hex'), pin.wasmSha256); }
  }
  const hashes = [...new Set(Object.values(pins).filter(p => p.wasmSha256).map(p => p.wasmSha256))];
  const codes = await rpc.getLedgerEntries(...hashes.map(hash => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(hash, 'hex')}))));
  assert.equal(codes.latestLedger, got.latestLedger, 'dependency capture crossed ledger; retry into a new file');
  assert.equal(codes.entries.length, hashes.length); const remaining = new Set(hashes);
  for (const row of codes.entries) assert(remaining.delete(sha(row.val.contractCode().code()))); assert.equal(remaining.size, 0);
  const report = {status: 'EXACT_PUBLIC_DEPENDENCY_PINS_VERIFIED', signing: false, submission: false,
    observedAt: new Date().toISOString(), rpc: config.rpcUrl, ledger: got.latestLedger, pins,
    entries: got.entries.map(row => ({keyXdr: row.key.toXDR('base64'), valueXdr: row.val.toXDR('base64'), lastModifiedLedgerSeq: row.lastModifiedLedgerSeq, liveUntilLedgerSeq: row.liveUntilLedgerSeq})),
    codes: codes.entries.map(row => ({sha256: sha(row.val.contractCode().code()), bytes: row.val.contractCode().code().length, liveUntilLedgerSeq: row.liveUntilLedgerSeq}))};
  fs.writeFileSync(output, json(report), {flag: 'wx'}); return {output, ledger: report.ledger, instances: report.entries.length, codes: report.codes.length};
}
module.exports = {build, capture};
if (require.main === module) {
  const [mode, value] = process.argv.slice(2);
  Promise.resolve().then(() => {
    if (mode === 'capture') { assert(value, 'new output file required'); return capture(value); }
    assert(!mode || mode === 'draft', 'draft [keeper public address] | capture NEW_OUTPUT_FILE'); return build(value || null);
  }).then(v => console.log(json(v))).catch(e => {console.error(e.message); process.exitCode = 1;});
}
