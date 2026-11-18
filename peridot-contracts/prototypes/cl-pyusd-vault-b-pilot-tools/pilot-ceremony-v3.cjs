// Unsigned-only PYUSD pilot ceremony, v3 (2026-11-15). No signer, key lookup, broadcaster,
// worker execution, or cash-market mutation is implemented. Retries are read-only (see `withRetry`).
//
// Successor of pilot-ceremony-v2.cjs (unchanged). Reviews, test evidence and helper hashes of v1/v2 do NOT transfer.
// v3 changes (each tested in tests/pilot-ceremony-v3.test.cjs):
//  (a) Deployment input is the v3 helper (operator-deployment-v3.cjs, UNCHANGED, sha256 325a7247…) and its segmented
//      journal: the pinned deployment config is the FULL_RELEASE_STEPS_1_TO_20 successor; the confirmed chain is
//      re-validated through D.loadJournal (config chain, read-only). The pilot's initialSourceSequence is the sequence
//      of the activation row (= FULL config initialSourceSequence + 1), not "deployment + 20".
//  (b) USER DECISION 15 Nov ~13:15 UTC: seed = backing cap = 800 PYUSD (8,000,000,000 raw); fallback exactly 2 x 400
//      with --acknowledge-split-exposure. max_deploy (per-command cap, step 7) stays 1,000 PYUSD = 10,000,000,000 raw.
//      The single 800 is offered only if its fresh quote is valid AND admissible AND 800 + quoted reserve (+50%) fits
//      the wallet; otherwise only the 2 x 400 fallback is offered.
//  (c) Open-command check before activation (`check-activation`, read-only): before activation stage_state traps and no
//      command can exist (stage_open requires the StgV1Config that activation writes), so the check proves
//      inactivity (stage_is_active false, no StgV1Config/StgV1State/StgV1Paused in the receipt instance), empty
//      custody, zero vault supply/liquidity/reserved/balances. After activation every gate enumerates every
//      stage_job(id) below next_id (before the seed, before each pause, after each read-back).
//  (d) USER RULES 15 Nov: refuse any financial transaction whose RPC host preflight declares > 97,000,000 instructions;
//      label >= 98,000,000 AT-LIMIT in every report. The 100M declared ceiling stays.
//  (e) Interactive operator flow as in operator-deployment-v3: prepare -> Ledger signature -> send -> reconcile. All
//      network reads are retried read-only on "crossed ledger" (immediately) and HTTP 429 / network errors (backoff);
//      nothing is journaled until a coherent one-ledger preparation succeeded. An unknown hash is never re-prepared.
//
// Kept from v2 (USER DECISIONS of 14 Nov 2026):
//  - Order: activation (deployment step 20) -> operator seed immediately -> pause-vault -> pause-receipt.
//  - Open-command gate BEFORE every seed and BEFORE every pause; any command that is not the operator's own expected
//    one refuses the step and prints what was found. There is no override.
//  - Before a pause the cap must be exactly filled by the operator's finalised principal.
//  - enable_verified_reward_view is not offered (METHODS below is the complete list).
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const D = require('./operator-deployment-v3.cjs');
const ROOT = path.resolve(__dirname, '../..');
const PAIR = D.PAIR;
const INSTRUCTION_LIMIT = 100000000; // declared ceiling; never the upload allowance
const HOST_STOP = 97000000, AT_LIMIT = 98000000; // user rules 15 Nov 2026
const cpuLabel = cpu => cpu >= AT_LIMIT ? 'AT-LIMIT' : cpu > HOST_STOP ? 'ABOVE_97M_STOP' : 'WITHIN_POLICY';
const METHODS = Object.freeze(['stage_open', 'stage_pause_deposits']);
const PAUSES = Object.freeze(['pause-vault', 'pause-receipt']);
const SEED_PLANS = Object.freeze({'single-800': Object.freeze(['8000000000']), 'split-2x400': Object.freeze(['4000000000', '4000000000'])});
const PARAMETERS = Object.freeze({capRaw: '8000000000', maxDeployRaw: '10000000000', toleranceBps: 20, reserveMarginBps: 5000, seedPlans: SEED_PLANS});
const MAX_ENUMERATED_COMMANDS = 32;
const SPLIT_EXPOSURE_WARNING = 'EXPOSURE OF THE 2x400 FALLBACK: after the first 400 PYUSD opens, 400 PYUSD of cap room is open to ANYONE until the second open lands. ' +
  'A foreign command in that window makes the second 400 revert on the cap and leaves a stranger in the pilot; this tool then refuses the second deposit and both pauses, and has no override. ' +
  'The first 400 is unguarded too: with an 800 cap and a 400 open, a stranger who opens up to 400 first does NOT make it revert; it is seen only at the read-back. ' +
  'The single 800 PYUSD seed has no such window (cap = seed). Choosing the fallback requires --acknowledge-split-exposure. Tolerance (20 bps) and reserve margin (5,000 bps) never change.';
