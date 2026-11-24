// CL collateral group Mainnet launch ceremony builder (fresh-stack option A). Sibling of
// research/cash-borrowing-activation-20261117/tools/ceremony.cjs, reusing its read-only lib.cjs.
// ONE unsigned transaction at a time, built from a fresh live-Mainnet simulation taken right before it is written.
// It never signs, never sends and never reads keys. The operator signs on the Ledger (clg-run.sh `step`).
//
//   addresses                         deterministic ids of the four new contracts (admin + salt; no network)
//   list                              ordered steps (G-numbers) with live done/pending status
//   simulate-all [--allow-unfrozen]   read-only: simulate every pending step whose target exists (fees, resources)
//   prepare <step> --out F.unsigned.xdr   fresh simulation; every earlier step must read back done
//   readback <step>                   live post-state check (exit 1 if not done)
//   gate <prices|liquidator|cf-eta>   non-transaction gates
//   caps                              offline: cap rule (inventory x close factor) + 85 % stage rule + liquidity rule
//   emergency <name> [on|off] --out F kill switches (see EMERGENCY below); never blocked by earlier steps
//   owner-open <admin|team3> <pyusdRaw> --out F   stage_open on the NEW receipt from a fresh vault v2 quote
//   repay <team3> --out F             repay the signer's whole USDC debt on the new USDC market
//   pending <vault|receipt|controller|usdcMarket>  read PendingUpgradeHash / ETA (persistent keys)
//   propose-upgrade|execute-upgrade|cancel-upgrade <target> <sha256> --out F   24 h upgrade path (PLACEHOLDER ABI)
//   verify-signed F.unsigned.xdr F.signed.xdr   same hash, exactly one valid signature by the expected source
//   reward-boundary <id>              read-only: boundary, F4 verdict (opened by a seize sync?), interval
//   reward-route-a <propose|attribute> <boundaryId> <raw> <evidenceHex> --out F   REFUSED for seize-opened boundaries (F4)
//   reward-route-b <propose|attribute> <segmentId> <raw> <evidenceHex> --out F    the release path after a seizure
//   reward-prepare <segmentId> [limit] / reward-settle <holder> <segmentId> <checkpoint> --out F   route B, permissionless
//   first-touch <cache-price|nav> --out F   G41a / G49a: admin pays the first (rent-bearing) cache_price / checkpoint_nav
//   trust-liquidator --out F          FALLBACK only: unsigned changeTrust USDC + PYUSD for the liquidator. The normal path
//                                     is src/trustlines.mjs on the host (LAUNCH.md section 6.3a)
//   rate-window [hash]                minutes since the first-liquidation rate raise: ok / WARN (25) / OVERDUE (30)
//   status <hash>                     RPC outcome (read-only)
//
// Refusals: any retired pre-incident contract as target or argument; the pilot vault/receipt except their two
// deposit latches; any cash market except the owner's own withdraw; unfrozen artifacts for uploads/deploys.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const CASH_TOOLS = process.env.CLG_CASH_TOOLS || '/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/research/cash-borrowing-activation-20261117/tools';
const {S, u, json, addr, u128, u32, bool, sym, key, chain, padResourceFee} = require(path.join(CASH_TOOLS, 'lib.cjs'));

const CFG_FILE = process.env.CLG_CONFIG || path.join(__dirname, 'clg-mainnet.json');
const sha = b => crypto.createHash('sha256').update(b).digest();
const saltOf = label => sha(Buffer.from(label, 'utf8'));
const u64v = x => S.nativeToScVal(u(x), {type: 'u64'});
const some = v => v; // Option<T>::Some(v) is the bare value in Soroban ScVal
const optAddr = a => (a == null ? S.xdr.ScVal.scvVoid() : addr(a));

// ---------------------------------------------------------------- pure (unit-tested)
function contractIdFor(network, deployer, label) {
  const preimage = S.xdr.HashIdPreimage.envelopeTypeContractId(new S.xdr.HashIdPreimageContractId({
    networkId: sha(Buffer.from(network, 'utf8')),
    contractIdPreimage: S.xdr.ContractIdPreimage.contractIdPreimageFromAddress(new S.xdr.ContractIdPreimageFromAddress({
      address: S.Address.fromString(deployer).toScAddress(), salt: saltOf(label)})),
  }));
  return S.StrKey.encodeContract(sha(preimage.toXDR()));
}
function ids(cfg) {
  const o = {};
  for (const [k, label] of Object.entries(cfg.salts)) o[k] = contractIdFor(cfg.network, cfg.admin, label);
  return o;
}
const xlm = s => BigInt(Math.round(Number(s) * 1e7));
const fmt7 = r => (Number(u(r)) / 1e7).toFixed(7);

// Cap rules (mirrors cash checkCapsVsInventory, plus the CL stage rule and the market-liquidity rule). Throws on violation.
function checkCaps(cfg) {
  const p = cfg.params, cc = p.capCoverage, rows = {};
  const cf = u(p.controller.closeFactorScaled), buf = u(cc.bufferScaled), cap = u(p.usdcMarket.borrowCapRaw);
  const r1 = cap * cf / 1_000_000n, r2 = (cap - r1) * cf / 1_000_000n;
  const need = (r1 + (cc.secondRound ? r2 : 0n)) * buf / 1_000_000n, inv = u(p.funding.liquidatorUsdcRaw);
  rows.inventory = {borrowCapRaw: String(cap), firstRoundRaw: String(r1), secondRoundRaw: String(r2), withBufferRaw: String(need), inventoryRaw: String(inv), ok: need <= inv};
  assert(need <= inv, `borrow cap x close factor x buffer ${need} exceeds CL liquidator USDC inventory ${inv}`);
  const w = cc.worstStage, lim = {entries: Math.floor(cc.stageEntriesMax * cc.liveShare), cpu: Math.floor(cc.stageCpuMax * cc.liveShare), writes: Math.floor(cc.stageWritesMax * cc.liveShare)};
  rows.stage = {stage: w.name, entries: w.entries, cpu: w.cpu, writes: w.writes, limits: lim, source: w.source,
    ok: w.entries <= lim.entries && w.cpu <= lim.cpu && w.writes <= lim.writes};
  assert(rows.stage.ok, `worst liquidation stage ${json(w)} above 85 % of the internal limits ${json(lim)}`);
  assert.equal(String(p.controller.liquidationFeeScaled), '0', 'liquidation fee must be 0 (fee on adds 4 entries to L3)');
  const supply = u(p.usdcMarket.adminSupplyRaw), borrowable = supply * BigInt(10000 - p.usdcMarket.minCashReserveBps) / 10000n;
  rows.liquidity = {adminSupplyRaw: String(supply), borrowableAtReserveRaw: String(borrowable), borrowCapRaw: String(cap), ok: borrowable >= cap};
  assert(borrowable >= cap, 'group USDC supply x (1 - reserve) is below the borrow cap');
  assert(u(p.usdcMarket.adminSupplyRaw) + u(p.usdcMarket.seedRaw) <= u(p.usdcMarket.supplyCapRaw), 'supply cap below seed + admin supply');
  const backing = u(p.receipt.backingCapRaw), rcf = u(p.receipt.cfScaled);
  rows.collateral = {backingCapRaw: String(backing), cfScaled: String(rcf), maxBorrowFromCapRaw: String(backing * rcf / 1_000_000n),
    note: 'backing cap is PYUSD cost basis; it bounds CL units, it does not bound USDC debt'};
  assert(rcf > 0n && rcf <= 500_000n, 'launch CF for the vault units must be in (0, 50 %]');
  assert(u(p.team.borrowUsdcRaw) * 1_000_000n < u(p.team.openPyusdRaw) * rcf * 98n / 100n, 'team borrow above 98 % of its CF limit');
  return rows;
}

