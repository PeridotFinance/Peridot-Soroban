'use strict';
// Offline tests (no network): node --test margin-ceremony.test.cjs
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const m = require('./margin-ceremony.cjs');
const {S} = require('/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/research/cash-borrowing-activation-20261117/tools/lib.cjs');
const base = () => JSON.parse(fs.readFileSync(path.join(__dirname, 'margin-mainnet.json'), 'utf8'));
const withGuardian = () => { const c = base(); c.signers.guardian.publicKey = c.signers.guardian.recommended; return c; };
const addr = a => S.nativeToScVal(a, {type: 'address'});

test('margin id is deterministic (admin + salt) and matches the fork deploy', () => {
  const cfg = base();
  assert.equal(m.marginId(cfg), m.marginId(base()));
  assert.equal(m.marginId(cfg), 'CA6Q7HULJTGFMTB7YLJWFKPQVIIWAP46PQHAY3H5V2EZ2R7HBCW6N7NZ', 'harness WP8_ID');
  assert.notEqual(m.contractIdFor(cfg.network, cfg.admin, cfg.salt + 'x'), m.marginId(cfg));
});

test('steps: M1..M16 in order, unpause is the LAST M-step, canary C1..C4 after it, FB1 only as fallback', () => {
  const st = m.steps(base()), ms = st.filter(s => s.n && s.n.startsWith('M'));
  ms.forEach((s, i) => assert.equal(s.n, `M${i + 1}`));
  assert.equal(ms.length, 16);
  assert.equal(ms.at(-1).id, 'unpause'); assert.equal(ms.at(-1).method, 'set_opens_paused');
  assert.equal(ms.at(-1).keeperGate, true);
  const idx = id => st.findIndex(s => s.id === id);
  assert.equal(st[idx('upload')].n, 'M1'); assert.equal(st[idx('deploy')].n, 'M2');
  assert(idx('gate.keeper') < idx('unpause') && idx('gate.config') < idx('unpause') && idx('backstop_deposit.seed') < idx('unpause'));
  assert.deepEqual(st.filter(s => s.canary).map(s => s.n), ['C1', 'C2', 'C3', 'C4']);
  assert(st.filter(s => s.canary).every(s => idx(s.id) > idx('unpause')));
  assert.deepEqual(st.filter(s => s.fallback).map(s => s.id), ['mint.usdc-pt']);
  for (const s of st) if (s.kind === 'tx') assert.equal(s.target, m.marginId(base()), `${s.id} targets only the margin contract`);
  for (const id of ['set_fees', 'set_max_slippage', 'set_headroom', 'set_backstop_params']) assert.equal(st[idx(id)].default, true);
});

test('refuses every receipt / controller hook function, on any target, and deposit pauses', () => {
  const cfg = base(), M = m.marginId(cfg);
  for (const fn of cfg.denyMethods) {
    for (const t of [cfg.deps.usdcReceipt, cfg.deps.xlmReceipt, cfg.deps.controller, M])
      assert.throws(() => m.assertCallAllowed(cfg, t, fn), /refused/, `${fn} on ${t}`);
  }
  assert.throws(() => m.assertCallAllowed(cfg, cfg.deps.usdcReceipt, 'set_margin_controller', {kind: 'mint'}), /hook/);
  assert.throws(() => m.assertCallAllowed(cfg, cfg.deps.controller, 'set_pause_deposit_g'), /refused/);
  assert.throws(() => m.assertCallAllowed(cfg, M, 'some_margin_controller_fn'), /refused/);
});

