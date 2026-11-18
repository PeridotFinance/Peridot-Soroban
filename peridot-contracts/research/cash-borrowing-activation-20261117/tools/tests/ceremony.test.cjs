'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const {S, MAINNET, addr, u128} = require('../lib.cjs');
const {steps, assertNotCl, renewClPauseOp, assertPauseKey, checkCapsVsInventory, checkFeedMeta, emergencyOp} = require('../ceremony.cjs');
const base = require(path.join(__dirname, '..', '..', 'params.proposed.json'));
// The real pause key is filled in later; tests use a throwaway generated public key (no secret is kept).
const PAUSE = S.Keypair.random().publicKey();
const params = {...base, pauseGuardian: {...base.pauseGuardian, publicKey: PAUSE}};
const op = (contract, method, ...args) => {
  const tx = new S.TransactionBuilder(new S.Account(MAINNET.admin, '1'), {fee: '100', networkPassphrase: MAINNET.network})
    .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(0).build();
  return tx.toEnvelope().v1().tx().operations()[0];
};

test('order: caps, fees, rates, symbols, oracle, gate, CFs, floors, liquidator gate, switch last', () => {
  const ids = steps(MAINNET, params).map(s => s.id);
  const idx = p => ids.findIndex(i => i.startsWith(p) || i.includes(p));
  assert(idx('set_supply_cap') < idx('set_borrow_cap') && idx('set_borrow_cap') < idx('enable_static_rates'));
  assert(idx('controller.set_oracle_asset_symbol') < ids.indexOf('controller.set_oracle'));
  assert(ids.indexOf('controller.set_oracle') < ids.indexOf('gate.price-readback'));
  assert(ids.indexOf('gate.price-readback') < idx('controller.set_market_cf'));
  assert(idx('set_min_cash_reserve_bps') < ids.indexOf('gate.liquidator-ready'));
  const sw = ids.filter(i => i.endsWith('set_cash_borrowing_enabled'));
  assert.deepEqual(sw, ['USDC.set_cash_borrowing_enabled', 'XLM.set_cash_borrowing_enabled', 'EURC.set_cash_borrowing_enabled']);
  assert.equal(ids.slice(-3).join(), sw.join());
  assert.equal(ids.length, 36);
});

test('PYUSD is never made borrowable and gets no CF, rate or switch step', () => {
  const py = steps(MAINNET, params).filter(s => s.market === 'PYUSD').map(s => s.method);
  assert.deepEqual(py.sort(), ['set_flash_loan_fee', 'set_supply_cap']);
});

test('no step targets or mentions the CL staged receipt; no fallback or static price selectors', () => {
  for (const s of steps(MAINNET, params).filter(s => s.kind === 'tx')) {
    assert.notEqual(s.target, MAINNET.clStagedReceipt);
    assert(!/fallback|set_price/.test(s.method));
    assertNotCl(MAINNET, op(s.target, s.method, ...s.args()));
  }
});

test('assertNotCl refuses the CL receipt as target or argument', () => {
  assert.throws(() => assertNotCl(MAINNET, op(MAINNET.clStagedReceipt, 'set_borrow_cap', u128(1))), /CL staged receipt/);
  assert.throws(() => assertNotCl(MAINNET, op(MAINNET.controller, 'set_market_cf', addr(MAINNET.clStagedReceipt), u128(500000))), /CL staged receipt/);
  assert.throws(() => assertNotCl(MAINNET, op(MAINNET.controller, 'set_pause_borrow', addr(MAINNET.clStagedReceipt), S.xdr.ScVal.scvBool(true))), /CL staged receipt/);
});

test('set_market_cf refuses a market whose CF is already set (later edits are timelocked)', () => {
  const s = steps(MAINNET, params).find(x => x.id === 'controller.set_market_cf.USDC');
  assert.throws(() => s.pre({markets: {USDC: {controllerCF: {value: 750000n}}}}), /timelocked/);
  s.pre({markets: {USDC: {controllerCF: null}}});
});

test('renew-cl-pause builds exactly controller.set_pause_borrow(CL staged receipt, true) and nothing else', () => {
  const ic = renewClPauseOp(MAINNET).body().invokeHostFunctionOp().hostFunction().invokeContract();
  assert.equal(S.Address.fromScAddress(ic.contractAddress()).toString(), MAINNET.controller);
  assert.equal(ic.functionName().toString(), 'set_pause_borrow');
  assert.deepEqual(ic.args().map(a => S.scValToNative(a)), [MAINNET.clStagedReceipt, true]);
  assert.throws(() => renewClPauseOp({...MAINNET, clStagedReceipt: undefined}), /no CL staged receipt/);
});

test('guardian step: after the reserve floors, before the liquidator gate and the switch; Ledger-signed set_pause_guardian(pause key)', () => {
  const list = steps(MAINNET, params), ids = list.map(s => s.id), i = ids.indexOf('controller.set_pause_guardian');
  assert(i > ids.lastIndexOf('EURC.set_min_cash_reserve_bps') && i < ids.indexOf('gate.liquidator-ready'));
  const s = list[i];
  assert.equal(s.target, MAINNET.controller); assert.equal(s.method, 'set_pause_guardian');
  assert.deepEqual(s.args().map(a => S.scValToNative(a)), [PAUSE]);
  assert.equal(s.done({controller: {PauseGuardian: {value: MAINNET.admin}}}), false);   // live today: the Ledger is guardian
  assert.equal(s.done({controller: {PauseGuardian: {value: PAUSE}}}), true);
  assert.equal(s.done({controller: {PauseGuardian: null}}), false);
});