// ---------------------------------------------------------------- step model
// signer: admin | team3. auth: none | root | root+transfer | create | open | classic.
function steps(cfg) {
  const I = ids(cfg), D = cfg.deps, A = cfg.admin, P = cfg.params, out = [];
  let n = 0;
  let fb = 0;
  const add = s => { s.n = s.kind === 'gate' ? null : s.fallback ? `FB${++fb}` : `G${++n}`; out.push(s); };
  const tx = (id, target, method, args, done, why, extra = {}) => add({id, kind: 'tx', signer: 'admin', target, method, args, done, why, auth: 'root', ...extra});
  // A. funding (owner's own cash-market pTokens)
  for (const [m, k] of [['XLM', 'withdrawXlmPTokensRaw'], ['USDC', 'withdrawUsdcPTokensRaw'], ['PYUSD', 'withdrawPyusdPTokensRaw']])
    add({id: `fund.withdraw.${m}`, kind: 'withdraw', signer: 'admin', market: m, amount: P.funding[k], why: `owner redeems ${fmt7(P.funding[k])} ${m} pTokens (1:1 at 21 Nov)`, auth: 'root+transfer-in'});
  // B. uploads
  for (const k of ['vault', 'receipt', 'controller']) add({id: `upload.${k}`, kind: 'upload', signer: 'admin', artifact: k, why: `upload reviewed ${k} WASM`, auth: 'none'});
  // C. deploys (deterministic ids, no constructor)
  for (const k of ['vault', 'controller', 'receipt', 'usdcMarket']) add({id: `deploy.${k}`, kind: 'deploy', signer: 'admin', artifact: k, salt: cfg.salts[k], contract: I[k], why: `create ${k} ${I[k]}`, auth: 'create'});
  const V = I.vault, C = I.controller, R = I.receipt, M = I.usdcMarket;
  const inst = (c, k) => async x => { try { return (await x.instance(c))[k]; } catch { return undefined; } };
  const view = (c, m, a = []) => async x => { const v = await x.tryView(c, m, a); return v && typeof v === 'object' && v.error ? undefined : v; };
  const raw = (c, k) => async x => { const r = await x.raw(c, k); return r ? r.value : undefined; };
  const js = v => JSON.stringify(v, (_, y) => typeof y === 'bigint' ? String(y) : y);
  const eq = (f, want) => async x => js(await f(x)) === js(want);
  // D. vault v2
  tx('vault.initialize', V, 'initialize', () => [addr(A), addr(D.pool), u32(0), addr(D.cali)], eq(inst(V, 'Initialized'), true), 'pool CAPIOQNU, underlying index 0 (PYUSD), oracle CALI');
  tx('vault.set_primary_reward_token', V, 'set_primary_reward_token', () => [addr(A), optAddr(D.aqua)], eq(inst(V, 'PrimaryRewardToken'), D.aqua), 'AQUA primary reward');
  tx('vault.set_max_deploy', V, 'set_max_deploy', () => [addr(A), u128(P.vault.maxDeployRaw)], async x => { const p = await inst(V, 'Params')(x); return !!p && String(p.max_deploy) === P.vault.maxDeployRaw; }, `per-command deploy limit ${fmt7(P.vault.maxDeployRaw)} PYUSD (pilot value)`);
  // controller
  tx('controller.initialize', C, 'initialize', () => [addr(A)], eq(view(C, 'get_admin'), A), 'admin = Ledger');
  tx('controller.set_oracle', C, 'set_oracle', () => [addr(D.cafj)], eq(view(C, 'get_oracle'), D.cafj), 'D1: CAFJ external USD feed (first set immediate; later changes 24 h)');
  tx('controller.set_oracle_asset_symbol.USDC', C, 'set_oracle_asset_symbol', () => [addr(D.usdc), sym(P.controller.usdcOracleSymbol)], eq(raw(C, key('OracleAssetSymbol', addr(D.usdc))), P.controller.usdcOracleSymbol), 'price USDC as Other(USDC)');
  // receipt StgV2 (base pToken path is a 1 PYUSD floor; staged path bounded by the backing cap)
  tx('receipt.initialize', R, 'initialize', () => [addr(D.pyusd), u128(0), u128(0), addr(A)], async x => { const t = await view(R, 'get_underlying_token')(x); if (t === D.pyusd) return true; const c = await view(R, 'stage2_config')(x); return t === D.usdc && !!c && c.controller === C; }, 'PYUSD receipt, rates 0 (after G26 StgV2 the view reports the USDC numeraire by design)');
  tx('receipt.set_peridottroller', R, 'set_peridottroller', () => [addr(C)], eq(raw(R, key('Peridottroller')), C), 'bind to the NEW group controller (frozen afterwards by StgV2)');
  tx('receipt.set_min_cash_reserve_bps', R, 'set_min_cash_reserve_bps', () => [addr(A), u32(P.receipt.minCashReserveBps)], eq(view(R, 'get_min_cash_reserve_bps'), P.receipt.minCashReserveBps), '100 % reserve (receipt never lends)');
  tx('receipt.set_supply_cap', R, 'set_supply_cap', () => [u128(P.receipt.supplyCapRaw)], async x => String(await raw(R, key('SupplyCap'))(x)) === P.receipt.supplyCapRaw, 'base pToken path: 1 PYUSD floor only');
  tx('receipt.seed_minimum_supply', R, 'seed_minimum_supply', () => [addr(A), u128(P.receipt.seedRaw)], async x => String(await inst(R, 'PermanentMinimumSupply')(x)) === P.receipt.seedRaw, '1 PYUSD permanent floor (owner pays)', {auth: 'root+transfer', transferMax: P.receipt.seedRaw});
  tx('vault.stage_bind', V, 'stage_bind', () => [addr(A), addr(R)], eq(inst(V, 'StgV1Receipt'), R), 'register the receipt as custodian (vault empty)');
  tx('vault.stage_configure_guardian', V, 'stage_configure_guardian', () => [addr(A), addr(A)], eq(inst(V, 'StgV1Guardian'), A), 'vault guardian = Ledger; opens vault deposits');
  tx('vault.stage_set_invest_keeper', V, 'stage_set_invest_keeper', () => [addr(A), addr(cfg.investKeeper)], eq(inst(V, 'StgV1InvestKeeper'), cfg.investKeeper), 'observation keeper GBHWPV6Q (sequential sessions with the pilot, never concurrent)');
  tx('receipt.stage_activate', R, 'stage_activate', () => [addr(A), addr(V), addr(A), u128(P.receipt.backingCapRaw)], async x => { const c = await view(R, 'stage_config')(x); return !!c && c.vault === V && String(c.backing_cap) === P.receipt.backingCapRaw; }, `staged mode; guardian Ledger; backing cap ${fmt7(P.receipt.backingCapRaw)} PYUSD (lower-only afterwards)`);
  tx('receipt.stage2_activate', R, 'stage2_activate', () => [addr(A), stg2Config(C, D)], async x => { const c = await view(R, 'stage2_config')(x); return !!c && c.controller === C; }, 'StgV2 collateral class {controller, pool, CALI, USDC}');
  // USDC lending market (cash-market code 5630a701)
  tx('usdcMarket.initialize', M, 'initialize', () => [addr(D.usdc), u128(0), u128(0), addr(A)], eq(view(M, 'get_underlying_token'), D.usdc), 'USDC lending market');
  tx('usdcMarket.set_min_cash_reserve_bps', M, 'set_min_cash_reserve_bps', () => [addr(A), u32(P.usdcMarket.minCashReserveBps)], eq(view(M, 'get_min_cash_reserve_bps'), P.usdcMarket.minCashReserveBps), `max utilisation ${(10000 - P.usdcMarket.minCashReserveBps) / 100} %`);
  for (const [m, k, rk] of [['set_supply_cap', 'supplyCapRaw', 'SupplyCap'], ['set_borrow_cap', 'borrowCapRaw', 'BorrowCap'], ['set_reserve_factor', 'reserveFactorScaled', 'ReserveFactorScaled'],
    ['set_flash_loan_fee', 'flashLoanFeeScaled', 'FlashLoanFeeScaled'], ['set_borrow_rate', 'borrowRateScaled', 'BorrowYearlyRateScaled']])
    tx(`usdcMarket.${m}`, M, m, () => [u128(P.usdcMarket[k])], async x => String(await raw(M, key(rk))(x)) === String(P.usdcMarket[k]), `${k} = ${P.usdcMarket[k]}`,
      m === 'set_borrow_rate' ? {historical: true} : {}); // the first-liquidation raise changes it for <= 30 min
  tx('usdcMarket.enable_static_rates', M, 'enable_static_rates', () => [addr(A)], async x => (await raw(M, key('RatesReady'))(x)) === true && (await raw(M, key('InterestModel'))(x)) == null, 'static 8 % APR, no JRM (D8)');
  tx('usdcMarket.seed_minimum_supply', M, 'seed_minimum_supply', () => [addr(A), u128(P.usdcMarket.seedRaw)], async x => String(await inst(M, 'PermanentMinimumSupply')(x)) === P.usdcMarket.seedRaw, '1 USDC permanent floor', {auth: 'root+transfer', transferMax: P.usdcMarket.seedRaw});
  tx('usdcMarket.set_peridottroller', M, 'set_peridottroller', () => [addr(C)], eq(raw(M, key('Peridottroller')), C), 'bind to the group controller');
  // group wiring
  tx('controller.add_market.usdcMarket', C, 'add_market', () => [addr(M)], eq(view(C, 'is_market_supported', [addr(M)]), true), 'list USDC market');
  tx('controller.add_market.receipt', C, 'add_market', () => [addr(R)], eq(view(C, 'is_market_supported', [addr(R)]), true), 'list CL receipt (G7: at most 2 markets)');
  tx('controller.group_set_sleeve', C, 'group_set_sleeve', () => [addr(R)], eq(inst(C, 'Sleeve'), R), 'G1: the receipt is the sleeve (fixed once)');
  tx('controller.group_set_liq_cache_only', C, 'group_set_liq_cache_only', () => [bool(true)], eq(inst(C, 'LiqCacheOnly'), true), 'D7: L3 reads only a cached price (80 entries)');
  tx('controller.set_market_cf.receipt', C, 'set_market_cf', () => [addr(R), u128(P.receipt.cfScaled)], eq(raw(C, key('MarketCF', addr(R))), u(P.receipt.cfScaled)), 'first set is immediate; G6: only the sleeve may hold a CF',
    {historical: true, pre: async x => assert.equal(await raw(C, key('MarketCF', addr(R)))(x), undefined, 'CF already set: a change is the 24 h path (cf-restore)')});
  add({id: 'gate.prices', kind: 'gate', gate: 'prices', why: 'controller get_price_usd(USDC) equals CAFJ Other(USDC); no fallback price'});
  // E. liquidity and liquidator
  tx('usdcMarket.deposit.admin', M, 'deposit', () => [addr(A), u128(P.usdcMarket.adminSupplyRaw)], async x => u((await view(M, 'balance', [addr(A)])(x)) || 0) >= u(P.usdcMarket.adminSupplyRaw), `owner supplies ${fmt7(P.usdcMarket.adminSupplyRaw)} USDC`, {auth: 'root+transfer', transferMax: P.usdcMarket.adminSupplyRaw});
  add({id: 'fund.liquidator.xlm', kind: 'classic', signer: 'admin', op: 'create', dest: () => need(cfg.liquidator.publicKey, 'liquidator.publicKey'), xlm: P.funding.liquidatorStartXlm,
    also: () => [{dest: need(cfg.heartbeat.publicKey, 'heartbeat.publicKey'), xlm: cfg.heartbeat.startXlm}],
    why: 'create the CL liquidator account (25 XLM) and its fee-only D3 heartbeat account (12 XLM) in one transaction'});
  add({id: 'fund.liquidator.usdc', kind: 'classic', signer: 'admin', op: 'pay', asset: 'USDC', dest: () => need(cfg.liquidator.publicKey, 'liquidator.publicKey'), amountRaw: P.funding.liquidatorUsdcRaw, why: 'CL liquidator USDC inventory (after its trustlines exist)'});
  add({id: 'gate.liquidator', kind: 'gate', gate: 'liquidator', historical: true, why: 'inventory, allowance, trustlines, role separation; bot dry-run soak clean'});
  tx('usdcMarket.set_cash_borrowing_enabled', M, 'set_cash_borrowing_enabled', () => [addr(A), bool(true)], eq(view(M, 'get_cash_borrowing_enabled'), true), 'THE SWITCH', {historical: true, pre: async x => assert.equal(await view(C, 'is_borrow_paused', [addr(M)])(x), false, 'controller borrow pause active')});
  // F. team position (T3, HD path 3)
  const T = () => need(cfg.signers.team3.publicKey, 'signers.team3.publicKey');
  add({id: 'team.create', kind: 'classic', signer: 'admin', op: 'create', dest: T, xlm: P.funding.team3StartXlm, why: 'create T3'});
  add({id: 'team.trust', kind: 'classic', signer: 'team3', op: 'trust', assets: ['PYUSD', 'USDC'], why: 'T3 trustlines PYUSD + USDC (one tx, two ops)'});
  add({id: 'team.fund.pyusd', kind: 'classic', signer: 'admin', op: 'pay', asset: 'PYUSD', dest: T, amountRaw: P.funding.team3PyusdRaw, why: 'T3 principal + reserve'});
  add({id: 'team.open', kind: 'open', signer: 'team3', amount: P.team.openPyusdRaw, why: 'T3 stage_open on the new receipt; keeper session runs convert/mint/invest/finalise'});
  tx('team.enter.receipt', C, 'enter_market', () => [addr(T()), addr(R)], async x => ((await view(C, 'get_user_markets', [addr(T())])(x)) || []).includes(R), 'T3 enters the CL market', {signer: 'team3'});
  tx('team.enter.usdcMarket', C, 'enter_market', () => [addr(T()), addr(M)], async x => ((await view(C, 'get_user_markets', [addr(T())])(x)) || []).includes(M), 'T3 enters the USDC market', {signer: 'team3'});
  // G. first liquidation, PRIMARY path (user-approved 21 Nov): temporary static-rate raise, then T3 borrows exactly 100 %.
  tx('rate.up', M, 'set_borrow_rate', () => [u128(P.firstLiquidation.raisedRateScaled)], eq(raw(M, key('BorrowYearlyRateScaled')), u(P.firstLiquidation.raisedRateScaled)),
    `RAISE static rate to ${Number(P.firstLiquidation.raisedRateScaled) / 1e4} % APR for <= ${P.firstLiquidation.maxRaiseMinutes} min; the 8 % reset (rate.down) is MANDATORY`,
    {historical: true, preSim: 'rate-both-directions', resetCommand: 'step g56-rate-down prepare rate.down'});
  tx('team.checkpoint_nav', R, 'checkpoint_nav', () => [], async () => false, 'L1a (no auth) right before the borrow; repeatable', {signer: 'team3', auth: 'none', repeatable: true, fast: true});
  tx('team.borrow', M, 'borrow', () => [addr(T()), u128(P.team.borrowUsdcRaw)], async x => u((await view(M, 'get_user_borrow_balance', [addr(T())])(x)) || 0) > 0n, `T3 borrows ${fmt7(P.team.borrowUsdcRaw)} USDC within 60 s of the checkpoint`, {signer: 'team3', historical: true, fast: true,
      amount: 'account_liquidity',
    pre: async x => { const st = await view(R, 'stage2_status')(x); assert(st && st.nav, 'no NAV checkpoint: run G52 first');
        const l = await x.rpc.getLatestLedger(); const age = Math.floor(Date.now() / 1000) - Number(st.nav.ts);
        assert(age <= 40, `NAV checkpoint ${age} s old (> 40 s): run G52 again with a new step name`); }});
  add({id: 'gate.first-liquidation', kind: 'gate', gate: 'first-liquidation', historical: true, why: 'one L3 + L5 landed for T3; liquidator holds the seized units (15 min without a shortfall -> reset the rate, then the CF-0 fallback FB1-FB3)'});
  add({id: 'team.repay', kind: 'repay', signer: 'team3', fast: true, why: 'T3 repays the remaining debt (repay never depends on the sleeve)'});
  tx('rate.down', M, 'set_borrow_rate', () => [u128(P.usdcMarket.borrowRateScaled)], eq(raw(M, key('BorrowYearlyRateScaled')), u(P.usdcMarket.borrowRateScaled)),
    'MANDATORY reset to 8 % APR, <= 30 min after rate.up, whether or not the liquidation happened', {fast: true});
  add({id: 'gate.rate-reset', kind: 'gate', gate: 'rate-reset', historical: true, why: 'BorrowYearlyRateScaled = 80000 and T3 debt = 0'});
  // FALLBACK only (no shortfall 15 min after the borrow): CF -> 0, liquidate, then the 24 h CF restore. Numbered FB1-FB3.
  tx('liq.disable_cf', C, 'emergency_disable_collateral', () => [addr(R), bool(true)], eq(raw(C, key('MarketCF', addr(R))), 0n), 'FALLBACK: CF -> 0 (durable, immediate)', {historical: true, fallback: true, fast: true});
  tx('cf.propose', C, 'set_market_cf', () => [addr(R), u128(P.receipt.cfScaled)], eq(raw(C, key('PendingMarketCF', addr(R))), u(P.receipt.cfScaled)), 'FALLBACK: PROPOSE CF restore (24 h); cancel = emergency disable-cf', {historical: true, fallback: true, fast: true});
  tx('cf.execute', C, 'set_market_cf', () => [addr(R), u128(P.receipt.cfScaled)], eq(raw(C, key('MarketCF', addr(R))), u(P.receipt.cfScaled)), 'FALLBACK: EXECUTE CF restore after the ETA', {historical: true, fallback: true, fast: true});
  // H. migration (pilot close is ledger-owner-call.cjs + keeper v5, outside this tool)
  add({id: 'pilot.close', kind: 'external', signer: 'admin', why: 'owner stage_prepare_close of ALL pilot units on CCGK via prototypes/cl-pyusd-vault-b-pilot-tools/ledger-owner-call.cjs (A3); keeper v5 runs A4 stage_withdraw + A5 stage_close_convert'});
  add({id: 'gate.pilot-closed', kind: 'gate', gate: 'pilot-closed', why: 'CCGK owner units 0, no open command, PYUSD back in the owner wallet'});
  add({id: 'migrate.open', kind: 'open', signer: 'admin', amount: null, why: 'owner stage_open on the NEW receipt with the PYUSD returned by the pilot close'});
  add({id: 'gate.migrated', kind: 'gate', gate: 'migrated', why: 'owner holds v2 units, job finalised, no open command'});
  tx('pilot.receipt.stage_pause_deposits', cfg.pilot.receipt, 'stage_pause_deposits', () => [addr(A)], eq(view(cfg.pilot.receipt, 'stage_deposits_paused'), true), 'ONE-WAY latch: CCGK takes no new staged deposits (exits unaffected)', {pilotLatch: true});
  tx('pilot.vault.stage_pause_deposits', cfg.pilot.vault, 'stage_pause_deposits', () => [addr(A)], eq(view(cfg.pilot.vault, 'stage_deposits_paused'), true), 'ONE-WAY latch: CD6N admits no new open (exits unaffected)', {pilotLatch: true});
  return out;
}
function need(v, name) { assert(v, `${name} not set in clg-mainnet.json (PLACEHOLDER)`); return v; }
function stg2Config(C, D) {
  // contracttype struct = ScMap with keys sorted by symbol
  const e = (k, v) => new S.xdr.ScMapEntry({key: sym(k), val: addr(v)});
  return S.xdr.ScVal.scvMap([e('controller', C), e('oracle', D.cali), e('pool', D.pool), e('usdc', D.usdc)]);
}