test('refuses retired, never-touch and foreign targets; mint may only deposit on the USDC receipt', () => {
  const cfg = base(), M = m.marginId(cfg);
  assert.throws(() => m.assertCallAllowed(cfg, 'CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX', 'set_opens_paused'), /retired/);
  assert.throws(() => m.assertCallAllowed(cfg, 'CC2MVMWNUSEBJ5ITIKTKTLL3URUPYPGRQB6SWX2HAKEKKXDMYF4EYP5C', 'open'), /retired/, 'Margin V3 Mainnet');
  assert.throws(() => m.assertCallAllowed(cfg, 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI', 'deposit', {kind: 'mint'}), /never-touch/);
  assert.throws(() => m.assertCallAllowed(cfg, cfg.deps.controller, 'set_market_cf'), /not the margin contract/);
  assert.throws(() => m.assertCallAllowed(cfg, cfg.deps.usdcReceipt, 'withdraw'), /not the margin contract/);
  assert.throws(() => m.assertCallAllowed(cfg, cfg.deps.xlmReceipt, 'deposit', {kind: 'mint'}), /only call the USDC receipt deposit/);
  assert.throws(() => m.assertCallAllowed(cfg, cfg.deps.usdcReceipt, 'withdraw', {kind: 'mint'}), /only call the USDC receipt deposit/);
  assert.throws(() => m.assertCallAllowed(cfg, M, 'liquidate'), /allowlist/, 'liquidate is the keeper\'s, never the admin ceremony');
  assert.doesNotThrow(() => m.assertCallAllowed(cfg, cfg.deps.usdcReceipt, 'deposit', {kind: 'mint'}));
  assert.doesNotThrow(() => m.assertCallAllowed(cfg, M, 'set_opens_paused'));
});

test('authorization trees are scanned: a hook or retired contract nested anywhere is refused', () => {
  const cfg = base(), M = m.marginId(cfg);
  const node = (c, fn, args, subs = []) => new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(
    new S.xdr.InvokeContractArgs({contractAddress: S.Address.fromString(c).toScAddress(), functionName: fn, args})), subInvocations: subs});
  const ok = node(M, 'backstop_deposit', [addr(cfg.admin)], [node(cfg.deps.usdcReceipt, 'transfer', [addr(cfg.admin), addr(M)])]);
  assert.doesNotThrow(() => m.scanAuthTree(cfg, ok));
  const hook = node(M, 'open', [], [node(cfg.deps.usdcReceipt, 'transfer', [], [node(cfg.deps.usdcReceipt, 'set_margin_controller', [])])]);
  assert.throws(() => m.scanAuthTree(cfg, hook), /set_margin_controller in the authorization tree/);
  const ret = node(M, 'open', [], [node('CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX', 'transfer', [])]);
  assert.throws(() => m.scanAuthTree(cfg, ret), /in the authorization tree/);
  const retArg = node(M, 'set_pool', [addr(cfg.admin), addr('CC2MVMWNUSEBJ5ITIKTKTLL3URUPYPGRQB6SWX2HAKEKKXDMYF4EYP5C')]);
  assert.throws(() => m.scanAuthTree(cfg, retArg), /authorized arguments/);
});

test('wrong artifact hash or size is refused; the frozen copy matches the pin', () => {
  const cfg = base(), wasm = fs.readFileSync(path.resolve(__dirname, cfg.artifact.path));
  assert.equal(m.checkArtifact(cfg, wasm), cfg.artifact.sha256);
  const bad = Buffer.from(wasm); bad[100] ^= 1;
  assert.throws(() => m.checkArtifact(cfg, bad), /not the pinned/);
  assert.throws(() => m.checkArtifact(cfg, Buffer.concat([wasm, Buffer.from([0])])), /not the pinned/);
  const c2 = base(); c2.artifact.bytes = 1; assert.throws(() => m.checkArtifact(c2, wasm), /size/);
});

test('deploy refuses a placeholder guardian; constructor args are (admin, controller, guardian, USDC, XLM)', () => {
  assert.throws(() => m.deployArgs(base()), /guardian.*PLACEHOLDER/);
  const cfg = withGuardian(), a = m.deployArgs(cfg).map(x => S.scValToNative(x));
  assert.deepEqual(a, [cfg.admin, cfg.deps.controller, cfg.signers.guardian.recommended, cfg.deps.usdc, cfg.deps.xlm]);
});

