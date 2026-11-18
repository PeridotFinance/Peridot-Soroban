'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const {S, MAINNET, addr, u128} = require('../lib.cjs');
const {steps, assertNotCl, renewClPauseOp} = require('../ceremony.cjs');
const params = require(path.join(__dirname, '..', '..', 'params.proposed.json'));
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
  assert.equal(ids.length, 35);
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