const SPLIT_FLAG = '--acknowledge-split-exposure';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
const plain = value => JSON.parse(json(value));
const u = value => {assert(/^(0|[1-9][0-9]*)$/.test(String(value)), 'exact unsigned integer required'); return BigInt(value);};
const eq = (actual, expected, label) => assert.equal(String(actual), String(expected), label);
const addr = value => new S.Address(value).toScVal();
const sum = values => values.reduce((a, b) => a + u(b), 0n);
const order = plan => {assert(SEED_PLANS[plan], 'unknown seed plan'); return [...SEED_PLANS[plan].map((_, i) => `seed-${i + 1}`), ...PAUSES];};
const isSeed = step => /^seed-[12]$/.test(step);
// Cap = exact seed total, for every plan this tool knows.
for (const amounts of Object.values(SEED_PLANS)) assert.equal(String(amounts.reduce((a, b) => a + BigInt(b), 0n)), PARAMETERS.capRaw, 'every seed plan must fill the cap exactly');
const pyusd = raw => {const n = BigInt(raw), neg = n < 0n, v = neg ? -n : n; return (neg ? '-' : '') + (v / 10000000n) + '.' + String(v % 10000000n).padStart(7, '0');};
// A refusal carries the evidence the operator must see. It is never downgraded to a warning.
class Refusal extends Error {
  constructor(code, message, report) {super(message); this.name = 'Refusal'; this.code = code; this.report = plain(report ?? null);}
}
// (e) Read-only retry. A crossed ledger is retried at once (nothing was journaled: deps run before any journal
// write); HTTP 429 / network errors back off. Refusals and every other error are returned unchanged.
const transient = e => !(e instanceof Refusal) && /429|Too Many|rate limit|crossed ledger|crossed a ledger|ledger changed|ledger advanced|ECONNRESET|ETIMEDOUT|ECONNREFUSED|socket hang up|timeout|fetch failed|502|503|504/i.test(String(e?.message || e));
const rateLimited = e => /429|Too Many|rate limit|ECONNRESET|ETIMEDOUT|ECONNREFUSED|socket hang up|timeout|fetch failed|502|503|504/i.test(String(e?.message || e));
async function retrying(fn, {tries = 25, sleep = ms => new Promise(r => setTimeout(r, ms)), log = m => process.stderr.write(m + '\n')} = {}) {
  for (let i = 1, backoff = 0; ; i++) {
    try {return await fn();} catch (e) {
      if (!transient(e) || i >= tries) throw e;
      const ms = rateLimited(e) ? Math.min(2000 * 2 ** backoff++, 30000) : 250;
      log(`read-only retry ${i}/${tries - 1} in ${ms} ms: ${String(e.message).split('\n')[0].slice(0, 100)}`); await sleep(ms);
    }
  }
}
// The CLI adapter's network-facing functions are wrapped; `now` is not.
function withRetry(deps, options) {
  const out = {...deps};
  for (const name of ['preActivation', 'inspect', 'prepare', 'transaction']) if (deps[name]) out[name] = (...a) => retrying(() => deps[name](...a), options);
  return out;
}
// (c) Before activation no stage command can exist: stage_open needs the StgV1Config that stage_activate writes, and
// stage_state traps. The gate therefore proves inactivity and emptiness; it never infers "no commands" from a failed read.
function preActivationGate(input, config, s) {
  const v = s.values, A = input.plan.admin, R = input.plan.proposedInstances.receipt.contract, findings = [];
  if (v.receiptActive !== false) findings.push(`receipt stage_is_active is ${v.receiptActive}; activation already happened or the read is wrong`);
  const stg = s.receiptInstanceKeys.filter(k => k.startsWith('StgV1')); if (stg.length) findings.push(`receipt instance already holds staged keys ${stg.join(', ')}`);
  if (v.receiptPaused !== true) findings.push(`receipt stage_deposits_paused is ${v.receiptPaused}; before activation it must read true (default)`);
  if (v.vaultPaused !== false) findings.push(`vault latch stage_deposits_paused is ${v.vaultPaused}; it must be open (false) before the seed`);
  for (const key of ['vaultSupply', 'positionLiquidity', 'custodyShares', 'vaultFundingBalance', 'vaultOtherBalance']) if (String(v[key]) !== '0') findings.push(`${key} is ${v[key]}, expected 0`);
  if ((v.reserved || []).map(String).join() !== '0,0') findings.push(`vault stage_reserved is [${(v.reserved || []).join(', ')}], expected [0, 0]`);
  if (v.boundReceipt !== R) findings.push(`vault stage_receipt is ${v.boundReceipt}`);
  if (v.vaultGuardian !== A) findings.push(`vault guardian is ${v.vaultGuardian}`);
  if (v.keeper !== config.keeperPublicAddress) findings.push(`vault keeper is ${v.keeper}, expected ${config.keeperPublicAddress}`);
  eq(s.account.sequence, u(config.initialSourceSequence) - 1n, 'Ledger sequence moved since the FULL config: regenerate it (and this pilot config) before activation');
  const decision = seedPlanDecision(s);
  return plain({gate: 'before-activation', ok: findings.length === 0, ledger: s.ledger, findings,
    commandsPossible: 'none: stage_open requires the StgV1Config written only by stage_activate; every stage_job below next_id is enumerated from the first post-activation gate on',
    seedPlanDecision: decision, quoteFull: s.quoteFull, wallet: decision.wallet,
    nextSteps: decision.available === 'single-800' ? 'activation (deployment step 20), then IMMEDIATELY seed-1 (single-800)' : `activation is NOT recommended until the seed can be prepared: ${decision.reason}`});
}
function pinned(pin) {
  assert(pin && typeof pin.path === 'string' && /^[a-f0-9]{64}$/.test(pin.sha256), 'reviewed path/hash required');
  const file = path.resolve(ROOT, pin.path), bytes = fs.readFileSync(file); assert.equal(sha(bytes), pin.sha256, 'pinned input changed');
  return {file, bytes, value: () => JSON.parse(bytes)};
}
// Everything a reviewer must approve (plan, deployment config, this helper, parameters) is pinned BEFORE
// activation. The completed deployment journal cannot be pinned by hash before step 20 exists, so it is
// bound by directory and verified structurally against the pinned plan/config on every run; its hash is
// recorded in the pilot journal at creation and must never change afterwards. This keeps "seed immediately
// after activation" possible without a config re-review while strangers can already open.
function loadInputs(config, {allowPendingActivation = false} = {}) {
  const plan = pinned(config.deployment.plan).value(), deployment = pinned(config.deployment.config).value();
  D.validatePlan(plan);
  assert.equal(plan.selected.backingCapRaw, PARAMETERS.capRaw, 'activation cap must equal the seed total of this tool');
  assert.equal(plan.selected.maxDeployRaw, PARAMETERS.maxDeployRaw, 'per-command cap (step 7) must stay 1,000 PYUSD');
  assert.equal(deployment.helperSha256, sha(fs.readFileSync(path.join(__dirname, 'operator-deployment-v3.cjs'))), 'deployment config belongs to another helper version');
  assert.equal(deployment.planDigestSha256, D.digest(plan));
  // (a) The pinned deployment config must be the FULL successor; the helper re-validates it (expiry not required here:
  // the pilot may run after the activation config expired) and, through loadJournal, every predecessor in the chain.
  assert.equal(deployment.releaseStage, D.STAGES.FULL, 'pilot requires the FULL_RELEASE_STEPS_1_TO_20 deployment config');
  D.validateCeremony(plan, deployment, null);
  assert.equal(deployment.releaseApproval.status, 'APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE');
  for (const name of ['independentReview', 'signedCore']) pinned(deployment.releaseApproval[name]);
  assert.deepEqual(deployment.releaseApproval.wasmSha256, {...PAIR});
  assert(typeof config.deployment.journalDirectory === 'string' && config.deployment.journalDirectory.length > 0, 'deployment journal directory required');
  const directory = path.resolve(ROOT, config.deployment.journalDirectory), file = path.join(directory, 'journal.json');
  const journal = D.loadJournal(directory, plan, deployment);
  assert(journal.entries.every(row => row.status === 'CONFIRMED'), 'deployment journal has an unconfirmed step');
  assert.equal(journal.active, null); assert.equal(journal.halted, false);
  const done = journal.entries.length, activationPending = done === D.ORDER.length - 1;
  assert(done === D.ORDER.length || allowPendingActivation && activationPending, `all ${D.ORDER.length} deployment steps must be confirmed (found ${done})`);
  if (activationPending) assert.equal(journal.attached, false, 'the FULL config is attached but activation is not confirmed');
  else {assert.equal(journal.attached, true, 'activation must be confirmed under the pinned FULL config');
    const last = journal.segments.at(-1); assert.equal(last.firstEntry, D.ORDER.length - 1); assert.equal(last.releaseStage, D.STAGES.FULL);
    eq(journal.entries.at(-1).sequence, config.initialSourceSequence, 'activation row sequence differs from the pilot initial sequence');}
  eq(config.initialSourceSequence, u(deployment.initialSourceSequence) + 1n, 'pilot must follow activation directly: pilot initial sequence = activation sequence = FULL config sequence + 1');
  const specs = {};
  for (const role of ['vault', 'receipt']) {
    const artifact = plan.candidateArtifacts.find(row => row.name === role);
    assert.equal(artifact.sha256, PAIR[role]); specs[role] = S.contract.Spec.fromWasm(pinned(artifact).bytes);
  }
  specs.controller = S.contract.Spec.fromWasm(pinned(deployment.controllerArtifact).bytes);
  return {plan, deployment, deploymentJournal: journal, activationPending,
    deploymentJournalSha256: activationPending ? null : sha(fs.readFileSync(file)), specs};
}
function validateConfig(config, input, now, requireFresh = true) {
  assert.equal(config.status, 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3');
  assert.equal(config.helperSha256, sha(fs.readFileSync(__filename)));
  assert.deepEqual(config.runtimePins, D.runtimePins());
  assert.equal(config.planDigestSha256, D.digest(input.plan));
  assert.equal(config.source, input.plan.admin); u(config.initialSourceSequence);
  assert.equal(config.keeperPublicAddress, input.deployment.keeperPublicAddress);
  assert(S.StrKey.isValidEd25519PublicKey(config.keeperPublicAddress)); assert.notEqual(config.keeperPublicAddress, config.source);
  assert.deepEqual(config.parameters, plain(PARAMETERS), 'cap, 20 bps tolerance, 5,000 bps reserve margin and the two seed plans are fixed; this tool never widens them');
  assert.deepEqual(config.approvedPauses, [...PAUSES], 'both post-finalisation guardian pauses must be explicit');
  assert.equal(config.protocolVersion, 29); assert.deepEqual(config.costModelXdrSha256, input.deployment.costModelXdrSha256);
  const url = new URL(config.rpcUrl); assert.equal(url.protocol, 'https:'); assert(!url.username && !url.password && !url.hash);
  assert(Number.isSafeInteger(config.expiresAt)); if (requireFresh) assert(config.expiresAt > now, 'pilot review expired');
  for (const name of ['feeBudgetStroops', 'maxTransactionFeeStroops', 'inclusionFeeStroops', 'inclusionFeeCapStroops', 'balanceFloorStroops']) assert(u(config[name]) > 0n);
  assert(u(config.maxTransactionFeeStroops) <= u(config.feeBudgetStroops));
  assert(u(config.inclusionFeeStroops) >= 100n && u(config.inclusionFeeStroops) <= u(config.inclusionFeeCapStroops));
  assert(u(config.inclusionFeeCapStroops) <= u(config.maxTransactionFeeStroops));
  assert(Number.isSafeInteger(config.timeoutSeconds) && config.timeoutSeconds > 0 && config.timeoutSeconds <= 300);
}
function contractOperation(input, role, method, args) {
  assert(METHODS.includes(method) || method === 'stage_accept_funds', 'method is outside the pilot ceremony');
  const spec = input.specs[role], fields = spec.getFunc(method).inputs(); assert.equal(args.length, fields.length);
  const target = input.plan.proposedInstances[role].contract;
  return new S.Contract(target).call(method, ...spec.funcArgsToScVals(method,
    Object.fromEntries(fields.map((f, i) => [f.name().toString(), args[i]]))));
}
function authorized(op, children = []) {
  return new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(
    op.body().invokeHostFunctionOp().hostFunction().invokeContract()), subInvocations: children});
}
function operation(input, step, seed) {
  const owner = input.plan.admin;
  if (PAUSES.includes(step)) return contractOperation(input, step === 'pause-vault' ? 'vault' : 'receipt', 'stage_pause_deposits', [owner]);
  assert(isSeed(step) && seed, 'outside the pilot ceremony: only the operator seed and the two guardian pauses exist');
  return contractOperation(input, 'receipt', 'stage_open', [owner, seed.principalRaw, seed.quote.swap_in,
    seed.quote.min_swap, seed.quote.min_shares, seed.quote.reserve, seed.deadline]);
}
function authRoots(input, step, seed) {
  const root = operation(input, step, seed);
  if (!isSeed(step)) return [authorized(root).toXDR('base64')];
  const owner = input.plan.admin, vault = input.plan.proposedInstances.vault.contract, receipt = input.plan.proposedInstances.receipt.contract;
  const job = {owner, kind: 0, phase: 0, deadline: seed.deadline, a: seed.principalRaw, b: '0',
    reserve: seed.quote.reserve, swap_in: seed.quote.swap_in, min_swap: seed.quote.min_swap, min_shares: seed.quote.min_shares, shares: '0'};
  const transfer = new S.Contract(input.deployment.dependencies.fundingToken.contract).call('transfer', addr(owner), addr(vault),
    S.nativeToScVal(u(seed.principalRaw) + u(seed.quote.reserve), {type: 'i128'}));
  const accept = contractOperation(input, 'vault', 'stage_accept_funds', [receipt, seed.id, job]);
  return [authorized(root, [authorized(accept, [authorized(transfer)])]).toXDR('base64')];
}
function validateCommon(input, config, s) {
  const {plan, deployment} = input, v = s.values, V = plan.proposedInstances.vault.contract, R = plan.proposedInstances.receipt.contract;
  assert(s.executableAndCodePinsVerified === true); assert(Number.isSafeInteger(s.ledger) && Number.isSafeInteger(s.closeTime));
  for (const key of ['vaultAdmin', 'receiptAdmin', 'controllerAdmin', 'controllerGuardian', 'vaultGuardian']) assert.equal(v[key], plan.admin, key);
  assert.equal(v.keeper, config.keeperPublicAddress); assert.equal(v.boundReceipt, R);
  assert.equal(v.fundingAsset, deployment.dependencies.fundingToken.contract); assert.equal(v.receiptFundingAsset, v.fundingAsset);
  assert.deepEqual(plain(v.receiptConfig), {version: 1, vault: V, guardian: plan.admin, backing_cap: PARAMETERS.capRaw});
  const c = v.vaultConfig;
  assert.equal(c.pool, deployment.dependencies.pool.contract); assert.equal(c.oracle, deployment.dependencies.oracle.contract);
  assert.equal(c.token0, v.fundingAsset); assert.equal(c.token1, deployment.dependencies.otherToken.contract);
  eq(c.dec0, 7); eq(c.dec1, 7); eq(c.underlying_index, 0); eq(c.tick_lower, -20); eq(c.tick_upper, 20);
  eq(v.vaultParams.max_deploy, PARAMETERS.maxDeployRaw); assert.equal(v.vaultParams.paused, false);
  eq(v.vaultParams.slippage_bps, 100); eq(v.vaultParams.max_pool_divergence_bps, 200);
  eq(v.vaultParams.oracle_max_age_mult, 2); eq(v.vaultParams.nav_root_max_age, 300); eq(v.vaultParams.nav_root_max_stale, 3600);
  assert.equal(v.rewardToken, deployment.aquaToken); assert.equal(v.receiptController, plan.controller);
  for (const key of ['active', 'marketSupported', 'controllerBorrowPaused']) assert.equal(v[key], true, key);
  for (const key of ['controllerDepositPaused', 'controllerRedeemPaused', 'cashBorrowing']) assert.equal(v[key], false, key);
  eq(v.collateralFactor, 0); eq(v.borrowed, 0); eq(v.minCashReserve, 10000);
  for (const key of ['receiptFloor', 'receiptSupply', 'receiptOwnFloor', 'receiptUnderlying', 'receiptFundingBalance']) eq(v[key], '10000000', key);
  eq(v.ownerPToken, 0); eq(v.ownerDirectVaultShares, 0); eq(v.exchangeRate, 1000000);
  assert.equal(typeof v.vaultPaused, 'boolean'); assert.equal(typeof v.receiptPaused, 'boolean');
  assert(Array.isArray(v.jobs) && Array.isArray(v.commands), 'snapshot must carry the complete command enumeration');
}
const quoteOk = q => !!q && q.valid === true && q.admissible === true;
function validateQuote(config, s, deadline, principalRaw) {
  const q = s.quote, amount = u(principalRaw);
  assert(quoteOk(q), 'fresh valid/admissible quote required');
  assert.equal(q.oracle_fresh, true); assert.equal(q.deposits_paused, false); assert.equal(q.composition_dirty, false); assert.equal(q.in_band, true);
  eq(q.ledger, s.ledger); eq(q.timestamp, s.closeTime); eq(s.quoteAmountRaw, principalRaw, 'quote was taken for another amount');
  assert(u(q.swap_in) > 0n && u(q.swap_in) < amount && u(q.swap_out) > 0n && u(q.shares) > 0n);
  // Exactly 20 bps and exactly minimum reserve + 50%. A quote computed with any other tolerance or margin fails here.
  const floor = x => {const n = u(x) * 9980n / 10000n; return n > 0n ? n : 1n;};
  eq(q.min_swap, floor(q.swap_out), 'min_swap is not the 20 bps floor'); eq(q.min_shares, floor(q.shares), 'min_shares is not the 20 bps floor');
  eq(q.reserve, u(q.min_reserve) + (u(q.min_reserve) * 5000n + 9999n) / 10000n, 'reserve is not minimum + 50%');
  eq(q.vault_slippage_bps, s.values.vaultParams.slippage_bps); eq(q.vault_divergence_bps, s.values.vaultParams.max_pool_divergence_bps);
  assert(deadline !== undefined && /^(0|[1-9][0-9]*)$/.test(String(deadline)), 'explicit deadline epoch required');
  assert(u(deadline) > BigInt(s.closeTime) && u(deadline) <= BigInt(s.closeTime + 3600), 'deadline outside exact ABI window');
  assert(BigInt(s.closeTime + config.timeoutSeconds) <= u(deadline), 'envelope timeout must not exceed chosen command deadline');
}
const jobView = row => ({id: String(row.id), owner: row.job.owner, kind: Number(row.job.kind), phase: Number(row.job.phase),
  a: String(row.job.a), b: String(row.job.b), shares: String(row.job.shares), reserve: String(row.job.reserve), deadline: String(row.job.deadline)});
