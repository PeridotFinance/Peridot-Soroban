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
test('steps are numbered G1..G61 in order, switch after the liquidator gate, latches last', () => {
  const st = m.steps(base()), signed = st.filter(s => s.n);
  assert.equal(signed.length, 61);
  signed.forEach((s, i) => assert.equal(s.n, `G${i + 1}`));
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