// Emergency builders. Durable = stays until the admin reverses it; 72h = expires on its own.
const EMERGENCY = {
  'kill-borrow': {durable: true, build: (cfg, I) => [I.usdcMarket, 'set_cash_borrowing_enabled', [addr(cfg.admin), bool(false)]], note: 'stops new USDC borrows; repay/liquidate unaffected'},
  'pause-borrow': {durable: false, build: (cfg, I, on) => [I.controller, 'set_pause_borrow', [addr(I.usdcMarket), bool(on)]], note: '72 h controller pause'},
  'pause-liquidation': {durable: false, build: (cfg, I, on) => [I.controller, 'set_pause_liquidation', [addr(I.receipt), bool(on)]], note: '72 h; stops seizure of CL units (bad oracle)'},
  'pause-redeem': {durable: false, build: (cfg, I, on) => [I.controller, 'set_pause_redeem', [addr(I.usdcMarket), bool(on)]], note: '72 h; USDC suppliers only'},
  'pause-deposit': {durable: false, build: (cfg, I, on) => [I.controller, 'set_pause_deposit', [addr(I.usdcMarket), bool(on)]], note: '72 h; no new USDC supply on the group market before go-live (23 Nov)'},
  'disable-cf': {durable: true, build: (cfg, I) => [I.controller, 'emergency_disable_collateral', [addr(I.receipt), bool(true)]], note: 'CF -> 0 now; EVERY borrower becomes liquidatable; also cancels a pending CF'},
  'latch-receipt': {durable: true, build: (cfg, I) => [I.receipt, 'stage_pause_deposits', [addr(cfg.admin)]], note: 'ONE-WAY: no new CL deposits on the new receipt'},
  'latch-vault': {durable: true, build: (cfg, I) => [I.vault, 'stage_pause_deposits', [addr(cfg.admin)]], note: 'ONE-WAY: no new open on vault v2'},
  'vault-pause': {durable: true, build: (cfg, I, on) => [I.vault, 'set_paused', [addr(cfg.admin), bool(on)]], note: 'REVERSIBLE admin pause of vault v2 (required before upgrade_wasm)'},
  'lower-backing-cap': {durable: true, build: (cfg, I, raw) => [I.receipt, 'stage_lower_backing_cap', [addr(cfg.admin), u128(raw)]], note: 'lower-only'},
};