// The open-command gate. `seeds` are the operator's journaled seeds; at a gate every one of them must be
// finalised. Returns a report; never throws for market findings so that `inspect` can print it.
function openCommandGate(input, s, seeds, gate) {
  const v = s.values, st = v.state, A = input.plan.admin, findings = [], foreign = [];
  const expectedIds = seeds.map(seed => String(seed.id));
  const nextId = u(st.next_id), enumerated = v.jobs.map(jobView);
  if (v.jobsTruncated || nextId > BigInt(MAX_ENUMERATED_COMMANDS)) findings.push(`${nextId} commands exist; more than the ${MAX_ENUMERATED_COMMANDS} this tool enumerates`);
  else if (BigInt(enumerated.length) !== nextId) findings.push(`enumeration incomplete: ${enumerated.length} of ${nextId} commands read`);
  if (nextId !== BigInt(seeds.length)) findings.push(`stage_state.next_id is ${nextId}; the operator has created ${seeds.length} command(s)`);
  for (const job of enumerated) {
    const index = expectedIds.indexOf(job.id);
    if (index < 0) {foreign.push({...job, ownerIsOperator: job.owner === A});
      findings.push(`unexpected command id ${job.id}: owner ${job.owner}${job.owner === A ? ' (operator, but not a journaled seed)' : ' (NOT the operator)'}, kind ${job.kind}, phase ${job.phase}, a ${job.a}, b ${job.b}, shares ${job.shares}`); continue;}
    if (job.owner !== A || job.kind !== 0) {foreign.push({...job, ownerIsOperator: job.owner === A}); findings.push(`command id ${job.id} is not the operator's open: owner ${job.owner}, kind ${job.kind}`);}
    else if (job.phase !== 4) findings.push(`operator seed id ${job.id} is in phase ${job.phase}, not finalised (4)`);
  }
  for (const id of expectedIds) if (!enumerated.some(job => job.id === id)) findings.push(`journaled operator seed id ${id} was not found on chain`);
  for (const key of ['open_jobs', 'pending_principal', 'pending_funding', 'pending_other', 'exit_funding', 'exit_other', 'reserve_liability', 'job_units'])
    if (String(st[key]) !== '0') findings.push(`stage_state.${key} is ${st[key]}, expected 0`);
  if (v.commands.length) findings.push(`stage_commands(operator) lists ${v.commands.length} unfinished command(s): ids ${v.commands.map(c => c.id).join(', ')}`);
  const principal = sum(seeds.map(seed => seed.principalRaw));
  if (String(st.principal_backing) !== String(principal)) findings.push(`stage_state.principal_backing is ${st.principal_backing}; the operator's finalised principal is ${principal}`);
  const shares = enumerated.filter(job => expectedIds.includes(job.id) && job.owner === A && job.kind === 0 && job.phase === 4).reduce((a, job) => a + u(job.shares), 0n);
  for (const [key, label] of [['total_units', 'stage_state.total_units']]) if (String(st[key]) !== String(shares)) findings.push(`${label} is ${st[key]}; the operator's seeds minted ${shares}`);
  for (const key of ['units', 'vaultSupply', 'custodyShares', 'beneficial', 'available'])
    if (String(v[key]) !== String(shares)) findings.push(`${key} is ${v[key]}; expected the operator's ${shares} (another beneficiary or holder may exist)`);
  if (String(v.locked) !== '0') findings.push(`operator locked shares are ${v.locked}, expected 0`);
  if (v.reserved.map(String).join() !== '0,0') findings.push(`vault stage_reserved is [${v.reserved.map(String).join(', ')}], expected [0, 0]`);
  if (gate.includes('pause')) {
    const cap = String(v.receiptConfig.backing_cap);
    if (seeds.length === 0) findings.push('no operator seed is journaled; nothing to pause after');
    if (String(principal) !== PARAMETERS.capRaw || cap !== PARAMETERS.capRaw || String(st.principal_backing) !== cap)
      findings.push(`cap is not exactly filled by the operator's principal: cap ${cap}, principal_backing ${st.principal_backing}, operator principal ${principal}`);
  }
  return plain({gate, ok: findings.length === 0, ledger: s.ledger, findings, foreign, expectedOperatorCommandIds: expectedIds,
    stageState: st, operatorUnfinishedCommands: v.commands, allCommands: enumerated,
    custody: {vaultSupply: v.vaultSupply, custodyShares: v.custodyShares, operatorBeneficial: v.beneficial, operatorUnits: v.units, reserved: v.reserved},
    cap: {backing_cap: v.receiptConfig.backing_cap, principal_backing: st.principal_backing, operatorPrincipal: principal}});
}
function requireGate(input, s, seeds, gate) {
  const report = openCommandGate(input, s, seeds, gate);
  if (!report.ok) throw new Refusal(report.foreign.length ? 'FOREIGN_COMMAND' : report.findings.some(f => f.startsWith('cap is not exactly')) ? 'CAP_NOT_EXACTLY_FILLED' : 'OPEN_COMMAND_GATE',
    `${gate}: refused. ${report.findings.join('; ')}. Stop; do not sign anything; follow "If a foreign command appears" in the command list.`, report);
  return report;
}
// Accounting of the operator's seeds: all but the last finalised, the last in any funded-through-finalised phase.
function validateLifecycle(input, config, s, seeds) {
  validateCommon(input, config, s); const v = s.values, st = v.state, A = input.plan.admin;
  assert(seeds.length >= 1 && seeds.length <= 2);
  const ids = seeds.map(seed => String(seed.id)), strangers = v.jobs.map(jobView).filter(job => !ids.includes(job.id));
  if (strangers.length || v.jobsTruncated || String(st.next_id) !== String(seeds.length)) throw new Refusal('FOREIGN_COMMAND',
    `a command that is not a journaled operator seed exists (next_id ${st.next_id}, journaled seeds ${seeds.length}). Stop; follow "If a foreign command appears" in the command list.`,
    {gate: 'lifecycle', ok: false, ledger: s.ledger, foreign: strangers, stageState: st, allCommands: v.jobs.map(jobView)});
  let wallet = u(seeds[0].walletBefore.funding), liquidity = 0n, done = 0n, result;
  for (const [i, seed] of seeds.entries()) {
    const last = i === seeds.length - 1, row = v.jobs.find(x => String(x.id) === String(seed.id)); eq(seed.id, i, 'seed ids are consecutive from 0');
    assert(row, 'operator seed missing on chain'); const j = row.job;
    assert(last ? [1, 2, 3, 5, 4].includes(j.phase) : j.phase === 4, 'pilot command must be the original open (earlier seed finalised), not cancelled/closed/emergency');
    assert.equal(j.owner, A); eq(j.kind, 0); eq(j.deadline, seed.deadline);
    for (const name of ['swap_in', 'min_swap', 'min_shares']) eq(j[name], seed.quote[name]);
    eq(seed.walletBefore.funding, wallet, 'seed baseline wallet differs from the reconciled wallet');
    const minted = [3, 5, 4].includes(j.phase), finalised = j.phase === 4, invested = [5, 4].includes(j.phase);
    const shares = minted ? u(j.shares) : 0n; if (minted) assert(shares >= u(seed.quote.min_shares) && shares > 0n); else eq(j.shares, 0);
    eq(j.reserve, finalised ? 0 : seed.quote.reserve);
    let vaultReserve = u(seed.quote.reserve); wallet -= u(seed.principalRaw) + vaultReserve;
    if (j.phase === 1) {eq(j.a, seed.principalRaw); eq(j.b, 0); vaultReserve += u(j.a);}
    else if (j.phase === 2) {eq(j.a, u(seed.principalRaw) - u(seed.quote.swap_in)); assert(u(j.b) >= u(seed.quote.min_swap)); vaultReserve += u(j.a);}
    let record = null;
    if (invested) {
      eq(j.a, 0); eq(j.b, 0); record = v.investmentJournals?.[String(seed.id)]; assert(record, 'investment journal required');
      assert.equal(record.owner, A); eq(record.shares, shares); eq(record.original_reserve, seed.quote.reserve);
      eq(u(record.cost) + u(record.refund), seed.quote.reserve); assert.equal(record.settled, finalised);
      eq(record.liquidity_before, liquidity); assert(u(record.liquidity_added) > 0n); eq(record.liquidity_after, liquidity + u(record.liquidity_added));
      liquidity = u(record.liquidity_after); assert(u(record.execution_ledger) <= BigInt(s.ledger));
      // Phase 5 erratum: the receipt keeps the original reserve, the vault holds only journal.refund.
      vaultReserve = finalised ? 0n : u(record.refund); if (finalised) wallet += u(record.refund);
    }
    if (!last) {done += shares; continue;}
    eq(st.open_jobs, finalised ? 0 : 1); eq(st.pending_principal, minted ? 0 : seed.principalRaw);
    eq(st.principal_backing, sum(seeds.map(x => x.principalRaw))); eq(st.exit_funding, 0); eq(st.exit_other, 0);
    eq(st.pending_funding, [1, 2].includes(j.phase) ? j.a : 0); eq(st.pending_other, j.phase === 2 ? j.b : 0);
    const total = done + shares, free = finalised ? total : done;
    for (const key of ['vaultSupply', 'custodyShares', 'beneficial']) eq(v[key], total, key);
    eq(st.total_units, total); eq(st.job_units, finalised ? 0 : shares); eq(v.locked, finalised ? 0 : shares); eq(v.units, free); eq(v.available, free);
    eq(st.reserve_liability, finalised ? 0 : seed.quote.reserve);
    if (finalised) assert.deepEqual(plain(v.commands), []);
    else {assert.equal(v.commands.length, 1); eq(v.commands[0].id, seed.id); assert.deepEqual(plain(v.commands[0].job), plain(j));}
    eq(v.positionLiquidity, liquidity); eq(v.reserved[0], vaultReserve); eq(v.reserved[1], j.phase === 2 ? j.b : 0);
    eq(v.ownerFundingBalance, wallet, 'wallet principal/reserve/refund reconciliation'); eq(v.ownerOtherBalance, seeds[0].walletBefore.other);
    result = {seed: i + 1, of: seeds.length, phase: j.phase, finalised, reserveOriginalRaw: String(seed.quote.reserve),
      refundableReserveRaw: invested ? String(record.refund) : String(seed.quote.reserve), investmentCostRaw: invested ? String(record.cost) : null,
      operatorPrincipalRaw: String(sum(seeds.map(x => x.principalRaw))), operatorSharesRaw: String(total)};
  }
  return result;
}
// Pre-flight funding reads, from the same one-ledger snapshot. Prints every shortfall; any one refuses the seed.
function fundingPreflight(input, config, s, remainingPrincipalRaw) {
  const v = s.values, shortfalls = [], need = u(remainingPrincipalRaw) + u(s.quote.reserve), have = u(v.ownerFundingBalance);
  if (have < need) shortfalls.push(`PYUSD: wallet holds ${pyusd(have)}, needs ${pyusd(need)} (remaining principal ${pyusd(remainingPrincipalRaw)} + freshly quoted reserve ${pyusd(s.quote.reserve)} incl. the 5,000 bps margin): short by ${pyusd(need - have)}. The 1 PYUSD floor is separate and already paid (read back as ${pyusd(v.receiptFloor)}).`);
  const a = s.account ?? {}, spendable = a.balance === undefined ? null : u(a.balance) - u(a.selling ?? 0), xlmNeed = u(config.feeBudgetStroops) + u(config.balanceFloorStroops);
  if (spendable === null) shortfalls.push('XLM: operator account balance was not read');
  else if (spendable < xlmNeed) shortfalls.push(`XLM: spendable ${pyusd(spendable)}, needs ${pyusd(xlmNeed)} (fee budget ${pyusd(config.feeBudgetStroops)} + balance floor covering the account reserve ${pyusd(config.balanceFloorStroops)}): short by ${pyusd(xlmNeed - spendable)}`);
  const why = {other: 'in-kind and emergency payouts are paid in both pool tokens', reward: 'a reward claim pays AQUA and rolls back without the trustline'};
  for (const role of ['other', 'reward']) {
    const line = v.trustlines?.[role];
    if (!line || line.present !== true) shortfalls.push(`TRUSTLINE: the operator has no ${line?.code ?? role} trustline (${why[role]})`);
    else if (line.authorized !== true) shortfalls.push(`TRUSTLINE: the operator's ${line.code} trustline is not authorized (${why[role]})`);
  }
  return plain({ok: shortfalls.length === 0, ledger: s.ledger, shortfalls, pyusd: {haveRaw: have, needRaw: need, remainingPrincipalRaw, quotedReserveRaw: s.quote.reserve, floorAlreadyPaidRaw: v.receiptFloor},
    xlm: {spendableStroops: spendable, needStroops: xlmNeed}, trustlines: v.trustlines ?? null});
}
// Which seed plans may be prepared now, from the two fresh quotes of one ledger. Exactly one alternative exists.
function seedPlanDecision(s) {
  const q = s.quoteFull, summary = q ? {valid: q.valid, admissible: q.admissible, oracle_fresh: q.oracle_fresh, deposits_paused: q.deposits_paused,
    composition_dirty: q.composition_dirty, in_band: q.in_band, ledger: q.ledger, swap_in: q.swap_in, min_swap: q.min_swap, min_shares: q.min_shares,
    min_reserve: q.min_reserve, reserve: q.reserve} : null;
  // (b) 800 is offered only when its quote is valid/admissible AND 800 + the quoted reserve (+50%) fits the wallet.
  const have = s.values?.ownerFundingBalance === undefined ? null : u(s.values.ownerFundingBalance);
  const need = quoteOk(q) ? u(PARAMETERS.capRaw) + u(q.reserve) : null;
  const fits = have !== null && need !== null && have >= need;
  const wallet = {haveRaw: have === null ? null : String(have), needRaw: need === null ? null : String(need), fits};
  if (quoteOk(q) && fits) return {available: 'single-800', quoteFull: summary, wallet, reason: 'the 800 PYUSD quote is valid and admissible at 20 bps and 800 + reserve fits the wallet; the split is not offered'};
  return {available: 'split-2x400', quoteFull: summary, wallet,
    reason: (quoteOk(q) ? `800 PYUSD + quoted reserve needs ${pyusd(need)} but the wallet holds ${have === null ? 'an unread amount' : pyusd(have)}` : 'the 800 PYUSD quote is not valid/admissible at 20 bps') +
      '; the only alternative is two 400 PYUSD deposits. Tolerance and reserve margin cannot be changed.'};
}
const splitRecord = before => plain({ledger: before.ledger, quoteFull: before.quoteFull, decision: seedPlanDecision(before), exposureAcknowledged: true, warning: SPLIT_EXPOSURE_WARNING});
function seedFrom(input, config, s, deadline, seeds, plan) {
  const index = seeds.length, principals = SEED_PLANS[plan]; assert(principals && index < principals.length, 'no further seed in this plan');
  const principalRaw = principals[index];
  validateCommon(input, config, s);
  const gate = requireGate(input, s, seeds, `before-seed-${index + 1}`);
  if (index === 0) {
    const decision = seedPlanDecision(s);
    if (decision.available !== plan) throw new Refusal(plan === 'single-800' ? 'SEED_800_NOT_OFFERED_ONLY_SPLIT' : 'SPLIT_NOT_OFFERED_800_ADMISSIBLE',
      `seed plan ${plan} refused: ${decision.reason}` + (plan === 'single-800' ? ' ' + SPLIT_EXPOSURE_WARNING : ''), {requestedPlan: plan, offeredPlan: decision.available, ledger: s.ledger, quoteFull: s.quoteFull ?? null,
        fixed: {toleranceBps: PARAMETERS.toleranceBps, reserveMarginBps: PARAMETERS.reserveMarginBps},
        ...(plan === 'single-800' ? {warning: SPLIT_EXPOSURE_WARNING, requiredFlag: SPLIT_FLAG} : {})});
  } else assert.equal(validateLifecycle(input, config, s, seeds).finalised, true, 'earlier seed must be finalised');
  assert.equal(s.values.vaultPaused, false); assert.equal(s.values.receiptPaused, false);
  if (!quoteOk(s.quote)) throw new Refusal('SEED_QUOTE_INADMISSIBLE_NO_FURTHER_ALTERNATIVE',
    `the ${principalRaw} raw quote at 20 bps is not valid/admissible; no other amount, tolerance or margin is offered. Wait and prepare again, or stop.`,
    {plan, seed: index + 1, ledger: s.ledger, quote: s.quote ?? null});
  validateQuote(config, s, deadline, principalRaw);
  const funding = fundingPreflight(input, config, s, String(sum(principals.slice(index))));
  if (!funding.ok) throw new Refusal('FUNDING_PREFLIGHT', `seed ${index + 1} refused, funding pre-flight failed: ${funding.shortfalls.join(' | ')}. The principal is never reduced to fit.`, funding);
  if (index === 0) {
    for (const key of ['vaultSupply', 'custodyShares', 'beneficial', 'locked', 'available', 'units', 'positionLiquidity', 'vaultFundingBalance', 'vaultOtherBalance']) eq(s.values[key], 0, key);
  }
  return plain({index, plan, id: s.values.state.next_id, principalRaw, deadline: String(deadline), quote: s.quote,
    walletBefore: {funding: s.values.ownerFundingBalance, other: s.values.ownerOtherBalance}, quoteLedger: s.ledger, quoteTimestamp: s.closeTime, gateLedger: gate.ledger, fundingPreflightOk: funding.ok});
}
// Gate first (printed evidence), then the exact accounting. `when` is 'before' (preparation) or 'after' (readback).
function pauseGate(input, config, s, seeds, step, plan, when = 'before') {
  validateCommon(input, config, s);
  const gate = requireGate(input, s, seeds, `${when}-${step}`);
  assert.equal(seeds.length, SEED_PLANS[plan].length, 'every seed of the plan must be journaled before a pause');
  assert.equal(validateLifecycle(input, config, s, seeds).finalised, true, 'wait for finalisation before guardian pauses');
  return gate;
}
// ctx = {plan, seeds: operator seeds journaled BEFORE this row}
function validatePrepared(input, config, row, expectedSequence, spent, ctx) {
  const steps = order(ctx.plan); assert(steps.includes(row.step)); assert.equal(row.step, steps[ctx.seeds.length + (isSeed(row.step) ? 0 : PAUSES.indexOf(row.step))], 'step out of order');
  const tx = S.TransactionBuilder.fromXDR(row.unsignedXdr, input.plan.network);
  assert.equal(tx.signatures.length, 0); assert.equal(tx.source, input.plan.admin); eq(tx.sequence, expectedSequence); eq(row.sequence, expectedSequence);
  assert.equal(tx.hash().toString('hex'), row.hash); assert.equal(tx.operations.length, 1); assert.equal(tx.memo.type, 'none');
  const body = tx.toEnvelope().v1().tx(), op = body.operations()[0]; assert(op.sourceAccount() == null);
  assert.equal(op.body().switch().name, 'invokeHostFunction');
  assert.equal(op.body().invokeHostFunctionOp().hostFunction().switch().name, 'hostFunctionTypeInvokeContract', 'pilot ceremony never uploads or creates');
  assert.equal(isSeed(row.step), row.seed !== undefined, 'only seed steps carry a seed');
  const expected = operation(input, row.step, row.seed).body().invokeHostFunctionOp().hostFunction();
  assert.equal(op.body().invokeHostFunctionOp().hostFunction().toXDR('base64'), expected.toXDR('base64'));
  const auth = op.body().invokeHostFunctionOp().auth();
  assert(auth.every(a => a.credentials().switch().name === 'sorobanCredentialsSourceAccount'));
  assert.deepEqual(auth.map(a => a.rootInvocation().toXDR('base64')), authRoots(input, row.step, row.seed));
  assert.equal(body.cond().switch().name, 'precondV2'); const cond = body.cond().v2();
  assert(cond.minSeqNum() == null); eq(cond.minSeqAge(), 0); eq(cond.minSeqLedgerGap(), 0); assert.equal(cond.extraSigners().length, 0);
  const data = body.ext().sorobanData(), r = data.resources(), fp = r.footprint(), limits = D.LIMITS;
  assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'restoration is separate');
  const keys = [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64')); assert.equal(new Set(keys).size, keys.length);
  assert(r.instructions() <= INSTRUCTION_LIMIT, `declared CPU ${r.instructions()} exceeds 100000000; the upload allowance never applies to pilot transactions and nothing is clamped`);
  // (d) The declared figure is the RPC host preflight (assembleTransaction copies it; it is >= the host's own count).
  if (r.instructions() > HOST_STOP) throw new Refusal('HOST_PREFLIGHT_ABOVE_97M', `${cpuLabel(r.instructions())}: host preflight ${r.instructions()} > ${HOST_STOP}; refused (user rule 15 Nov 2026), nothing is clamped`,
    {step: row.step, declaredInstructions: r.instructions(), label: cpuLabel(r.instructions())});
  assert(keys.length <= limits.keys && fp.readWrite().length <= limits.writes);
  assert(keys.length + fp.readWrite().length <= limits.entries && r.diskReadBytes() <= limits.readBytes && r.writeBytes() <= limits.writeBytes);
  assert(Buffer.from(tx.toXDR(), 'base64').length + 72 <= limits.transactionBytes);
  eq(tx.fee, row.maxFeeStroops); eq(tx.fee, u(config.inclusionFeeStroops) + u(data.resourceFee()));
  assert(u(data.resourceFee()) >= u(row.simulationResourceFeeStroops));
  assert(u(tx.fee) <= u(config.maxTransactionFeeStroops) && u(spent) + u(tx.fee) <= u(config.feeBudgetStroops));
  assert(u(row.balanceStroops) - u(row.sellingLiabilities) - u(tx.fee) >= u(config.balanceFloorStroops));
  assert(u(config.balanceFloorStroops) >= u(row.minimumReserveStroops)); assert.equal(row.networkMemoryAndEventCapsVerified, true);
  assert(Number.isSafeInteger(row.ledger) && row.ledger > 0 && Number.isSafeInteger(row.closeTime));
  eq(row.before.ledger, row.ledger); eq(row.before.closeTime, row.closeTime);
  eq(tx.ledgerBounds.minLedger, row.ledger + 1); eq(tx.ledgerBounds.maxLedger, row.ledger + 20);
  eq(tx.timeBounds.minTime, row.closeTime); eq(tx.timeBounds.maxTime, row.closeTime + config.timeoutSeconds);
  assert(Number(tx.timeBounds.maxTime) <= config.expiresAt, 'envelope must expire within reviewed preparation window');
  // The journaled snapshot must itself pass the gate; the journaled gate report must be the one it produces.
  if (isSeed(row.step)) {
    assert.deepEqual(row.seed, seedFrom(input, config, row.before, row.seed.deadline, ctx.seeds, ctx.plan));
    assert.deepEqual(row.gate, requireGate(input, row.before, ctx.seeds, `before-seed-${ctx.seeds.length + 1}`), 'journaled gate differs from the journaled snapshot');
  } else {
    assert.deepEqual(row.gate, pauseGate(input, config, row.before, ctx.seeds, row.step, ctx.plan), 'journaled gate differs from the journaled snapshot');
    assert.equal(row.before.values.vaultPaused, row.step === 'pause-receipt'); assert.equal(row.before.values.receiptPaused, false);
  }
}
function validateReadback(input, config, row, snapshot, seeds, plan) {
  assert(snapshot.ledger >= row.appliedLedger, 'readback predates applied transaction');
  if (isSeed(row.step)) {const progress = validateLifecycle(input, config, snapshot, seeds);
    assert.equal(snapshot.values.vaultPaused, false); assert.equal(snapshot.values.receiptPaused, false); return progress;}
  // After each pause the gate must still hold: nothing foreign, cap exactly filled, nothing in flight.
  const gate = pauseGate(input, config, snapshot, seeds, row.step, plan, 'after'), result = validateLifecycle(input, config, snapshot, seeds);
  assert.equal(snapshot.values.vaultPaused, true); assert.equal(snapshot.values.receiptPaused, row.step === 'pause-receipt');
  return {...result, gate: gate.gate, gateOk: gate.ok};
}

