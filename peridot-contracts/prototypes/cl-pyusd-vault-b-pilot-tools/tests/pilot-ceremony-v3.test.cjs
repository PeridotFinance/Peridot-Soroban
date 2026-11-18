'use strict';
const J = require('./support/pre-activation-journal.cjs');
// Offline tests for pilot-ceremony-v3.cjs (adapted from the v2 suite; v3-specific tests at the end). Public identifiers only; no key is generated, read or used.
// "Approved"/"reviewed" status strings below exist only inside in-memory or temp-directory TEST fixtures.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const P = require('../pilot-ceremony-v3.cjs'), D = require('../operator-deployment-v3.cjs');
const ROOT = path.resolve(__dirname, '../../..'), BASE = 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/', REL = 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/';
const FULL_CONFIG = REL + 'activate/' + fs.readdirSync(path.join(ROOT, REL, 'activate')).filter(f => /^deploy-config-full-release-.*\.UNREVIEWED\.json$/.test(f)).sort()[0];
const read = file => JSON.parse(fs.readFileSync(path.join(ROOT, file)));
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const clone = value => structuredClone(value);
const STRANGER = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 77));
const floor = x => String(BigInt(x) * 9980n / 10000n);
const isSeed = step => step.startsWith('seed-');
function quoteFor(amount, ledger, closeTime, ok = true) {
  const half = String(BigInt(amount) / 2n);
  return {valid: ok, admissible: ok, oracle_fresh: true, deposits_paused: false, composition_dirty: false, in_band: true, guard_ready: false, tick: 0, ledger, timestamp: String(closeTime),
    swap_in: half, swap_out: half, min_swap: floor(half), shares: String(amount), min_shares: floor(amount), invest0: half, invest1: half,
    min_reserve: '100000000', reserve: '150000000', vault_slippage_bps: 100, vault_divergence_bps: 200};
}
function fixture(t) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'unsigned-pilot-v3-'))); t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  const plan = read(REL + 'deploy/mainnet-deployment-plan-v3-seed800.json'), deployment = read(FULL_CONFIG);
  // keeper = the bound Mainnet keeper from the FULL config
  const specs = {};
  for (const role of ['vault', 'receipt']) specs[role] = S.contract.Spec.fromWasm(fs.readFileSync(path.join(ROOT, plan.candidateArtifacts.find(a => a.name === role).path)));
  specs.controller = S.contract.Spec.fromWasm(fs.readFileSync(path.join(ROOT, deployment.controllerArtifact.path)));
  const input = {plan, deployment, specs, deploymentJournalSha256: 'd'.repeat(64)};
  const config = {status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3', helperSha256: sha(fs.readFileSync(path.join(__dirname, '../pilot-ceremony-v3.cjs'))), runtimePins: D.runtimePins(),
    planDigestSha256: D.digest(plan), source: plan.admin, keeperPublicAddress: deployment.keeperPublicAddress, initialSourceSequence: '9',
    parameters: JSON.parse(JSON.stringify(P.PARAMETERS)), approvedPauses: [...P.PAUSES],
    protocolVersion: 29, costModelXdrSha256: deployment.costModelXdrSha256, rpcUrl: 'https://rpc.invalid', expiresAt: 5000,
    feeBudgetStroops: '100000', maxTransactionFeeStroops: '10000', inclusionFeeStroops: '1000', inclusionFeeCapStroops: '1000', balanceFloorStroops: '10000000', timeoutSeconds: 180};
  const A = plan.admin, V = plan.proposedInstances.vault.contract, R = plan.proposedInstances.receipt.contract;
  const zero = Object.fromEntries(['next_id', 'total_units', 'job_units', 'pending_funding', 'pending_other', 'reserve_liability', 'exit_funding', 'exit_other', 'principal_backing', 'pending_principal'].map(k => [k, '0']));
  zero.open_jobs = 0;
  const common = {vaultAdmin: A, receiptAdmin: A, controllerAdmin: A, controllerGuardian: A, vaultGuardian: A, keeper: config.keeperPublicAddress, boundReceipt: R,
    fundingAsset: deployment.dependencies.fundingToken.contract, receiptFundingAsset: deployment.dependencies.fundingToken.contract,
    receiptConfig: {version: 1, vault: V, guardian: A, backing_cap: '8000000000'},
    vaultConfig: {pool: deployment.dependencies.pool.contract, oracle: deployment.dependencies.oracle.contract, token0: deployment.dependencies.fundingToken.contract,
      token1: deployment.dependencies.otherToken.contract, dec0: 7, dec1: 7, underlying_index: 0, tick_lower: -20, tick_upper: 20},
    vaultParams: {max_deploy: '10000000000', paused: false, slippage_bps: 100, max_pool_divergence_bps: 200, oracle_max_age_mult: 2, nav_root_max_age: 300, nav_root_max_stale: 3600, harvest_cooldown: '60'},
    rewardToken: deployment.aquaToken, receiptController: plan.controller, active: true, marketSupported: true, controllerBorrowPaused: true,
    controllerDepositPaused: false, controllerRedeemPaused: false, cashBorrowing: false, collateralFactor: 0, borrowed: '0', minCashReserve: 10000,
    receiptFloor: '10000000', receiptSupply: '10000000', receiptOwnFloor: '10000000', receiptUnderlying: '10000000', receiptFundingBalance: '10000000',
    ownerPToken: '0', ownerDirectVaultShares: '0', exchangeRate: '1000000', vaultFundingBalance: '0', vaultOtherBalance: '0', ownerOtherBalance: '177062510'};
  // The simulated chain. `phases[i]` is the on-chain phase of the operator's i-th seed; `mutate` injects anything else.
  const world = {ledger: 100, phases: [], pauseV: false, pauseR: false, quoteFull: true, quotePart: true, mutate: null,
    account: {sequence: '9', balance: '5000000000', selling: '0', reserveUnits: 4},
    trustlines: {funding: {code: 'PYUSD', present: true, authorized: true, balance: '11000000000', limit: '9223372036854775807'},
      other: {code: 'USDC', present: true, authorized: true, balance: '177062510', limit: '9223372036854775807'},
      reward: {code: 'AQUA', present: true, authorized: true, balance: '0', limit: '9223372036854775807'}}};
  function chain(seeds, phases, quoteAmount) {
    const closeTime = 1000 + (world.ledger - 100) * 5, z = clone(common), st = {...zero, next_id: String(phases.length)};
    let wallet = 11000000000n, liquidity = 0n, total = 0n; z.jobs = []; z.jobsTruncated = false; z.investmentJournals = {}; z.commands = []; z.reserved = ['0', '0']; z.locked = '0';
    phases.forEach((phase, i) => {
      const seed = seeds[i], minted = [3, 5, 4].includes(phase), final = phase === 4, invested = [5, 4].includes(phase), shares = minted ? seed.principalRaw : '0';
      const rest = String(BigInt(seed.principalRaw) - BigInt(seed.quote.swap_in));
      const job = {owner: A, kind: 0, phase, deadline: seed.deadline, a: invested ? '0' : phase === 1 ? seed.principalRaw : rest, b: invested || phase === 1 ? '0' : seed.quote.swap_out,
        reserve: final ? '0' : seed.quote.reserve, swap_in: seed.quote.swap_in, min_swap: seed.quote.min_swap, min_shares: seed.quote.min_shares, shares};
      z.jobs.push({id: String(i), job}); wallet -= BigInt(seed.principalRaw) + BigInt(seed.quote.reserve); total += BigInt(shares);
      st.principal_backing = String(BigInt(st.principal_backing) + BigInt(seed.principalRaw));
      if (invested) {z.investmentJournals[String(i)] = {owner: A, shares, original_reserve: seed.quote.reserve, cost: '50000000', refund: '100000000', settled: final,
        liquidity_before: String(liquidity), liquidity_added: '500000000', liquidity_after: String(liquidity + 500000000n), execution_ledger: world.ledger - 1}; liquidity += 500000000n;}
      if (final) {wallet += 100000000n; return;}
      Object.assign(st, {job_units: shares, pending_funding: minted ? '0' : job.a, pending_other: phase === 2 ? job.b : '0', reserve_liability: seed.quote.reserve,
        pending_principal: minted ? '0' : seed.principalRaw, open_jobs: 1});
      z.commands = [{id: String(i), job: clone(job)}]; z.locked = shares;
      z.reserved = [invested ? '100000000' : String((minted ? 0n : BigInt(job.a)) + BigInt(seed.quote.reserve)), phase === 2 ? job.b : '0'];
    });
    st.total_units = String(total); z.state = st; z.vaultPaused = world.pauseV; z.receiptPaused = world.pauseR;
    for (const key of ['vaultSupply', 'custodyShares', 'beneficial']) z[key] = String(total);
    z.units = z.available = String(total - BigInt(z.locked)); z.positionLiquidity = String(liquidity); z.ownerFundingBalance = String(wallet);
    z.trustlines = clone(world.trustlines);
    const s = {ledger: world.ledger, closeTime, executableAndCodePinsVerified: true, account: clone(world.account), values: z};
    if (quoteAmount) {s.quote = quoteFor(quoteAmount, world.ledger, closeTime, quoteAmount === '8000000000' ? world.quoteFull : world.quotePart); s.quoteAmountRaw = quoteAmount;}
    if (seeds.length === 0) s.quoteFull = quoteFor('8000000000', world.ledger, closeTime, world.quoteFull);
    if (world.mutate) world.mutate(s);
    return s;
  }
  const snapshot = (seeds, quoteAmount = null) => chain(seeds, world.phases.slice(0, seeds.length), quoteAmount);
  function build(step, sequence, seed, gate, before, cpu = 10000000) {
    const op = P.operation(input, step, seed);
    op.body().invokeHostFunctionOp().auth(P.authRoots(input, step, seed).map(root => new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(), rootInvocation: S.xdr.SorobanAuthorizedInvocation.fromXDR(root, 'base64')})));
    const data = new S.SorobanDataBuilder().setResources(cpu, 0, 1000).setResourceFee('500').build();
    const tx = new S.TransactionBuilder(new S.Account(A, String(BigInt(sequence) - 1n)), {fee: '1000', networkPassphrase: plan.network})
      .addOperation(op).setSorobanData(data).setLedgerbounds(before.ledger + 1, before.ledger + 20).setTimebounds(before.closeTime, before.closeTime + 180).build();
    return JSON.parse(JSON.stringify({step, ...(seed ? {seed} : {}), gate, before, unsignedXdr: tx.toXDR(), hash: tx.hash().toString('hex'), sequence, ledger: before.ledger, closeTime: before.closeTime,
      maxFeeStroops: tx.fee, balanceStroops: '5000000000', sellingLiabilities: '0', minimumReserveStroops: '10000000', simulationResourceFeeStroops: '500', networkMemoryAndEventCapsVerified: true}));
  }
  // Mirrors the production adapter's order of checks, on the simulated chain.
  function fakePrepare({step, sequence, seeds, plan: seedPlan, deadline}, cpu) {
    const before = snapshot(seeds, isSeed(step) ? P.SEED_PLANS[seedPlan][seeds.length] : null); let seed, gate;
    if (isSeed(step)) {seed = P.seedFrom(input, config, before, deadline, seeds, seedPlan); gate = P.requireGate(input, before, seeds, `before-seed-${seeds.length + 1}`);}
    else gate = P.pauseGate(input, config, before, seeds, step, seedPlan);
    return build(step, sequence, seed, gate, before, cpu);
  }
  let count = 0;
  const deps = {now: () => 1001, prepare: async request => {count++; return fakePrepare(request);},
    transaction: async row => ({status: 'SUCCESS', hash: row.hash, source: A, sequence: row.sequence, ledger: row.ledger + 1, feeCharged: '1200'}),
    inspect: async seeds => snapshot(seeds)};
  const directory = path.join(dir, 'journal');
  const f = {dir, directory, input, config, world, snapshot, chain, build, fakePrepare, deps, count: () => count, A, V, R,
    prepare: (options = {deadline: String(1000 + (world.ledger - 100) * 5 + 800)}) => P.prepare(input, config, directory, options, deps),
    pause: () => P.prepare(input, config, directory, {}, deps),
    reconcile: () => P.reconcile(input, config, directory, deps),
    exists: () => fs.existsSync(path.join(directory, 'journal.json')),
    journal: () => JSON.parse(fs.readFileSync(path.join(directory, 'journal.json'))),
    // Apply the outstanding seed and let the keeper finish it: on-chain phase 4.
    async seedThroughFinalised(options) {const r = await f.prepare(options); world.ledger += 2; world.phases.push(4); await f.reconcile(); return r;}};
  return f;
}
const stranger = (phase = 1, a = '500000000') => ({owner: STRANGER, kind: 0, phase, deadline: '1900', a, b: '0', reserve: '7500000', swap_in: '250000000', min_swap: '249500000', min_shares: '499000000', shares: '0'});
// A stranger's command with every state consequence the receipt would show for it.
function addForeign(s, job = stranger(), funded = true) {
  const st = s.values.state, id = String(st.next_id); s.values.jobs.push({id, job}); st.next_id = String(BigInt(st.next_id) + 1n);
  if (funded) {st.open_jobs = Number(st.open_jobs) + 1; st.pending_principal = job.a; st.pending_funding = job.a; st.reserve_liability = job.reserve;
    st.principal_backing = String(BigInt(st.principal_backing) + BigInt(job.a)); s.values.reserved = [String(BigInt(job.a) + BigInt(job.reserve)), '0'];}
}
const refusal = (code, check) => error => {assert(error instanceof P.Refusal, 'expected a Refusal, got: ' + error.message); assert.equal(error.code, code); if (check) check(error.report, error); return true;};