// ---------------------------------------------------------------- F4: reward route A vs seizure boundaries
// A seize sync (`stage_seize_reassign`) that opens a boundary publishes ("seizesync",) -> (from, to, n, opened=true) in the
// same transaction that writes Boundary(Count) with holder = from and timestamp = the ledger close time. Route A
// (propose_reward_attribution / attribute_rewards) on such a boundary strands the other holders' share (finding F4), so
// it is refused; route B (reward segments, then per-holder settle) is the only release path after a seizure.
function decodeSeizeEvents(raw) {
  return raw.map(ev => {
    const v = S.scValToNative(S.xdr.ScVal.fromXDR(ev.value, 'base64'));
    return {from: v[0], to: v[1], n: String(v[2]), opened: v[3] === true, ledger: ev.ledger, ts: Math.floor(Date.parse(ev.ledgerClosedAt) / 1000), txHash: ev.txHash};
  });
}
// Pure (unit-tested). coverage = {oldestLedgerCloseTime} of the event query. Throws unless route A is provably safe.
function assertRouteAAllowed(id, b, events, coverage) {
  assert(b, `boundary ${id} does not exist`);
  assert(coverage && Number.isFinite(Number(coverage.oldestLedgerCloseTime)), 'refused: seize-sync events could not be read');
  assert(Number(b.timestamp) >= Number(coverage.oldestLedgerCloseTime),
    `refused: boundary ${id} (ts ${b.timestamp}) is older than the RPC event window (from ${coverage.oldestLedgerCloseTime}); a seize sync cannot be excluded. Use route B`);
  const hit = events.find(e => e.opened && e.from === b.holder && Number(e.ts) === Number(b.timestamp));
  assert(!hit, `refused (F4): boundary ${id} was opened by seizesync ${hit && hit.txHash} at ledger ${hit && hit.ledger}; route A would strand other holders' rewards. Use route B (reward-route-b, reward-prepare, reward-settle)`);
  return true;
}
async function readSeizeEvents(cfg, c, vault) {
  const call = async (method, params) => {
    const r = await (await fetch(cfg.rpc, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json();
    if (r.error) throw Error(`${method}: ${JSON.stringify(r.error)}`); return r.result;
  };
  const topic = S.xdr.ScVal.scvSymbol('seizesync').toXDR('base64'), filters = [{type: 'contract', contractIds: [vault], topics: [[topic]]}];
  let first = await call('getEvents', {startLedger: (await call('getLatestLedger')).sequence - 1, filters, pagination: {limit: 1}});
  const oldest = first.oldestLedger, coverage = {oldestLedger: oldest, oldestLedgerCloseTime: first.oldestLedgerCloseTime};
  assert(oldest && first.oldestLedgerCloseTime, 'RPC did not report its event window');
  const out = []; let r = await call('getEvents', {startLedger: oldest, filters, pagination: {limit: 200}});
  for (let pages = 0; ; pages++) {
    out.push(...r.events); if (r.events.length < 200) break;
    assert(pages < 500, 'too many seize events to page'); r = await call('getEvents', {filters, pagination: {cursor: r.cursor, limit: 200}});
  }
  return {events: decodeSeizeEvents(out), coverage};
}

// ---------------------------------------------------------------- guards
function allowedTargets(cfg) {
  const I = ids(cfg);
  return new Set([I.vault, I.controller, I.receipt, I.usdcMarket]);
}
function assertTargetAllowed(cfg, target, step) {
  const retired = new Set(cfg.retired);
  assert(!retired.has(target), `refused: ${target} is a retired pre-incident contract`);
  if (step && step.pilotLatch) { assert([cfg.pilot.receipt, cfg.pilot.vault].includes(target)); return; }
  if (step && step.kind === 'withdraw') { assert(Object.values(cfg.cash).some(m => m.receipt === target)); return; }
  assert(allowedTargets(cfg).has(target), `refused: ${target} is not one of the four new contracts`);
}
function assertNoRetiredArgs(cfg, op) {
  const ic = op.body().invokeHostFunctionOp().hostFunction().invokeContract();
  const s = JSON.stringify(ic.args().map(a => S.scValToNative(a)), (_, x) => typeof x === 'bigint' ? String(x) : x);
  for (const r of cfg.retired) assert(!s.includes(r), `refused: retired contract ${r} in arguments`);
}
const fnOf = inv => inv.function();
function checkAuth(auth, op, source, policy, opts = {}) {
  if (policy === 'none') { assert.equal(auth.length, 0, 'no authorization expected'); return; }
  assert.equal(auth.length, 1, 'exactly one authorization entry required');
  const a = auth[0], cred = a.credentials().switch().name;
  assert.equal(cred, 'sorobanCredentialsSourceAccount', 'only the transaction source may authorize');
  const root = a.rootInvocation(), f = fnOf(root);
  if (policy === 'create') {
    assert(/CreateContract/i.test(f.switch().name), 'root must be the contract creation');
    assert.equal(root.subInvocations().length, 0); return;
  }
  assert.equal(f.contractFn().toXDR('base64'), op.body().invokeHostFunctionOp().hostFunction().invokeContract().toXDR('base64'), 'authorized root differs from the operation');
  const subs = root.subInvocations();
  if (policy === 'root') { assert.equal(subs.length, 0, 'admin setter must not authorize nested calls'); return; }
  const isTransfer = (inv, token, from, to, max) => {
    const cf = fnOf(inv).contractFn(), args = cf.args().map(x => S.scValToNative(x));
    return S.Address.fromScAddress(cf.contractAddress()).toString() === token && cf.functionName().toString() === 'transfer'
      && args[0] === from && args[1] === to && u(args[2]) <= u(max) && inv.subInvocations().length === 0;
  };
  if (policy === 'root+transfer') {
    assert.equal(subs.length, 1, 'exactly one nested token transfer expected');
    assert(isTransfer(subs[0], opts.token, source, opts.to, opts.max), 'nested authorization is not the expected bounded transfer'); return;
  }
  if (policy === 'open') {
    assert.equal(subs.length, 1, 'stage_open: exactly one nested call');
    const v = fnOf(subs[0]).contractFn();
    assert.equal(S.Address.fromScAddress(v.contractAddress()).toString(), opts.vault); assert.equal(v.functionName().toString(), 'stage_accept_funds');
    assert.equal(subs[0].subInvocations().length, 1);
    assert(isTransfer(subs[0].subInvocations()[0], opts.token, source, opts.vault, opts.max), 'stage_open: transfer is not owner -> vault, principal + reserve'); return;
  }
  throw Error('unknown auth policy ' + policy);
}
// Upload instruction allowance: 100M by default; above that ONLY for a hash pinned in
// limits.uploadCpuAllowance.hashes, and only once the user has set acceptedByUser=true (pending decision).
function uploadCpuMax(cfg, sha256, simulateOnly) {
  const A = cfg.limits.uploadCpuAllowance, base = A ? A.defaultUploadCpuMax : 100000000;
  const pin = A && A.hashes && A.hashes[sha256];
  if (!pin || pin.allowance <= base) return base;
  assert(A.acceptedByUser === true || simulateOnly, `upload of ${sha256} needs ${pin.measuredCpu} instructions, above the ${base} policy: user decision limits.uploadCpuAllowance.acceptedByUser required (PENDING)`);
  return pin.allowance;
}
// Entry line (project rule): entries = memory + disk reads + writes = footprint keys + read-write keys (harness
// convention: L3 = 63 keys + 17 writes = 80). Every prepared step must stay <= 85; an exception must be pinned in
// clg-mainnet.json limits.entryExceptions[stepId] = {measured, allowance} (default: none, refuse). Exit-path builders
// (owner withdraw / emergency extract; none is prepared by this tool today) use EXIT_PATH_MAX_ENTRIES (D-exit91).
const ENTRY_LINE = 85, EXIT_PATH_MAX_ENTRIES = 91;
function entriesOf(m) { return m.keys + m.writes; }
function entryLimitFor(cfg, stepId, {exitPath = false} = {}) {
  const ex = cfg.limits && cfg.limits.entryExceptions && cfg.limits.entryExceptions[stepId];
  if (ex) { assert(Number(ex.allowance) >= Number(ex.measured) && Number(ex.allowance) <= 100, `bad entry exception for ${stepId}`); return Number(ex.allowance); }
  return exitPath ? EXIT_PATH_MAX_ENTRIES : ENTRY_LINE;
}
function checkEntries(metrics, limit, stepId) {
  const e = entriesOf(metrics);
  assert(e <= limit, `${stepId}: ${e} entries (${metrics.keys} keys + ${metrics.writes} writes) above the ${limit}-entry line; refused (pin an exception in limits.entryExceptions only on a user decision)`);
  return e;
}
function checkResources(data, cpuMax) {
  const r = data.resources(), fp = r.footprint(), keys = [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64'));
  assert(Number(r.instructions()) <= cpuMax, `CPU ${r.instructions()} above ${cpuMax}`);
  assert(keys.length <= 100 && new Set(keys).size === keys.length, 'more than 100 or duplicate keys');
  assert(fp.readWrite().length <= 50, 'more than 50 writes');
  assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'archived entries: restore first (outside this tool)');
  return {cpu: Number(r.instructions()), keys: keys.length, writes: fp.readWrite().length, diskReadBytes: r.diskReadBytes(), writeBytes: r.writeBytes(), resourceFeeStroops: String(data.resourceFee().toString())};
}

// ---------------------------------------------------------------- chain-facing
function artifact(cfg, k) {
  const a = cfg.artifacts[k], file = path.resolve(__dirname, a.path), wasm = fs.readFileSync(file);
  const h = sha(wasm).toString('hex');
  assert.equal(h, a.sha256, `${k}: artifact ${h} is not the pinned ${a.sha256}`);
  assert.equal(wasm.length, a.bytes, `${k}: artifact size ${wasm.length} != ${a.bytes}`);
  return {wasm, sha256: h};
}
async function codeExists(c, hex) {
  const k = S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(hex, 'hex')}));
  const r = await c.rpc.getLedgerEntries(k); return !!(r.entries && r.entries.length);
}
async function contractWasm(c, id) { try { return await c.codeHash(id); } catch { return null; } }
async function simulateBuild(cfg, c, op, source, {cpuMax, feeCapStroops, policy, authOpts, entryLimit = ENTRY_LINE, stepId = '?'}) {
  const acc = await c.rpc.getAccount(source), latest = await c.rpc.getLatestLedger(), L = cfg.limits;
  const tx = new S.TransactionBuilder(acc, {fee: String(L.classicFeeStroops), networkPassphrase: cfg.network}).addOperation(op)
    .setLedgerbounds(latest.sequence + 1, latest.sequence + L.window).setTimeout(L.timeout).build();
  const sim = await c.rpc.simulateTransaction(tx);
  assert(S.rpc.Api.isSimulationSuccess(sim), 'simulation failed: ' + String(sim.error || '').split('\n')[0].slice(0, 300));
  assert(!sim.restorePreamble, 'restoration needed: outside this tool');
  checkAuth((sim.result && sim.result.auth) || [], tx.toEnvelope().v1().tx().operations()[0], source, policy, authOpts);
  const minFee = BigInt(sim.minResourceFee);
  const prepared = S.rpc.assembleTransaction(tx, padResourceFee(sim)).build();
  const data = prepared.toEnvelope().v1().tx().ext().sorobanData(), metrics = checkResources(data, cpuMax);
  metrics.entries = checkEntries(metrics, entryLimit, stepId); metrics.entryLimit = entryLimit;
  assert(u(prepared.fee) <= feeCapStroops, `fee ${prepared.fee} above the cap ${feeCapStroops}`);
  assert.equal(prepared.signatures.length, 0); assert.equal(prepared.source, source);
  metrics.minResourceFeeXlm = Number(minFee) / 1e7; // simulated (unpadded); maxFee = 1.2 x this + 0.001 + inclusion bid
  const retval = sim.result && sim.result.retval ? S.scValToNative(sim.result.retval) : undefined;
  return {prepared, metrics, retval, simulatedLedger: sim.latestLedger, validLedgers: {min: latest.sequence + 1, maxExclusive: latest.sequence + L.window}};
}
async function classicBuild(cfg, c, ops, source) {
  const acc = await c.rpc.getAccount(source), latest = await c.rpc.getLatestLedger(), L = cfg.limits;
  let b = new S.TransactionBuilder(acc, {fee: String(L.classicFeeStroops), networkPassphrase: cfg.network});
  for (const o of ops) b = b.addOperation(o);
  const prepared = b.setLedgerbounds(latest.sequence + 1, latest.sequence + L.window).setTimeout(L.timeout).build();
  return {prepared, metrics: {classic: true, ops: ops.length}, simulatedLedger: latest.sequence, validLedgers: {min: latest.sequence + 1, maxExclusive: latest.sequence + L.window}};
}
const assetOf = (cfg, code) => { const [cd, iss] = cfg.classicAssets[code].split(':'); return new S.Asset(cd, iss); };
// Pure (unit-tested): the liquidator must hold BOTH trustlines with the exact issuers, authorized. Without PYUSD,
// stage_emergency_pay of seized units reverts with SAC error #13 (no trustline).
function checkTrustlines(balances, cfg, codes = ['USDC', 'PYUSD']) {
  const rows = {};
  for (const code of codes) {
    const [cd, iss] = cfg.classicAssets[code].split(':');
    const b = (balances || []).find(x => x.asset_code === cd && x.asset_issuer === iss);
    rows[code] = {present: !!b, authorized: !!b && b.is_authorized !== false, limit: b ? b.limit : null};
  }
  return {ok: codes.every(c => rows[c].present && rows[c].authorized), rows};
}
async function horizonAccount(cfg, g) { const r = await fetch(`${cfg.horizon}/accounts/${g}`); return r.status === 200 ? r.json() : null; }