// Operator gate file: a valid example, then one mutation per rule.
const now = Date.parse('2026-12-03T10:00:00Z');
const goodGate = cfg => ({gate: 'margin-keeper-live', marginController: m.marginId(cfg),
  keeper: {host: 'droplet', hostName: 'peridot-keeper-fra1', commit: 'a'.repeat(40), mode: 'live', dryRun: false, liveSince: '2026-12-03T08:00:00Z'},
  dryStage: {from: '2026-12-02T20:00:00Z', to: '2026-12-03T07:55:00Z', errors: 0},
  alerts: {testedAt: '2026-12-03T08:30:00Z', liquidatableOver10min: true, impaired: true, keeperDown: true},
  exclusion: {cashLiquidatorExcludesMargin: true, commit: 'abcdef1'}, operator: 'Joshua', attestation: cfg.keeperGate.attestation, writtenAt: '2026-12-03T09:00:00Z'});
test('keeper gate: a complete operator file passes', () => {
  const cfg = base(); assert.deepEqual(m.keeperGateReasons(cfg, goodGate(cfg), m.marginId(cfg), now), []);
});
test('keeper gate: unpause is refused before the keeper is live on the Droplet with tested alerts', () => {
  const cfg = base(), M = m.marginId(cfg), R = g => m.keeperGateReasons(cfg, g, M, now), mut = f => { const g = goodGate(cfg); f(g); return R(g); };
  assert.match(R(null)[0], /missing/);
  assert(mut(g => { g.keeper.dryRun = true; g.keeper.mode = 'dry'; }).some(r => /LIVE/.test(r)));
  assert(mut(g => { g.keeper.host = 'mac'; }).some(r => /Droplet/.test(r)));
  assert(mut(g => { g.keeper.hostName = 'joshuas-macbook'; }).some(r => /Droplet/.test(r)));
  assert(mut(g => { g.dryStage.to = '2026-12-02T20:30:00Z'; }).some(r => /dry stage shorter/.test(r)));
  assert(mut(g => { g.dryStage.errors = 2; }).some(r => /0 errors/.test(r)));
  assert(mut(g => { g.alerts.impaired = false; }).some(r => /alerts not tested/.test(r)));
  assert(mut(g => { g.alerts.testedAt = '2026-11-20T00:00:00Z'; }).some(r => /alert test older/.test(r)));
  assert(mut(g => { g.exclusion.cashLiquidatorExcludesMargin = false; }).some(r => /exclusion/.test(r)));
  assert(mut(g => { g.operator = 'Claude agent'; }).some(r => /agents never write/.test(r)));
  assert(mut(g => { g.attestation = 'ok'; }).some(r => /attestation/.test(r)));
  assert(mut(g => { g.writtenAt = '2026-12-01T00:00:00Z'; }).some(r => /written more than/.test(r)));
  assert(mut(g => { g.marginController = 'CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX'; }).some(r => /marginController/.test(r)));
  assert(mut(g => { g.keeper.commit = 'FILL'; }).some(r => /placeholders/.test(r)));
});

test('canary amounts: $5 equity at 3x, debt within the leverage bound, collateral at the oracle', () => {
  const px = [12_170_821_168_736n, 100_000_000_000_000n], pu = [100_100_759_862_319n, 100_000_000_000_000n];
  const L = m.canaryAmounts(true, '50000000', '3000000', px, pu, 50), Sh = m.canaryAmounts(false, '50000000', '3000000', px, pu, 50);
  const usd = (a, p) => a * p[0] / p[1];
  assert(usd(L.collateral, px) <= 50_000_000n && usd(L.collateral, px) > 49_999_000n);
  assert(usd(L.borrow, pu) <= 100_000_000n * 9950n / 10000n);
  assert(usd(Sh.borrow, px) <= 100_000_000n && usd(Sh.collateral, pu) <= 50_000_000n);
  assert(L.collateral > 400_000_000n && L.collateral < 420_000_000n, '~41 XLM');
});

