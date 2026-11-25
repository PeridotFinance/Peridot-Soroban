import test from 'node:test';
import assert from 'node:assert/strict';
import * as S from '@stellar/stellar-sdk';
import {expectedLiquidation, maxPayFor, marginProfit, rankLiquidatable, applyMarginEvents, trackLiquidatable, isNewLedger,
  marginAlerts, gateAlerts, simErrorCode, liquidationFromEvents, excludedBorrower} from '../src/margin-plan.mjs';
import {decodeSimEvents} from '../src/chain.mjs';
import {FIX} from './support/margin-stub.mjs';

const fees = Object.fromEntries(Object.entries(FIX.fees).map(([k, v]) => [k, BigInt(v)]));
const PU = FIX.pricesP0.usdc.map(BigInt);
const px = a => a.map(BigInt);

test('model reproduces the fork payments: L5 underwater -20 %, L2 solvent -40 % (D + 1 % + funding), short pays D', () => {
  const l5 = FIX.checks.L5_underwater_minus20;
  const a = expectedLiquidation({side: 'long', debt: BigInt(l5.derived.debt), funding: BigInt(l5.derived.funding), heldPt: BigInt(l5.derived.heldPt)},
    {rateHeld: 1_000_000n, priceHeld: px(l5.derived.priceXlm), priceDebt: PU, fees});
  assert.equal(a.underwater, true); assert.equal(a.pay, BigInt(l5.fork.paid)); assert.equal(a.seizePt, BigInt(l5.derived.heldPt));
  const l2 = FIX.checks.L2_solvent_minus40;
  const b = expectedLiquidation({side: 'long', debt: BigInt(l2.fork.debt), funding: BigInt(l2.derived.funding), heldPt: BigInt(l2.derived.heldPt)},
    {rateHeld: 1_000_000n, priceHeld: px(l2.derived.priceXlm), priceDebt: PU, fees});
  assert.equal(b.underwater, false); assert.equal(b.pay, BigInt(l2.fork.paid));
  // backstop gets bonus 1 % (rounded up) + funding: 2,476,962 raw USDC ~ the fork's +2,476,949 pT NAV delta (USDC rate ~1.000005)
  const toBs = BigInt(l2.fork.paid) - BigInt(l2.fork.debt);
  assert(toBs - BigInt(l2.fork.backstopNavDeltaPt) >= 0n && toBs - BigInt(l2.fork.backstopNavDeltaPt) < 50n);
  const s3 = FIX.checks.S3_solvent_plus40;
  const c = expectedLiquidation({side: 'short', debt: BigInt(s3.fork.debt), funding: 999n, heldPt: 100_000_000_000n},
    {rateHeld: 1_000_000n, priceHeld: PU, priceDebt: [12424660835662n * 140n / 100n, 10n ** 14n], fees});
  assert.equal(c.pay, BigInt(s3.fork.paid));
});

test('max_pay covers the worst case plus a buffer and never exceeds inventory', () => {
  const p = {side: 'long', debt: 1_000_000_000n, funding: 5_000n};
  const {worst, maxPay} = maxPayFor(p, fees, {bufferBps: 50});
  assert.equal(worst, 1_000_000_000n + 10_000_000n + 5_000n);
  assert(maxPay > worst && maxPay <= worst + worst / 200n + 2n);
  assert.equal(maxPayFor(p, fees, {inventory: 900_000_000n}).maxPay, 900_000_000n);
  assert.equal(maxPayFor({side: 'short', debt: 7n, funding: 99n}, fees).worst, 7n);   // short: funding comes from held USDC
});

test('profit check = bonus - fees: long nets 3 % - 1 %, the tx fee is subtracted, and dust/loss is refused', () => {
  const base = {rateHeld: 1_000_000n, priceHeld: [10n ** 13n, 10n ** 14n], priceDebt: [10n ** 14n, 10n ** 14n], priceXlm: [10n ** 13n, 10n ** 14n]};
  // pay 102 USDC (D=100 + 1 % + 1 funding), seize 1,040 XLM @ $0.10 = $104 -> +$2, fee 0.2 XLM = $0.02
  const ok = marginProfit({...base, pay: 1_020_000_000n, seizePt: 10_400_000_000n, feeStroops: 2_000_000n});
  assert.equal(ok.ok, true); assert.equal(ok.profitUsd, 20_000_000n - 200_000n); assert.equal(ok.bps, 194);
  assert.equal(marginProfit({...base, pay: 1_020_000_000n, seizePt: 10_400_000_000n, feeStroops: 2_000_000n, minProfitBps: 200}).ok, false);
  // dust: $0.10 bonus does not cover a 2 XLM fee
  assert.equal(marginProfit({...base, pay: 5_000_000n, seizePt: 60_000_000n, feeStroops: 20_000_000n}).ok, false);
  assert.equal(marginProfit({...base, pay: 1_020_000_000n, seizePt: 10_200_000_000n, feeStroops: 0n}).ok, false);   // break-even is not profit
});

