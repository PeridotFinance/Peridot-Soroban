'use strict';
// Offline tests for pilot-ceremony-v2.cjs. Public identifiers only; no key is generated, read or used.
// "Approved"/"reviewed" status strings below exist only inside in-memory or temp-directory TEST fixtures.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const P = require('../pilot-ceremony-v2.cjs'), D = require('../operator-deployment-v2.cjs');
const ROOT = path.resolve(__dirname, '../../..'), BASE = 'research/cl-pyusd-tight-pilot-20261111/vault-b-20261113/';
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
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'unsigned-pilot-v2-'))); t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  const plan = read(BASE + 'mainnet-deployment-plan-DRAFT.json'), deployment = read(BASE + 'operator-deployment-review-20261114-1/ceremony-config-DRAFT.json');
  deployment.keeperPublicAddress = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 19));
  const specs = {};
  for (const role of ['vault', 'receipt']) specs[role] = S.contract.Spec.fromWasm(fs.readFileSync(path.join(ROOT, plan.candidateArtifacts.find(a => a.name === role).path)));
  specs.controller = S.contract.Spec.fromWasm(fs.readFileSync(path.join(ROOT, deployment.controllerArtifact.path)));
  const input = {plan, deployment, specs, deploymentJournalSha256: 'd'.repeat(64)};
  const config = {status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V2', helperSha256: sha(fs.readFileSync(path.join(__dirname, '../pilot-ceremony-v2.cjs'))), runtimePins: D.runtimePins(),
    planDigestSha256: D.digest(plan), source: plan.admin, keeperPublicAddress: deployment.keeperPublicAddress, initialSourceSequence: '9',
    parameters: JSON.parse(JSON.stringify(P.PARAMETERS)), approvedPauses: [...P.PAUSES],
    protocolVersion: 29, costModelXdrSha256: deployment.costModelXdrSha256, rpcUrl: 'https://rpc.invalid', expiresAt: 5000,
    feeBudgetStroops: '100000', maxTransactionFeeStroops: '10000', inclusionFeeStroops: '1000', inclusionFeeCapStroops: '1000', balanceFloorStroops: '10000000', timeoutSeconds: 180};
  const A = plan.admin, V = plan.proposedInstances.vault.contract, R = plan.proposedInstances.receipt.contract;
  const zero = Object.fromEntries(['next_id', 'total_units', 'job_units', 'pending_funding', 'pending_other', 'reserve_liability', 'exit_funding', 'exit_other', 'principal_backing', 'pending_principal'].map(k => [k, '0']));
  zero.open_jobs = 0;
  const common = {vaultAdmin: A, receiptAdmin: A, controllerAdmin: A, controllerGuardian: A, vaultGuardian: A, keeper: config.keeperPublicAddress, boundReceipt: R,
    fundingAsset: deployment.dependencies.fundingToken.contract, receiptFundingAsset: deployment.dependencies.fundingToken.contract,
    receiptConfig: {version: 1, vault: V, guardian: A, backing_cap: '10000000000'},
    vaultConfig: {pool: deployment.dependencies.pool.contract, oracle: deployment.dependencies.oracle.contract, token0: deployment.dependencies.fundingToken.contract,
      token1: deployment.dependencies.otherToken.contract, dec0: 7, dec1: 7, underlying_index: 0, tick_lower: -20, tick_upper: 20},
    vaultParams: {max_deploy: '10000000000', paused: false, slippage_bps: 100, max_pool_divergence_bps: 200, oracle_max_age_mult: 2, nav_root_max_age: 300, nav_root_max_stale: 3600, harvest_cooldown: '60'},
    rewardToken: deployment.aquaToken, receiptController: plan.controller, active: true, marketSupported: true, controllerBorrowPaused: true,
    controllerDepositPaused: false, controllerRedeemPaused: false, cashBorrowing: false, collateralFactor: 0, borrowed: '0', minCashReserve: 10000,
    receiptFloor: '10000000', receiptSupply: '10000000', receiptOwnFloor: '10000000', receiptUnderlying: '10000000', receiptFundingBalance: '10000000',
    ownerPToken: '0', ownerDirectVaultShares: '0', exchangeRate: '1000000', vaultFundingBalance: '0', vaultOtherBalance: '0', ownerOtherBalance: '177062510'};
  // The simulated chain. `phases[i]` is the on-chain phase of the operator's i-th seed; `mutate` injects anything else.
  const world = {ledger: 100, phases: [], pauseV: false, pauseR: false, quote1000: true, quotePart: true, mutate: null,
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
    if (quoteAmount) {s.quote = quoteFor(quoteAmount, world.ledger, closeTime, quoteAmount === '10000000000' ? world.quote1000 : world.quotePart); s.quoteAmountRaw = quoteAmount;}
    if (seeds.length === 0) s.quote1000 = quoteFor('10000000000', world.ledger, closeTime, world.quote1000);
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
  const f = fixture(t), before = f.snapshot([], '10000000000'), seed = P.seedFrom(f.input, f.config, before, '1800', [], 'single-1000');
  assert.equal(seed.principalRaw, '10000000000'); assert.equal(seed.id, '0'); assert.equal(seed.quote.reserve, '150000000');
  for (const mutate of [s => s.values.ownerFundingBalance = '10149999999', s => s.quote.ledger++, s => s.quote.oracle_fresh = false, s => s.values.vaultPaused = true,
    s => s.values.receiptPaused = true, s => s.quoteAmountRaw = '5000000000', s => s.values.vaultFundingBalance = '1']) {
    const s = clone(before); mutate(s); assert.throws(() => P.seedFrom(f.input, f.config, s, '1800', [], 'single-1000'));
  }
  for (const deadline of [undefined, 999, 1100, 4601]) assert.throws(() => P.seedFrom(f.input, f.config, before, deadline, [], 'single-1000'));
  const root = S.xdr.SorobanAuthorizedInvocation.fromXDR(P.authRoots(f.input, 'seed-1', seed)[0], 'base64');
  assert.equal(root.function().contractFn().functionName().toString(), 'stage_open');
  assert.equal(root.subInvocations()[0].function().contractFn().functionName().toString(), 'stage_accept_funds');
  const transfer = root.subInvocations()[0].subInvocations()[0].function().contractFn();
  assert.equal(transfer.functionName().toString(), 'transfer'); assert.equal(String(S.scValToNative(transfer.args()[2])), '10150000000');
});
test('NEGATIVE widened tolerance or changed reserve margin is rejected in config and in any quote', t => {
  const f = fixture(t); P.validateConfig(f.config, f.input, 1001);
  for (const change of [c => c.parameters.toleranceBps = 50, c => c.parameters.toleranceBps = 21, c => c.parameters.reserveMarginBps = 2500, c => c.parameters.reserveMarginBps = 10000,
    c => c.parameters.capRaw = '20000000000', c => c.parameters.seedPlans['split-4x250'] = ['2500000000'], c => c.parameters.seedPlans['split-2x500'] = ['4000000000', '6000000000'], c => delete c.parameters.seedPlans]) {
    const c = clone(f.config); change(c); assert.throws(() => P.validateConfig(c, f.input, 1001), /fixed; this tool never widens them/);
  }
  const before = f.snapshot([], '10000000000');
  const wide = clone(before); wide.quote.min_swap = String(BigInt(wide.quote.swap_out) * 9950n / 10000n); wide.quote.min_shares = String(BigInt(wide.quote.shares) * 9950n / 10000n);
  assert.throws(() => P.seedFrom(f.input, f.config, wide, '1800', [], 'single-1000'), /min_swap is not the 20 bps floor/);
  const shares = clone(before); shares.quote.min_shares = String(BigInt(shares.quote.shares) * 9950n / 10000n);
  assert.throws(() => P.seedFrom(f.input, f.config, shares, '1800', [], 'single-1000'), /min_shares is not the 20 bps floor/);
  for (const reserve of ['125000000', '100000000', '149999999', '200000000']) {
    const s = clone(before); s.quote.reserve = reserve; assert.throws(() => P.seedFrom(f.input, f.config, s, '1800', [], 'single-1000'), /reserve is not minimum \+ 50%/);
  }
  assert.deepEqual(Object.keys(P.SEED_PLANS), ['single-1000', 'split-2x500']); assert(Object.isFrozen(P.PARAMETERS) && Object.isFrozen(P.SEED_PLANS));
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
  f.world.quote1000 = false; await assert.rejects(f.prepare(), refusal('FOREIGN_COMMAND'));
  await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true}), refusal('FOREIGN_COMMAND')); assert.equal(f.exists(), false);
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
      assert.equal(report.cap.backing_cap, '10000000000'); assert.equal(report.cap.operatorPrincipal, '10000000000'); assert.match(error.message, /cap is not exactly filled by the operator's principal/);}));
  }
  assert.equal(f.journal().entries.length, 1);
  // Half the cap filled (the split after its first deposit) is not "filled": the gate itself says so.
  const g = fixture(t); g.world.quote1000 = false; await g.seedThroughFinalised({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true});
  const seeds = g.journal().entries.map(row => row.seed), s = g.snapshot(seeds);
  const report = P.openCommandGate(g.input, s, seeds, 'before-pause-vault'); assert.equal(report.ok, false);
  assert.deepEqual(report.findings, ["cap is not exactly filled by the operator's principal: cap 10000000000, principal_backing 5000000000, operator principal 5000000000"]);
  assert.throws(() => P.pauseGate(g.input, g.config, s, seeds, 'pause-vault', 'split-2x500'), refusal('CAP_NOT_EXACTLY_FILLED'));
  assert.equal(P.openCommandGate(g.input, s, seeds, 'before-seed-2').ok, true);
});
test('NEGATIVE pause is refused until the operator seed is finalised (phases 1, 2, 3 and 5)', async t => {
  for (const phase of [1, 2, 3, 5]) {
    const f = fixture(t); await f.prepare(); f.world.ledger += 2; f.world.phases.push(phase); await f.reconcile();
    await assert.rejects(f.pause(), refusal('OPEN_COMMAND_GATE', (report, error) => assert.match(error.message, new RegExp(`operator seed id 0 is in phase ${phase}, not finalised`))));
    assert.equal(f.journal().entries.length, 1);
  }
});
test('NEGATIVE inadmissible 1,000 PYUSD quote: only the 2x500 split is offered, nothing else, nothing journaled', async t => {
  const f = fixture(t); f.world.quote1000 = false;
  await assert.rejects(f.prepare(), refusal('SEED_1000_INADMISSIBLE_ONLY_SPLIT_OFFERED', (report, error) => {
    assert.equal(report.requestedPlan, 'single-1000'); assert.equal(report.offeredPlan, 'split-2x500'); assert.deepEqual(report.fixed, {toleranceBps: 20, reserveMarginBps: 5000});
    assert.equal(report.quote1000.valid, false); assert.match(error.message, /only alternative is two 500 PYUSD deposits\. Tolerance and reserve margin cannot be changed/);}));
  assert.equal(f.exists(), false);
  for (const seedPlan of ['split-4x250', 'single-500', 'split-2x500-wide', '']) await assert.rejects(f.prepare({deadline: '1800', seedPlan}), /seed plan must be single-1000 or split-2x500/);
  for (const partial of [{valid: true, admissible: false}, {valid: false, admissible: true}]) {
    const s = f.snapshot([], '10000000000'); Object.assign(s.quote1000, partial); assert.equal(P.seedPlanDecision(s).available, 'split-2x500');
  }
  const offered = await f.prepare({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true});
  assert.equal(offered.step, 'seed-1'); assert.equal(offered.seedPlan, 'split-2x500'); assert.equal(offered.seed.principalRaw, '5000000000'); assert.equal(offered.seed.quote.min_swap, floor('2500000000'));
  const journal = f.journal(); assert.equal(journal.seedPlan, 'split-2x500'); assert.equal(journal.splitPrecondition.quote1000.valid, false); assert.equal(journal.splitPrecondition.ledger, 100);
  // The journaled precondition cannot be rewritten into an admissible one, and the plan cannot be switched afterwards.
  journal.splitPrecondition.quote1000.valid = true; journal.splitPrecondition.quote1000.admissible = true; fs.writeFileSync(path.join(f.directory, 'journal.json'), JSON.stringify(journal));
  assert.throws(() => P.loadJournal(f.input, f.config, f.directory), /split requires the journaled inadmissible 1,000 PYUSD quote/);
});
test('NEGATIVE split is not offered while the 1,000 quote is admissible; an inadmissible 500 quote has no further alternative', async t => {
  const f = fixture(t);
  await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true}), refusal('SPLIT_NOT_OFFERED_1000_ADMISSIBLE', report => assert.equal(report.offeredPlan, 'single-1000')));
  f.world.quote1000 = false; f.world.quotePart = false;
  await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true}), refusal('SEED_QUOTE_INADMISSIBLE_NO_FURTHER_ALTERNATIVE', (report, error) => assert.match(error.message, /no other amount, tolerance or margin is offered/)));
  assert.equal(f.exists(), false);
});
test('split path end to end: second 500 only after the first finalised and the gate passes again; then both pauses', async t => {
  const f = fixture(t); f.world.quote1000 = false;
  await f.prepare({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true}); f.world.ledger += 2;
  for (const phase of [1, 2, 3, 5]) { // first deposit applied but not finalised: reconcile confirms it, the second is refused
    f.world.phases = [phase];
    if (f.journal().active) assert.equal((await f.reconcile()).progress.phase, phase);
    await assert.rejects(f.prepare({deadline: '1900'}), refusal('OPEN_COMMAND_GATE', (report, error) => {assert.equal(report.gate, 'before-seed-2'); assert.match(error.message, /not finalised/);}));
  }
  f.world.phases = [4]; f.world.ledger += 2;
  await assert.rejects(f.prepare({deadline: '1900', seedPlan: 'single-1000'}), /seed plan was fixed by the first journaled seed/);
  await assert.rejects(f.pause(), /explicit command deadline required/); // the next step is the second seed, never a pause
  f.world.quotePart = false; await assert.rejects(f.prepare({deadline: '1900'}), refusal('SEED_QUOTE_INADMISSIBLE_NO_FURTHER_ALTERNATIVE')); f.world.quotePart = true;
  const second = await f.prepare({deadline: '1900'}); assert.equal(second.step, 'seed-2'); assert.equal(second.seed.id, '1'); assert.equal(second.seed.principalRaw, '5000000000');
  assert.equal(second.seed.quoteLedger, f.world.ledger, 'second deposit carries its own fresh quote'); assert.equal(second.seed.walletBefore.funding, '5950000000'); assert.equal(second.sequence, '11');
  f.world.ledger += 2; f.world.phases = [4, 5]; assert.equal((await f.reconcile()).progress.phase, 5);
  await assert.rejects(f.pause(), refusal('OPEN_COMMAND_GATE'));
  f.world.phases = [4, 4]; f.world.ledger += 2;
  for (const step of P.PAUSES) {
    const row = await f.pause(); assert.equal(row.step, step); assert.equal(row.gate.cap.operatorPrincipal, '10000000000'); assert.equal(row.gate.cap.principal_backing, '10000000000');
    f.world.ledger += 2; if (step === 'pause-vault') f.world.pauseV = true; else f.world.pauseR = true;
    const done = await f.reconcile(); assert.equal(done.progress.operatorSharesRaw, '10000000000'); assert.equal(done.progress.gateOk, true);
  }
  const journal = f.journal(); assert.deepEqual(journal.entries.map(row => row.step), ['seed-1', 'seed-2', 'pause-vault', 'pause-receipt']);
  assert.equal(journal.active, null); assert.equal(journal.feesCharged, '4800'); await assert.rejects(f.pause(), /already complete/);
});
test('NEGATIVE split stuck state: a stranger taking the freed half blocks the second deposit and the pause; no override exists', async t => {
  const f = fixture(t); f.world.quote1000 = false; await f.seedThroughFinalised({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true});
  f.world.mutate = s => addForeign(s, stranger(1, '5000000000'));
  await assert.rejects(f.prepare({deadline: '1900'}), refusal('FOREIGN_COMMAND', report => assert.equal(report.gate, 'before-seed-2')));
  await assert.rejects(f.pause(), /explicit command deadline required/);
  const seeds = f.journal().entries.map(row => row.seed);
  assert.throws(() => P.pauseGate(f.input, f.config, f.snapshot(seeds), seeds, 'pause-vault', 'split-2x500'), refusal('FOREIGN_COMMAND'));
  assert.equal(f.journal().entries.length, 1); assert.deepEqual(Object.keys(P).filter(name => /override|force|skip/i.test(name)), []);
});
test('single path end to end: seed, keeper finalises, pause-vault, pause-receipt, each separately confirmed', async t => {
  const f = fixture(t); const seed = await f.prepare(); assert.equal(seed.step, 'seed-1'); assert.equal(seed.seedPlan, 'single-1000'); assert.equal(seed.signing, false); assert.equal(seed.submission, false);
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
test('NEGATIVE pilot transactions stay at 100,000,000: the upload allowance never applies to the seed or the pauses', async t => {
  const f = fixture(t), request = {step: 'seed-1', sequence: '10', seeds: [], plan: 'single-1000', deadline: '1800'};
  P.validatePrepared(f.input, f.config, f.fakePrepare(request, 100000000), '10', '0', {plan: 'single-1000', seeds: []});
  for (const cpu of [100000001, 104870950, 110000000]) {
    const row = f.fakePrepare(request, cpu), original = row.unsignedXdr;
    assert.throws(() => P.validatePrepared(f.input, f.config, row, '10', '0', {plan: 'single-1000', seeds: []}), /exceeds 100000000; the upload allowance never applies/); assert.equal(row.unsignedXdr, original);
  }
  await f.seedThroughFinalised(); const seeds = [f.journal().entries[0].seed];
  const pause = f.fakePrepare({step: 'pause-vault', sequence: '11', seeds, plan: 'single-1000'}, 100000001);
  assert.throws(() => P.validatePrepared(f.input, f.config, pause, '11', f.journal().feesCharged, {plan: 'single-1000', seeds}), /exceeds 100000000/);
  f.deps.prepare = async request => f.fakePrepare(request, 104870950); await assert.rejects(f.pause(), /exceeds 100000000/); assert.equal(f.journal().entries.length, 1);
  assert.equal(P.INSTRUCTION_LIMIT, 100000000); assert.equal(D.LIMITS.instructions, 100000000);
});
test('preparation refuses expired review, altered quote, wrong order, different source sequence and fee shortage', async t => {
  const f = fixture(t), request = {step: 'seed-1', sequence: '10', seeds: [], plan: 'single-1000', deadline: '1800'}, ctx = {plan: 'single-1000', seeds: []};
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
  assert.equal(view.status, 'READ_ONLY'); assert.equal(view.nextStep, 'seed-1'); assert.equal(view.gate.ok, true); assert.equal(view.seedPlanDecision.available, 'single-1000'); assert.equal(f.exists(), false);
  f.world.quote1000 = false; assert.equal((await P.inspect(f.input, f.config, f.directory, f.deps)).seedPlanDecision.available, 'split-2x500'); f.world.quote1000 = true;
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
  const f = fixture(t), calls = [], row = f.fakePrepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-1000', deadline: '1800'});
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
        return [{key, val: {contractData: () => ({val: () => ({instance: () => ({executable: () => ex})})})}}];}
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
          rootInvocation: S.xdr.SorobanAuthorizedInvocation.fromXDR(P.authRoots(f.input, 'seed-1', P.seedFrom(f.input, f.config, world, '1800', [], 'single-1000'))[0], 'base64')})]}};
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
        value = quoteFor(String(amount), 100, 1000, String(amount) === '10000000000' ? f.world.quote1000 : f.world.quotePart);}
      else {assert(map[method], 'unexpected view ' + method); value = v[map[method]];}
      return ok(spec.nativeToScVal(value, spec.getFunc(method).outputs()[0]));
    }};
  return {rpc, state};
}
test('fake RPC: coherent one-ledger snapshot through the frozen ABIs enumerates every command id; quotes always use 20 / 5000', async t => {
  const f = fixture(t), {rpc, state} = fakeRpc(f); state.quoteAmount = '10000000000';
  const snapshot = await P.adapter(f.input, f.config, rpc).inspect([]); P.validateCommon(f.input, f.config, snapshot); assert(state.calls > 40);
  assert.deepEqual(snapshot.values.jobs, []); assert.equal(P.openCommandGate(f.input, snapshot, [], 'before-seed-1').ok, true); assert.equal(snapshot.quote1000.valid, true);
  assert.deepEqual(state.trustlineReads, ['PYUSD', 'USDC', 'AQUA']); assert.deepEqual(snapshot.values.trustlines.reward, f.world.trustlines.reward); assert.equal(snapshot.account.balance, '5000000000');
  // The live state read by the lead at about 14:05 UTC on 14 Nov (no AQUA trustline) refuses the seed through the real adapter path.
  f.world.trustlines.reward = {code: 'AQUA', present: false};
  await assert.rejects(P.adapter(f.input, f.config, rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-1000', deadline: '1800'}), refusal('FUNDING_PREFLIGHT', (report, error) => assert.match(error.message, /no AQUA trustline/)));
  f.world.trustlines.reward = {code: 'AQUA', present: true, authorized: true, balance: '0', limit: '9223372036854775807'};
  const row = await P.adapter(f.input, f.config, rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-1000', deadline: '1800'});
  P.validatePrepared(f.input, f.config, row, '10', '0', {plan: 'single-1000', seeds: []});
  assert.equal(S.TransactionBuilder.fromXDR(row.unsignedXdr, f.input.plan.network).signatures.length, 0); assert.equal(row.gate.ok, true);
  // A stranger's command is found by id although stage_commands(operator) is empty.
  f.world.mutate = s => addForeign(s); state.methods.length = 0;
  await assert.rejects(P.adapter(f.input, f.config, rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-1000', deadline: '1800'}),
    refusal('FOREIGN_COMMAND', report => {assert.equal(report.foreign[0].owner, STRANGER); assert.deepEqual(report.operatorUnfinishedCommands, []);}));
  assert.equal(state.methods.filter(m => m === 'stage_job').length, 1); assert(!state.methods.includes('stage_open'), 'no financial simulation after a refusal');
  const view = await P.adapter(f.input, f.config, rpc).inspect([]); assert.equal(view.values.jobs.length, 1); assert.equal(view.values.jobs[0].job.owner, STRANGER);
  f.world.mutate = null; state.shifted = true; state.latestCalls = 0; await assert.rejects(() => P.adapter(f.input, f.config, rpc).inspect([]));
});
test('NEGATIVE fake RPC: an archived command record is a refusal, never "no commands"; over-limit simulation is not prepared', async t => {
  const f = fixture(t); f.world.mutate = s => addForeign(s, stranger(99, '0'), false);
  const archived = fakeRpc(f, {archivedJob: true});
  await assert.rejects(P.adapter(f.input, f.config, archived.rpc).inspect([]), /view stage_job failed or needs a separate restore/);
  await assert.rejects(P.adapter(f.input, f.config, archived.rpc).prepare({step: 'seed-1', sequence: '10', seeds: [], plan: 'single-1000', deadline: '1800'}), /needs a separate restore/);
  const g = fixture(t), over = fakeRpc(g, {cpu: 100000001}); g.deps.prepare = P.adapter(g.input, g.config, over.rpc).prepare; over.state.quoteAmount = '10000000000';
  await assert.rejects(g.prepare({deadline: '1800'}), /exceeds 100000000/); assert.equal(g.exists(), false);
});
test('loadInputs binds plan, deployment config and the live-verified 20-step journal; preflight works before activation', async t => {
  const f = fixture(t), evidence = path.join(f.dir, 'TEST-ONLY-fake-evidence.json'); fs.writeFileSync(evidence, '{"TEST_ONLY":true}');
  const pin = file => ({path: file, sha256: sha(fs.readFileSync(file))});
  const plan = read(BASE + 'mainnet-deployment-plan-DRAFT.json'), readbacks = {}, authRoots = {};
  for (const step of D.ORDER) {authRoots[step] = []; readbacks[step] = {before: [{test: 'injected adapter only'}], after: [{test: 'injected adapter only'}]};}
  const contract = byte => S.StrKey.encodeContract(Buffer.alloc(32, byte));
  const deployment = {status: 'REVIEWED_UNSIGNED_CEREMONY', planDigestSha256: D.digest(plan), helperSha256: sha(fs.readFileSync(path.join(__dirname, '../operator-deployment-v2.cjs'))), runtimePins: D.runtimePins(),
    expiresAt: 5000, rpcUrl: 'https://rpc.invalid', protocolVersion: 29, source: plan.admin, initialSourceSequence: '8', keeperPublicAddress: f.config.keeperPublicAddress, aquaToken: contract(13),
    controllerArtifact: f.input.deployment.controllerArtifact, releaseApproval: {status: 'APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE', wasmSha256: {...D.PAIR}, independentReview: pin(evidence), signedCore: pin(evidence)},
    costModelXdrSha256: f.config.costModelXdrSha256, uploadCpuPolicy: D.UPLOAD_CPU_POLICY, uploadCpuException: JSON.parse(JSON.stringify(D.UPLOAD_ALLOWANCE)),
    feeBudgetStroops: '100000', maxTransactionFeeStroops: '10000', inclusionFeeStroops: '1000', inclusionFeeCapStroops: '2000', balanceFloorStroops: '10000000', timeoutSeconds: 180, readbacks, authRoots,
    dependencies: {pool: {contract: plan.steps.find(s => s.step === 'initialize-vault').args[1], wasmSha256: '1'.repeat(64)}, oracle: {contract: plan.steps.find(s => s.step === 'initialize-vault').args[3], wasmSha256: '2'.repeat(64)},
      fundingToken: {contract: plan.steps.find(s => s.step === 'initialize-receipt').args[0], wasmSha256: '3'.repeat(64)}, otherToken: {contract: contract(14), wasmSha256: '5'.repeat(64)}, rewardToken: {contract: contract(13), wasmSha256: '4'.repeat(64)}}};
  const planFile = path.join(f.dir, 'plan.json'), configFile = path.join(f.dir, 'deployment.json'), state = path.join(f.dir, 'deployment-journal');
  fs.writeFileSync(planFile, JSON.stringify(plan)); fs.writeFileSync(configFile, JSON.stringify(deployment));
  const deps = {now: () => 1001, prepare: async (step, sequence) => {
    const cpu = step === 'upload-vault' ? 104870950 : 10000000, data = new S.SorobanDataBuilder().setResources(cpu, 0, 123532).setResourceFee('500').build();
    const tx = new S.TransactionBuilder(new S.Account(plan.admin, String(BigInt(sequence) - 1n)), {fee: '1000', networkPassphrase: plan.network})
      .addOperation(D.operation(plan, deployment, step)).setSorobanData(data).setLedgerbounds(101, 120).setTimebounds(1000, 1180).build();
    return {step, sequence, unsignedXdr: tx.toXDR(), hash: tx.hash().toString('hex'), maxFeeStroops: tx.fee, ledger: 100, closeTime: 1000, balanceStroops: '5000000000', sellingLiabilities: '0', networkMemoryAndEventCapsVerified: true};},
    transaction: async row => ({status: 'SUCCESS', hash: row.hash, sequence: row.sequence, source: plan.admin, feeCharged: '100', ledger: 101}), readback: async () => ({ledger: 102, executableAndCodePinsVerified: true})};
  const pilot = {...f.config, initialSourceSequence: '28', deployment: {plan: pin(planFile), config: pin(configFile), journalDirectory: state}};
  for (let i = 0; i < D.ORDER.length - 1; i++) {await D.prepare(plan, deployment, state, deps); await D.reconcile(plan, deployment, state, deps);}
  // 19 of 20 confirmed: only the offline preflight accepts it, and no pilot journal can start.
  assert.throws(() => P.loadInputs(pilot), /all 20 deployment steps must be confirmed \(found 19\)/);
  const pending = P.loadInputs(pilot, {allowPendingActivation: true}); assert.equal(pending.activationPending, true); assert.equal(pending.deploymentJournalSha256, null);
  P.validateConfig(pilot, pending, 1001); assert.throws(() => P.loadJournal(pending, pilot, path.join(f.dir, 'pilot')), /completed deployment journal required/);
  await D.prepare(plan, deployment, state, deps);
  assert.throws(() => P.loadInputs(pilot, {allowPendingActivation: true}), /unconfirmed step/); // activation prepared but not reconciled
  await D.reconcile(plan, deployment, state, deps);
  const input = P.loadInputs(pilot); assert.equal(input.activationPending, false); assert.equal(input.deploymentJournalSha256, sha(fs.readFileSync(path.join(state, 'journal.json'))));
  assert.equal(P.loadJournal(input, pilot, path.join(f.dir, 'pilot')).deploymentJournalSha256, input.deploymentJournalSha256);
  assert.throws(() => P.loadInputs({...pilot, initialSourceSequence: '29'}), /pilot must follow activation directly/);
  assert.throws(() => P.loadInputs({...pilot, deployment: {...pilot.deployment, plan: {path: planFile, sha256: '0'.repeat(64)}}}), /pinned input changed/);
  assert.throws(() => P.loadInputs({...pilot, deployment: {...pilot.deployment, journalDirectory: path.join(f.dir, 'empty')}}), /all 20 deployment steps/);
  assert.throws(() => P.loadInputs({...pilot, deployment: {plan: pilot.deployment.plan, config: pilot.deployment.config}}), /journal directory required/);
});

test('NEGATIVE the 2x500 fallback needs an explicit acknowledgement of its exposure; the warning is printed with the offer and journaled', async t => {
  const f = fixture(t); f.world.quote1000 = false;
  await assert.rejects(f.prepare(), refusal('SEED_1000_INADMISSIBLE_ONLY_SPLIT_OFFERED', (report, error) => {
    assert.equal(report.warning, P.SPLIT_EXPOSURE_WARNING); assert.equal(report.requiredFlag, '--acknowledge-split-exposure');
    assert.match(error.message, /500 PYUSD of cap room is open to ANYONE until the second open lands/); assert.match(error.message, /leaves a stranger in the pilot/);
    assert.match(error.message, /The first 500 is unguarded too: .*does NOT make it revert; it is seen only at the read-back/);}));
  for (const acknowledgeSplitExposure of [undefined, false, 'yes', 1])
    await assert.rejects(f.prepare({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure}), refusal('SPLIT_EXPOSURE_NOT_ACKNOWLEDGED', report => assert.equal(report.warning, P.SPLIT_EXPOSURE_WARNING)));
  assert.equal(f.exists(), false);
  await assert.rejects(P.prepare(f.input, f.config, f.directory, {deadline: '1800', acknowledgeSplitExposure: true}, f.deps), /applies only to the first deposit of the 2x500 fallback/);
  const first = await f.prepare({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true}); assert.equal(first.warning, P.SPLIT_EXPOSURE_WARNING);
  assert.equal(f.journal().splitPrecondition.exposureAcknowledged, true); assert.equal(f.journal().splitPrecondition.warning, P.SPLIT_EXPOSURE_WARNING);
  const journal = f.journal(); journal.splitPrecondition.exposureAcknowledged = false; fs.writeFileSync(path.join(f.directory, 'journal.json'), JSON.stringify(journal));
  assert.throws(() => P.loadJournal(f.input, f.config, f.directory));
});
test('NEGATIVE funding pre-flight: PYUSD, XLM and the USDC and AQUA trustlines are read before every seed; every shortfall is printed', async t => {
  const f = fixture(t), ok = P.fundingPreflight(f.input, f.config, f.snapshot([], '10000000000'), '10000000000');
  assert.equal(ok.ok, true); assert.equal(ok.pyusd.needRaw, '10150000000'); assert.equal(ok.pyusd.floorAlreadyPaidRaw, '10000000'); assert.equal(ok.xlm.needStroops, '10100000');
  // Wallet as read by the lead at about 14:05 UTC on 14 Nov 2026: 864.6435532 PYUSD, 6.2066669 XLM, USDC trustline present, NO AQUA trustline.
  f.world.mutate = s => {s.values.ownerFundingBalance = '8646435532'; s.account.balance = '62066669';}; f.world.trustlines.reward = {code: 'AQUA', present: false};
  f.config.feeBudgetStroops = '100000000'; f.config.maxTransactionFeeStroops = '10000';
  await assert.rejects(f.prepare(), refusal('FUNDING_PREFLIGHT', (report, error) => {
    assert.equal(report.shortfalls.length, 3);
    assert.match(report.shortfalls[0], /^PYUSD: wallet holds 864\.6435532, needs 1015\.0000000 .*short by 150\.3564468\. The 1 PYUSD floor is separate and already paid \(read back as 1\.0000000\)/);
    assert.match(report.shortfalls[1], /^XLM: spendable 6\.2066669, needs 11\.0000000 .*short by 4\.7933331/);
    assert.match(report.shortfalls[2], /^TRUSTLINE: the operator has no AQUA trustline \(a reward claim pays AQUA and rolls back without the trustline\)/);
    assert.match(error.message, /The principal is never reduced to fit/);}));
  assert.equal(f.exists(), false);
  const g = fixture(t);
  for (const [change, pattern] of [[w => w.trustlines.other = {code: 'USDC', present: false}, /no USDC trustline \(in-kind and emergency payouts are paid in both pool tokens\)/],
    [w => w.trustlines.reward.authorized = false, /AQUA trustline is not authorized/], [w => delete w.trustlines.reward, /no reward trustline/],
    [w => w.account.selling = '4990000000', /^XLM: spendable 1\.0000000/], [w => w.mutate = s => {s.values.ownerFundingBalance = '10149999999';}, /short by 0\.0000001/],
    [w => w.mutate = s => {delete s.account;}, /account balance was not read/]]) {
    const w = structuredClone({account: g.world.account, trustlines: g.world.trustlines}); Object.assign(g.world, structuredClone(w), {mutate: null}); change(g.world);
    await assert.rejects(g.prepare(), refusal('FUNDING_PREFLIGHT', report => assert(report.shortfalls.some(x => pattern.test(x)), JSON.stringify(report.shortfalls))));
    Object.assign(g.world, w, {mutate: null});
  }
  // Second deposit of the split is checked against its own remaining principal and fresh reserve.
  g.world.quote1000 = false; await g.seedThroughFinalised({deadline: '1800', seedPlan: 'split-2x500', acknowledgeSplitExposure: true});
  // (A PYUSD shortfall cannot arise here in a consistent state: the first deposit already required the whole remaining principal.)
  g.world.trustlines.reward = {code: 'AQUA', present: false};
  await assert.rejects(g.prepare({deadline: '1900'}), refusal('FUNDING_PREFLIGHT', report => {assert.equal(report.pyusd.remainingPrincipalRaw, '5000000000'); assert.match(report.shortfalls[0], /no AQUA trustline/);}));
  g.world.trustlines.reward = {code: 'AQUA', present: true, authorized: true, balance: '0', limit: '9223372036854775807'}; g.world.account.balance = '10099999';
  await assert.rejects(g.prepare({deadline: '1900'}), refusal('FUNDING_PREFLIGHT', report => assert.match(report.shortfalls[0], /^XLM: spendable 1\.0099999, needs 1\.0100000/)));
  assert.equal(g.journal().entries.length, 1);
});