async function opFor(cfg, c, s) {
  const I = ids(cfg), L = cfg.limits, src = cfg.signers[s.signer].publicKey;
  const inv = (target, method, args, extra = {}) => {
    assertTargetAllowed(cfg, target, s);
    const op = new S.Contract(target).call(method, ...args); assertNoRetiredArgs(cfg, op);
    return {op, cpuMax: L.invokeCpuMax, feeCapStroops: BigInt(L.invokeMaxFeeStroops), policy: s.auth, ...extra};
  };
  if (s.kind === 'tx' && s.amount === 'account_liquidity') {
    // Borrow EXACTLY 100 % of the limit: liquidity from the controller at the live state; the sim proves shortfall == 0.
    const t = s.args()[0], [liq, short] = (await c.view(I.controller, 'account_liquidity', [t])).value.map(x => u(x));
    assert.equal(short, 0n, 'T3 already in shortfall'); assert(liq > 0n, 'no borrowing power: checkpoint (G53) first');
    // account_liquidity is USD-valued: amount x price / scale (controller). Borrow = floor(liq x scale / price), so the
    // hypothetical extra debt is <= liq (shortfall stays 0) and the leftover is < 1 raw unit.
    const [price, scale] = (await c.view(I.controller, 'get_price_usd', [addr(cfg.deps.usdc)])).value.map(x => u(x));
    const amt = borrowAtLimit(liq, price, scale);
    assert(amt > 0n && amt <= u(cfg.params.usdcMarket.borrowCapRaw), 'borrow outside (0, borrow cap]');
    const o = inv(s.target, s.method, [t, u128(amt)]); o.summary = {borrowRaw: String(amt), liquidityUsd: String(liq), price: String(price), scale: String(scale), note: 'exactly 100 % of the limit'}; return o;
  }
  if (s.kind === 'tx') {
    const o = inv(s.target, s.method, s.args());
    if (s.auth === 'root+transfer') o.authOpts = {token: s.target === I.receipt ? cfg.deps.pyusd : cfg.deps.usdc, to: s.target, max: s.transferMax};
    return o;
  }
  if (s.kind === 'withdraw') {
    const m = cfg.cash[s.market].receipt;
    const bal = u((await c.view(m, 'balance', [addr(src)])).value);
    assert(u(s.amount) <= bal, `amount above owner pToken balance ${bal}`);
    // withdraw pays out with the market's own transfer: the owner authorizes only the root call
    return {...inv(m, 'withdraw', [addr(src), u128(s.amount)]), policy: 'root'};
  }
  if (s.kind === 'upload') {
    assert(cfg.artifacts.frozen === true || s.allowUnfrozen, 'artifacts not frozen (PLACEHOLDER): uploads refused');
    const {wasm, sha256} = artifact(cfg, s.artifact);
    assert(!(await codeExists(c, sha256)), `code ${sha256} already on chain: skip`);
    return {op: S.Operation.uploadContractWasm({wasm}), cpuMax: uploadCpuMax(cfg, sha256, s.allowUnfrozen), feeCapStroops: xlm(L.uploadMaxFeeXlm[s.artifact]), policy: 'none', summary: {sha256, bytes: wasm.length}};
  }
  if (s.kind === 'deploy') {
    assert(cfg.artifacts.frozen === true || s.allowUnfrozen, 'artifacts not frozen (PLACEHOLDER): deploys refused');
    const h = cfg.artifacts[s.artifact].sha256;
    assert(await codeExists(c, h), `code ${h} not uploaded yet`);
    assert.equal(await contractWasm(c, s.contract), null, `${s.contract} already exists`);
    const op = S.Operation.createCustomContract({address: S.Address.fromString(src), wasmHash: Buffer.from(h, 'hex'), salt: saltOf(s.salt)});
    return {op, cpuMax: L.invokeCpuMax, feeCapStroops: BigInt(L.invokeMaxFeeStroops), policy: 'create', summary: {contract: s.contract, wasm: h}};
  }
  if (s.kind === 'open') {
    const owner = src, amount = s.amount || (await openAmountFromWallet(cfg, c, owner));
    const q = (await c.view(I.vault, 'stage_quote_open', [u128(amount), u32(20), u32(5000)])).value;
    assert(q.valid === true && q.admissible === true && q.oracle_fresh === true && q.in_band === true && q.deposits_paused === false && q.composition_dirty === false, 'open quote not admissible: ' + json(q));
    const latest = await c.rpc.getLatestLedger(), now = Math.floor(Date.now() / 1000), deadline = BigInt(now + 3000);
    const op = inv(I.receipt, 'stage_open', [addr(owner), u128(amount), u128(q.swap_in), u128(q.min_swap), u128(q.min_shares), u128(q.reserve), u64v(deadline)], {policy: 'open'});
    op.authOpts = {vault: I.vault, token: cfg.deps.pyusd, max: String(u(amount) + u(q.reserve))};
    op.summary = {amount: String(amount), quote: q, deadline: String(deadline), quoteLedger: latest.sequence};
    return op;
  }
  if (s.kind === 'repay') {
    const owner = src, debt = u((await c.view(I.usdcMarket, 'get_user_borrow_balance', [addr(owner)])).value);
    assert(debt > 0n, 'no debt');
    const amount = debt + debt / 10000n + 1000n; // interest accrues between simulation and inclusion; market caps at debt (check sim)
    const op = inv(I.usdcMarket, 'repay', [addr(owner), u128(amount)], {policy: 'root+transfer'});
    op.authOpts = {token: cfg.deps.usdc, to: I.usdcMarket, max: String(amount)}; op.summary = {debtRaw: String(debt), repayRaw: String(amount)};
    return op;
  }
  if (s.kind === 'classic') {
    let ops;
    if (s.op === 'create') {
      const all = [{dest: s.dest(), xlm: s.xlm}, ...(s.also ? s.also() : [])];
      assert.equal(new Set(all.map(a => a.dest)).size, all.length, 'accounts must be distinct');
      ops = [];
      for (const a of all) {
        assert(S.StrKey.isValidEd25519PublicKey(a.dest)); assert.notEqual(a.dest, cfg.admin);
        assert(!(await horizonAccount(cfg, a.dest)), `${a.dest} already exists`);
        ops.push(S.Operation.createAccount({destination: a.dest, startingBalance: String(a.xlm)}));
      }
    } else if (s.op === 'pay') {
      const d = s.dest(); assert.notEqual(d, cfg.admin);
      ops = [S.Operation.payment({destination: d, asset: assetOf(cfg, s.asset), amount: fmt7(s.amountRaw)})];
    } else if (s.op === 'trust') ops = s.assets.map(a => S.Operation.changeTrust({asset: assetOf(cfg, a)}));
    return {classic: ops, summary: {ops: s.op}};
  }
  throw Error('no builder for ' + s.kind);
}
async function openAmountFromWallet(cfg, c, owner) {
  const bal = u((await c.view(cfg.deps.pyusd, 'balance', [addr(owner)])).value);
  const amount = bal - bal / 200n - 10_000_000n; // keep ~0.5 % + 1 PYUSD for the reserve and dust
  assert(amount > 0n && amount <= u(cfg.params.vault.maxDeployRaw), `open amount ${amount} outside (0, max_deploy]`);
  return amount;
}