test('liquidatable = LTV >= own liq_ltv (boundary inclusive), worst ratio first', () => {
  const r = rankLiquidatable([
    {id: '2', ltv: 832_790n, liqLtv: 833_113n},     // fork L3 at -20 %: just below its own liq LTV
    {id: '1', ltv: 1_000_165n, liqLtv: 899_983n},   // fork L5
    {id: '4', ltv: 961_666n, liqLtv: 900_633n},     // fork S5a at +20 %
    {id: '7', ltv: 900_000n, liqLtv: 900_000n},     // boundary
    {id: '9', ltv: null, liqLtv: 1n},
  ]);
  assert.deepEqual(r.map(p => p.id), ['1', '4', '7']);
});

test('discovery: margin_open adds data[0], close/liquidate remove; backstop requests tracked; impaired flagged', () => {
  const ev = [
    {ledger: 1, name: 'margin_open', data: [5n, 'G..', 'X', 1n, 1n, 1n, 1n]}, {ledger: 2, name: 'margin_open', data: [6n]},
    {ledger: 3, name: 'margin_close', data: [5n, 1n, 1n, 1n]}, {ledger: 4, name: 'margin_liquidate', data: [1n, 'G', 'G', 1n, 1n, 0n]},
    {ledger: 5, name: 'bs_request', data: ['GB', 10n, 100n, 200n]}, {ledger: 6, name: 'bs_request', data: ['GC', 1n, 1n, 2n]},
    {ledger: 7, name: 'bs_withdraw', data: ['GC', 1n, 1n]}, {ledger: 8, name: 'margin_impaired', data: ['G', 1n, 0n, 1n, 1n]},
  ];
  const r = applyMarginEvents({active: ['1'], backers: {}}, ev);
  assert.deepEqual([...r.active].sort(), ['6']); assert.deepEqual(r.backers, {GB: '200'}); assert.equal(r.impairedSeen, true);
});

test('CAP-88: same timestamp + new sequence is a new observation; the same sequence twice is not; timer uses the keeper clock', () => {
  const t0 = 1_000_000;
  let s = trackLiquidatable({}, ['1'], 100, t0);
  s = trackLiquidatable(s, ['1'], 100, t0 + 30_000);      // same ledger sequence seen again (e.g. two cycles, slow RPC)
  assert.equal(s['1'].observations, 1); assert.equal(s['1'].firstSeenMs, t0);
  s = trackLiquidatable(s, ['1'], 101, t0 + 31_000);      // next ledger, SAME close timestamp (sub-second ledgers)
  s = trackLiquidatable(s, ['1'], 102, t0 + 32_000);
  assert.equal(s['1'].observations, 3); assert.equal(s['1'].lastLedger, 102);
  assert.equal(trackLiquidatable(s, [], 103, t0)['1'], undefined);   // recovered: episode ends
  assert.equal(isNewLedger(null, 5), true); assert.equal(isNewLedger(5, 5), false); assert.equal(isNewLedger(5, 6), true);
  // repeated ledger timestamps never freeze the 10-minute timer: it is the keeper clock, not a ledger-timestamp delta
  const a = marginAlerts({liqSince: {'1': {firstSeenMs: t0, firstLedger: 100, observations: 3}}}, {liquidatableAlertMs: 600_000, hfDownMin: 1_100_000n}, t0 + 600_001);
  assert.deepEqual(a.map(x => x.key), ['liquidatable:1']);
  assert.deepEqual(marginAlerts({liqSince: {'1': {firstSeenMs: t0}}}, {liquidatableAlertMs: 600_000, hfDownMin: 1_100_000n}, t0 + 600_000), []);
});

