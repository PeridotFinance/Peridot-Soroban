// Fresh, rerunnable generator of the v3 deployment config from LIVE public Mainnet reads (one ledger for every
// ledger-entry pin) plus read-only upload simulations. No signer, no key access, no sendTransaction.
// v3 (2026-11-15). Copy of operator-deployment-draft-v2.cjs's readback/auth derivation (unchanged logic), with:
//  - every pin taken from a fresh read instead of saved captures: source sequence, cost-model hashes, controller
//    executable and code, pool-derived dependency pins, absence of both proposed instances and both code hashes;
//  - fees: inclusion bid = max(1,000, 5 x live Soroban p99) stroops, refused above 100,000 (cash-tooling rule);
//    upload cap = 110% of the larger fresh upload resource fee + inclusion cap; non-upload cap 5 XLM;
//    budget = 110% of both fresh upload fees + 10 XLM allowance for steps 3-18; balance floor 5 XLM;
//  - status TOOL_REVIEWED_PRE_RELEASE_NOT_OPERATOR_APPROVED (operator reviews it in the command list).
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const D = require('./operator-deployment-v3.cjs');
const ROOT = path.resolve(__dirname, '../..');
// 800 PYUSD seed/cap plan (user decision 15 Nov 2026 ~13:15 UTC), derived from vault-b-20261113/mainnet-deployment-plan-DRAFT.json.
const PLAN = 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/deploy/mainnet-deployment-plan-v3-seed800.json';
const SAVED_CAPTURE = 'research/cl-pyusd-tight-pilot-20261111/live-pool-state-20261113-v1/state.json';
const CONTROLLER = 'target/lending-medium-final-20261111/artifacts/simple_peridottroller.wasm';
const CONTROLLER_SHA256 = '847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902';
const RPC_URL = 'https://rpc.lightsail.network';
const XLM = 10000000n;
// Keeper PUBLIC address supplied by the user via the lead on 15 Nov 2026 (~13:00 UTC). Compared as the FULL string;
// never derived from account history (a lookalike GBHWIIIG…ZON4 sent dust to the Ledger account on 15 Nov).
const KEEPER_SUPPLIED_20261115 = 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4';
const FEES = Object.freeze({inclusionMultiplier: 5n, inclusionMinimum: 1000n, inclusionRefuseAbove: 100000n,
  uploadCapPercent: 110n, nonUploadCapStroops: 5n * XLM, steps3to18AllowanceStroops: 10n * XLM,
  balanceFloorStroops: 5n * XLM, fundingMarginStroops: 5n * XLM, timeoutSeconds: 180, expirySeconds: 86400});