// Before the raise: the raise AND the 8 % reset must both simulate OK now on the group USDC market (5630a701,
// immediate setter, cap 1000 % APR, supply rate <= new borrow rate). Refuses otherwise; nothing is written.
async function rateBothDirections(cfg, c) {
  const I = ids(cfg), M = I.usdcMarket, P = cfg.params, sup = await c.raw(M, key('YearlyRateScaled'));
  checkRatePlan(P, sup ? sup.value : 0n);
  for (const r of [P.firstLiquidation.raisedRateScaled, P.usdcMarket.borrowRateScaled]) {
    const op = new S.Contract(M).call('set_borrow_rate', u128(r));
    await simulateBuild(cfg, c, op, cfg.admin, {cpuMax: cfg.limits.invokeCpuMax, feeCapStroops: BigInt(cfg.limits.invokeMaxFeeStroops), policy: 'root'});
  }
}
// Pure (unit-tested).
function checkRatePlan(P, supplyRate) {
  const up = u(P.firstLiquidation.raisedRateScaled), down = u(P.usdcMarket.borrowRateScaled);
  assert(up <= 10_000_000n, 'raise above the 1000 % APR cap (MAX_YEARLY_RATE_SCALED)');
  assert(up > down, 'raise must be above the normal rate');
  assert.equal(String(down), '80000', 'reset must be the approved 8 % APR');
  assert(u(supplyRate) <= down, `supply rate ${supplyRate} above 80000: the reset would be refused`);
  assert(Number(P.firstLiquidation.maxRaiseMinutes) <= 30 && Number(P.firstLiquidation.warnMinutes) < Number(P.firstLiquidation.maxRaiseMinutes), 'raise window must be <= 30 min with an earlier warning');
  return true;
}
// Pure (unit-tested): largest borrow whose USD value (floor(amount x price / scale)) does not exceed the liquidity.
function borrowAtLimit(liq, price, scale) {
  let a = u(liq) * u(scale) / u(price);
  while (a > 0n && a * u(price) / u(scale) > u(liq)) a -= 1n;
  while ((a + 1n) * u(price) / u(scale) <= u(liq)) a += 1n;
  return a;
}
// Pure (unit-tested): minutes since the raise -> ok | warn | overdue.
function rateWindow(raisedAtSec, nowSec, P) {
  const m = (nowSec - raisedAtSec) / 60, w = Number(P.firstLiquidation.warnMinutes), max = Number(P.firstLiquidation.maxRaiseMinutes);
  return {minutes: Math.round(m * 10) / 10, level: m >= max ? 'OVERDUE' : m >= w ? 'WARN' : 'ok', resetBy: new Date((raisedAtSec + max * 60) * 1000).toISOString()};
}