test('guardian step refuses a missing, malformed, admin or liquidator key', () => {
  assert.throws(() => assertPauseKey(MAINNET, base), /pauseGuardian.publicKey not set/);
  assert.throws(() => steps(MAINNET, base).find(s => s.id === 'controller.set_pause_guardian').args(), /not set/);
  assert.throws(() => assertPauseKey(MAINNET, {...base, pauseGuardian: {publicKey: 'GNOTAKEY'}}), /valid G-address/);
  assert.throws(() => assertPauseKey(MAINNET, {...base, pauseGuardian: {publicKey: MAINNET.admin}}), /admin/);
  const lq = S.Keypair.random().publicKey();
  assert.throws(() => assertPauseKey(MAINNET, {...base, liquidator: {...base.liquidator, publicKey: lq}, pauseGuardian: {publicKey: lq}}), /liquidator/);
  assert.equal(assertPauseKey(MAINNET, params), PAUSE);
});

test('params without pauseGuardian (Testnet rehearsal) keep the historical 35-step list', () => {
  const {pauseGuardian, ...old} = base;
  assert.equal(steps(MAINNET, old).length, 35);
});

test('borrow caps are covered by liquidator inventory at the close factor with the interest buffer', () => {
  const rows = checkCapsVsInventory(params);
  assert.deepEqual(Object.keys(rows).sort(), ['EURC', 'USDC', 'XLM']);
  assert.equal(rows.USDC.withBufferRaw, '4950000000');       // 900 x 50% x 1.10 = 495 USDC <= 500
  assert.equal(rows.XLM.withBufferRaw, '24750000000');       // 4,500 x 50% x 1.10 = 2,475 XLM <= 2,500
  assert.equal(rows.EURC.withBufferRaw, '2475000000');       // 450 x 50% x 1.10 = 247.5 EURC <= 250
  const old = {...params, markets: {...params.markets, USDC: {...params.markets.USDC, borrowCapRaw: '10000000000'}}};
  assert.throws(() => checkCapsVsInventory(old), /USDC: cap x close factor x buffer/);   // the previous 1,000 USDC cap fails
});

test('D1 feed pins: USD base, 14 decimals, 300 s, k=2 -> 600 s; PYUSD must stay unpriced', () => {
  const good = {base: ['Other', 'USD'], decimals: 14, resolution: 300, k: 2, prices: {USDC: {price: 1n}, XLM: {price: 1n}, EURC: {price: 1n}, PYUSD: null}};
  assert.deepEqual(checkFeedMeta(good, params), {ok: true, errs: []});
  assert.match(checkFeedMeta({...good, base: ['Stellar', 'CCW67']}, params).errs.join(), /base/);       // Stellar DEX feed is USDC-based
  assert.match(checkFeedMeta({...good, resolution: 60}, params).errs.join(), /resolution/);
  assert.match(checkFeedMeta({...good, k: 3}, params).errs.join(), /multiplier/);
  assert.match(checkFeedMeta({...good, prices: {...good.prices, PYUSD: {price: 1n}}}, params).errs.join(), /PYUSD unexpectedly priced/);
  assert.match(checkFeedMeta({...good, prices: {...good.prices, EURC: null}}, params).errs.join(), /EURC/);
});

test('emergency builders: K1 durable switch, K2/unpause via the admin set_pause_borrow, revoke resets the guardian to the Ledger', () => {
  const dec = o => { const ic = o.body().invokeHostFunctionOp().hostFunction().invokeContract();
    return [S.Address.fromScAddress(ic.contractAddress()).toString(), ic.functionName().toString(), ic.args().map(a => S.scValToNative(a))]; };
  const R = MAINNET.markets.USDC.receipt;
  assert.deepEqual(dec(emergencyOp(MAINNET, params, 'kill', 'USDC')), [R, 'set_cash_borrowing_enabled', [MAINNET.admin, false]]);
  assert.deepEqual(dec(emergencyOp(MAINNET, params, 'pause-borrow', 'USDC')), [MAINNET.controller, 'set_pause_borrow', [R, true]]);
  assert.deepEqual(dec(emergencyOp(MAINNET, params, 'unpause-borrow', 'USDC')), [MAINNET.controller, 'set_pause_borrow', [R, false]]);
  assert.deepEqual(dec(emergencyOp(MAINNET, params, 'revoke-guardian')), [MAINNET.controller, 'set_pause_guardian', [MAINNET.admin]]);
  assert.throws(() => emergencyOp(MAINNET, params, 'pause-borrow', 'PYUSD'), /borrowable/);
  for (const cmd of ['kill', 'pause-borrow', 'unpause-borrow']) for (const n of ['USDC', 'XLM', 'EURC']) assertNotCl(MAINNET, emergencyOp(MAINNET, params, cmd, n));
});