test('seed keeps the full principal, adds the reserve on top and requires exact 20 bps / +50% arithmetic and a deadline', t => {
  const f = fixture(t), before = f.snapshot([], '8000000000'), seed = P.seedFrom(f.input, f.config, before, '1800', [], 'single-800');
  assert.equal(seed.principalRaw, '8000000000'); assert.equal(seed.id, '0'); assert.equal(seed.quote.reserve, '150000000');
  for (const mutate of [s => s.values.ownerFundingBalance = '8149999999', s => s.quote.ledger++, s => s.quote.oracle_fresh = false, s => s.values.vaultPaused = true,
    s => s.values.receiptPaused = true, s => s.quoteAmountRaw = '4000000000', s => s.values.vaultFundingBalance = '1']) {
    const s = clone(before); mutate(s); assert.throws(() => P.seedFrom(f.input, f.config, s, '1800', [], 'single-800'));
  }
  for (const deadline of [undefined, 999, 1100, 4601]) assert.throws(() => P.seedFrom(f.input, f.config, before, deadline, [], 'single-800'));
  const root = S.xdr.SorobanAuthorizedInvocation.fromXDR(P.authRoots(f.input, 'seed-1', seed)[0], 'base64');
  assert.equal(root.function().contractFn().functionName().toString(), 'stage_open');
  assert.equal(root.subInvocations()[0].function().contractFn().functionName().toString(), 'stage_accept_funds');
  const transfer = root.subInvocations()[0].subInvocations()[0].function().contractFn();
  assert.equal(transfer.functionName().toString(), 'transfer'); assert.equal(String(S.scValToNative(transfer.args()[2])), '8150000000');
});
test('NEGATIVE widened tolerance or changed reserve margin is rejected in config and in any quote', t => {
  const f = fixture(t); P.validateConfig(f.config, f.input, 1001);
  for (const change of [c => c.parameters.toleranceBps = 50, c => c.parameters.toleranceBps = 21, c => c.parameters.reserveMarginBps = 2500, c => c.parameters.reserveMarginBps = 10000,
    c => c.parameters.capRaw = '20000000000', c => c.parameters.seedPlans['split-4x250'] = ['2500000000'], c => c.parameters.seedPlans['split-2x400'] = ['4000000000', '6000000000'], c => delete c.parameters.seedPlans]) {
    const c = clone(f.config); change(c); assert.throws(() => P.validateConfig(c, f.input, 1001), /fixed; this tool never widens them/);
  }
  const before = f.snapshot([], '8000000000');
  const wide = clone(before); wide.quote.min_swap = String(BigInt(wide.quote.swap_out) * 9950n / 10000n); wide.quote.min_shares = String(BigInt(wide.quote.shares) * 9950n / 10000n);
  assert.throws(() => P.seedFrom(f.input, f.config, wide, '1800', [], 'single-800'), /min_swap is not the 20 bps floor/);
  const shares = clone(before); shares.quote.min_shares = String(BigInt(shares.quote.shares) * 9950n / 10000n);
  assert.throws(() => P.seedFrom(f.input, f.config, shares, '1800', [], 'single-800'), /min_shares is not the 20 bps floor/);
  for (const reserve of ['125000000', '100000000', '149999999', '200000000']) {
    const s = clone(before); s.quote.reserve = reserve; assert.throws(() => P.seedFrom(f.input, f.config, s, '1800', [], 'single-800'), /reserve is not minimum \+ 50%/);
  }
  assert.deepEqual(Object.keys(P.SEED_PLANS), ['single-800', 'split-2x400']); assert(Object.isFrozen(P.PARAMETERS) && Object.isFrozen(P.SEED_PLANS));
});
test('NEGATIVE foreign command before the seed: every variant refuses, prints what was found and journals nothing', async t => {
  const variants = {
    'stranger funded open': s => addForeign(s),
    'stranger cancelled command with no residue': s => addForeign(s, stranger(99, '0'), false),
    'stranger finished emergency exit': s => addForeign(s, {...stranger(22, '0'), kind: 2}, false),
    'operator command that is not a journaled seed': s => addForeign(s, {...stranger(), owner: s.values.vaultAdmin}),
    'liability without a readable command': s => {s.values.state.open_jobs = 1; s.values.state.pending_principal = '500000000';},
    'unfinished command listed for the operator': s => s.values.commands.push({id: '0', job: {...stranger(), owner: s.values.vaultAdmin}}),
    'shares of another beneficiary in custody': s => {s.values.vaultSupply = '500'; s.values.custodyShares = '500';},
    'more commands than can be enumerated': s => {s.values.state.next_id = '33'; s.values.jobsTruncated = true;},
  };
  for (const [name, mutate] of Object.entries(variants)) {
    const f = fixture(t); f.world.mutate = mutate;
    await assert.rejects(f.prepare(), error => {
      assert(error instanceof P.Refusal, name + ': ' + error.message); assert(['FOREIGN_COMMAND', 'OPEN_COMMAND_GATE'].includes(error.code), name);
      assert.equal(error.report.gate, 'before-seed-1'); assert.equal(error.report.ok, false); assert(error.report.findings.length > 0, name);
      assert(error.report.stageState && Array.isArray(error.report.allCommands) && Array.isArray(error.report.operatorUnfinishedCommands)); return true;
    });
    assert.equal(f.exists(), false, name + ': nothing may be journaled');
  }
  const f = fixture(t); f.world.mutate = s => addForeign(s);
  await assert.rejects(f.prepare(), refusal('FOREIGN_COMMAND', (report, error) => {
    assert.deepEqual(report.foreign, [{id: '0', owner: STRANGER, kind: 0, phase: 1, a: '500000000', b: '0', shares: '0', reserve: '7500000', deadline: '1900', ownerIsOperator: false}]);
    assert.match(error.message, /unexpected command id 0: owner G[A-Z0-9]+ \(NOT the operator\), kind 0, phase 1/); assert.match(error.message, /Stop; do not sign anything/);
  }));
  // The gate outranks the quote: with a foreign command present the split is not even discussed.
  f.world.quoteFull = false; await assert.rejects(f.prepare(), refusal('FOREIGN_COMMAND'));
  await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true}), refusal('FOREIGN_COMMAND')); assert.equal(f.exists(), false);
});
test('NEGATIVE foreign command before pause-vault and before pause-receipt refuses the pause; a mint after the pause is why', async t => {
  const f = fixture(t); await f.seedThroughFinalised();
  f.world.mutate = s => addForeign(s);
  await assert.rejects(f.pause(), refusal('FOREIGN_COMMAND', report => {assert.equal(report.gate, 'before-pause-vault'); assert.equal(report.foreign[0].owner, STRANGER);
    assert.equal(report.stageState.open_jobs, 2 - 1); assert.equal(report.stageState.pending_principal, '500000000');}));
  assert.equal(f.journal().entries.length, 1, 'no pause row may be journaled');
  f.world.mutate = null; const first = await f.pause(); assert.equal(first.step, 'pause-vault'); assert.equal(first.gate.gate, 'before-pause-vault'); assert.equal(first.gate.ok, true);
  f.world.ledger += 2; f.world.pauseV = true; await f.reconcile();
  // A command that was funded before the vault pause and is now visible: the receipt pause is refused too.
  f.world.mutate = s => addForeign(s, stranger(2));
  await assert.rejects(f.pause(), refusal('FOREIGN_COMMAND', report => assert.equal(report.gate, 'before-pause-receipt')));
  f.world.mutate = s => addForeign(s, {...stranger(4, '0'), shares: '499500000'}, false); // already minted to the stranger
  await assert.rejects(f.pause(), refusal('FOREIGN_COMMAND')); assert.equal(f.journal().entries.length, 2);
  // It also blocks the read-back of an applied pause instead of confirming it.
  f.world.mutate = null; await f.pause(); f.world.ledger += 2; f.world.pauseR = true; f.world.mutate = s => addForeign(s);
  await assert.rejects(f.reconcile(), refusal('FOREIGN_COMMAND', report => assert.equal(report.gate, 'after-pause-receipt')));
  assert.equal(f.journal().entries[2].status, 'APPLIED_PENDING_READBACK');
});
test('NEGATIVE cap not exactly filled by the operator principal refuses the pause', async t => {
  const f = fixture(t); await f.seedThroughFinalised();
  for (const mutate of [s => s.values.state.principal_backing = '9999999999', s => s.values.state.principal_backing = '10000000001']) {
    f.world.mutate = mutate;
    await assert.rejects(f.pause(), refusal('CAP_NOT_EXACTLY_FILLED', (report, error) => {
      assert.equal(report.cap.backing_cap, '8000000000'); assert.equal(report.cap.operatorPrincipal, '8000000000'); assert.match(error.message, /cap is not exactly filled by the operator's principal/);}));
  }
  assert.equal(f.journal().entries.length, 1);
  // Half the cap filled (the split after its first deposit) is not "filled": the gate itself says so.
  const g = fixture(t); g.world.quoteFull = false; await g.seedThroughFinalised({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true});
  const seeds = g.journal().entries.map(row => row.seed), s = g.snapshot(seeds);
  const report = P.openCommandGate(g.input, s, seeds, 'before-pause-vault'); assert.equal(report.ok, false);
  assert.deepEqual(report.findings, ["cap is not exactly filled by the operator's principal: cap 8000000000, principal_backing 4000000000, operator principal 4000000000"]);
  assert.throws(() => P.pauseGate(g.input, g.config, s, seeds, 'pause-vault', 'split-2x400'), refusal('CAP_NOT_EXACTLY_FILLED'));
  assert.equal(P.openCommandGate(g.input, s, seeds, 'before-seed-2').ok, true);
});
test('NEGATIVE pause is refused until the operator seed is finalised (phases 1, 2, 3 and 5)', async t => {
  for (const phase of [1, 2, 3, 5]) {
    const f = fixture(t); await f.prepare(); f.world.ledger += 2; f.world.phases.push(phase); await f.reconcile();
    await assert.rejects(f.pause(), refusal('OPEN_COMMAND_GATE', (report, error) => assert.match(error.message, new RegExp(`operator seed id 0 is in phase ${phase}, not finalised`))));
    assert.equal(f.journal().entries.length, 1);
  }
});
test('NEGATIVE inadmissible 800 PYUSD quote: only the 2x400 split is offered, nothing else, nothing journaled', async t => {
  const f = fixture(t); f.world.quoteFull = false;
  await assert.rejects(f.prepare(), refusal('SEED_800_NOT_OFFERED_ONLY_SPLIT', (report, error) => {
    assert.equal(report.requestedPlan, 'single-800'); assert.equal(report.offeredPlan, 'split-2x400'); assert.deepEqual(report.fixed, {toleranceBps: 20, reserveMarginBps: 5000});
    assert.equal(report.quoteFull.valid, false); assert.match(error.message, /only alternative is two 400 PYUSD deposits\. Tolerance and reserve margin cannot be changed/);}));
  assert.equal(f.exists(), false);
  for (const seedPlan of ['split-4x250', 'single-500', 'split-2x400-wide', '']) await assert.rejects(f.prepare({deadline: '1800', seedPlan}), /seed plan must be single-800 or split-2x400/);
  for (const partial of [{valid: true, admissible: false}, {valid: false, admissible: true}]) {
    const s = f.snapshot([], '8000000000'); Object.assign(s.quoteFull, partial); assert.equal(P.seedPlanDecision(s).available, 'split-2x400');
  }
  const offered = await f.prepare({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true});
  assert.equal(offered.step, 'seed-1'); assert.equal(offered.seedPlan, 'split-2x400'); assert.equal(offered.seed.principalRaw, '4000000000'); assert.equal(offered.seed.quote.min_swap, floor('2000000000'));
  const journal = f.journal(); assert.equal(journal.seedPlan, 'split-2x400'); assert.equal(journal.splitPrecondition.quoteFull.valid, false); assert.equal(journal.splitPrecondition.ledger, 100);
  // The journaled precondition cannot be rewritten into an admissible one, and the plan cannot be switched afterwards.
  journal.splitPrecondition.quoteFull.valid = true; journal.splitPrecondition.quoteFull.admissible = true; fs.writeFileSync(path.join(f.directory, 'journal.json'), JSON.stringify(journal));
  assert.throws(() => P.loadJournal(f.input, f.config, f.directory), /split requires the journaled 800 PYUSD quote that was inadmissible or did not fit the wallet|journaled split precondition differs from the journaled snapshot/);
});
test('NEGATIVE split is not offered while the 800 quote is admissible and fits; an inadmissible 400 quote has no further alternative', async t => {
  const f = fixture(t);
  await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true}), refusal('SPLIT_NOT_OFFERED_800_ADMISSIBLE', report => assert.equal(report.offeredPlan, 'single-800')));
  f.world.quoteFull = false; f.world.quotePart = false;
  await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true}), refusal('SEED_QUOTE_INADMISSIBLE_NO_FURTHER_ALTERNATIVE', (report, error) => assert.match(error.message, /no other amount, tolerance or margin is offered/)));
  assert.equal(f.exists(), false);
});
test('split path end to end: second 400 only after the first finalised and the gate passes again; then both pauses', async t => {
  const f = fixture(t); f.world.quoteFull = false;
  await f.prepare({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true}); f.world.ledger += 2;
  for (const phase of [1, 2, 3, 5]) { // first deposit applied but not finalised: reconcile confirms it, the second is refused
    f.world.phases = [phase];
    if (f.journal().active) assert.equal((await f.reconcile()).progress.phase, phase);
    await assert.rejects(f.prepare({deadline: '1900'}), refusal('OPEN_COMMAND_GATE', (report, error) => {assert.equal(report.gate, 'before-seed-2'); assert.match(error.message, /not finalised/);}));
  }
  f.world.phases = [4]; f.world.ledger += 2;
  await assert.rejects(f.prepare({deadline: '1900', seedPlan: 'single-800'}), /seed plan was fixed by the first journaled seed/);
  await assert.rejects(f.pause(), /explicit command deadline required/); // the next step is the second seed, never a pause
  f.world.quotePart = false; await assert.rejects(f.prepare({deadline: '1900'}), refusal('SEED_QUOTE_INADMISSIBLE_NO_FURTHER_ALTERNATIVE')); f.world.quotePart = true;
  const second = await f.prepare({deadline: '1900'}); assert.equal(second.step, 'seed-2'); assert.equal(second.seed.id, '1'); assert.equal(second.seed.principalRaw, '4000000000');
  assert.equal(second.seed.quoteLedger, f.world.ledger, 'second deposit carries its own fresh quote'); assert.equal(second.seed.walletBefore.funding, '6950000000'); assert.equal(second.sequence, '11');
  f.world.ledger += 2; f.world.phases = [4, 5]; assert.equal((await f.reconcile()).progress.phase, 5);
  await assert.rejects(f.pause(), refusal('OPEN_COMMAND_GATE'));
  f.world.phases = [4, 4]; f.world.ledger += 2;
  for (const step of P.PAUSES) {
    const row = await f.pause(); assert.equal(row.step, step); assert.equal(row.gate.cap.operatorPrincipal, '8000000000'); assert.equal(row.gate.cap.principal_backing, '8000000000');
    f.world.ledger += 2; if (step === 'pause-vault') f.world.pauseV = true; else f.world.pauseR = true;
    const done = await f.reconcile(); assert.equal(done.progress.operatorSharesRaw, '8000000000'); assert.equal(done.progress.gateOk, true);
  }
  const journal = f.journal(); assert.deepEqual(journal.entries.map(row => row.step), ['seed-1', 'seed-2', 'pause-vault', 'pause-receipt']);
  assert.equal(journal.active, null); assert.equal(journal.feesCharged, '4800'); await assert.rejects(f.pause(), /already complete/);
});
test('NEGATIVE split stuck state: a stranger taking the freed half blocks the second deposit and the pause; no override exists', async t => {
  const f = fixture(t); f.world.quoteFull = false; await f.seedThroughFinalised({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true});
  f.world.mutate = s => addForeign(s, stranger(1, '4000000000'));
  await assert.rejects(f.prepare({deadline: '1900'}), refusal('FOREIGN_COMMAND', report => assert.equal(report.gate, 'before-seed-2')));
  await assert.rejects(f.pause(), /explicit command deadline required/);
  const seeds = f.journal().entries.map(row => row.seed);
  assert.throws(() => P.pauseGate(f.input, f.config, f.snapshot(seeds), seeds, 'pause-vault', 'split-2x400'), refusal('FOREIGN_COMMAND'));
  assert.equal(f.journal().entries.length, 1); assert.deepEqual(Object.keys(P).filter(name => /override|force|skip/i.test(name)), []);
});
test('single path end to end: seed, keeper finalises, pause-vault, pause-receipt, each separately confirmed', async t => {
  const f = fixture(t); const seed = await f.prepare(); assert.equal(seed.step, 'seed-1'); assert.equal(seed.seedPlan, 'single-800'); assert.equal(seed.signing, false); assert.equal(seed.submission, false);
  assert.equal(seed.gate.gate, 'before-seed-1'); assert.equal(seed.gate.ok, true); assert.equal(seed.gate.stageState.next_id, '0');
  f.world.ledger += 2;
  for (const phase of [1, 2, 3, 5, 4]) {f.world.phases = [phase]; if (f.journal().active) assert.equal((await f.reconcile()).progress.phase, phase);
    else assert.equal(P.validateLifecycle(f.input, f.config, f.snapshot([f.journal().entries[0].seed]), [f.journal().entries[0].seed]).phase, phase);}
  const five = (f.world.phases = [5], f.snapshot([f.journal().entries[0].seed])); assert.equal(five.values.reserved[0], '100000000'); assert.equal(five.values.state.reserve_liability, '150000000');
  five.values.reserved[0] = '150000000'; assert.throws(() => P.validateLifecycle(f.input, f.config, five, [f.journal().entries[0].seed]));
  f.world.phases = [4];
  for (const step of P.PAUSES) {
    const row = await f.pause(); assert.equal(row.step, step); f.world.ledger += 2; if (step === 'pause-vault') f.world.pauseV = true; else f.world.pauseR = true; await f.reconcile();
  }
  assert.equal(f.journal().entries.length, 3); assert.equal(f.journal().active, null); assert.equal(f.journal().feesCharged, '3600');
  await assert.rejects(f.pause(), /already complete/); await assert.rejects(f.prepare(), /already complete/);
});
test('wrong pair roles, keeper, floor, cap, cash balance, borrow state and lifecycle ownership fail closed', async t => {
  const f = fixture(t); await f.prepare(); f.world.ledger += 2; f.world.phases = [5]; const seeds = [f.journal().entries[0].seed];
  for (const mutate of [s => s.values.keeper = f.A, s => s.values.boundReceipt = f.input.plan.controller, s => s.values.receiptFloor = '1', s => s.values.receiptConfig.backing_cap = '1',
    s => s.values.receiptFundingBalance = '0', s => s.values.cashBorrowing = true, s => s.values.ownerDirectVaultShares = '1', s => s.values.jobs[0].job.owner = f.config.keeperPublicAddress,
    s => s.values.custodyShares = '1', s => s.values.state.job_units = '1', s => s.values.investmentJournals['0'].settled = true, s => s.values.ownerFundingBalance = '11000000000',
    s => s.values.jobs[0].job.kind = 1, s => s.values.jobs[0].job.phase = 99, s => s.values.jobs[0].job.phase = 21, s => delete s.values.jobs]) {
    const s = f.snapshot(seeds); mutate(s); assert.throws(() => P.validateLifecycle(f.input, f.config, s, seeds));
  }
});
test('NEGATIVE host-preflight stop 97,000,000 (user rule 15 Nov); AT-LIMIT >= 98M; 100M declared ceiling; no upload allowance; nothing journaled', async t => {
  const f = fixture(t), request = {step: 'seed-1', sequence: '10', seeds: [], plan: 'single-800', deadline: '1800'};
  P.validatePrepared(f.input, f.config, f.fakePrepare(request, 97000000), '10', '0', {plan: 'single-800', seeds: []});
  for (const [cpu, label] of [[97000001, 'ABOVE_97M_STOP'], [98000000, 'AT-LIMIT'], [99932089, 'AT-LIMIT'], [100000000, 'AT-LIMIT']]) {
    const row = f.fakePrepare(request, cpu), original = row.unsignedXdr;
    assert.throws(() => P.validatePrepared(f.input, f.config, row, '10', '0', {plan: 'single-800', seeds: []}),
      refusal('HOST_PREFLIGHT_ABOVE_97M', (report, error) => {assert.equal(report.label, label); assert.match(error.message, new RegExp('^' + label));})); assert.equal(row.unsignedXdr, original);
  }
  for (const cpu of [100000001, 104870950, 110000000]) assert.throws(() => P.validatePrepared(f.input, f.config, f.fakePrepare(request, cpu), '10', '0', {plan: 'single-800', seeds: []}), /exceeds 100000000; the upload allowance never applies/);
  assert.equal(P.cpuLabel(97999999), 'ABOVE_97M_STOP'); assert.equal(P.cpuLabel(98000000), 'AT-LIMIT'); assert.equal(P.cpuLabel(96999999), 'WITHIN_POLICY');
  f.deps.prepare = async r => f.fakePrepare(r, 97000001); await assert.rejects(f.prepare(), refusal('HOST_PREFLIGHT_ABOVE_97M')); assert.equal(f.exists(), false);
  f.deps.prepare = async r => f.fakePrepare(r); const seed = await f.prepare(); assert.equal(seed.cpuLabel, 'WITHIN_POLICY'); assert.equal(seed.declaredInstructions, 10000000);
  assert.equal(P.INSTRUCTION_LIMIT, 100000000); assert.equal(P.HOST_STOP, 97000000); assert.equal(P.AT_LIMIT, 98000000); assert.equal(D.LIMITS.instructions, 100000000);
});
test('preparation refuses expired review, altered quote, wrong order, different source sequence and fee shortage', async t => {
  const f = fixture(t), request = {step: 'seed-1', sequence: '10', seeds: [], plan: 'single-800', deadline: '1800'}, ctx = {plan: 'single-800', seeds: []};
  assert.throws(() => P.validateConfig({...f.config, expiresAt: 1000}, f.input, 1001), /expired/);
  assert.throws(() => P.validateConfig({...f.config, status: 'DRAFT_NOT_APPROVED_UNSIGNED_ONLY'}, f.input, 1001));
  assert.throws(() => P.validateConfig({...f.config, keeperPublicAddress: null}, f.input, 1001));
  for (const change of [r => r.seed.quote.min_shares = '1', r => r.sequence = '12', r => r.maxFeeStroops = '0', r => r.balanceStroops = '1', r => r.networkMemoryAndEventCapsVerified = false,
    r => r.gate.findings.push('edited'), r => delete r.gate, r => r.step = 'pause-vault', r => r.before.values.jobs.push({id: '0', job: stranger()})]) {
    const row = f.fakePrepare(request); change(row); assert.throws(() => P.validatePrepared(f.input, f.config, row, '10', '0', ctx));
  }
  assert.throws(() => P.validatePrepared(f.input, f.config, f.fakePrepare(request), '10', '99999', ctx));
});
test('durable unknown hash blocks re-preparation; applied result is durable before read-back; failure halts', async t => {
  const f = fixture(t); const result = await f.prepare(); const original = f.deps.transaction;
  f.deps.transaction = async () => ({status: 'UNKNOWN'});
  assert.equal((await f.reconcile()).status, 'UNKNOWN_NO_RETRY'); await assert.rejects(f.prepare(), /reconcile existing hash/); assert.equal(f.count(), 1);
  assert.equal(f.journal().active, 'seed-1'); assert.equal(f.journal().entries[0].hash, result.hash);
  let polls = 0; f.deps.transaction = async row => {polls++; return original(row);}; f.deps.inspect = async () => {throw Error('temporary read unavailable');};
  await assert.rejects(f.reconcile(), /unavailable/); assert.equal(f.journal().entries[0].status, 'APPLIED_PENDING_READBACK');
  f.deps.now = () => 99999; f.world.ledger += 2; f.world.phases = [5]; f.deps.inspect = async seeds => f.snapshot(seeds);
  assert.equal((await f.reconcile()).progress.phase, 5); assert.equal(polls, 1); assert.equal(f.journal().feesCharged, '1200');
  const g = fixture(t); await g.prepare(); const old = g.deps.transaction; g.deps.transaction = async row => ({...await old(row), status: 'FAILED'});
  await assert.rejects(g.reconcile(), /failed/); assert.equal(g.journal().halted, true); await assert.rejects(g.prepare(), /failed/);
  const journal = g.journal(); journal.entries[0].seed.principalRaw = '999'; fs.writeFileSync(path.join(g.directory, 'journal.json'), JSON.stringify(journal));
  assert.throws(() => P.loadJournal(g.input, g.config, g.directory));
});
test('inspect is read-only and prints the gate report instead of throwing; the pilot journal is bound to the completed deployment journal', async t => {
  const f = fixture(t); let view = await P.inspect(f.input, f.config, f.directory, f.deps);
  assert.equal(view.status, 'READ_ONLY'); assert.equal(view.nextStep, 'seed-1'); assert.equal(view.gate.ok, true); assert.equal(view.seedPlanDecision.available, 'single-800'); assert.equal(f.exists(), false);
  f.world.quoteFull = false; assert.equal((await P.inspect(f.input, f.config, f.directory, f.deps)).seedPlanDecision.available, 'split-2x400'); f.world.quoteFull = true;
  f.world.mutate = s => addForeign(s); view = await P.inspect(f.input, f.config, f.directory, f.deps);
  assert.equal(view.gate.ok, false); assert.equal(view.gate.foreign[0].owner, STRANGER); assert.equal(view.signing, false); assert.equal(view.submission, false);
  f.world.mutate = null; await f.seedThroughFinalised(); view = await P.inspect(f.input, f.config, f.directory, f.deps);
  assert.equal(view.nextStep, 'pause-vault'); assert.equal(view.gate.gate, 'before-pause-vault'); assert.equal(view.gate.ok, true); assert.equal(view.progress.finalised, true);
  assert.equal(f.journal().deploymentJournalSha256, 'd'.repeat(64));
  assert.throws(() => P.loadJournal({...f.input, deploymentJournalSha256: 'e'.repeat(64)}, f.config, f.directory), /completed deployment journal changed/);
  assert.throws(() => P.loadJournal({...f.input, deploymentJournalSha256: null}, f.config, path.join(f.dir, 'fresh')), /completed deployment journal required/);
});
test('no tool offers enable_verified_reward_view or any method outside its closed list', t => {
  const f = fixture(t); assert.deepEqual([...P.METHODS], ['stage_open', 'stage_pause_deposits']);
  for (const step of ['enable_verified_reward_view', 'verified-reward-view', 'checkpoint_rewards', 'upload-vault', 'activate-receipt', 'seed-3', 'seed-1'])
    assert.throws(() => P.operation(f.input, step, undefined));
  for (const step of ['enable_verified_reward_view', 'verified-reward-view', 'checkpoint_rewards', 'seed-1', 'pause-vault', 'stage_open'])
    assert.throws(() => D.operation(f.input.plan, f.input.deployment, step), /outside upload\/create\/configuration ceremony/);
  assert(!Object.values(D.review(f.input.plan, f.input.deployment).steps).some(s => /verified/.test(s.name)));
  assert(!D.ORDER.some(name => /verified/.test(name)));
});
test('fake RPC transaction reconciliation validates the public network and preserves unknown outcomes', async t => {
  const f = fixture(t), calls = [], row = f.fakePrepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-800', deadline: '1800'});
  const rpc = {getNetwork: async () => ({passphrase: f.input.plan.network}), getTransaction: async hash => {calls.push(hash); return {status: 'NOT_FOUND'};}};
  assert.deepEqual(await P.adapter(f.input, f.config, rpc).transaction(row), {status: 'UNKNOWN'}); assert.equal(calls.length, 1);
  rpc.getNetwork = async () => ({passphrase: S.Networks.TESTNET}); await assert.rejects(() => P.adapter(f.input, f.config, rpc).transaction(row)); assert.equal(calls.length, 1);
});
// A complete fake-RPC backend that answers from the simulated chain through the exact frozen ABIs.
function fakeRpc(f, options = {}) {
  const {plan, deployment, specs} = f.input, {A, V, R} = f, C = plan.controller, code = new Map();
  for (const a of [...plan.candidateArtifacts, deployment.controllerArtifact]) code.set(a.sha256, fs.readFileSync(path.join(ROOT, a.path)));
  for (const [name, dep] of Object.entries(deployment.dependencies)) if (!dep.asset) {const b = Buffer.from('FAKE_RPC_TEST_CODE:' + name); dep.wasmSha256 = sha(b); code.set(dep.wasmSha256, b);}
  const pins = new Map([[V, {wasmSha256: P.PAIR.vault}], [R, {wasmSha256: P.PAIR.receipt}], [C, {wasmSha256: deployment.controllerArtifact.sha256}], ...Object.values(deployment.dependencies).map(d => [d.contract, d])]);
  const account = {accountId: () => new S.Address(A).toScAddress().accountId(), signers: () => [], thresholds: () => Buffer.from([1, 0, 0, 0]), ext: () => ({switch: () => 0}),
    seqNum: () => S.xdr.SequenceNumber.fromString('9'), balance: () => S.xdr.Int64.fromString('5000000000'), numSubEntries: () => 0};
  const settings = read(BASE + 'network-config-20261114.json').settings, state = {shifted: false, latestCalls: 0, calls: 0, methods: [], seeds: [], quoteAmount: null, trustlineReads: []};
  const rpc = {getNetwork: async () => ({passphrase: plan.network}), getLatestLedger: async () => ({sequence: state.shifted && ++state.latestCalls > 1 ? 101 : 100, closeTime: '1000', protocolVersion: '29'}),
    getLedgers: async () => ({latestLedger: 100, ledgers: [{headerXdr: {header: () => ({ledgerSeq: () => 100, scpValue: () => ({closeTime: () => S.xdr.Uint64.fromString('1000')}), baseFee: () => 100, baseReserve: () => 5000000})}}]}),
    getLedgerEntries: async (...keys) => ({latestLedger: 100, entries: keys.flatMap(key => {
      if (key.switch().name === 'configSetting') return [{key, val: S.xdr.LedgerEntryData.fromXDR(settings[key.configSetting().configSettingId().name].xdr, 'base64')}];
      if (key.switch().name === 'account') return [{key, val: {account: () => account}}];
      if (key.switch().name === 'contractCode') {const bytes = code.get(key.contractCode().hash().toString('hex')); return bytes ? [{key, val: {contractCode: () => ({code: () => bytes})}}] : [];}
      if (key.switch().name === 'trustline') {const asset = key.trustLine().asset(), code = (asset.switch().name === 'assetTypeCreditAlphanum4' ? asset.alphaNum4() : asset.alphaNum12()).assetCode().toString().replace(/\0/g, '');
        const line = Object.values(f.world.trustlines).find(l => l.code === code); state.trustlineReads.push(code);
        return line?.present ? [{key, val: {trustLine: () => ({flags: () => line.authorized ? 1 : 0, balance: () => S.xdr.Int64.fromString(line.balance), limit: () => S.xdr.Int64.fromString(line.limit)})}}] : [];}
      assert.equal(key.switch().name, 'contractData'); const cd = key.contractData(), id = S.Address.fromScAddress(cd.contract()).toString();
      if (cd.key().switch().name === 'scvLedgerKeyContractInstance') {const pin = pins.get(id); assert(pin);
        const ex = pin.asset ? S.xdr.ContractExecutable.contractExecutableStellarAsset() : S.xdr.ContractExecutable.contractExecutableWasm(Buffer.from(pin.wasmSha256, 'hex'));
        return [{key, val: {contractData: () => ({val: () => ({instance: () => ({executable: () => ex, storage: () => (id === R ? state.receiptStorage || [] : [])})})})}}];}
      const name = S.scValToNative(cd.key())[0], value = name === 'Peridottroller' ? C : name === 'PauseGuardian' ? A : null;
      return value ? [{key, val: {contractData: () => ({val: () => new S.Address(value).toScVal()})}}] : [];
    })}),
    simulateTransaction: async tx => {
      state.calls++; const fn = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().hostFunction().invokeContract();
      const id = S.Address.fromScAddress(fn.contractAddress()).toString(), method = fn.functionName().toString(); state.methods.push(method);
      const world = f.snapshot(state.seeds, state.quoteAmount), v = world.values;
      const ok = (retval, extra = {}) => ({latestLedger: 100, transactionData: new S.SorobanDataBuilder(), result: {auth: [], retval}, ...extra});
      if (method === 'stage_open') return {_parsed: true, latestLedger: 100, minResourceFee: '500', transactionData: new S.SorobanDataBuilder().setResources(options.cpu ?? 10000000, 0, 1000).setResourceFee('500'),
        result: {retval: S.nativeToScVal(BigInt(v.state.next_id), {type: 'u64'}), auth: [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
          rootInvocation: S.xdr.SorobanAuthorizedInvocation.fromXDR(P.authRoots(f.input, 'seed-1', P.seedFrom(f.input, f.config, world, '1800', [], 'single-800'))[0], 'base64')})]}};
      const map = id === V ? {get_admin: 'vaultAdmin', stage_guardian: 'vaultGuardian', stage_investment_keeper: 'keeper', stage_receipt: 'boundReceipt', stage_funding_asset: 'fundingAsset',
        get_config: 'vaultConfig', get_params: 'vaultParams', get_primary_reward_token: 'rewardToken', stage_deposits_paused: 'vaultPaused', total_supply: 'vaultSupply', get_position_liquidity: 'positionLiquidity', stage_reserved: 'reserved', stage_beneficial: 'beneficial', stage_locked: 'locked', stage_available: 'available'} :
        id === R ? {get_admin: 'receiptAdmin', get_underlying_token: 'receiptFundingAsset', stage_config: 'receiptConfig', stage_is_active: 'active', stage_deposits_paused: 'receiptPaused', stage_state: 'state', get_minimum_supply: 'receiptFloor', get_total_ptokens: 'receiptSupply', get_total_underlying: 'receiptUnderlying', get_exchange_rate: 'exchangeRate', get_cash_borrowing_enabled: 'cashBorrowing', get_total_borrowed: 'borrowed', get_min_cash_reserve_bps: 'minCashReserve', stage_units: 'units', stage_commands: 'commands'} :
        {get_admin: 'controllerAdmin', is_market_supported: 'marketSupported', is_deposit_paused: 'controllerDepositPaused', is_redeem_paused: 'controllerRedeemPaused', is_borrow_paused: 'controllerBorrowPaused', get_market_cf: 'collateralFactor'};
      let value, spec = id === V ? specs.vault : id === R ? specs.receipt : specs.controller;
      if (method === 'balance') {
        const owner = S.scValToNative(fn.args()[0]); value = id === V ? owner === A ? '0' : v.custodyShares : id === deployment.dependencies.fundingToken.contract ? owner === A ? v.ownerFundingBalance : owner === R ? v.receiptFundingBalance : '0' : owner === A ? v.ownerOtherBalance : '0';
        return ok(S.nativeToScVal(BigInt(value), {type: 'i128'}));
      }
      if (method === 'stage_job') {
        const row = v.jobs.find(x => x.id === String(S.scValToNative(fn.args()[0]))); assert(row, 'tool read a command id at or above next_id');
        if (options.archivedJob) return ok(spec.nativeToScVal(row.job, spec.getFunc(method).outputs()[0]), {restorePreamble: {}});
        value = row.job;
      } else if (method === 'get_ptoken_balance') value = S.scValToNative(fn.args()[0]) === R ? v.receiptOwnFloor : v.ownerPToken;
      else if (method === 'stage_quote_open') {const [amount, tolerance, margin] = fn.args().map(S.scValToNative); assert.equal(tolerance, 20); assert.equal(margin, 5000);
        value = quoteFor(String(amount), 100, 1000, String(amount) === '8000000000' ? f.world.quoteFull : f.world.quotePart);}
      else {assert(map[method], 'unexpected view ' + method); value = v[map[method]];}
      return ok(spec.nativeToScVal(value, spec.getFunc(method).outputs()[0]));
    }};
  return {rpc, state};
}
test('fake RPC: coherent one-ledger snapshot through the frozen ABIs enumerates every command id; quotes always use 20 / 5000', async t => {
  const f = fixture(t), {rpc, state} = fakeRpc(f); state.quoteAmount = '8000000000';
  const snapshot = await P.adapter(f.input, f.config, rpc).inspect([]); P.validateCommon(f.input, f.config, snapshot); assert(state.calls > 40);
  assert.deepEqual(snapshot.values.jobs, []); assert.equal(P.openCommandGate(f.input, snapshot, [], 'before-seed-1').ok, true); assert.equal(snapshot.quoteFull.valid, true);
  assert.deepEqual(state.trustlineReads, ['PYUSD', 'USDC', 'AQUA']); assert.deepEqual(snapshot.values.trustlines.reward, f.world.trustlines.reward); assert.equal(snapshot.account.balance, '5000000000');
  // The live state read by the lead at about 14:05 UTC on 14 Nov (no AQUA trustline) refuses the seed through the real adapter path.
  f.world.trustlines.reward = {code: 'AQUA', present: false};
  await assert.rejects(P.adapter(f.input, f.config, rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-800', deadline: '1800'}), refusal('FUNDING_PREFLIGHT', (report, error) => assert.match(error.message, /no AQUA trustline/)));
  f.world.trustlines.reward = {code: 'AQUA', present: true, authorized: true, balance: '0', limit: '9223372036854775807'};
  const row = await P.adapter(f.input, f.config, rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-800', deadline: '1800'});
  P.validatePrepared(f.input, f.config, row, '10', '0', {plan: 'single-800', seeds: []});
  assert.equal(S.TransactionBuilder.fromXDR(row.unsignedXdr, f.input.plan.network).signatures.length, 0); assert.equal(row.gate.ok, true);
  // A stranger's command is found by id although stage_commands(operator) is empty.
  f.world.mutate = s => addForeign(s); state.methods.length = 0;
  await assert.rejects(P.adapter(f.input, f.config, rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-800', deadline: '1800'}),
    refusal('FOREIGN_COMMAND', report => {assert.equal(report.foreign[0].owner, STRANGER); assert.deepEqual(report.operatorUnfinishedCommands, []);}));
  assert.equal(state.methods.filter(m => m === 'stage_job').length, 1); assert(!state.methods.includes('stage_open'), 'no financial simulation after a refusal');
  const view = await P.adapter(f.input, f.config, rpc).inspect([]); assert.equal(view.values.jobs.length, 1); assert.equal(view.values.jobs[0].job.owner, STRANGER);
  f.world.mutate = null; state.shifted = true; state.latestCalls = 0; await assert.rejects(() => P.adapter(f.input, f.config, rpc).inspect([]));
});
test('NEGATIVE fake RPC: an archived command record is a refusal, never "no commands"; over-limit simulation is not prepared', async t => {
  const f = fixture(t); f.world.mutate = s => addForeign(s, stranger(99, '0'), false);
  const archived = fakeRpc(f, {archivedJob: true});
  await assert.rejects(P.adapter(f.input, f.config, archived.rpc).inspect([]), /view stage_job failed or needs a separate restore/);
  await assert.rejects(P.adapter(f.input, f.config, archived.rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-800', deadline: '1800'}), /needs a separate restore/);
  const g = fixture(t), over = fakeRpc(g, {cpu: 100000001}); g.deps.prepare = P.adapter(g.input, g.config, over.rpc).prepare; over.state.quoteAmount = '8000000000';
  await assert.rejects(g.prepare({deadline: '1800'}), /exceeds 100000000/); assert.equal(g.exists(), false);
});
test('NEGATIVE the 2x400 fallback needs an explicit acknowledgement of its exposure; the warning is printed with the offer and journaled', async t => {
  const f = fixture(t); f.world.quoteFull = false;
  await assert.rejects(f.prepare(), refusal('SEED_800_NOT_OFFERED_ONLY_SPLIT', (report, error) => {
    assert.equal(report.warning, P.SPLIT_EXPOSURE_WARNING); assert.equal(report.requiredFlag, '--acknowledge-split-exposure');
    assert.match(error.message, /400 PYUSD of cap room is open to ANYONE until the second open lands/); assert.match(error.message, /leaves a stranger in the pilot/);
    assert.match(error.message, /The first 400 is unguarded too: .*does NOT make it revert; it is seen only at the read-back/);}));
  for (const acknowledgeSplitExposure of [undefined, false, 'yes', 1])
    await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure}), refusal('SPLIT_EXPOSURE_NOT_ACKNOWLEDGED', report => assert.equal(report.warning, P.SPLIT_EXPOSURE_WARNING)));
  assert.equal(f.exists(), false);
  await assert.rejects(P.prepare(f.input, f.config, f.directory, {deadline: '1800', acknowledgeSplitExposure: true}, f.deps), /applies only to the first deposit of the 2x400 fallback/);
  const first = await f.prepare({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true}); assert.equal(first.warning, P.SPLIT_EXPOSURE_WARNING);
  assert.equal(f.journal().splitPrecondition.exposureAcknowledged, true); assert.equal(f.journal().splitPrecondition.warning, P.SPLIT_EXPOSURE_WARNING);
  const journal = f.journal(); journal.splitPrecondition.exposureAcknowledged = false; fs.writeFileSync(path.join(f.directory, 'journal.json'), JSON.stringify(journal));
  assert.throws(() => P.loadJournal(f.input, f.config, f.directory));
});
test('NEGATIVE funding pre-flight: PYUSD, XLM and the USDC and AQUA trustlines are read before every seed; every shortfall is printed', async t => {
  const f = fixture(t), ok = P.fundingPreflight(f.input, f.config, f.snapshot([], '8000000000'), '8000000000');
  assert.equal(ok.ok, true); assert.equal(ok.pyusd.needRaw, '8150000000'); assert.equal(ok.pyusd.floorAlreadyPaidRaw, '10000000'); assert.equal(ok.xlm.needStroops, '10100000');
  // Ledger wallet PYUSD as read on 15 Nov 2026 (807.5723328) against the fixture's large 15 PYUSD reserve; 6.2066669 XLM; NO AQUA trustline.
  f.world.mutate = s => {s.values.ownerFundingBalance = '8075723328'; s.account.balance = '62066669';}; f.world.trustlines.reward = {code: 'AQUA', present: false};
  f.config.feeBudgetStroops = '100000000'; f.config.maxTransactionFeeStroops = '10000';
  // v3: with 807.5723328 PYUSD the single 800 does not fit 800 + reserve, so ONLY the 2x400 fallback is offered (before any funding check).
  await assert.rejects(f.prepare(), refusal('SEED_800_NOT_OFFERED_ONLY_SPLIT', (report, error) => {
    assert.deepEqual(report.offeredPlan, 'split-2x400'); assert.match(error.message, /800 PYUSD \+ quoted reserve needs 815\.0000000 but the wallet holds 807\.5723328/);}));
  const funding = P.fundingPreflight(f.input, f.config, f.snapshot([], '8000000000'), '8000000000');
  assert.equal(funding.ok, false); assert.equal(funding.shortfalls.length, 3);
  assert.match(funding.shortfalls[0], /^PYUSD: wallet holds 807\.5723328, needs 815\.0000000 .*short by 7\.4276672\. The 1 PYUSD floor is separate and already paid \(read back as 1\.0000000\)/);
  assert.match(funding.shortfalls[1], /^XLM: spendable 6\.2066669, needs 11\.0000000 .*short by 4\.7933331/);
  assert.match(funding.shortfalls[2], /^TRUSTLINE: the operator has no AQUA trustline \(a reward claim pays AQUA and rolls back without the trustline\)/);
  f.world.mutate = s => {s.account.balance = '62066669';};
  await assert.rejects(f.prepare(), refusal('FUNDING_PREFLIGHT', (report, error) => {assert.equal(report.shortfalls.length, 2); assert.match(error.message, /The principal is never reduced to fit/);}));
  assert.equal(f.exists(), false);
  const g = fixture(t);
  for (const [change, pattern] of [[w => w.trustlines.other = {code: 'USDC', present: false}, /no USDC trustline \(in-kind and emergency payouts are paid in both pool tokens\)/],
    [w => w.trustlines.reward.authorized = false, /AQUA trustline is not authorized/], [w => delete w.trustlines.reward, /no reward trustline/],
    [w => w.account.selling = '4990000000', /^XLM: spendable 1\.0000000/],
    [w => w.mutate = s => {delete s.account;}, /account balance was not read/]]) {
    const w = structuredClone({account: g.world.account, trustlines: g.world.trustlines}); Object.assign(g.world, structuredClone(w), {mutate: null}); change(g.world);
    await assert.rejects(g.prepare(), refusal('FUNDING_PREFLIGHT', report => assert(report.shortfalls.some(x => pattern.test(x)), JSON.stringify(report.shortfalls))));
    Object.assign(g.world, w, {mutate: null});
  }
  // v3: one raw unit short of 800 + reserve is not a funding refusal of the 800 but a fallback offer (wallet fit is part of the decision).
  g.world.mutate = s => {s.values.ownerFundingBalance = '8149999999';};
  await assert.rejects(g.prepare(), refusal('SEED_800_NOT_OFFERED_ONLY_SPLIT', report => {assert.equal(report.offeredPlan, 'split-2x400');}));
  const tight = g.snapshot([], '8000000000'); assert.deepEqual(P.seedPlanDecision(tight).wallet, {haveRaw: '8149999999', needRaw: '8150000000', fits: false});
  assert.match(P.fundingPreflight(g.input, g.config, tight, '8000000000').shortfalls[0], /short by 0\.0000001/); g.world.mutate = null;
  // Second deposit of the split is checked against its own remaining principal and fresh reserve.
  g.world.quoteFull = false; await g.seedThroughFinalised({deadline: '1800', seedPlan: 'split-2x400', acknowledgeSplitExposure: true});
  // (A PYUSD shortfall cannot arise here in a consistent state: the first deposit already required the whole remaining principal.)
  g.world.trustlines.reward = {code: 'AQUA', present: false};
  await assert.rejects(g.prepare({deadline: '1900'}), refusal('FUNDING_PREFLIGHT', report => {assert.equal(report.pyusd.remainingPrincipalRaw, '4000000000'); assert.match(report.shortfalls[0], /no AQUA trustline/);}));
  g.world.trustlines.reward = {code: 'AQUA', present: true, authorized: true, balance: '0', limit: '9223372036854775807'}; g.world.account.balance = '10099999';
  await assert.rejects(g.prepare({deadline: '1900'}), refusal('FUNDING_PREFLIGHT', report => assert.match(report.shortfalls[0], /^XLM: spendable 1\.0099999, needs 1\.0100000/)));
  assert.equal(g.journal().entries.length, 1);
});

// ---------------- v3-specific tests ----------------
const CONFIRMED_JOURNAL = REL + 'deploy/run-seed800-20261115T132829Z/journal';
// Temp REVIEWED copy of the generated FULL config (TEST fixture only: the real file stays UNREVIEWED until the operator's mark-reviewed).
function reviewedFull(t, edit = c => c) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'pilot-v3-full-'))); t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  const full = edit({...read(FULL_CONFIG), status: 'REVIEWED_UNSIGNED_CEREMONY'}), file = path.join(dir, 'full.json'); fs.writeFileSync(file, JSON.stringify(full));
  // 16 Nov: the real journal now also holds the confirmed step 20; these pre-activation tests use the derived 19-step
  // journal (support/pre-activation-journal.cjs, cross-checked with the operator's backup).
  const journal = path.join(dir, 'journal'); fs.mkdirSync(journal, {mode: 0o700}); fs.writeFileSync(path.join(journal, 'journal.json'), JSON.stringify(J.preActivation(), null, 2) + '\n', {mode: 0o600});
  return {dir, file, full, journal, pin: f => ({path: f, sha256: sha(fs.readFileSync(f))})};
}
function pilotFor(x, extra = {}) {
  const plan = REL + 'deploy/mainnet-deployment-plan-v3-seed800.json';
  return {status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3', helperSha256: sha(fs.readFileSync(path.join(__dirname, '../pilot-ceremony-v3.cjs'))), runtimePins: D.runtimePins(),
    planDigestSha256: x.full.planDigestSha256, deployment: {plan: {path: plan, sha256: sha(fs.readFileSync(path.join(ROOT, plan)))}, config: x.pin(x.file), journalDirectory: x.journal},
    source: x.full.source, initialSourceSequence: String(BigInt(x.full.initialSourceSequence) + 1n), keeperPublicAddress: x.full.keeperPublicAddress,
    parameters: JSON.parse(JSON.stringify(P.PARAMETERS)), approvedPauses: [...P.PAUSES], protocolVersion: 29, costModelXdrSha256: x.full.costModelXdrSha256,
    rpcUrl: 'https://rpc.invalid', expiresAt: 4102444800, feeBudgetStroops: '100000000', maxTransactionFeeStroops: '50000000', inclusionFeeStroops: '1000',
    inclusionFeeCapStroops: '100000', balanceFloorStroops: '50000000', timeoutSeconds: 180, ...extra};
}
// A step-20 row that the UNCHANGED helper accepts under the FULL config (fake chain; never signed).
function activationRow(full, plan) {
  const op = D.operation(plan, full, 'activate-receipt'), seq = String(BigInt(full.initialSourceSequence) + 1n);
  op.body().invokeHostFunctionOp().auth(full.authRoots['activate-receipt'].map(root => new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(), rootInvocation: S.xdr.SorobanAuthorizedInvocation.fromXDR(root, 'base64')})));
  const data = new S.SorobanDataBuilder().setResources(8880945, 0, 1332).setResourceFee('1539670').build(), ledger = 65497700, close = Math.floor(Date.parse(full.generatedAt) / 1000) + 5;
  const tx = new S.TransactionBuilder(new S.Account(plan.admin, full.initialSourceSequence), {fee: full.inclusionFeeStroops, networkPassphrase: plan.network})
    .addOperation(op).setSorobanData(data).setLedgerbounds(ledger + 1, ledger + 20).setTimebounds(close, close + full.timeoutSeconds).build();
  return {step: 'activate-receipt', sequence: seq, unsignedXdr: tx.toXDR(), hash: tx.hash().toString('hex'), maxFeeStroops: tx.fee, ledger, closeTime: close,
    balanceStroops: '1213499703', sellingLiabilities: '0', networkMemoryAndEventCapsVerified: true};
}
test('v3 loadInputs: FULL successor over the REAL confirmed 19-step journal; pilot sequence = activation sequence; activation via the unchanged helper', async t => {
  const realBefore = sha(fs.readFileSync(path.join(ROOT, CONFIRMED_JOURNAL, 'journal.json')));
  const x = reviewedFull(t), plan = read(REL + 'deploy/mainnet-deployment-plan-v3-seed800.json'), pilot = pilotFor(x);
  assert.throws(() => P.loadInputs(pilot), /all 20 deployment steps must be confirmed \(found 19\)/);
  const pending = P.loadInputs(pilot, {allowPendingActivation: true}); assert.equal(pending.activationPending, true); assert.equal(pending.deploymentJournalSha256, null);
  assert.equal(pending.deploymentJournal.entries.length, 19); assert.equal(pending.deploymentJournal.attached, false);
  assert.throws(() => P.loadInputs({...pilot, initialSourceSequence: String(BigInt(pilot.initialSourceSequence) + 1n)}, {allowPendingActivation: true}), /pilot must follow activation directly/);
  assert.throws(() => P.loadInputs({...pilot, initialSourceSequence: x.full.initialSourceSequence}, {allowPendingActivation: true}), /pilot must follow activation directly/);
  // A pre-release config, or a FULL config left unreviewed, is never a pilot input.
  for (const edit of [c => ({...c, releaseStage: D.STAGES.PRE}), c => ({...c, status: 'TOOL_GENERATED_FULL_RELEASE_PENDING_OPERATOR_REVIEW'})]) {
    const y = reviewedFull(t, edit); assert.throws(() => P.loadInputs(pilotFor(y), {allowPendingActivation: true}));
  }
  // Apply step 20 on the COPY of the journal through the helper's own prepare/reconcile with an injected fake chain.
  const now = Math.floor(Date.parse(x.full.generatedAt) / 1000) + 10, row = activationRow(x.full, plan);
  const deps = {now: () => now, prepare: async () => structuredClone(row), transaction: async r => ({status: 'SUCCESS', hash: r.hash, source: plan.admin, sequence: r.sequence, ledger: r.ledger + 2, feeCharged: '1540670'}),
    readback: async () => ({ledger: row.ledger + 3, executableAndCodePinsVerified: true, checks: []})};
  const prepared = await D.prepare(plan, x.full, x.journal, deps); assert.equal(prepared.step, 'activate-receipt'); assert.equal(prepared.sequence, pilot.initialSourceSequence);
  assert.throws(() => P.loadInputs(pilot, {allowPendingActivation: true}), /unconfirmed step/);
  await D.reconcile(plan, x.full, x.journal, deps);
  const input = P.loadInputs(pilot); assert.equal(input.activationPending, false); assert.equal(input.deploymentJournal.entries.length, 20);
  assert.equal(input.deploymentJournalSha256, sha(fs.readFileSync(path.join(x.journal, 'journal.json'))));
  assert.deepEqual(input.deploymentJournal.segments.map(s => s.releaseStage), [D.STAGES.PRE, D.STAGES.FULL]);
  assert.throws(() => P.loadInputs({...pilot, deployment: {...pilot.deployment, plan: {...pilot.deployment.plan, sha256: '0'.repeat(64)}}}), /pinned input changed/);
  // The real confirmed journal was only read (it holds 20 confirmed steps since 16 Nov).
  assert.equal(sha(fs.readFileSync(path.join(ROOT, CONFIRMED_JOURNAL, 'journal.json'))), realBefore);
  assert.equal(J.live().entries.length, 20);
  // And the operator's real reviewed pilot config loads over the real 20-step journal, with activation applied.
  const real = P.loadInputs(read(REL + 'activate/pilot-config-v3-gateway.json'));
  assert.equal(real.activationPending, false); assert.equal(real.deploymentJournal.entries.at(-1).hash, J.STEP20);
});
test('v3 config generator: tool status is never accepted; only the operator mark-reviewed status passes; parameters are fixed at 800/2x400/1,000', t => {
  const x = reviewedFull(t), generated = P.makeConfig(x.file, x.journal, {now: 1794772000});
  assert.equal(generated.status, P.PENDING_STATUS); assert.equal(generated.initialSourceSequence, String(BigInt(x.full.initialSourceSequence) + 1n));
  assert.deepEqual(generated.parameters.seedPlans, {'single-800': ['8000000000'], 'split-2x400': ['4000000000', '4000000000']});
  assert.equal(generated.parameters.capRaw, '8000000000'); assert.equal(generated.parameters.maxDeployRaw, '10000000000'); assert.equal(generated.hostPreflightStop, 97000000);
  const input = P.loadInputs({...generated, status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3'}, {allowPendingActivation: true});
  assert.throws(() => P.validateConfig(generated, input, 1794772000));
  const file = path.join(x.dir, 'pilot.UNREVIEWED.json'); fs.writeFileSync(file, JSON.stringify(generated));
  const out = P.markReviewed(file, 1794772001); assert.equal(out.status, 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3');
  const reviewed = JSON.parse(fs.readFileSync(out.file)); P.validateConfig(reviewed, input, 1794772002);
  assert.throws(() => P.markReviewed(file, 1794772001), /EEXIST|exists/);
  for (const bad of [c => c.parameters.capRaw = '10000000000', c => c.parameters.maxDeployRaw = '8000000000', c => c.parameters.toleranceBps = 30, c => c.parameters.seedPlans['split-2x400'] = ['5000000000', '3000000000']])
    {const c = structuredClone(reviewed); bad(c); assert.throws(() => P.validateConfig(c, input, 1794772002));}
});
function preSnapshot(f, edit = () => {}) {
  const s = {ledger: 200, closeTime: 2000, account: {sequence: String(BigInt(f.config.initialSourceSequence) - 1n), balance: '1213499703'}, receiptInstanceKeys: ['Admin', 'Initialized', 'UnderlyingToken'],
    values: {receiptActive: false, receiptPaused: true, vaultPaused: false, vaultSupply: '0', positionLiquidity: '0', reserved: ['0', '0'], boundReceipt: f.R, keeper: f.config.keeperPublicAddress,
      vaultGuardian: f.A, custodyShares: '0', vaultFundingBalance: '0', vaultOtherBalance: '0', ownerFundingBalance: '8075723328',
      quoteFull: {...quoteFor('8000000000', 200, 2000), min_reserve: '25728', reserve: '38592'}}};
  s.quoteFull = s.values.quoteFull; edit(s); return s;
}
test('v3 open-command check BEFORE activation: inactivity and emptiness are proven; every deviation refuses; 800 offered only if it fits the wallet', t => {
  const f = fixture(t), ok = P.preActivationGate(f.input, f.config, preSnapshot(f));
  assert.equal(ok.ok, true, JSON.stringify(ok.findings)); assert.equal(ok.seedPlanDecision.available, 'single-800');
  assert.deepEqual(ok.wallet, {haveRaw: '8075723328', needRaw: '8000038592', fits: true}); assert.match(ok.commandsPossible, /stage_open requires the StgV1Config/);
  for (const [edit, pattern] of [[s => s.values.receiptActive = true, /stage_is_active is true/], [s => s.receiptInstanceKeys.push('StgV1Config'), /staged keys StgV1Config/],
    [s => s.receiptInstanceKeys.push('StgV1State'), /StgV1State/], [s => s.values.receiptPaused = false, /must read true/], [s => s.values.vaultPaused = true, /vault latch/],
    [s => s.values.custodyShares = '1', /custodyShares is 1/], [s => s.values.vaultSupply = '5', /vaultSupply is 5/], [s => s.values.vaultFundingBalance = '1', /vaultFundingBalance/],
    [s => s.values.reserved = ['0', '1'], /stage_reserved/], [s => s.values.keeper = 'GBHWIIIGBBEPRJSJIDSPWZE5USLY7TPUTHROK3VZ3ZTJ2GEKTBOPZON4', /vault keeper is GBHWIIIG/],
    [s => s.values.boundReceipt = f.V, /stage_receipt/], [s => s.values.vaultGuardian = STRANGER, /guardian/]]) {
    const r = P.preActivationGate(f.input, f.config, preSnapshot(f, edit)); assert.equal(r.ok, false); assert(r.findings.some(x => pattern.test(x)), JSON.stringify(r.findings));
  }
  assert.throws(() => P.preActivationGate(f.input, f.config, preSnapshot(f, s => s.account.sequence = String(BigInt(s.account.sequence) + 1n))), /Ledger sequence moved/);
  const short = P.preActivationGate(f.input, f.config, preSnapshot(f, s => s.values.ownerFundingBalance = '8000038591'));
  assert.equal(short.seedPlanDecision.available, 'split-2x400'); assert.equal(short.wallet.fits, false); assert.match(short.nextSteps, /NOT recommended/);
  const bad = P.preActivationGate(f.input, f.config, preSnapshot(f, s => {s.values.quoteFull.admissible = false;}));
  assert.equal(bad.seedPlanDecision.available, 'split-2x400');
});
test('v3 read-only retries: crossed ledger and HTTP 429 are retried, refusals and other errors are not; a retried prepare journals exactly once', async t => {
  const sleeps = [], opts = {sleep: async ms => sleeps.push(ms), log: () => {}};
  let n = 0; assert.equal(await P.retrying(async () => {if (++n < 3) throw new Error('read/quote crossed ledger; prepare nothing'); return 'ok';}, opts), 'ok'); assert.deepEqual(sleeps, [250, 250]);
  n = 0; sleeps.length = 0; assert.equal(await P.retrying(async () => {if (++n < 4) throw new Error('Request failed with status code 429'); return 'ok';}, opts), 'ok'); assert.deepEqual(sleeps, [2000, 4000, 8000]);
  n = 0; await assert.rejects(P.retrying(async () => {n++; throw new P.Refusal('FOREIGN_COMMAND', 'crossed ledger in text but a refusal', {});}, opts), refusal('FOREIGN_COMMAND')); assert.equal(n, 1);
  n = 0; await assert.rejects(P.retrying(async () => {n++; throw new Error('Ledger source sequence changed');}, opts), /sequence changed/); assert.equal(n, 1);
  n = 0; await assert.rejects(P.retrying(async () => {n++; throw new Error('429');}, {...opts, tries: 3}), /429/); assert.equal(n, 3);
  const f = fixture(t); let calls = 0; const inner = f.deps.prepare;
  const deps = P.withRetry({...f.deps, prepare: async r => {if (++calls < 3) throw new Error(calls === 1 ? 'read/quote crossed ledger; prepare nothing' : 'Request failed with status code 429'); return inner(r);}}, opts);
  const seed = await P.prepare(f.input, f.config, f.directory, {deadline: '1800'}, deps); assert.equal(calls, 3); assert.equal(f.journal().entries.length, 1); assert.equal(seed.step, 'seed-1');
  await assert.rejects(P.prepare(f.input, f.config, f.directory, {deadline: '1800'}, deps), /reconcile existing hash/); assert.equal(f.journal().entries.length, 1);
  // Unknown on-chain status is reported, never retried into a second preparation.
  const unknown = await P.reconcile(f.input, f.config, f.directory, P.withRetry({...f.deps, transaction: async () => ({status: 'NOT_FOUND'})}, opts)); assert.equal(unknown.status, 'UNKNOWN_NO_RETRY');
  assert.equal(P.transient(new Error('ledger changed; no unsigned transaction prepared')), true); assert.equal(P.transient(new Error('not admin')), false);
});

test('v3 production adapter preActivation (fake RPC through the frozen ABIs): quote beside values, receipt instance keys read, gate and decision coherent', async t => {
  const f = fixture(t), {rpc, state} = fakeRpc(f); f.config.initialSourceSequence = '10';
  f.world.mutate = s => {s.values.active = false; s.values.receiptPaused = true;};
  const snap = await P.adapter(f.input, f.config, rpc).preActivation();
  assert.equal(snap.quoteFull.valid, true); assert.equal(snap.values.quoteFull, snap.quoteFull); assert.deepEqual(snap.receiptInstanceKeys, []);
  assert(state.methods.includes('stage_quote_open') && state.methods.includes('stage_is_active')); assert(!state.methods.includes('stage_state'), 'stage_state traps before activation and is not read');
  const gate = P.preActivationGate(f.input, f.config, snap); assert.equal(gate.ok, true, JSON.stringify(gate.findings)); assert.equal(gate.seedPlanDecision.available, 'single-800');
  assert.equal(gate.wallet.fits, true); assert.equal(gate.quoteFull.valid, true);
  state.receiptStorage = [new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('StgV1Config')]), val: S.xdr.ScVal.scvVoid()})];
  const after = P.preActivationGate(f.input, f.config, await P.adapter(f.input, f.config, rpc).preActivation()); assert.equal(after.ok, false); assert.match(after.findings.join(), /StgV1Config/);
});