async function gate(cfg, c, name) {
  const I = ids(cfg), P = cfg.params;
  if (name === 'prices') {
    const got = await c.tryView(I.controller, 'get_price_usd', [addr(cfg.deps.usdc)]);
    const direct = (await c.view(cfg.deps.cafj, 'lastprice', [S.xdr.ScVal.scvVec([sym('Other'), sym('USDC')])])).value;
    const dev = got && direct && !got.error ? Number((u(got[0]) - u(direct.price)) * 10000n / u(direct.price)) : null;
    const fb = await c.raw(I.controller, key('FallbackPrice', addr(cfg.deps.usdc)));
    const ok = dev !== null && Math.abs(dev) <= 5 && !fb;
    return {gate: name, ok, controller: got, direct, deviationBps: dev, fallbackPrice: fb ? fb.value : null};
  }
  if (name === 'liquidator') {
    const lq = cfg.liquidator.publicKey; if (!lq) return {gate: name, ok: false, reason: 'liquidator.publicKey not set'};
    const bal = await c.tryView(cfg.deps.usdc, 'balance', [addr(lq)]), alw = await c.tryView(cfg.deps.usdc, 'allowance', [addr(lq), addr(I.usdcMarket)]);
    const acct = await horizonAccount(cfg, lq), tl = checkTrustlines(acct ? acct.balances : [], cfg);
    const hbk = cfg.heartbeat && cfg.heartbeat.publicKey, hba = hbk ? await horizonAccount(cfg, hbk) : null;
    const hbXlm = hba ? Number((hba.balances.find(b => b.asset_type === 'native') || {}).balance || 0) : 0;
    const others = [cfg.admin, cfg.signers.team3.publicKey, 'GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G', 'GBSK3FYCGNYZLHHPSYRJNE53NZXDFL6ED24KLT3GUUETLFKDAUD7PQX3'];
    const rows = {usdcRaw: String(bal), minRaw: P.funding.liquidatorUsdcRaw, allowanceRaw: String(alw), trustlines: tl.rows,
      heartbeat: {account: hbk, xlm: hbXlm, minXlm: Number(cfg.heartbeat.minXlm)},
      roles: !others.includes(lq) && !!hbk && hbk !== lq && !others.includes(hbk)};
    // The allowance is reported only: the bot's APPROVE stage sets it right before its first L3 (bots/cl-liquidator README).
    const ok = typeof bal === 'bigint' && bal >= u(P.funding.liquidatorUsdcRaw) && tl.ok && rows.roles
      && hbXlm >= Number(cfg.heartbeat.minXlm);
    return {gate: name, ok, liquidator: lq, rows, manual: 'also: bot dry-run log >= 2 h, zero errors, every borrower planned, D3 status read; heartbeat tests pass and the Testnet heartbeat gap stats (p95 <= 20 s, 0 gaps > 60 s) are recorded (LAUNCH.md section 6.4)'};
  }
  if (name === 'rate-reset') {
    const I = ids(cfg), r = await c.raw(I.usdcMarket, key('BorrowYearlyRateScaled'));
    const t3 = cfg.signers.team3.publicKey, debt = t3 ? await c.tryView(I.usdcMarket, 'get_user_borrow_balance', [addr(t3)]) : null;
    return {gate: name, ok: !!r && String(r.value) === String(cfg.params.usdcMarket.borrowRateScaled) && debt === 0n, rate: r ? String(r.value) : null, t3Debt: String(debt)};
  }
  if (name === 'cf-eta') {
    const eta = await c.raw(I.controller, key('PendingMarketCFEta', addr(I.receipt)));
    const now = Math.floor(Date.now() / 1000);
    return {gate: name, ok: !!eta && now >= Number(eta.value) + 30, eta: eta ? String(eta.value) : null, etaUtc: eta ? new Date(Number(eta.value) * 1000).toISOString() : null};
  }
  if (name === 'first-liquidation') {
    const st = await c.tryView(I.receipt, 'stage2_status'), lq = cfg.liquidator.publicKey;
    const units = lq ? await c.tryView(I.receipt, 'stage_units', [addr(lq)]) : null;
    const ben = lq ? await c.tryView(I.vault, 'stage_beneficial', [addr(lq)]) : null;
    const ok = !!st && !st.error && Number(st.next_pending) >= 1 && typeof ben === 'bigint' && ben > 0n;
    return {gate: name, ok, status: st, liquidatorUnits: units, liquidatorBeneficial: ben};
  }
  if (name === 'pilot-closed') {
    const units = await c.tryView(cfg.pilot.receipt, 'stage_units', [addr(cfg.admin)]), cmds = await c.tryView(cfg.pilot.receipt, 'stage_commands', [addr(cfg.admin)]);
    return {gate: name, ok: units === 0n && Array.isArray(cmds) && cmds.length === 0, units, commands: cmds};
  }
  if (name === 'migrated') {
    const units = await c.tryView(I.receipt, 'stage_units', [addr(cfg.admin)]), cmds = await c.tryView(I.receipt, 'stage_commands', [addr(cfg.admin)]);
    return {gate: name, ok: typeof units === 'bigint' && units > 0n && Array.isArray(cmds) && cmds.length === 0, units, commands: cmds};
  }
  throw Error('unknown gate ' + name);
}
async function isDone(cfg, c, s) {
  if (s.kind === 'gate') return (await gate(cfg, c, s.gate)).ok;
  if (s.kind === 'upload') return codeExists(c, cfg.artifacts[s.artifact].sha256);
  if (s.kind === 'deploy') return (await contractWasm(c, s.contract)) === cfg.artifacts[s.artifact].sha256;
  if (s.kind === 'tx') { try { return !!(await s.done(c)); } catch { return false; } }
  if (s.kind === 'external') return (await gate(cfg, c, 'pilot-closed')).ok;
  return null; // withdraw / classic / open / repay: read back manually (printed in LAUNCH.md)
}
function write(cfg, out, res, summary) {
  assert(out && out.endsWith('.unsigned.xdr'), '--out FILE.unsigned.xdr required');
  const meta = out.replace(/\.unsigned\.xdr$/, '.unsigned.json');
  assert(!fs.existsSync(out) && !fs.existsSync(meta), 'output exists: never overwrite a prepared transaction');
  fs.writeFileSync(out, res.prepared.toXDR() + '\n', {mode: 0o600, flag: 'wx'});
  const record = {status: 'UNSIGNED_PREPARED_SIGN_ON_LEDGER', signing: false, submission: false, network: cfg.network, file: out,
    hash: res.prepared.hash().toString('hex'), source: res.prepared.source, hdPath: summary.hdPath, sequence: res.prepared.sequence,
    maxFeeStroops: res.prepared.fee, maxFeeXlm: Number(res.prepared.fee) / 1e7, validLedgers: res.validLedgers, simulatedLedger: res.simulatedLedger, metrics: res.metrics, ...summary};
  fs.writeFileSync(meta, json(record) + '\n', {mode: 0o600, flag: 'wx'});
  return record;
}
async function buildStep(cfg, c, s) {
  const src = need(cfg.signers[s.signer].publicKey, `signers.${s.signer}.publicKey`);
  const o = await opFor(cfg, c, s);
  const res = o.classic ? await classicBuild(cfg, c, o.classic, src)
    : await simulateBuild(cfg, c, o.op, src, {cpuMax: o.cpuMax, feeCapStroops: o.feeCapStroops, policy: o.policy, authOpts: o.authOpts,
        entryLimit: entryLimitFor(cfg, s.id, {exitPath: !!s.exitPath}), stepId: s.id});
  // The host's own answer must equal the precomputed id (salt + admin), or nothing is written.
  if (s.kind === 'deploy') assert.equal(String(res.retval), s.contract, `host would create ${res.retval}, not the pinned ${s.contract}`);
  return {res, summary: {step: s.id, n: s.n, signer: s.signer, hdPath: cfg.signers[s.signer].hdPath, call: s.method || s.kind, target: s.target || null, why: s.why, ...(o.summary || {})}};
}