function atomic(file, value) {
  const tmp = `${file}.tmp-${process.pid}`, fd = fs.openSync(tmp, 'wx', 0o600);
  try {fs.writeFileSync(fd, json(value)); fs.fsyncSync(fd);} finally {fs.closeSync(fd);}
  fs.renameSync(tmp, file); const parent = fs.openSync(path.dirname(file), 'r');
  try {fs.fsyncSync(parent);} finally {fs.closeSync(parent);}
}
function locked(directory, run) {
  fs.mkdirSync(directory, {recursive: true, mode: 0o700});
  assert.equal(fs.realpathSync(directory), path.resolve(directory), 'canonical journal directory required');
  const lock = path.join(directory, 'lock'), fd = fs.openSync(lock, 'wx', 0o600);
  return Promise.resolve().then(run).finally(() => {fs.closeSync(fd); fs.unlinkSync(lock);});
}
const seedsOf = entries => entries.filter(row => isSeed(row.step)).map(row => row.seed);
function loadJournal(input, config, directory) {
  const file = path.join(directory, 'journal.json');
  const journal = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file)) : {schema: 'unsigned-pyusd-pilot-v3',
    configDigestSha256: D.digest(config), deploymentJournalSha256: input.deploymentJournalSha256, seedPlan: null, splitPrecondition: null,
    entries: [], active: null, halted: false, feesCharged: '0'};
  assert.equal(journal.schema, 'unsigned-pyusd-pilot-v3'); assert.equal(journal.configDigestSha256, D.digest(config));
  assert(/^[a-f0-9]{64}$/.test(String(input.deploymentJournalSha256)), 'completed deployment journal required');
  assert.equal(journal.deploymentJournalSha256, input.deploymentJournalSha256, 'completed deployment journal changed after the pilot started');
  if (journal.entries.length === 0) {assert.equal(journal.seedPlan, null); assert.equal(journal.splitPrecondition, null);}
  else {
    assert(SEED_PLANS[journal.seedPlan], 'journal must fix the seed plan with its first entry');
    // The split exists only because the journaled 800 quote of the first preparation was not valid/admissible or did not fit the wallet.
    if (journal.seedPlan === 'split-2x400') {assert(journal.splitPrecondition && seedPlanDecision(journal.entries[0].before).available === 'split-2x400', 'split requires the journaled 800 PYUSD quote that was inadmissible or did not fit the wallet');
      assert.deepEqual(journal.splitPrecondition, splitRecord(journal.entries[0].before), 'journaled split precondition differs from the journaled snapshot');}
    else assert.equal(journal.splitPrecondition, null);
  }
  const steps = journal.seedPlan ? order(journal.seedPlan) : []; assert(journal.entries.length <= steps.length); let fees = 0n;
  for (let i = 0; i < journal.entries.length; i++) {
    const row = journal.entries[i], before = seedsOf(journal.entries.slice(0, i)), through = seedsOf(journal.entries.slice(0, i + 1)); assert.equal(row.step, steps[i]);
    assert(['PREPARED', 'APPLIED_PENDING_READBACK', 'CONFIRMED', 'FAILED'].includes(row.status));
    validatePrepared(input, config, row, String(u(config.initialSourceSequence) + BigInt(i + 1)), fees, {plan: journal.seedPlan, seeds: before});
    assert.equal(row.feeCharged !== undefined, row.status !== 'PREPARED');
    if (row.feeCharged !== undefined) {assert(u(row.feeCharged) <= u(row.maxFeeStroops)); fees += u(row.feeCharged);
      assert(Number.isSafeInteger(row.appliedLedger) && row.appliedLedger > row.ledger);}
    if (row.status === 'CONFIRMED') validateReadback(input, config, row, row.readback, through, journal.seedPlan);
    if (i < journal.entries.length - 1) assert.equal(row.status, 'CONFIRMED');
  }
  eq(fees, journal.feesCharged);
  const tail = journal.entries.at(-1); assert.equal(journal.active, tail && tail.status !== 'CONFIRMED' ? tail.step : null);
  assert.equal(journal.halted, tail?.status === 'FAILED'); return journal;
}
// Engine dependencies are explicit for unsigned fake-RPC tests. CLI always uses
// loadInputs() and the real read/simulate-only adapter; there is no bypass flag.
async function prepare(input, config, directory, {deadline, seedPlan, acknowledgeSplitExposure} = {}, deps = adapter(input, config)) {
  validateConfig(config, input, deps.now());
  return locked(directory, async () => {
    const journal = loadJournal(input, config, directory); assert.equal(journal.halted, false, 'failed transaction requires investigation');
    assert.equal(journal.active, null, 'reconcile existing hash; never prepare a replacement blindly');
    if (journal.seedPlan && seedPlan !== undefined) assert.equal(seedPlan, journal.seedPlan, 'the seed plan was fixed by the first journaled seed and cannot change');
    const plan = journal.seedPlan ?? seedPlan ?? 'single-800'; assert(SEED_PLANS[plan], 'seed plan must be single-800 or split-2x400');
    const step = order(plan)[journal.entries.length]; assert(step, 'pilot ceremony already complete');
    // The fallback is the operator's explicit, informed choice; it is never entered silently.
    if (plan === 'split-2x400' && journal.entries.length === 0 && acknowledgeSplitExposure !== true) throw new Refusal('SPLIT_EXPOSURE_NOT_ACKNOWLEDGED',
      `the 2x400 fallback requires ${SPLIT_FLAG}. ${SPLIT_EXPOSURE_WARNING}`, {warning: SPLIT_EXPOSURE_WARNING, requiredFlag: SPLIT_FLAG});
    if (plan !== 'split-2x400' || journal.entries.length > 0) assert.equal(acknowledgeSplitExposure, undefined, `${SPLIT_FLAG} applies only to the first deposit of the 2x400 fallback`);
    if (isSeed(step)) assert(deadline !== undefined, 'explicit command deadline required'); else assert.equal(deadline, undefined, 'a pause takes no deadline');
    const sequence = String(u(config.initialSourceSequence) + BigInt(journal.entries.length + 1)), seeds = seedsOf(journal.entries);
    const row = await deps.prepare({step, sequence, seeds, plan, deadline});
    assert.equal(row.step, step); validatePrepared(input, config, row, sequence, journal.feesCharged, {plan, seeds});
    row.status = 'PREPARED';
    if (journal.entries.length === 0) {journal.seedPlan = plan;
      journal.splitPrecondition = plan === 'split-2x400' ? splitRecord(row.before) : null;}
    journal.entries.push(row); journal.active = step;
    // Durable hash/sequence/unsigned envelope precede any operator handoff.
    atomic(path.join(directory, 'journal.json'), journal);
    const declared = S.TransactionBuilder.fromXDR(row.unsignedXdr, input.plan.network).toEnvelope().v1().tx().ext().sorobanData().resources().instructions();
    const txb = S.TransactionBuilder.fromXDR(row.unsignedXdr, input.plan.network);
    return {status: 'UNSIGNED_PREPARED_OPERATOR_REVIEW_REQUIRED', step, seedPlan: plan, hash: row.hash, sequence, unsignedXdr: row.unsignedXdr,
      declaredInstructions: declared, cpuLabel: cpuLabel(declared), validLedgers: {min: Number(txb.ledgerBounds.minLedger), max: Number(txb.ledgerBounds.maxLedger)},
      validUntilUtc: new Date(Number(txb.timeBounds.maxTime) * 1000).toISOString(),
      maxFeeStroops: row.maxFeeStroops, seed: row.seed ?? null, gate: row.gate, ...(plan === 'split-2x400' ? {warning: SPLIT_EXPOSURE_WARNING} : {}), signing: false, submission: false};
  });
}
async function reconcile(input, config, directory, deps = adapter(input, config)) {
  // Expiry prevents new preparation, never public reconciliation of the old hash.
  validateConfig(config, input, deps.now(), false);
  return locked(directory, async () => {
    const journal = loadJournal(input, config, directory); assert(journal.active, 'no outstanding operation');
    const row = journal.entries.at(-1); assert.notEqual(row.status, 'FAILED', 'failed transaction is halted');
    if (row.status === 'PREPARED') {
      const result = await deps.transaction(row);
      if (!['SUCCESS', 'FAILED'].includes(result.status)) return {status: 'UNKNOWN_NO_RETRY', hash: row.hash, signing: false, submission: false};
      assert.equal(result.hash, row.hash); eq(result.sequence, row.sequence); assert.equal(result.source, input.plan.admin);
      assert(u(result.feeCharged) <= u(row.maxFeeStroops)); assert(Number.isSafeInteger(result.ledger) && result.ledger > row.ledger);
      row.feeCharged = String(result.feeCharged); row.appliedLedger = result.ledger;
      journal.feesCharged = String(u(journal.feesCharged) + u(result.feeCharged));
      row.status = result.status === 'SUCCESS' ? 'APPLIED_PENDING_READBACK' : 'FAILED'; journal.halted = result.status === 'FAILED';
      atomic(path.join(directory, 'journal.json'), journal);
    }
    assert.equal(journal.halted, false, 'operator transaction failed; no retry');
    const seeds = seedsOf(journal.entries), snapshot = await deps.inspect(seeds), progress = validateReadback(input, config, row, snapshot, seeds, journal.seedPlan);
    row.readback = snapshot; row.status = 'CONFIRMED'; journal.active = null;
    atomic(path.join(directory, 'journal.json'), journal);
    return {status: 'CONFIRMED_WITH_READBACK', step: row.step, hash: row.hash, progress, signing: false, submission: false};
  });
}
// Read-only. Always prints the gate report, also when it is not clean. gate.ok === true means the next step
// may be prepared; false while a seed is in flight, when a transaction is outstanding, or when something foreign exists.
async function inspect(input, config, directory, deps = adapter(input, config)) {
  validateConfig(config, input, deps.now(), false); const journal = loadJournal(input, config, directory);
  const applied = seedsOf(journal.entries.filter(row => row.status !== 'PREPARED')), snapshot = await deps.inspect(applied);
  validateCommon(input, config, snapshot);
  const plan = journal.seedPlan, tail = journal.entries.at(-1);
  const next = journal.active ? null : plan ? order(plan)[journal.entries.length] ?? null : 'seed-1';
  const pauseNext = !!next && PAUSES.includes(next), allDone = !!plan && !journal.active && journal.entries.length === order(plan).length;
  const gate = openCommandGate(input, snapshot, applied, allDone ? 'after-pause-receipt' : pauseNext ? `before-${next}` : `before-seed-${applied.length + 1}`);
  let progress; try {progress = applied.length ? validateLifecycle(input, config, snapshot, applied) : {phase: 'NOT_OPENED'};} catch (error) {progress = {refused: error.message};}
  return {status: 'READ_ONLY', seedPlan: plan, nextStep: next, outstandingHash: journal.active ? tail.hash : null, outstandingStatus: journal.active ? tail.status : null,
    gate, progress, seedPlanDecision: applied.length === 0 && !journal.entries.length ? seedPlanDecision(snapshot) : null,
    paused: {vault: snapshot.values.vaultPaused, receipt: snapshot.values.receiptPaused}, snapshot, signing: false, submission: false};
}
module.exports = {PAIR, PARAMETERS, SEED_PLANS, PAUSES, METHODS, SPLIT_EXPOSURE_WARNING, SPLIT_FLAG, fundingPreflight, INSTRUCTION_LIMIT, MAX_ENUMERATED_COMMANDS, Refusal, order, loadInputs, validateConfig, validateCommon,
  validateQuote, openCommandGate, requireGate, seedPlanDecision, seedFrom, pauseGate, validateLifecycle, operation, authRoots, validatePrepared, validateReadback,
  loadJournal, adapter, prepare, reconcile, inspect};
