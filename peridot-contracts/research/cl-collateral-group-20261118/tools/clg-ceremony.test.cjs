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