async function main(argv) {
  const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; }, has = k => argv.includes(k);
  const cfg = JSON.parse(fs.readFileSync(CFG_FILE, 'utf8'));
  assert.equal(cfg.network, S.Networks.PUBLIC);
  const cmd = argv[0];
  if (cmd === 'addresses') return ids(cfg);
  if (cmd === 'caps') return {caps: checkCaps(cfg)};
  if (cmd === 'verify-signed') {
    const a = S.TransactionBuilder.fromXDR(fs.readFileSync(argv[1], 'utf8').trim(), cfg.network), b = S.TransactionBuilder.fromXDR(fs.readFileSync(argv[2], 'utf8').trim(), cfg.network);
    assert.equal(a.hash().toString('hex'), b.hash().toString('hex'), 'signed transaction differs from the prepared one');
    assert.equal(b.signatures.length, 1, 'exactly one signature');
    assert(S.Keypair.fromPublicKey(b.source).verify(b.hash(), b.signatures[0].signature()), 'signature does not verify for the source (wrong Ledger account / HD path?)');
    return {ok: true, hash: b.hash().toString('hex'), source: b.source};
  }
  const c = chain(cfg), list = steps(cfg);
  if (['list', 'simulate-all', 'prepare', 'readback'].includes(cmd)) checkCaps(cfg);
  const find = id => { const s = list.find(x => x.id === id); assert(s, `unknown step ${id}`); return s; };
  if (cmd === 'prepare') assert.notEqual(find(argv[1]).kind, 'external', 'external step: use the command printed in LAUNCH.md');
  if (cmd === 'list' || cmd === 'simulate-all') {
    const rows = [];
    for (const s of list) {
      const row = {n: s.n, id: s.id, kind: s.kind, signer: s.signer || null, why: s.why, done: await isDone(cfg, c, s)};
      const simulable = ['tx', 'upload', 'deploy'].includes(s.kind) ? row.done === false : s.kind === 'withdraw';
      if (cmd === 'simulate-all' && simulable) {
        try { const r = await buildStep(cfg, c, {...s, allowUnfrozen: has('--allow-unfrozen')}); row.sim = {...r.res.metrics, maxFeeXlm: Number(r.res.prepared.fee) / 1e7, ledger: r.res.simulatedLedger}; }
        catch (e) { row.simError = e.message.slice(0, 200); }
      }
      rows.push(row);
    }
    return {network: cfg.network, ledger: (await c.rpc.getLatestLedger()).sequence, addresses: ids(cfg), frozen: cfg.artifacts.frozen, steps: rows};
  }
  if (cmd === 'readback') { const s = find(argv[1]); const d = await isDone(cfg, c, s); if (d !== true) process.exitCode = 1; return {step: s.id, n: s.n, done: d}; }
  if (cmd === 'gate') { const r = await gate(cfg, c, argv[1]); if (!r.ok) process.exitCode = 1; return r; }
  if (cmd === 'prepare') {
    const s = find(argv[1]); assert.notEqual(s.kind, 'gate', 'gates are evaluated with `gate`');
    // fast steps (G52/G53) skip the read-back walk: the 60 s NAV window does not allow ~50 reads; their pre() checks instead.
    for (const prev of s.fast ? [] : list.slice(0, list.indexOf(s))) {
      if (prev.repeatable || prev.historical) continue;
      // A failed RPC read looks like "not done": re-read up to twice (3 s apart) before refusing.
      let d = await isDone(cfg, c, prev);
      for (let i = 0; d === false && i < 2; i++) { await new Promise(r => setTimeout(r, 3000)); d = await isDone(cfg, c, prev); }
      assert(d !== false, `earlier step ${prev.n || ''} ${prev.id} not read back as done`);
    }
    if (!s.repeatable) assert.notEqual(await isDone(cfg, c, s), true, `${s.id} already done`);
    if (s.pre) await s.pre(c);
    if (s.preSim === 'rate-both-directions') await rateBothDirections(cfg, c);
    const {res, summary} = await buildStep(cfg, c, s);
    const extra = s.resetCommand ? {mandatoryReset: s.resetCommand, resetDeadlineMinutes: cfg.params.firstLiquidation.maxRaiseMinutes} : {};
    return write(cfg, opt('--out'), res, {...summary, ...extra, readback: `node clg-ceremony.cjs readback ${s.id}`});
  }
  if (cmd === 'emergency') {
    const name = argv[1], e = EMERGENCY[name]; assert(e, 'emergency: ' + Object.keys(EMERGENCY).join('|'));
    const arg = argv[2] === 'on' ? true : argv[2] === 'off' ? false : argv[2];
    const [target, method, args] = e.build(cfg, ids(cfg), arg);
    const s = {id: `emergency.${name}`, kind: 'tx', signer: 'admin', target, method, args: () => args, auth: 'root'};
    const {res, summary} = await buildStep(cfg, c, s);
    return write(cfg, opt('--out'), res, {...summary, emergency: true, durable: e.durable, note: e.note});
  }
  if (cmd === 'owner-open' || cmd === 'repay') {
    const signer = argv[1]; assert(cfg.signers[signer], 'signer admin|team3');
    const s = cmd === 'repay' ? {id: `repay.${signer}`, kind: 'repay', signer} : {id: `open.${signer}`, kind: 'open', signer, amount: argv[2] || null};
    const {res, summary} = await buildStep(cfg, c, s);
    return write(cfg, opt('--out'), res, summary);
  }
  if (cmd === 'pending') {
    const t = ids(cfg)[argv[1]]; assert(t, 'target vault|receipt|controller|usdcMarket');
    const p = await c.tryView(t, 'get_pending_upgrade');
    const ok = Array.isArray(p);
    return {target: t, wasm: await contractWasm(c, t), pendingHash: ok ? Buffer.from(p[0]).toString('hex') : null, eta: ok ? String(p[1]) : null,
      etaUtc: ok ? new Date(Number(p[1]) * 1000).toISOString() : null, raw: ok || p === null ? undefined : p};
  }
  if (['propose-upgrade', 'execute-upgrade', 'cancel-upgrade'].includes(cmd)) {
    // ABI of commit 6e83fd7: vault takes admin_addr first; receipt, controller and USDC market (5630a701) take only the hash.
    // cancel_upgrade exists on all four (vault: cancel_upgrade(admin_addr)). Vault execute requires set_paused(admin, true) first.
    const kind = argv[1], t = ids(cfg)[kind]; assert(t, 'target vault|receipt|controller|usdcMarket');
    const live = await contractWasm(c, t), hex = cmd === 'cancel-upgrade' ? null : argv[2];
    if (cmd !== 'cancel-upgrade') { assert(/^[0-9a-f]{64}$/.test(hex || ''), 'sha256 hex required'); assert(await codeExists(c, hex), `code ${hex} not uploaded`); assert.notEqual(hex, live, 'already live'); }
    const hb = hex && S.xdr.ScVal.scvBytes(Buffer.from(hex, 'hex')), pre = kind === 'vault' ? [addr(cfg.admin)] : [];
    const method = {'propose-upgrade': 'propose_upgrade_wasm', 'execute-upgrade': 'upgrade_wasm', 'cancel-upgrade': 'cancel_upgrade'}[cmd];
    const args = cmd === 'cancel-upgrade' ? pre : [...pre, hb];
    const s = {id: `${cmd}.${kind}`, kind: 'tx', signer: 'admin', target: t, method, args: () => args, auth: 'root'};
    const {res, summary} = await buildStep(cfg, c, s);
    return write(cfg, opt('--out'), res, {...summary, hash: hex, live, readback: `node clg-ceremony.cjs pending ${kind}`});
  }
  if (cmd === 'reward-boundary') {
    const V = ids(cfg).vault, id = argv[1], b = await c.tryView(V, 'reward_boundary', [u64v(id)]);
    let verdict;
    try { const {events, coverage} = await readSeizeEvents(cfg, c, V); assertRouteAAllowed(id, b, events, coverage); verdict = 'route A allowed (no seizure sync opened it)'; }
    catch (e) { verdict = e.message; }
    const interval = b && !b.error ? await c.tryView(V, 'reward_interval', [u64v(b.interval)]) : null;
    return {boundary: b, verdict, interval};
  }
  if (cmd === 'reward-route-a' || cmd === 'reward-route-b') {
    // route A: <propose|attribute> <boundaryId> <amountRaw> <evidenceHex>; route B: same with a reward SEGMENT id.
    const V = ids(cfg).vault, [, which, id, amount, ev] = argv;
    assert(['propose', 'attribute'].includes(which), 'propose|attribute');
    assert(/^[0-9]+$/.test(id || '') && /^[0-9]+$/.test(amount || '') && /^[0-9a-f]{64}$/.test(ev || '') && !/^0+$/.test(ev), 'id, raw amount, 32-byte non-zero evidence hex');
    if (cmd === 'reward-route-a') {
      const b = await c.view(V, 'reward_boundary', [u64v(id)]).then(r => r.value).catch(() => null);
      let seize; try { seize = await readSeizeEvents(cfg, c, V); } catch (e) { throw Error('refused: seize-sync events could not be read (' + e.message.slice(0, 120) + ')'); }
      assertRouteAAllowed(id, b, seize.events, seize.coverage);
    }
    const method = cmd === 'reward-route-a' ? (which === 'propose' ? 'propose_reward_attribution' : 'attribute_rewards') : (which === 'propose' ? 'propose_reward_segment' : 'attribute_reward_segment');
    const s = {id: `${cmd}.${which}.${id}`, kind: 'tx', signer: 'admin', target: V, method, args: () => [u64v(id), u128(amount), S.xdr.ScVal.scvBytes(Buffer.from(ev, 'hex'))], auth: 'root'};
    const {res, summary} = await buildStep(cfg, c, s);
    return write(cfg, opt('--out'), res, {...summary, note: which === 'propose' ? 'executable 24 h later with the same amount and evidence' : 'governed attribution'});
  }
  if (cmd === 'reward-prepare' || cmd === 'reward-settle') {
    // Route B, permissionless (no auth; the admin only pays the fee): prepare_reward_segment(id, limit<=4) once, then
    // settle_reward_segment(holder, id, checkpoint) for EVERY holder of the segment (each settles once).
    const V = ids(cfg).vault;
    const args = cmd === 'reward-prepare' ? [u64v(argv[1]), u32(argv[2] || 4)] : [addr(argv[1]), u64v(argv[2]), u32(argv[3])];
    const s = {id: `${cmd}.${argv.slice(1, 3).join('.')}`, kind: 'tx', signer: 'admin', target: V, method: cmd === 'reward-prepare' ? 'prepare_reward_segment' : 'settle_reward_segment', args: () => args, auth: 'none'};
    const {res, summary} = await buildStep(cfg, c, s);
    return write(cfg, opt('--out'), res, summary);
  }
  if (cmd === 'first-touch') {
    // G41a / G49a: the first cache_price(USDC) / checkpoint_nav, paid by the admin, so the bot's resource-fee ceiling
    // never meets a rent-bearing first call. Permissionless calls: no auth entry.
    const I = ids(cfg), which = argv[1];
    assert(which === 'cache-price' || which === 'nav', 'first-touch cache-price|nav');
    const s = which === 'nav' ? {id: 'G49a.first-touch.nav', target: I.receipt, method: 'checkpoint_nav', args: () => []}
      : {id: 'G41a.first-touch.cache-price', target: I.controller, method: 'cache_price', args: () => [addr(cfg.deps.usdc)]};
    const {res, summary} = await buildStep(cfg, c, {...s, kind: 'tx', signer: 'admin', auth: 'none'});
    return write(cfg, opt('--out'), res, {...summary, n: s.id.split('.')[0]});
  }
  if (cmd === 'trust-liquidator') {
    // Unsigned changeTrust USDC + PYUSD (one tx, two ops) with the CL LIQUIDATOR as source. It is signed ON THE
    // LIQUIDATOR HOST (stellar tx sign --sign-with-key <its alias>), never on the Mac or the Ledger.
    const lq = need(cfg.liquidator.publicKey, 'liquidator.publicKey');
    assert(await horizonAccount(cfg, lq), 'liquidator account does not exist yet (G43 first)');
    const res = await classicBuild(cfg, c, ['USDC', 'PYUSD'].map(a => S.Operation.changeTrust({asset: assetOf(cfg, a)})), lq);
    return write(cfg, opt('--out'), res, {step: 'liquidator.trust', call: 'changeTrust(USDC) + changeTrust(PYUSD)', signer: 'liquidator (host key)', hdPath: null});
  }
  if (cmd === 'rate-window') {
    // Raise time = ledger close time of the rate.up transaction (hash from clg-run.sh's send file, or argv[1]).
    const dir = process.env.CLG_RUN_DIR || '/Users/joshua/Peridot/clg-launch-20261122';
    let hash = argv[1];
    if (!hash) { const f = fs.readdirSync(dir).filter(x => /rate-up\.send\.json$/.test(x)).map(x => path.join(dir, x)).find(x => /"status": "SUCCESS"/.test(fs.readFileSync(x, 'utf8')));
      if (!f) return {raised: false}; hash = (fs.readFileSync(f, 'utf8').match(/"tx_hash": "([0-9a-f]{64})"/) || [])[1]; }
    const t = await c.rpc.getTransaction(hash), at = Number(t.raw.createdAt);
    const r = await c.raw(ids(cfg).usdcMarket, key('BorrowYearlyRateScaled'));
    const w = rateWindow(at, Math.floor(Date.now() / 1000), cfg.params);
    const reset = r && String(r.value) === String(cfg.params.usdcMarket.borrowRateScaled);
    if (!reset && w.level !== 'ok') process.exitCode = 2;
    return {raised: true, hash, raisedAt: new Date(at * 1000).toISOString(), ...w, currentRate: r ? String(r.value) : null, reset,
      next: reset ? 'rate is back at 8 %' : 'step g56-rate-down prepare rate.down'};
  }
  if (cmd === 'status') { const r = await c.rpc.getTransaction(argv[1]); return {hash: argv[1], status: r.status, ledger: r.ledger}; }
  throw Error('usage: see header');
}
module.exports = {uploadCpuMax, contractIdFor, ids, steps, checkCaps, checkAuth, assertTargetAllowed, stg2Config, EMERGENCY, assertRouteAAllowed, decodeSeizeEvents, readSeizeEvents, checkTrustlines, checkRatePlan, rateWindow, borrowAtLimit, ENTRY_LINE, EXIT_PATH_MAX_ENTRIES, entriesOf, entryLimitFor, checkEntries};
if (require.main === module) main(process.argv.slice(2)).then(r => console.log(json(r))).catch(e => { console.error('REFUSED/FAILED:', e.message); process.exit(1); });
