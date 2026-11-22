'use strict';
// Offline tests (no network): node --test clg-ceremony.test.cjs
const test = require('node:test');
const assert = require('node:assert/strict');
const m = require('./clg-ceremony.cjs');
const base = () => JSON.parse(require('node:fs').readFileSync(require('node:path').join(__dirname, 'clg-mainnet.json'), 'utf8'));

test('contract ids are deterministic and distinct', () => {
  const a = m.ids(base()), b = m.ids(base());
  assert.deepEqual(a, b);
  assert.equal(new Set(Object.values(a)).size, 4);
  for (const v of Object.values(a)) assert.match(v, /^C[A-Z2-7]{55}$/);
});
test('launch caps pass; a larger borrow cap or smaller inventory is refused', () => {
  assert.equal(m.checkCaps(base()).inventory.ok, true);
  const c1 = base(); c1.params.usdcMarket.borrowCapRaw = '3000000000'; assert.throws(() => m.checkCaps(c1), /inventory|below the borrow cap/);
  const c2 = base(); c2.params.funding.liquidatorUsdcRaw = '1000000000'; assert.throws(() => m.checkCaps(c2), /inventory/);
  const c3 = base(); c3.params.capCoverage.worstStage.entries = 86; assert.throws(() => m.checkCaps(c3), /85 %/);
  const c4 = base(); c4.params.controller.liquidationFeeScaled = '50000'; assert.throws(() => m.checkCaps(c4), /fee must be 0/);
  const c5 = base(); c5.params.receipt.cfScaled = '600000'; assert.throws(() => m.checkCaps(c5), /50 %/);
});
test('steps are numbered G1..G60 (+FB1-FB3 fallback) in order, switch after the liquidator gate, latches last', () => {
  const st = m.steps(base()), signed = st.filter(s => s.n);
  const main = signed.filter(s => !s.fallback), fbs = signed.filter(s => s.fallback);
  assert.equal(main.length, 60);
  main.forEach((s, i) => assert.equal(s.n, `G${i + 1}`));
  assert.deepEqual(fbs.map(s => s.n), ['FB1', 'FB2', 'FB3']);
  const idx = id => st.findIndex(s => s.id === id);
  assert(idx('gate.liquidator') < idx('usdcMarket.set_cash_borrowing_enabled'));
  assert(idx('gate.first-liquidation') < idx('pilot.close'));
  assert(idx('gate.migrated') < idx('pilot.receipt.stage_pause_deposits'));
  assert.equal(st.at(-1).id, 'pilot.vault.stage_pause_deposits');
});
test('retired contracts and foreign targets are refused', () => {
  const cfg = base();
  assert.throws(() => m.assertTargetAllowed(cfg, 'CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX'), /retired/);
  assert.throws(() => m.assertTargetAllowed(cfg, cfg.pilot.vault), /not one of the four/);
  assert.doesNotThrow(() => m.assertTargetAllowed(cfg, cfg.pilot.vault, {pilotLatch: true}));
  assert.doesNotThrow(() => m.assertTargetAllowed(cfg, m.ids(cfg).receipt));
});
test('Stg2Config is a symbol-sorted map', () => {
  const cfg = base(), v = m.stg2Config('CDSJ3ACUWASAHCPRTXJLU3M452OBFXRVSPS2QLKOFUUHKLCKLIDFTEOA', cfg.deps);
  assert.deepEqual(v.map().map(e => e.key().sym().toString()), ['controller', 'oracle', 'pool', 'usdc']);
});
test('F4: route A is refused for a boundary opened by a seize sync, or when events cannot be proven', () => {
  const S = require('/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
  const from = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP', to = 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4';
  const val = S.nativeToScVal([S.Address.fromString(from), S.Address.fromString(to), S.nativeToScVal(5n, {type: 'u128'}), S.xdr.ScVal.scvBool(true)]);
  const raw = [{value: val.toXDR('base64'), ledger: 100, ledgerClosedAt: '2026-11-22T15:00:00Z', txHash: 'ab'}];
  const ev = m.decodeSeizeEvents(raw), ts = Date.parse('2026-11-22T15:00:00Z') / 1000;
  assert.equal(ev[0].opened, true); assert.equal(ev[0].from, from);
  const cov = {oldestLedgerCloseTime: String(ts - 1000)};
  assert.throws(() => m.assertRouteAAllowed('3', {holder: from, timestamp: String(ts)}, ev, cov), /F4/);
  assert.equal(m.assertRouteAAllowed('2', {holder: from, timestamp: String(ts - 5)}, ev, cov), true);
  assert.equal(m.assertRouteAAllowed('4', {holder: to, timestamp: String(ts)}, ev, cov), true);
  const closed = m.decodeSeizeEvents([{...raw[0], value: S.nativeToScVal([S.Address.fromString(from), S.Address.fromString(to), S.nativeToScVal(5n, {type: 'u128'}), S.xdr.ScVal.scvBool(false)]).toXDR('base64')}]);
  assert.equal(m.assertRouteAAllowed('3', {holder: from, timestamp: String(ts)}, closed, cov), true);
  assert.throws(() => m.assertRouteAAllowed('3', {holder: from, timestamp: String(ts)}, ev, null), /could not be read/);
  assert.throws(() => m.assertRouteAAllowed('1', {holder: from, timestamp: String(ts - 2000)}, [], cov), /older than the RPC event window/);
  assert.throws(() => m.assertRouteAAllowed('9', null, [], cov), /does not exist/);
});
test('liquidator gate needs BOTH trustlines with the exact issuers (PYUSD missing -> SAC #13 on emergency pay)', () => {
  const cfg = base(), U = {asset_code: 'USDC', asset_issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN', is_authorized: true};
  const P = {asset_code: 'PYUSD', asset_issuer: 'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5', is_authorized: true};
  assert.equal(m.checkTrustlines([{asset_type: 'native'}, U, P], cfg).ok, true);
  assert.equal(m.checkTrustlines([U], cfg).ok, false);
  assert.equal(m.checkTrustlines([U], cfg).rows.PYUSD.present, false);
  assert.equal(m.checkTrustlines([P], cfg).ok, false);
  assert.equal(m.checkTrustlines([U, {...P, asset_issuer: 'GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA'}], cfg).ok, false);
  assert.equal(m.checkTrustlines([U, {...P, is_authorized: false}], cfg).ok, false);
  assert.equal(m.checkTrustlines([], cfg).ok, false);
});
test('first liquidation: raise, exact-limit borrow, mandatory reset next to it, fallback after', () => {
  const cfg = base(), st = m.steps(cfg), idx = id => st.findIndex(s => s.id === id), get = id => st[idx(id)];
  assert(idx('rate.up') < idx('team.checkpoint_nav') && idx('team.checkpoint_nav') < idx('team.borrow'));
  assert(idx('team.borrow') < idx('team.repay') && idx('team.repay') < idx('rate.down') && idx('rate.down') < idx('pilot.close'));
  assert.equal(get('rate.up').n, 'G52'); assert.equal(get('rate.down').n, 'G56');
  assert.equal(get('rate.up').resetCommand, 'step g56-rate-down prepare rate.down');
  assert.equal(get('rate.up').preSim, 'rate-both-directions');
  assert.equal(get('rate.down').fast, true, 'the reset never waits for the liquidation gate');
  assert.equal(get('team.borrow').amount, 'account_liquidity');
  for (const id of ['liq.disable_cf', 'cf.propose', 'cf.execute']) assert.equal(get(id).fallback, true);
});
test('rate plan: cap 1000 %, reset exactly 8 %, supply rate must allow the reset, window <= 30 min', () => {
  const P = base().params;
  assert.equal(m.checkRatePlan(P, 0n), true);
  assert.throws(() => m.checkRatePlan(P, 80001n), /supply rate/);
  assert.throws(() => m.checkRatePlan({...P, firstLiquidation: {...P.firstLiquidation, raisedRateScaled: '10000001'}}, 0n), /cap/);
  assert.throws(() => m.checkRatePlan({...P, firstLiquidation: {...P.firstLiquidation, maxRaiseMinutes: 45}}, 0n), /30 min/);
  assert.throws(() => m.checkRatePlan({...P, usdcMarket: {...P.usdcMarket, borrowRateScaled: '90000'}}, 0n), /8 %/);
  assert.equal(m.rateWindow(0, 24 * 60, P).level, 'ok');
  assert.equal(m.rateWindow(0, 25 * 60, P).level, 'WARN');
  assert.equal(m.rateWindow(0, 30 * 60, P).level, 'OVERDUE');
});
test('exact-limit borrow: USD value never exceeds the liquidity, leftover below one raw unit', () => {
  for (const [liq, price, scale] of [[249_900_000n, 100_172_340_000_000n, 100_000_000_000_000n], [1n, 99_990_000_000_000n, 100_000_000_000_000n], [123_456_789n, 100_000_000_000_000n, 100_000_000_000_000n]]) {
    const a = m.borrowAtLimit(liq, price, scale);
    assert(a * price / scale <= liq);
    assert((a + 1n) * price / scale > liq);
  }
});
test('entry line: 85 for every step, 91 only for exit paths, exceptions only when pinned by step id', () => {
  const cfg = base();
  assert.equal(m.ENTRY_LINE, 85); assert.equal(m.EXIT_PATH_MAX_ENTRIES, 91);
  assert.equal(cfg.limits.entryLine, 85); assert.equal(cfg.limits.exitPathMaxEntries, 91);
  assert.deepEqual(cfg.limits.entryExceptions, {}, 'no launch step has an exception');
  assert.equal(m.entriesOf({keys: 63, writes: 17}), 80);
  assert.equal(m.checkEntries({keys: 63, writes: 17}, m.entryLimitFor(cfg, 'any'), 'L3-like'), 80);
  assert.equal(m.checkEntries({keys: 70, writes: 15}, 85, 'edge'), 85);
  assert.throws(() => m.checkEntries({keys: 70, writes: 16}, m.entryLimitFor(cfg, 'team.borrow'), 'team.borrow'), /86 entries .* above the 85-entry line/);
  assert.equal(m.entryLimitFor(cfg, 'x', {exitPath: true}), 91);
  assert.equal(m.checkEntries({keys: 60, writes: 31}, m.entryLimitFor(cfg, 'exit', {exitPath: true}), 'exit'), 91);
  assert.throws(() => m.checkEntries({keys: 61, writes: 31}, m.entryLimitFor(cfg, 'exit', {exitPath: true}), 'exit'), /above the 91-entry line/);
  const c2 = base(); c2.limits.entryExceptions = {'team.open': {measured: 88, allowance: 88}};
  assert.equal(m.entryLimitFor(c2, 'team.open'), 88); assert.equal(m.entryLimitFor(c2, 'team.borrow'), 85);
  const c3 = base(); c3.limits.entryExceptions = {'team.open': {measured: 90, allowance: 88}};
  assert.throws(() => m.entryLimitFor(c3, 'team.open'), /bad entry exception/);
  for (const s of m.steps(cfg)) assert.notEqual(s.exitPath, true, `${s.id} must not use the exit-path ceiling`);
});