// Pilot config generator (tool). The operator reviews the file and runs mark-reviewed; agents never do.
const PENDING_STATUS = 'TOOL_GENERATED_PILOT_V3_PENDING_OPERATOR_REVIEW';
const PLAN_FILE = 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/deploy/mainnet-deployment-plan-v3-seed800.json';
function makeConfig(deploymentConfigFile, journalDirectory, {now = Math.floor(Date.now() / 1000), rpcUrl = 'https://rpc.lightsail.network'} = {}) {
  const rel = f => path.relative(ROOT, path.resolve(ROOT, f)), pin = f => ({path: rel(f), sha256: sha(fs.readFileSync(path.resolve(ROOT, f)))});
  const deployment = JSON.parse(fs.readFileSync(path.resolve(ROOT, deploymentConfigFile)));
  const config = {status: PENDING_STATUS, helperSha256: sha(fs.readFileSync(__filename)), runtimePins: D.runtimePins(),
    planDigestSha256: deployment.planDigestSha256, deployment: {plan: pin(PLAN_FILE), config: pin(deploymentConfigFile), journalDirectory: rel(journalDirectory)},
    source: deployment.source, initialSourceSequence: String(u(deployment.initialSourceSequence) + 1n), keeperPublicAddress: deployment.keeperPublicAddress,
    parameters: plain(PARAMETERS), approvedPauses: [...PAUSES], protocolVersion: 29, costModelXdrSha256: deployment.costModelXdrSha256, rpcUrl,
    expiresAt: now + 86400, feeBudgetStroops: String(10n * 10000000n), maxTransactionFeeStroops: String(5n * 10000000n),
    inclusionFeeStroops: deployment.inclusionFeeStroops, inclusionFeeCapStroops: deployment.inclusionFeeCapStroops, balanceFloorStroops: String(5n * 10000000n), timeoutSeconds: 180,
    hostPreflightStop: HOST_STOP, atLimitFrom: AT_LIMIT,
    notes: ['Seed = cap = 800 PYUSD; fallback 2 x 400 only via --seed-plan split-2x400 --acknowledge-split-exposure when 800 is not offered.',
      'Pilot sequence follows activation directly: no other Ledger transaction between activation and the last pause.',
      'Pauses are permanent latches in these bytes (no setter reopens deposits).']};
  const input = loadInputs({...config, status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3'}, {allowPendingActivation: true});
  validateConfig({...config, status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3'}, input, now);
  return config;
}
function markReviewed(file, now = Math.floor(Date.now() / 1000)) {
  assert(file.endsWith('.UNREVIEWED.json')); const config = JSON.parse(fs.readFileSync(file)); assert.equal(config.status, PENDING_STATUS);
  const reviewed = {...config, status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3', operatorReview: {by: 'operator (mark-reviewed)', atUnix: now, unreviewedSha256: sha(fs.readFileSync(file))}};
  validateConfig(reviewed, loadInputs(reviewed, {allowPendingActivation: true}), now);
  const out = file.replace(/\.UNREVIEWED\.json$/, '.json'); fs.writeFileSync(out, json(reviewed), {flag: 'wx'});
  return {status: reviewed.status, file: out, sha256: sha(fs.readFileSync(out)), signing: false, submission: false};
}
// Read-only RPC wrapper: cached passphrase, bounded concurrent reads (HTTP 429), start right after a ledger closes.
function paced(rpc, {concurrency = 12, sleep = ms => new Promise(r => setTimeout(r, ms))} = {}) {
  let network = null, active = 0; const queue = [];
  const limited = name => async (...a) => {while (active >= concurrency) await new Promise(r => queue.push(r)); active++;
    try {return await rpc[name](...a);} finally {active--; const next = queue.shift(); if (next) next();}};
  return {getNetwork: async () => network ??= await rpc.getNetwork(), getLatestLedger: (...a) => rpc.getLatestLedger(...a), getLedgers: (...a) => rpc.getLedgers(...a),
    getLedgerEntries: limited('getLedgerEntries'), simulateTransaction: limited('simulateTransaction'), getTransaction: (...a) => rpc.getTransaction(...a),
    async alignToNewLedger(maxMs = 15000) {const start = (await rpc.getLatestLedger()).sequence, t0 = Date.now();
      while (Date.now() - t0 < maxMs) {await sleep(200); if ((await rpc.getLatestLedger()).sequence > start) return;}}};
}
module.exports.makeConfig = makeConfig; module.exports.markReviewed = markReviewed; module.exports.preActivationGate = preActivationGate;
module.exports.retrying = retrying; module.exports.withRetry = withRetry; module.exports.transient = transient; module.exports.paced = paced;
module.exports.HOST_STOP = HOST_STOP; module.exports.AT_LIMIT = AT_LIMIT; module.exports.cpuLabel = cpuLabel; module.exports.PENDING_STATUS = PENDING_STATUS;
if (require.main === module) (async () => {
  const [mode, file, directory, ...rest] = process.argv.slice(2);
  const usage = 'usage: node pilot-ceremony-v3.cjs make-config <FULL deployment config> <deployment journal dir> <out.UNREVIEWED.json> | mark-reviewed <file.UNREVIEWED.json> --i-reviewed-this-config (OPERATOR ONLY) | preflight <reviewed-config.json> | check-activation <reviewed-config.json> | inspect|reconcile <reviewed-config.json> <journal-directory> | prepare <reviewed-config.json> <journal-directory> <open-deadline-epoch> [--seed-plan single-800|split-2x400 [--acknowledge-split-exposure]]  (pauses take no deadline)';
  assert(['make-config', 'mark-reviewed', 'preflight', 'check-activation', 'inspect', 'prepare', 'reconcile'].includes(mode) && file, usage);
  if (mode === 'make-config') {assert(directory && rest.length === 1 && rest[0].endsWith('.UNREVIEWED.json') && !fs.existsSync(rest[0]), usage);
    fs.writeFileSync(rest[0], json(makeConfig(file, directory)), {flag: 'wx'}); process.stdout.write(json({status: PENDING_STATUS, file: rest[0], sha256: sha(fs.readFileSync(rest[0])), signing: false, submission: false})); return;}
  if (mode === 'mark-reviewed') {assert(directory === '--i-reviewed-this-config' && rest.length === 0, usage); process.stdout.write(json(markReviewed(path.resolve(file)))); return;}
  const config = JSON.parse(fs.readFileSync(file));
  if (mode === 'preflight') {
    // Offline. Run BEFORE signing activation so that the seed can be prepared immediately afterwards.
    assert(directory === undefined, usage); const input = loadInputs(config, {allowPendingActivation: true});
    validateConfig(config, input, Math.floor(Date.now() / 1000));
    process.stdout.write(json({status: 'PREFLIGHT_OFFLINE_OK', deploymentStepsConfirmed: input.deploymentJournal.entries.length, activationPending: input.activationPending,
      nextSourceSequence: String(u(config.initialSourceSequence) + 1n), expiresAt: config.expiresAt, parameters: PARAMETERS, signing: false, submission: false})); return;
  }
  const rpc = paced(new S.rpc.Server(config.rpcUrl, {timeout: 60}));
  if (mode === 'check-activation') {
    assert(directory === undefined, usage); const input = loadInputs(config, {allowPendingActivation: true});
    assert(input.activationPending, 'activation already confirmed: use inspect'); validateConfig(config, input, Math.floor(Date.now() / 1000));
    const s = await adapter(input, config, rpc).preActivation(), gate = preActivationGate(input, config, s);
    process.stdout.write(json({status: 'READ_ONLY', gate, snapshot: s, signing: false, submission: false})); if (!gate.ok || gate.seedPlanDecision.available !== 'single-800') process.exitCode = 3; return;
  }
  assert(directory, usage); let deadline, seedPlan, acknowledgeSplitExposure;
  for (let i = 0; i < rest.length; i++) {
    if (rest[i] === '--seed-plan') {assert(mode === 'prepare' && rest[i + 1], usage); seedPlan = rest[++i];}
    else if (rest[i] === SPLIT_FLAG) {assert(mode === 'prepare', usage); acknowledgeSplitExposure = true;}
    else {assert(mode === 'prepare' && deadline === undefined && /^[0-9]+$/.test(rest[i]), usage); deadline = rest[i];}
  }
  const input = loadInputs(config), deps = adapter(input, config, rpc);
  const result = mode === 'prepare' ? await prepare(input, config, path.resolve(directory), {deadline, seedPlan, acknowledgeSplitExposure}, deps) :
    await (mode === 'reconcile' ? reconcile : inspect)(input, config, path.resolve(directory), deps);
  process.stdout.write(json(result));
  if (mode === 'inspect' && (result.gate.ok !== true || result.outstandingHash)) process.exitCode = 3;
})().catch(error => {
  if (error instanceof Refusal) process.stdout.write(json({status: 'REFUSED', code: error.code, message: error.message, found: error.report, signing: false, submission: false}));
  process.stderr.write(error.message + '\n'); process.exitCode = 1;
});
function adapter(input, config, rpc = new S.rpc.Server(config.rpcUrl, {timeout: 45})) {
  const {plan, deployment} = input, A = plan.admin, V = plan.proposedInstances.vault.contract, R = plan.proposedInstances.receipt.contract;
  const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(),
    key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
  const dataKey = (id, name) => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(),
    key: S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(name)]), durability: S.xdr.ContractDataDurability.persistent()}));
  const sameLedger = (result, ledger) => assert.equal(result.latestLedger, ledger, 'read/quote crossed ledger; prepare nothing');
  const u64 = value => S.nativeToScVal(u(value), {type: 'u64'});
  async function network() {assert.equal((await rpc.getNetwork()).passphrase, plan.network);}
  async function entries(keys, ledger) {const result = await rpc.getLedgerEntries(...keys); sameLedger(result, ledger); return result.entries;}
  function tx(account, latest, op) {
    return new S.TransactionBuilder(new S.Account(A, account.sequence), {networkPassphrase: plan.network, fee: String(config.inclusionFeeStroops)})
      .addOperation(op).setLedgerbounds(latest.sequence + 1, latest.sequence + 20)
      .setTimebounds(Number(latest.closeTime), Number(latest.closeTime) + config.timeoutSeconds).build();
  }
  async function context(requireCosts) {
    await network(); const latest = await rpc.getLatestLedger();
    assert(Number.isSafeInteger(latest.sequence) && Number.isSafeInteger(Number(latest.closeTime)));
    if (requireCosts) eq(latest.protocolVersion, config.protocolVersion);
    const pins = [{contract: V, wasmSha256: PAIR.vault}, {contract: R, wasmSha256: PAIR.receipt},
      {contract: plan.controller, wasmSha256: deployment.controllerArtifact.sha256}, ...Object.values(deployment.dependencies)];
    assert.equal(new Set(pins.map(p => p.contract)).size, pins.length);
    const accountKey = S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: new S.Address(A).toScAddress().accountId()}));
    const rawRules = [{label: 'receiptController', key: dataKey(R, 'Peridottroller')}, {label: 'controllerGuardian', key: dataKey(plan.controller, 'PauseGuardian')}];
    for (const id of [V, R, plan.controller]) for (const name of ['PendingAdmin', 'PendingUpgradeHash', 'PendingUpgradeEta']) rawRules.push({label: id + ':' + name, key: dataKey(id, name), absent: true});
    // Classic trustlines of the operator for the three pool/reward assets (existence and authorization).
    const lines = Object.entries({funding: deployment.dependencies.fundingToken, other: deployment.dependencies.otherToken, reward: deployment.dependencies.rewardToken}).map(([role, pin]) => {
      assert(pin.asset, `${role} token must be pinned as a classic asset to read the operator trustline`);
      return {role, code: pin.asset.code, key: S.xdr.LedgerKey.trustline(new S.xdr.LedgerKeyTrustLine({accountId: new S.Address(A).toScAddress().accountId(), asset: new S.Asset(pin.asset.code, pin.asset.issuer).toTrustLineXDRObject()}))};});
    const requested = [...pins.map(p => instanceKey(p.contract)), accountKey, ...rawRules.map(r => r.key), ...lines.map(l => l.key)];
    const got = await entries(requested, latest.sequence), map = new Map(got.map(r => [r.key.toXDR('base64'), r])); assert.equal(map.size, got.length);
    const hashes = new Set();
    for (const pin of pins) {
      const row = map.get(instanceKey(pin.contract).toXDR('base64')); assert(row, 'required instance absent/archived');
      const executable = row.val.contractData().val().instance().executable();
      if (pin.asset) {assert.equal(executable.switch().name, 'contractExecutableStellarAsset'); assert.equal(new S.Asset(pin.asset.code, pin.asset.issuer).contractId(plan.network), pin.contract);}
      else {assert.equal(executable.switch().name, 'contractExecutableWasm'); assert.equal(executable.wasmHash().toString('hex'), pin.wasmSha256); hashes.add(pin.wasmSha256);}
    }
    const codes = await entries([...hashes].map(h => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(h, 'hex')}))), latest.sequence);
    assert.equal(codes.length, hashes.size); for (const row of codes) assert(hashes.delete(sha(row.val.contractCode().code()))); assert.equal(hashes.size, 0);
    const raw = {};
    for (const rule of rawRules) {const row = map.get(rule.key.toXDR('base64')); if (rule.absent) assert(!row, 'pending role/upgrade blocks pilot'); else {assert(row); raw[rule.label] = S.scValToNative(row.val.contractData().val());}}
    raw.trustlines = Object.fromEntries(lines.map(l => {const row = map.get(l.key.toXDR('base64')), t = row?.val.trustLine();
      return [l.role, t ? {code: l.code, present: true, authorized: (t.flags() & 1) === 1, balance: t.balance().toString(), limit: t.limit().toString()} : {code: l.code, present: false}];}));
    const accountRow = map.get(accountKey.toXDR('base64')); assert(accountRow); const a = accountRow.val.account();
    assert.equal(S.StrKey.encodeEd25519PublicKey(a.accountId().ed25519()), A); assert.equal(a.signers().length, 0); assert.equal(a.thresholds()[0], 1); assert([...a.thresholds()].slice(1).every(n => n <= 1));
    const v1 = a.ext().switch() === 1 ? a.ext().v1() : null, v2 = v1 && v1.ext().switch() === 2 ? v1.ext().v2() : null;
    const account = {sequence: a.seqNum().toString(), balance: a.balance().toString(), selling: v1 ? v1.liabilities().selling().toString() : '0',
      reserveUnits: 2 + a.numSubEntries() + (v2 ? v2.numSponsoring() - v2.numSponsored() : 0)};
    let minimumReserve = null;
    if (requireCosts) {
      const headers = await rpc.getLedgers({startLedger: latest.sequence, pagination: {limit: 1}}); sameLedger(headers, latest.sequence); assert.equal(headers.ledgers.length, 1);
      const header = headers.ledgers[0].headerXdr.header(); eq(header.ledgerSeq(), latest.sequence); eq(header.scpValue().closeTime(), latest.closeTime);
      assert(u(config.inclusionFeeStroops) >= BigInt(header.baseFee())); minimumReserve = BigInt(header.baseReserve()) * BigInt(account.reserveUnits);
      const names = ['configSettingContractComputeV0', 'configSettingContractEventsV0', 'configSettingContractCostParamsCpuInstructions', 'configSettingContractCostParamsMemoryBytes'];
      const settings = await entries(names.map(name => S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[name]()}))), latest.sequence);
      assert.equal(settings.length, names.length); const values = new Map(settings.map(row => [row.val.configSetting().switch().name, row.val.configSetting()]));
      assert(values.get(names[0]).contractCompute().txMemoryLimit() <= D.LIMITS.memory); assert(values.get(names[1]).contractEvents().txMaxContractEventsSizeBytes() <= D.LIMITS.events);
      assert.equal(sha(values.get(names[2]).toXDR()), config.costModelXdrSha256.cpu); assert.equal(sha(values.get(names[3]).toXDR()), config.costModelXdrSha256.memory);
    }
    // An archived record never reads as "no commands": a restore preamble is a refusal.
    const read = async (contract, method, args) => {
      const sim = await rpc.simulateTransaction(tx(account, latest, new S.Contract(contract).call(method, ...args)));
      sameLedger(sim, latest.sequence); assert(S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble && sim.result, `view ${method} failed or needs a separate restore; nothing is prepared`); assert.equal(sim.result.auth.length, 0);
      return plain(S.scValToNative(sim.result.retval));
    };
    return {latest, account, minimumReserve, raw, read, fresh: async () => assert.equal((await rpc.getLatestLedger()).sequence, latest.sequence)};
  }
  const quoteArgs = amount => [S.nativeToScVal(u(amount), {type: 'u128'}), S.nativeToScVal(PARAMETERS.toleranceBps, {type: 'u32'}), S.nativeToScVal(PARAMETERS.reserveMarginBps, {type: 'u32'})];
  // quoteAmount: raw principal to quote for the next seed, or null. With no journaled seed the 1,000 quote is always read too.
  async function snapshot(c, seeds, quoteAmount) {
    const tasks = {}, add = (label, target, method, args = []) => {tasks[label] = c.read(target, method, args);};
    for (const [label, method] of Object.entries({vaultAdmin: 'get_admin', vaultGuardian: 'stage_guardian', keeper: 'stage_investment_keeper',
      boundReceipt: 'stage_receipt', fundingAsset: 'stage_funding_asset', vaultConfig: 'get_config', vaultParams: 'get_params', rewardToken: 'get_primary_reward_token',
      vaultPaused: 'stage_deposits_paused', vaultSupply: 'total_supply', positionLiquidity: 'get_position_liquidity', reserved: 'stage_reserved'})) add(label, V, method);
    for (const [label, method] of Object.entries({beneficial: 'stage_beneficial', locked: 'stage_locked', available: 'stage_available', ownerDirectVaultShares: 'balance'})) add(label, V, method, [addr(A)]);
    add('custodyShares', V, 'balance', [addr(R)]);
    for (const [label, method] of Object.entries({receiptAdmin: 'get_admin', receiptFundingAsset: 'get_underlying_token', receiptConfig: 'stage_config',
      active: 'stage_is_active', receiptPaused: 'stage_deposits_paused', state: 'stage_state', receiptFloor: 'get_minimum_supply', receiptSupply: 'get_total_ptokens',
      receiptUnderlying: 'get_total_underlying', exchangeRate: 'get_exchange_rate', cashBorrowing: 'get_cash_borrowing_enabled', borrowed: 'get_total_borrowed', minCashReserve: 'get_min_cash_reserve_bps'})) add(label, R, method);
    add('receiptOwnFloor', R, 'get_ptoken_balance', [addr(R)]); add('ownerPToken', R, 'get_ptoken_balance', [addr(A)]);
    add('units', R, 'stage_units', [addr(A)]); add('commands', R, 'stage_commands', [addr(A)]);
    add('controllerAdmin', plan.controller, 'get_admin');
    for (const [label, method] of Object.entries({marketSupported: 'is_market_supported', controllerDepositPaused: 'is_deposit_paused',
      controllerRedeemPaused: 'is_redeem_paused', controllerBorrowPaused: 'is_borrow_paused', collateralFactor: 'get_market_cf'})) add(label, plan.controller, method, [addr(R)]);
    const funding = deployment.dependencies.fundingToken.contract, other = deployment.dependencies.otherToken.contract;
    for (const [label, token, owner] of [['ownerFundingBalance', funding, A], ['ownerOtherBalance', other, A], ['receiptFundingBalance', funding, R], ['vaultFundingBalance', funding, V], ['vaultOtherBalance', other, V]]) add(label, token, 'balance', [addr(owner)]);
    if (quoteAmount) add('quote', V, 'stage_quote_open', quoteArgs(quoteAmount));
    if (seeds.length === 0) add('quoteFull', V, 'stage_quote_open', quoteArgs(PARAMETERS.capRaw));
    const values = Object.fromEntries(await Promise.all(Object.entries(tasks).map(async ([key, promise]) => [key, await promise])));
    // Every command ever created, by id. stage_commands is per owner and cannot show a stranger.
    const count = u(values.state.next_id); values.jobsTruncated = count > BigInt(MAX_ENUMERATED_COMMANDS);
    const ids = Array.from({length: Number(values.jobsTruncated ? BigInt(MAX_ENUMERATED_COMMANDS) : count)}, (_, i) => String(i));
    values.jobs = await Promise.all(ids.map(async id => ({id, job: await c.read(R, 'stage_job', [u64(id)])})));
    values.investmentJournals = {};
    for (const seed of seeds) {
      const row = values.jobs.find(x => x.id === String(seed.id));
      if (row && row.job.owner === A && [5, 4].includes(row.job.phase)) values.investmentJournals[String(seed.id)] = await c.read(V, 'stage_investment_journal', [u64(seed.id)]);
    }
    const q = values.quote, q1000 = values.quoteFull; delete values.quote; delete values.quoteFull;
    return {ledger: c.latest.sequence, closeTime: Number(c.latest.closeTime), account: c.account, executableAndCodePinsVerified: true,
      values: {...values, ...c.raw}, ...(quoteAmount ? {quote: q, quoteAmountRaw: String(quoteAmount)} : {}), ...(seeds.length === 0 ? {quoteFull: q1000} : {})};
  }
  // (c) Pre-activation reads, one ledger. Nothing here can create, sign or send.
  async function preActivation() {
    if (rpc.alignToNewLedger) await rpc.alignToNewLedger(); await network(); const latest = await rpc.getLatestLedger();
    const accountKey = S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: new S.Address(A).toScAddress().accountId()}));
    const got = await entries([instanceKey(R), instanceKey(V), accountKey], latest.sequence), map = new Map(got.map(r => [r.key.toXDR('base64'), r]));
    const inst = id => {const row = map.get(instanceKey(id).toXDR('base64')); assert(row, 'instance absent/archived'); return row.val.contractData().val().instance();};
    for (const [id, hash] of [[R, PAIR.receipt], [V, PAIR.vault]]) eq(inst(id).executable().wasmHash().toString('hex'), hash, 'executable differs');
    const receiptKeys = (inst(R).storage() || []).map(e => {const k = S.scValToNative(e.key()); return Array.isArray(k) ? String(k[0]) : String(k);});
    const a = map.get(accountKey.toXDR('base64')).val.account(), account = {sequence: a.seqNum().toString(), balance: a.balance().toString()};
    const read = async (contract, method, args = []) => {
      const sim = await rpc.simulateTransaction(tx(account, latest, new S.Contract(contract).call(method, ...args)));
      sameLedger(sim, latest.sequence); assert(S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble && sim.result, `view ${method} failed or needs restore`);
      return plain(S.scValToNative(sim.result.retval));
    };
    const funding = deployment.dependencies.fundingToken.contract, other = deployment.dependencies.otherToken.contract;
    const labels = {receiptActive: [R, 'stage_is_active'], receiptPaused: [R, 'stage_deposits_paused'], vaultPaused: [V, 'stage_deposits_paused'],
      vaultSupply: [V, 'total_supply'], positionLiquidity: [V, 'get_position_liquidity'], reserved: [V, 'stage_reserved'], boundReceipt: [V, 'stage_receipt'],
      keeper: [V, 'stage_investment_keeper'], vaultGuardian: [V, 'stage_guardian']};
    const values = Object.fromEntries(await Promise.all(Object.entries(labels).map(async ([k, [c, m]]) => [k, await read(c, m)])));
    for (const [k, token, owner] of [['custodyShares', V, R], ['vaultFundingBalance', funding, V], ['vaultOtherBalance', other, V], ['ownerFundingBalance', funding, A]]) values[k] = await read(token, 'balance', [addr(owner)]);
    values.quoteFull = await read(V, 'stage_quote_open', [S.nativeToScVal(u(PARAMETERS.capRaw), {type: 'u128'}), S.nativeToScVal(PARAMETERS.toleranceBps, {type: 'u32'}), S.nativeToScVal(PARAMETERS.reserveMarginBps, {type: 'u32'})]);
    assert.equal((await rpc.getLatestLedger()).sequence, latest.sequence, 'read/quote crossed ledger; prepare nothing');
    const quoteFull = values.quoteFull; delete values.quoteFull; // same shape as snapshot(): the quote sits beside `values`
    return {ledger: latest.sequence, closeTime: Number(latest.closeTime), account, receiptInstanceKeys: receiptKeys, values: {...values, quoteFull}, quoteFull};
  }
  return withRetry({now: () => Math.floor(Date.now() / 1000), preActivation,
    inspect: async seeds => {if (rpc.alignToNewLedger) await rpc.alignToNewLedger(); const c = await context(false); const s = await snapshot(c, seeds, null); await c.fresh(); return s;},
    prepare: async ({step, sequence, seeds, plan: seedPlan, deadline}) => {
      if (rpc.alignToNewLedger) await rpc.alignToNewLedger(); // each (retried) attempt starts right after a ledger closes
      const c = await context(true); eq(u(c.account.sequence) + 1n, sequence, 'Ledger source sequence changed');
      const before = await snapshot(c, seeds, isSeed(step) ? SEED_PLANS[seedPlan][seeds.length] : null);
      let seed, gate;
      if (isSeed(step)) {seed = seedFrom(input, config, before, deadline, seeds, seedPlan); gate = requireGate(input, before, seeds, `before-seed-${seeds.length + 1}`);}
      else {gate = pauseGate(input, config, before, seeds, step, seedPlan);
        assert.equal(before.values.vaultPaused, step === 'pause-receipt'); assert.equal(before.values.receiptPaused, false); assert.equal(deadline, undefined);}
      const unsigned = tx(c.account, c.latest, operation(input, step, seed)); const sim = await rpc.simulateTransaction(unsigned);
      sameLedger(sim, c.latest.sequence); assert(S.rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble && sim.result, 'financial simulation failed or needs restore');
      assert(sim.result.auth.every(a => a.credentials().switch().name === 'sorobanCredentialsSourceAccount'));
      assert.deepEqual(sim.result.auth.map(a => a.rootInvocation().toXDR('base64')), authRoots(input, step, seed));
      if (isSeed(step)) eq(S.scValToNative(sim.result.retval), seed.id); else assert.equal(sim.result.retval.switch().name, 'scvVoid');
      const assembled = S.rpc.assembleTransaction(unsigned, sim).build(); await c.fresh();
      return plain({step, ...(seed ? {seed} : {}), gate, before, unsignedXdr: assembled.toXDR(), hash: assembled.hash().toString('hex'), sequence: assembled.sequence,
        ledger: c.latest.sequence, closeTime: Number(c.latest.closeTime), maxFeeStroops: assembled.fee, balanceStroops: c.account.balance,
        sellingLiabilities: c.account.selling, minimumReserveStroops: c.minimumReserve, simulationResourceFeeStroops: sim.minResourceFee, networkMemoryAndEventCapsVerified: true});
    },
    transaction: async row => {
      await network(); const result = await rpc.getTransaction(row.hash); if (!['SUCCESS', 'FAILED'].includes(result.status)) return {status: 'UNKNOWN'};
      assert(result.envelopeXdr && result.resultXdr); const tx = S.TransactionBuilder.fromXDR(result.envelopeXdr.toXDR('base64'), plan.network);
      assert.equal(tx.hash().toString('hex'), row.hash); assert.equal(result.resultXdr.result().switch().name, result.status === 'SUCCESS' ? 'txSuccess' : 'txFailed');
      return {status: result.status, hash: row.hash, source: tx.source, sequence: tx.sequence, ledger: result.ledger, feeCharged: result.resultXdr.feeCharged().toString()};
    },
  });
}
