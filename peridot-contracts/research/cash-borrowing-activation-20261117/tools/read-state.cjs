// Read-only snapshot of every setting the borrowing activation touches. Usage: node read-state.cjs [--testnet cfg.json] > out.json
'use strict';
const fs = require('node:fs');
const {S, MAINNET, json, addr, key, chain} = require('./lib.cjs');
async function readState(cfg, c = chain(cfg)) {
  const C = cfg.controller, out = {network: (await c.rpc.getNetwork()).passphrase, observedAt: new Date().toISOString()};
  out.controller = {id: C, wasm: await c.codeHash(C)};
  const ctrlViews = ['get_admin', 'get_oracle', 'get_close_factor_scaled', 'get_liquidation_incentive_scaled', 'get_liquidation_fee_scaled', 'get_reserve_recipient'];
  for (const m of ctrlViews) out.controller[m] = await c.tryView(C, m);
  for (const k of ['PauseGuardian', 'OracleMaxAgeMultiplier', 'PendingOracle', 'PendingOracleEta', 'PendingCloseFactorScaled', 'PendingCloseFactorEta',
    'PendingLiqIncentiveScaled', 'PendingLiqIncentiveEta', 'PendingUpgradeHash', 'SupportedMarkets', 'PauseBorrow', 'PauseBorrowUntil', 'PauseLiquidation',
    'PauseLiquidationUntil', 'PauseDeposit', 'PauseDepositUntil', 'PauseRedeem', 'PauseRedeemUntil', 'MarginLiquidationControllers', 'PeridotToken'])
    out.controller[k] = await c.raw(C, key(k));
  const targets = {...Object.fromEntries(Object.entries(cfg.markets).map(([k, v]) => [k, v])), CL_STAGED: cfg.clStagedReceipt ? {receipt: cfg.clStagedReceipt} : undefined};
  out.markets = {};
  for (const [name, m] of Object.entries(targets)) {
    if (!m) continue;
    const R = m.receipt, r = {receipt: R, wasm: await c.codeHash(R)};
    r.controllerCF = (await c.raw(C, key('MarketCF', addr(R)))); r.pendingCF = await c.raw(C, key('PendingMarketCF', addr(R)));
    r.pendingCFEta = await c.raw(C, key('PendingMarketCFEta', addr(R)));
    for (const v of ['is_market_supported', 'is_borrow_paused', 'is_liquidation_paused', 'is_deposit_paused', 'is_redeem_paused']) r[v] = await c.tryView(C, v, [addr(R)]);
    for (const v of ['get_admin', 'get_underlying_token', 'get_min_cash_reserve_bps', 'get_cash_borrowing_enabled', 'get_borrowable_cash', 'get_available_liquidity',
      'get_total_borrowed', 'get_total_reserves', 'get_total_underlying', 'get_exchange_rate', 'get_margin_controller', 'get_boosted_vault', 'get_minimum_supply', 'get_cl_custody'])
      r[v] = await c.tryView(R, v);
    r.preview_flash_loan_fee_1e7 = await c.tryView(R, 'preview_flash_loan_fee', [S.nativeToScVal(10000000n, {type: 'u128'})]);
    for (const k of ['Peridottroller', 'InterestModel', 'RatesReady', 'YearlyRateScaled', 'BorrowYearlyRateScaled', 'ReserveFactorScaled', 'AdminFeeScaled',
      'FlashLoanFeeScaled', 'SupplyCap', 'BorrowCap', 'TotalBorrowPrincipal', 'CollateralFactorScaled', 'BorrowIndex', 'LastUpdateTime'])
      r[k] = await c.raw(R, key(k));
    if (m.underlying) { r.oracleSymbol = await c.raw(C, key('OracleAssetSymbol', addr(m.underlying))); r.priceCache = await c.raw(C, key('PriceCache', addr(m.underlying)));
      r.fallbackPrice = await c.raw(C, key('FallbackPrice', addr(m.underlying))); r.supportedToken = await c.raw(C, key('SupportedToken', addr(m.underlying))); }
    out.markets[name] = r;
  }
  out.ledger = (await c.rpc.getLatestLedger()).sequence;
  return out;
}
module.exports = {readState};
if (require.main === module) {
  const i = process.argv.indexOf('--testnet'); const cfg = i > 0 ? JSON.parse(fs.readFileSync(process.argv[i + 1], 'utf8')) : MAINNET;
  readState(cfg).then(s => console.log(json(s))).catch(e => { console.error(e.message); process.exit(1); });
}