// Steps 3-18 cannot be simulated before the code exists. ESTIMATE from the 12 Nov cash-market deployment's actual
// fees (research/stellar-redeployment-20261111/LIVE-CEREMONY-COMPLETE-20261112.md), per-transaction averages.
const PRECEDENT = Object.freeze({creationEach: 178317n, initializationEach: 4139106n, configurationEach: 219880n, permanentSeedEach: 831022n,
  source: 'five creations 0.0891585 XLM; five initializations 2.0695530 XLM; 21 configuration calls 0.4617483 XLM; four permanent seeds 0.3324090 XLM'});
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const read = file => JSON.parse(fs.readFileSync(path.resolve(ROOT, file)));
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
const addr = value => new S.Address(value).toScVal();
const key = name => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(name)]);
const ceilDiv = (a, b) => (a + b - 1n) / b;
const dec = raw => { const n = BigInt(raw); return `${n / XLM}.${String(n % XLM).padStart(7, '0')}`; };
function steps3to18Estimate() {
  const rows = [['create-vault', PRECEDENT.creationEach], ['create-receipt', PRECEDENT.creationEach],
    ['initialize-vault', PRECEDENT.initializationEach], ['initialize-receipt', PRECEDENT.initializationEach],
    ...['vault-reward-token', 'vault-command-cap', 'controller-list-new-market', 'set_pause_deposit', 'set_pause_redeem', 'set_pause_borrow',
      'bind-controller', 'cash-reserve', 'floor-only-supply-cap', 'bind-custody', 'vault-guardian'].map(s => [s, PRECEDENT.configurationEach]),
    ['permanent-floor', PRECEDENT.permanentSeedEach]];
  assert.deepEqual(rows.map(r => r[0]).sort(), D.PRE_RELEASE_STEPS.slice(2).slice().sort());
  return {label: 'ESTIMATE, not a quote: steps 3-18 cannot be simulated until the code is uploaded (and each step until the previous one applied).',
    precedent: PRECEDENT.source, perStep: Object.fromEntries(rows.map(([s, v]) => [s, String(v)])),
    totalStroops: String(rows.reduce((t, r) => t + r[1], 0n)),
    caution: 'The CL vault initialisation and stage_bind write more state than the cash markets did; the 10 XLM budget allowance (~8x) covers that, the 5 XLM per-transaction cap bounds each step.'};
}
function inclusionBid(p99) {
  const bid = [FEES.inclusionMinimum, FEES.inclusionMultiplier * BigInt(p99)].reduce((a, b) => a > b ? a : b);
  assert(bid <= FEES.inclusionRefuseAbove, `inclusion bid ${bid} above ${FEES.inclusionRefuseAbove} stroops: refused (fee surge)`);
  return bid;
}
// live = {dependencies, costModelXdrSha256, sequence, sequenceLedger, fees: {inclusion, uploadQuotes: {vault, receipt}}, observedAt, provenance}
function build(live, keeperPublicAddress = null, predecessorConfig = null) {
  const plan = read(PLAN); D.validatePlan(plan);
  const dep = live.dependencies, controllerBytes = fs.readFileSync(path.resolve(ROOT, CONTROLLER));
  assert.equal(sha(controllerBytes), CONTROLLER_SHA256);
  if (keeperPublicAddress !== null) { assert(S.StrKey.isValidEd25519PublicKey(keeperPublicAddress)); assert.notEqual(keeperPublicAddress, plan.admin);
    assert.equal(keeperPublicAddress, KEEPER_SUPPLIED_20261115, 'keeper address differs from the full user-supplied address (beware GBHW…ZON4 lookalikes)'); }
  const inclusion = BigInt(live.fees.inclusion), uploads = Object.values(live.fees.uploadQuotes).map(BigInt);
  // A successor after both uploads are confirmed has no upload left: its upload cap is then the non-upload cap.
  const maxUpload = uploads.reduce((a, b) => a > b ? a : b, 0n);
  const uploadCap = uploads.length ? ceilDiv(maxUpload * FEES.uploadCapPercent, 100n) + FEES.inclusionRefuseAbove : FEES.nonUploadCapStroops;
  const uploadBudget = uploads.reduce((t, q) => t + ceilDiv(q * FEES.uploadCapPercent, 100n) + inclusion, 0n);
  const config = {status: D.PRE_RELEASE_STATUS, releaseStage: D.STAGES.PRE,
    userDecision: {date: '2026-11-15', text: D.USER_DECISION_20261115,
      leadInterpretation: 'Steps 1-18 now; step 19 waits for the keeper public address; step 20 is held with the seed and the two pauses.'},
    reviewedBy: 'tool (operator-deployment-draft-v3.cjs fresh); NOT operator-approved; the operator reviews it in DEPLOY-NOW-COMMANDS.md',
    planDigestSha256: D.digest(plan),
    helperSha256: sha(fs.readFileSync(path.join(__dirname, 'operator-deployment-v3.cjs'))), runtimePins: D.runtimePins(),
    generatedAt: live.observedAt, expiresAt: Math.floor(Date.parse(live.observedAt) / 1000) + FEES.expirySeconds,
    rpcUrl: RPC_URL, protocolVersion: 29, source: plan.admin,
    initialSourceSequence: live.sequence, keeperPublicAddress, aquaToken: dep.rewardToken.contract,
    controllerArtifact: {path: CONTROLLER, sha256: sha(controllerBytes), size: controllerBytes.length}, dependencies: dep,
    releaseApproval: {status: 'NOT_REQUIRED_IN_PRE_RELEASE_STAGE', wasmSha256: Object.fromEntries(plan.candidateArtifacts.map(a => [a.name, a.sha256])), independentReview: null, signedCore: null},
    costModelXdrSha256: live.costModelXdrSha256,
    uploadCpuPolicy: D.UPLOAD_CPU_POLICY, uploadCpuException: JSON.parse(JSON.stringify(D.UPLOAD_ALLOWANCE)),
    feeBudgetStroops: String(uploadBudget + FEES.steps3to18AllowanceStroops), maxUploadTransactionFeeStroops: String(uploadCap),
    maxTransactionFeeStroops: String(FEES.nonUploadCapStroops),
    inclusionFeeStroops: String(inclusion), inclusionFeeCapStroops: String(FEES.inclusionRefuseAbove), balanceFloorStroops: String(FEES.balanceFloorStroops),
    timeoutSeconds: FEES.timeoutSeconds, predecessorConfig,
    feeDerivation: {inclusion: `max(1000, 5 x live Soroban p99 ${live.fees.p99}) = ${inclusion} stroops; refused above 100000`,
      uploadQuotesStroops: live.fees.uploadQuotes, uploadCap: '110% of the larger fresh upload resource fee + 100000 inclusion cap',
      budget: '110% of each fresh upload resource fee + inclusion, plus 10 XLM allowance for steps 3-18', nonUploadCap: '5 XLM per transaction',
      balanceFloor: '5 XLM (covers the account reserve with one more trustline: (2 + 4 subentries) x 0.5 = 3 XLM)'},
    authRoots: {}, authShapes: {}, readbacks: {},
    provenance: {...live.provenance, generator: 'prototypes/cl-pyusd-vault-b-pilot-tools/operator-deployment-draft-v3.cjs', generatorSha256: sha(fs.readFileSync(__filename))},
    notes: ['No signatures/submissions. Immutable staged-vault/receipt identities; never upgrades or touches a cash market.',
      'No other transaction from the Ledger account between steps 1 and 18: every step uses initialSourceSequence + n. If the sequence changed, regenerate a successor config (fresh --predecessor).',
      'Keeper-dependent authorization (step 19) and readbacks (steps 19, 20) stay unresolved in this stage and are not required.',
      'USER DECISION 14 Nov 2026: up to 110M declared CPU for the two exact frozen code uploads only; every other transaction stays at 100M. Hash-bound in the helper, not configurable.',
      'USER DECISION 15 Nov 2026 ~13:15 UTC: seed = backing cap = 800 PYUSD (fallback 2 x 400). The 1 PYUSD floor (step 16) is additional; the tool refuses step 16 below 801 PYUSD. The execution reserve is quoted and re-checked before the seed.']};
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
    [...baseR, ...floorR, ...controlledR, view('receipt', 'stage_config', [], {version: 1, vault: V, guardian: A, backing_cap: plan.selected.backingCapRaw}),
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
const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
const codeKey = hash => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(hash, 'hex')}));
const ROLES = {fundingToken: 'Token0', otherToken: 'Token1', rewardToken: 'RewardToken', rewardFeed: 'RewardBoostFeed', boostToken: 'RewardBoostToken',
  poolVault: 'VaultAddr', configStorage: 'ConfigStorage', poolPlane: 'Plane', poolRouter: 'Router'};
