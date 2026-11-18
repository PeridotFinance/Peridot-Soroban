'use strict';
// Offline tests for funding-preflight-v3.cjs (pre-release stage).
const test = require('node:test');
const assert = require('node:assert/strict');
const F = require('../funding-preflight-v3.cjs');
const config = {releaseStage: 'PRE_RELEASE_DEPLOYMENT_STEPS_1_TO_18', source: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP',
  initialSourceSequence: '246894391124295826', feeBudgetStroops: '3500000000', balanceFloorStroops: '50000000',
  keeperPublicAddress: 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4'};
const req = {status: 'PRE_RELEASE_FUNDING_REQUIREMENTS', xlm: {requiredBeforeStep1Stroops: '3600000000'}, pyusd: {requiredBeforeStep16Raw: '8010000000'}};
const line = balance => ({present: true, authorized: true, balance});
const observed = over => ({ledger: 1, sequence: '246894391124295826', xlmBalanceStroops: '4062066669', xlmSellingLiabilitiesStroops: '0',
  trustlines: {PYUSD: line('10146435532'), USDC: line('177062510'), AQUA: line('0')}, keeperAccountExists: true, ...over});

test('funded: XLM gate for step 1 and PYUSD gate for step 16 pass independently', () => {
  const r = F.evaluate(req, config, observed({}));
  assert.equal(r.status, 'PRE_RELEASE_FUNDED_FOR_STEP_1'); assert.equal(r.step16, 'PYUSD_READY_FOR_STEP_16');
  assert.equal(r.information.length, 1); assert.match(r.information[0], /213\.6435532 PYUSD left for the execution reserve/);
});
test('the 15 Nov 12:34 UTC wallet is blocked before step 1 with the exact shortfall', () => {
  const r = F.evaluate(req, config, observed({xlmBalanceStroops: '62066669', trustlines: {PYUSD: line('8646435532'), USDC: line('177062510'), AQUA: {present: false}}, keeperAccountExists: false}));
  assert.equal(r.status, 'BLOCKED_BEFORE_STEP_1');
  assert.match(r.shortfallsBeforeStep1[0], /short by 353\.7933331 XLM/);
  // 864.64 PYUSD covers the 800 seed + 1 floor (user decision 15 Nov ~13:15 UTC).
  assert.equal(r.step16, 'PYUSD_READY_FOR_STEP_16'); assert.equal(r.information.length, 3);
});
test('selling liabilities are not spendable; a changed sequence blocks step 1; PYUSD alone does not block step 1', () => {
  assert.equal(F.evaluate(req, config, observed({xlmSellingLiabilitiesStroops: '462066670'})).status, 'BLOCKED_BEFORE_STEP_1');
  const s = F.evaluate(req, config, observed({sequence: '246894391124295827'}));
  assert.equal(s.status, 'BLOCKED_BEFORE_STEP_1'); assert.match(s.shortfallsBeforeStep1[0], /SEQUENCE/);
  const p = F.evaluate(req, config, observed({trustlines: {PYUSD: line('8009999999'), USDC: line('1'), AQUA: line('0')}}));
  assert.equal(p.status, 'PRE_RELEASE_FUNDED_FOR_STEP_1'); assert.equal(p.step16, 'PYUSD_NOT_READY_FOR_STEP_16');
  assert.match(p.shortfallsBeforeStep16[0], /short by 0\.0000001 PYUSD/); assert.match(p.information.at(-1), /nothing is left for the execution reserve/);
  // 13:05 UTC wallet: 808.5723328 -> 7.5723328 left for the reserve after floor and seed.
  const now = F.evaluate(req, config, observed({trustlines: {PYUSD: line('8085723328'), USDC: line('1'), AQUA: line('0')}}));
  assert.equal(now.step16, 'PYUSD_READY_FOR_STEP_16'); assert.match(now.information[0], /7\.5723328 PYUSD left/);
});
test('requirements below fee budget + floor are refused', () => {
  assert.throws(() => F.evaluate({...req, xlm: {requiredBeforeStep1Stroops: '3549999999'}}, config, observed({})), /below fee budget/);
});
test('step-16 gate: only PYUSD gates mid-ceremony; spent XLM and a moved sequence are reported, not gating', () => {
  const mid = observed({xlmBalanceStroops: '700000000', sequence: '246894391124295845', trustlines: {PYUSD: line('8085723328'), USDC: line('1'), AQUA: line('0')}});
  assert.equal(F.evaluate(req, config, mid).status, 'BLOCKED_BEFORE_STEP_1');
  const r = F.evaluate(req, config, mid, 'step16');
  assert.equal(r.status, 'PYUSD_READY_FOR_STEP_16'); assert.equal(r.notGatingHere.length, 2);
  const short = F.evaluate(req, config, observed({trustlines: {PYUSD: line('8000000000'), USDC: line('1'), AQUA: line('0')}}), 'step16');
  assert.equal(short.status, 'BLOCKED_BEFORE_STEP_16'); assert.match(short.shortfallsBeforeStep16[0], /short by 1\.0000000 PYUSD/);
  assert.throws(() => F.evaluate(req, config, mid, 'step20'));
});