test('sizing: required backstop $625 at the full pilot book <= $750 seed; cash headroom arithmetic', () => {
  const P = base().params;
  const req = m.requiredBackstop(P.headroom, P.caps.XLM.side_equity_usd, P.caps.USDC.side_equity_usd, P.caps.XLM.per_position_equity_usd);
  assert.equal(req, 6_250_000_000n); assert(req <= BigInt(P.seedPtokensRaw));
  assert.equal(m.requiredBackstop(P.headroom, 0, 0, P.caps.XLM.per_position_equity_usd), 1_150_000_000n, '$115 with an empty book');
  assert.equal(m.pilotMaxDebtUsd(P.caps.XLM, P.maxLeverage.XLM), 4_000_000_000n, '$400 max debt per side');
  const h = m.cashHeadroom({borrowCap: 9_000_000_000n, totalBorrowed: 6n, borrowableCash: 8_823_001_918n}, 3_995_973_662n);
  assert.equal(h.effectiveRaw, '8823001918'); assert.equal(h.headroomAfterPilotRaw, '4827028256'); assert.equal(h.fits, true);
  assert.equal(m.cashHeadroom({borrowCap: 45_000_000_000n, totalBorrowed: 7n, borrowableCash: 77_236_942_435n}, 32_865_489_883n).headroomAfterPilotRaw, '12134510110');
  assert.equal(m.cashHeadroom({borrowCap: 100n, totalBorrowed: 0n, borrowableCash: 50n}, 60n).fits, false);
});

test('struct encodings are symbol-sorted maps with the contract field names', () => {
  const P = base().params;
  assert.deepEqual(m.feesVal(P.fees).map().map(e => e.key().sym().toString()), ['bonus_backstop', 'bonus_liquidator', 'close_fee', 'funding_apr', 'open_fee']);
  assert.deepEqual(m.capsVal(P.caps.XLM).map().map(e => e.key().sym().toString()), ['min_equity_usd', 'per_position_equity_usd', 'side_equity_usd', 'side_oi_usd']);
  assert.deepEqual(m.headroomVal(P.headroom).map().map(e => e.key().sym().toString()), ['close_reserve_usd', 'coef_largest', 'coef_long', 'coef_short', 'hf_stress', 'hf_target', 'stress']);
  const bs = m.bsParamsVal(P.backstopParams).map();
  assert.deepEqual(bs.map(e => [e.key().sym().toString(), e.val().switch().name]), [['cooldown_s', 'scvU64'], ['min_deposit_pt', 'scvU128'], ['window_s', 'scvU64']]);
});

test('kill switches: guardian may only pause; resume is keeper-gated; no deposit-pause switch exists', () => {
  const E = m.EMERGENCY, cfg = withGuardian(), M = m.marginId(cfg);
  assert.equal(E['pause-opens'].signer('guardian'), 'guardian');
  const [fn, args] = E['pause-opens'].build(cfg, M, 'guardian');
  assert.equal(fn, 'set_opens_paused'); assert.deepEqual(args.map(a => S.scValToNative(a)), [cfg.signers.guardian.recommended, true]);
  assert.equal(E['resume-opens'].keeperGate, true);
  assert(!Object.keys(E).some(k => /deposit/.test(k) && !/bs-/.test(k)), 'no cash-market deposit pause');
  for (const [k, e] of Object.entries(E)) {
    const a = k === 'set-guardian' ? [cfg.signers.guardian.recommended] : k === 'set-caps' ? ['xlm', '1', '1', '1', '0'] : /upgrade/.test(k) ? ['a'.repeat(64)] : ['1', '0'];
    const [method] = e.build(cfg, M, ...a);
    assert.doesNotThrow(() => m.assertCallAllowed(cfg, M, method), `${k} -> ${method}`);
  }
});