function pinOf(row, id) {
  const i = row.val.contractData().val().instance(), pin = {contract: id};
  if (i.executable().switch().name === 'contractExecutableWasm') pin.wasmSha256 = i.executable().wasmHash().toString('hex');
  else {
    const info = S.scValToNative(S.xdr.ScVal.scvMap(i.storage())).AssetInfo[1];
    pin.asset = {code: info.asset_code.replace(/\0+$/, ''), issuer: S.StrKey.encodeEd25519PublicKey(info.issuer)};
    assert.equal(new S.Asset(pin.asset.code, pin.asset.issuer).contractId(S.Networks.PUBLIC), id);
  }
  return pin;
}
// All ledger-entry pins on ONE ledger (two getLedgerEntries calls must report the same latestLedger), then
// read-only upload simulations. rpc injectable for offline tests.
// done = number of CONFIRMED steps in the journal a successor config continues (0 for a new journal).
async function readLive(rpc, done = 0) {
  assert(Number.isSafeInteger(done) && done >= 0 && done <= 19);
  const plan = read(PLAN), A = plan.admin;
  assert.equal((await rpc.getNetwork()).passphrase, plan.network);
  const pool = plan.steps.find(s => s.step === 'initialize-vault').args[1], oracle = plan.steps.find(s => s.step === 'initialize-vault').args[3];
  const accountKey = S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: new S.Address(A).toScAddress().accountId()}));
  const cost = ['configSettingContractCostParamsCpuInstructions', 'configSettingContractCostParamsMemoryBytes'].map(name =>
    S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[name]()})));
  const V = plan.proposedInstances.vault, R = plan.proposedInstances.receipt;
  const first = [accountKey, ...cost, instanceKey(pool), instanceKey(oracle), instanceKey(plan.controller), instanceKey(V.contract), instanceKey(R.contract),
    codeKey(V.wasmSha256), codeKey(R.wasmSha256), codeKey(CONTROLLER_SHA256)];
  const got = await rpc.getLedgerEntries(...first), ledger = got.latestLedger;
  const by = new Map(got.entries.map(row => [row.key.toXDR('base64'), row])), get = k => by.get(k.toXDR('base64'));
  const account = get(accountKey)?.val.account(); assert(account, 'Ledger account not found');
  assert.equal(account.signers().length, 0, 'Ledger account has extra signers'); assert.equal(account.thresholds()[0], 1);
  // Freshness: neither proposed instance nor either frozen code hash exists yet.
  // Freshness: what the journal has not confirmed must be absent; what it has confirmed must be present (exact bytes).
  const absence = {vaultInstance: !get(instanceKey(V.contract)), receiptInstance: !get(instanceKey(R.contract)),
    vaultCode: !get(codeKey(V.wasmSha256)), receiptCode: !get(codeKey(R.wasmSha256))};
  const confirmedAt = {vaultCode: 1, receiptCode: 2, vaultInstance: 3, receiptInstance: 4};
  for (const [name, absent] of Object.entries(absence)) {
    if (done < confirmedAt[name]) assert(absent, `${name} already present on chain at ledger ${ledger} but not confirmed in the journal: reconcile, do not upload/create again`);
    else assert(!absent, `${name} is confirmed in the journal but absent on chain at ledger ${ledger}: stop for the lead`);
  }
  for (const [role, pin] of [['vault', V], ['receipt', R]]) {
    if (done >= confirmedAt[`${role}Code`]) assert.equal(sha(get(codeKey(pin.wasmSha256)).val.contractCode().code()), pin.wasmSha256, `${role} code bytes differ`);
    if (done >= confirmedAt[`${role}Instance`]) assert.equal(get(instanceKey(pin.contract)).val.contractData().val().instance().executable().wasmHash().toString('hex'), pin.wasmSha256, `${role} instance executable differs`);
  }
  const controller = get(instanceKey(plan.controller)); assert(controller, 'fresh controller instance missing');
  assert.equal(controller.val.contractData().val().instance().executable().wasmHash().toString('hex'), CONTROLLER_SHA256, 'fresh controller executable differs');
  assert.equal(sha(get(codeKey(CONTROLLER_SHA256)).val.contractCode().code()), CONTROLLER_SHA256);
  const poolRow = get(instanceKey(pool)), oracleRow = get(instanceKey(oracle)); assert(poolRow && oracleRow);
  const fields = new Map((poolRow.val.contractData().val().instance().storage() || []).map(e => {
    const k = S.scValToNative(e.key()); return [Array.isArray(k) ? k[0] : k, e.val()]; }));
  const saved = read(SAVED_CAPTURE), ids = {pool, oracle};
  for (const [role, field] of Object.entries(ROLES)) {
    ids[role] = S.Address.fromScVal(fields.get(field)).toString();
    assert.equal(ids[role], saved.pool_fields[field], `pool field ${field} changed since the 13 Nov capture; review before deploying`);
  }
  const extra = Object.entries(ids).filter(([role]) => !['pool', 'oracle'].includes(role)).map(([, id]) => instanceKey(id));
  const second = await rpc.getLedgerEntries(...extra); assert.equal(second.latestLedger, ledger, 'dependency reads crossed a ledger; rerun');
  for (const row of second.entries) by.set(row.key.toXDR('base64'), row);
  const dependencies = Object.fromEntries(Object.entries(ids).map(([role, id]) => { const row = get(instanceKey(id)); assert(row, `dependency ${role} missing`); return [role, pinOf(row, id)]; }));
  const hashes = [...new Set(Object.values(dependencies).filter(p => p.wasmSha256).map(p => p.wasmSha256))];
  const codes = await rpc.getLedgerEntries(...hashes.map(codeKey)); assert.equal(codes.latestLedger, ledger, 'code reads crossed a ledger; rerun');
  assert.equal(codes.entries.length, hashes.length); const left = new Set(hashes);
  for (const row of codes.entries) assert(left.delete(sha(row.val.contractCode().code()))); assert.equal(left.size, 0);
  const costModelXdrSha256 = {cpu: sha(get(cost[0]).val.configSetting().toXDR()), memory: sha(get(cost[1]).val.configSetting().toXDR())};
  const feeStats = await rpc.getFeeStats(), p99 = String(feeStats.sorobanInclusionFee.p99), inclusion = inclusionBid(p99);
  const latest = await rpc.getLatestLedger(); assert.equal(latest.protocolVersion, 29);
  // Read-only simulations of the two exact frozen uploads (never sent).
  const uploadQuotes = {}, uploadDetails = {};
  for (const role of ['vault', 'receipt'].filter(r => done < confirmedAt[`${r}Code`])) {
    const art = plan.candidateArtifacts.find(a => a.name === role), bytes = fs.readFileSync(path.resolve(ROOT, art.path));
    assert.equal(sha(bytes), D.PAIR[role], 'frozen bytes changed on disk');
    const tx = new S.TransactionBuilder(new S.Account(A, account.seqNum().toString()), {fee: String(inclusion), networkPassphrase: plan.network})
      .addOperation(S.Operation.uploadContractWasm({wasm: bytes})).setTimeout(0).build();
    const sim = await rpc.simulateTransaction(tx);
    assert(S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble, `upload-${role} simulation failed`);
    const r = sim.transactionData.build().resources(), cpu = r.instructions();
    const allowance = D.instructionAllowance(tx.toEnvelope().v1().tx().operations()[0], `upload-${role}`);
    uploadQuotes[role] = String(sim.minResourceFee);
    uploadDetails[role] = {sha256: D.PAIR[role], bytes: bytes.length, simulationLedger: sim.latestLedger, declaredInstructions: cpu,
      allowance: allowance.limit, withinAllowance: cpu <= allowance.limit, resourceFeeStroops: String(sim.minResourceFee),
      totalWithInclusionStroops: String(BigInt(sim.minResourceFee) + inclusion), totalXlm: dec(BigInt(sim.minResourceFee) + inclusion),
      diskReadBytes: r.diskReadBytes(), writeBytes: r.writeBytes()};
    assert(cpu <= allowance.limit, `BLOCKER: fresh upload-${role} declares ${cpu} CPU > ${allowance.limit}; nothing is clamped`);
  }
  const observedAt = new Date(Number(latest.closeTime) * 1000).toISOString();
  return {plan, ledger, observedAt, confirmedSteps: done, sequence: account.seqNum().toString(), sequenceLedger: ledger, dependencies, costModelXdrSha256,
    account: {balanceStroops: account.balance().toString(), subentries: account.numSubEntries()}, absence,
    fees: {p99, inclusion: String(inclusion), uploadQuotes, uploadDetails, sorobanInclusionFee: feeStats.sorobanInclusionFee, feeStatsLedger: feeStats.latestLedger},
    provenance: {rpc: RPC_URL, pinsLedger: ledger, latestLedgerAtQuote: latest.sequence, latestCloseTime: latest.closeTime,
      savedCaptureCrossCheck: {path: SAVED_CAPTURE, sha256: sha(fs.readFileSync(path.resolve(ROOT, SAVED_CAPTURE)))}}};
}
function fundingRequirements(config, live) {
  const est = steps3to18Estimate(), uploads = Object.values(live.fees.uploadDetails).reduce((t, u) => t + BigInt(u.totalWithInclusionStroops), 0n);
  const budget = BigInt(config.feeBudgetStroops), floor = BigInt(config.balanceFloorStroops), margin = FEES.fundingMarginStroops;
  const leadFormula = uploads + BigInt(est.totalStroops) + floor + margin, enforced = budget + floor + margin;
  return {status: 'PRE_RELEASE_FUNDING_REQUIREMENTS', network: S.Networks.PUBLIC, rpcUrl: RPC_URL, account: config.source,
    deploymentConfig: null, xlm: {freshUploadQuoteStroops: String(uploads), steps3to18EstimateStroops: est.totalStroops,
      balanceFloorStroops: String(floor), marginStroops: String(margin), feeBudgetStroops: String(budget),
      leadFormulaStroops: String(leadFormula), requiredBeforeStep1Stroops: String(enforced > leadFormula ? enforced : leadFormula),
      rule: 'required = max(fresh upload quote + steps 3-18 estimate + floor + margin, fee budget + floor + margin): prepare refuses any step that would exceed the budget or breach the floor, so funding to the budget means no step can be refused for XLM.'},
    pyusd: {requiredBeforeStep16Raw: String(BigInt(D.SEED_RAW) + BigInt(D.FLOOR_RAW)), seedPrincipalRaw: D.SEED_RAW, permanentFloorRaw: D.FLOOR_RAW,
      rule: '800 PYUSD seed principal + 1 PYUSD permanent floor (leaves the wallet at step 16); the seed later also needs the fresh quoted execution reserve (+50%), re-checked before stage_open'},
    estimate: est, signing: false, submission: false};
}
async function fresh(outputDir, keeperPublicAddress = null, predecessorFile = null, rpc = new S.rpc.Server(RPC_URL, {timeout: 60}), journalDir = null) {
  fs.mkdirSync(outputDir, {recursive: true});
  // A successor must name the journal it continues; every entry there must be CONFIRMED and nothing active.
  assert.equal(!!predecessorFile, !!journalDir, '--predecessor and --journal go together');
  let done = 0;
  if (predecessorFile) {
    const plan = read(PLAN), previous = JSON.parse(fs.readFileSync(predecessorFile));
    const journal = D.loadJournal(path.resolve(journalDir), plan, previous);
    assert(journal.attached && journal.active === null && !journal.halted && journal.entries.every(r => r.status === 'CONFIRMED'),
      'successor needs a journal whose every entry is CONFIRMED (reconcile first; an UNKNOWN entry needs the lead)');
    done = journal.entries.length;
  }
  const live = await readLive(rpc, done);
  let predecessor = null;
  if (predecessorFile) predecessor = {path: path.relative(ROOT, path.resolve(predecessorFile)), sha256: sha(fs.readFileSync(predecessorFile))};
  const config = build(live, keeperPublicAddress, predecessor);
  D.validateCeremony(live.plan, config, Math.floor(Date.parse(live.observedAt) / 1000));
  const stamp = live.observedAt.replace(/[-:]/g, '').replace(/\.\d+Z$/, 'Z');
  const files = {config: path.join(outputDir, `deploy-config-pre-release-${stamp}-L${live.ledger}.json`),
    quote: path.join(outputDir, `fresh-quote-${stamp}-L${live.ledger}.json`), funding: path.join(outputDir, `funding-requirements-pre-release-${stamp}-L${live.ledger}.json`)};
  for (const f of Object.values(files)) assert(!fs.existsSync(f), `preserve previous output ${f}`);
  fs.writeFileSync(files.config, json(config), {flag: 'wx'});
  const funding = fundingRequirements(config, live);
  funding.deploymentConfig = {path: path.relative(ROOT, files.config), sha256: sha(fs.readFileSync(files.config))};
  fs.writeFileSync(files.funding, json(funding), {flag: 'wx'});
  const {plan, ...quote} = live;
  fs.writeFileSync(files.quote, json({status: 'READ_ONLY_FRESH_QUOTE_NOT_SENT', signing: false, submission: false, ...quote,
    estimateSteps3to18: funding.estimate, fundingRequiredBeforeStep1Stroops: funding.xlm.requiredBeforeStep1Stroops}), {flag: 'wx'});
  return {files: Object.fromEntries(Object.entries(files).map(([k, v]) => [k, path.relative(ROOT, v)])), ledger: live.ledger, observedAt: live.observedAt,
    sequence: live.sequence, uploads: live.fees.uploadDetails, inclusionFeeStroops: live.fees.inclusion,
    feeBudgetStroops: config.feeBudgetStroops, maxUploadTransactionFeeStroops: config.maxUploadTransactionFeeStroops,
    requiredBeforeStep1Stroops: funding.xlm.requiredBeforeStep1Stroops, steps3to18EstimateStroops: funding.estimate.totalStroops};
}
module.exports = {KEEPER_SUPPLIED_20261115, build, readLive, fresh, fundingRequirements, inclusionBid, steps3to18Estimate, FEES, PRECEDENT};
if (require.main === module) {
  const args = process.argv.slice(2), mode = args.shift(), opt = name => { const i = args.indexOf(name); return i < 0 ? null : args.splice(i, 2)[1]; };
  Promise.resolve().then(() => {
    assert.equal(mode, 'fresh', 'usage: fresh OUTPUT_DIRECTORY [--keeper G...] [--predecessor PREVIOUS_CONFIG.json --journal JOURNAL_DIRECTORY]');
    const keeper = opt('--keeper'), predecessor = opt('--predecessor'), journal = opt('--journal'); assert.equal(args.length, 1, 'output directory required');
    return fresh(path.resolve(args[0]), keeper, predecessor, undefined, journal);
  }).then(v => console.log(json(v))).catch(e => { console.error(e.message); process.exitCode = 1; });
}