test('alerts: pooled_health vs hf_target and hf_down 1.10, impaired, refused opens, inventory per side', () => {
  const th = {liquidatableAlertMs: 600_000, hfDownMin: 1_100_000n};
  const quiet = {health: {hf: 3_779_962n, hfDown: 4_319_158n}, headroom: {hf_target: 1_250_000n}, bsShares: {USDC: 0n, XLM: 0n}, impaired: false,
    positions: [{side: 'long', worstPay: 100n}, {side: 'short', worstPay: 50n}], inventory: {USDC: 100n, XLM: 50n}};
  assert.deepEqual(marginAlerts(quiet, th, 0), []);   // fork -40 % pooled health: fine
  const keys = s => marginAlerts({...quiet, ...s}, th, 0).map(a => a.key);
  assert.deepEqual(keys({health: {hf: 1_249_999n, hfDown: 2_000_000n}}), ['pooled_health']);
  assert.deepEqual(keys({health: {hf: 2_000_000n, hfDown: 1_099_999n}}), ['pooled_health']);
  assert.deepEqual(keys({health: {hf: 981_084n, hfDown: 900_000n}}), ['pooled_health']);   // fork seizure state
  assert.deepEqual(keys({health: {hf: (1n << 128n) - 1n, hfDown: (1n << 128n) - 1n}}), []);  // no debt
  assert.deepEqual(keys({impaired: true}), ['impaired']);
  assert.deepEqual(keys({bsShares: {USDC: 0n, XLM: 4_490_772_757n}}), ['opens_refused']);
  assert.deepEqual(keys({inventory: {USDC: 99n, XLM: 50n}}), ['inventory:USDC']);
  assert.deepEqual(keys({inventory: {USDC: 100n, XLM: 49n}}), ['inventory:XLM']);
});

test('alert gate: first sight fires, repeats are rate-limited, cleared keys re-arm', () => {
  const a = [{key: 'x'}];
  let g = gateAlerts({}, a, 0, 600_000); assert.equal(g.fire.length, 1);
  g = gateAlerts(g.last, a, 30_000, 600_000); assert.equal(g.fire.length, 0);
  g = gateAlerts(g.last, a, 600_000, 600_000); assert.equal(g.fire.length, 1);
  g = gateAlerts(g.last, [], 610_000, 600_000); assert.deepEqual(g.last, {});
  assert.equal(gateAlerts(g.last, a, 620_000, 600_000).fire.length, 1);
});

test('sim events: seize from margin_liquidate data[4], pay from the keeper net debt-token transfers; decoded from real XDR', () => {
  const K = 'GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQDZ7H', MC = 'CADQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQP5KR';
  const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
  const ev = (contract, topics, data) => new S.xdr.DiagnosticEvent({inSuccessfulContractCall: true, event: new S.xdr.ContractEvent({
    ext: new S.xdr.ExtensionPoint(0), contractId: S.StrKey.decodeContract(contract), type: S.xdr.ContractEventType.contract(),
    body: new S.xdr.ContractEventBody(0, new S.xdr.ContractEventV0({topics, data}))})});
  const a = v => S.nativeToScVal(v, {type: 'address'}), i = v => S.nativeToScVal(v, {type: 'i128'}), u = v => S.nativeToScVal(v, {type: 'u128'});
  const diags = [
    ev(USDC, [S.xdr.ScVal.scvSymbol('transfer'), a(K), a(MC), S.nativeToScVal('USDC:GA5Z')], i(1_000n)),
    ev(USDC, [S.xdr.ScVal.scvSymbol('transfer'), a(MC), a(K), S.nativeToScVal('USDC:GA5Z')], i(7n)),     // refund of d - repaid + dust
    ev(MC, [S.xdr.ScVal.scvSymbol('margin_liquidate')], S.xdr.ScVal.scvVec([S.nativeToScVal(3n, {type: 'u64'}), a(K), a(K), u(990n), u(12_345n), u(0n)])),
  ];
  const decoded = decodeSimEvents(diags.map(d => d.toXDR('base64')));
  assert.equal(decoded.length, 3);
  assert.deepEqual(liquidationFromEvents(decoded, {margin: MC, keeper: K, debtToken: USDC}), {seizePt: 12_345n, repaid: 990n, paid: 993n});
  assert.equal(liquidationFromEvents(decoded.slice(0, 2), {margin: MC, keeper: K, debtToken: USDC}), null);   // no event -> model fallback
  assert.deepEqual(decodeSimEvents(['garbage']), []);
});

test('error codes and the cash-pass exclusion', () => {
  assert.equal(simErrorCode('HostError: Error(Contract, #35)'), 35); assert.equal(simErrorCode('x'), null);
  const never = new Set(['CMARGIN']);
  assert.equal(excludedBorrower(never, 'CMARGIN'), true); assert.equal(excludedBorrower(never, 'GUSER'), false);
});
