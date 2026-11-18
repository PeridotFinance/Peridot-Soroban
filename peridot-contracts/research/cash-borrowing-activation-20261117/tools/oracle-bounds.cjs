// Read-only: the D1 oracle facts the ceremony pins (feed metadata, symbols, staleness bound, guardian, controller risk
// parameters). Usage: node oracle-bounds.cjs > ../../cash-borrowing-ops-20261118/evidence/oracle-bounds-<date>.json. Read-only.
'use strict';
const {S, MAINNET, json, addr, key, sym, chain} = require('./lib.cjs');
const FIAT = 'CBKGPWGKSKZF52CFHMTRR23TBWTPMRDIYZ4O2P5VS65BMHYH4DXMCJZC';
const other = s => S.xdr.ScVal.scvVec([sym('Other'), sym(s)]);
const stellar = a => S.xdr.ScVal.scvVec([sym('Stellar'), addr(a)]);
async function readBounds(cfg = MAINNET, c = chain(cfg)) {
  const F = cfg.oracles.external, out = {observedAt: new Date().toISOString(), network: cfg.network, feed: {id: F}};
  for (const m of ['base', 'decimals', 'resolution', 'period', 'last_timestamp', 'version']) out.feed[m] = await c.tryView(F, m);
  out.feed.lastprice = {};
  for (const s of ['USDC', 'XLM', 'EURC', 'PYUSD']) out.feed.lastprice[`Other(${s})`] = await c.tryView(F, 'lastprice', [other(s)]);
  out.feed.lastprice['Stellar(PYUSD SAC)'] = await c.tryView(F, 'lastprice', [stellar(cfg.markets.PYUSD.underlying)]);
  out.fiat = {id: FIAT, resolution: await c.tryView(FIAT, 'resolution'), decimals: await c.tryView(FIAT, 'decimals'), EUR: await c.tryView(FIAT, 'lastprice', [other('EUR')])};
  const C = cfg.controller;
  out.controller = {id: C, get_oracle: await c.tryView(C, 'get_oracle'), close: await c.tryView(C, 'get_close_factor_scaled'),
    incentive: await c.tryView(C, 'get_liquidation_incentive_scaled'), liqFee: await c.tryView(C, 'get_liquidation_fee_scaled'),
    OracleMaxAgeMultiplier: await c.raw(C, key('OracleMaxAgeMultiplier')), PauseGuardian: await c.raw(C, key('PauseGuardian'))};
  const k = out.controller.OracleMaxAgeMultiplier ? Number(out.controller.OracleMaxAgeMultiplier.value) : 2;
  const res = Number(out.feed.resolution);
  out.derived = {k, kSource: out.controller.OracleMaxAgeMultiplier ? 'stored' : 'contract default 2', resolutionSeconds: res,
    maxAgeSeconds: k * res, rule: 'stale when price.timestamp + k*resolution < ledger.timestamp, or timestamp in the future (controller 3547-3743, 3791-3810)',
    feedAgeSeconds: Math.floor(Date.now() / 1000) - Number(out.feed.last_timestamp)};
  out.ledger = (await c.rpc.getLatestLedger()).sequence;
  return out;
}
module.exports = {readBounds};
if (require.main === module) readBounds().then(r => console.log(json(r))).catch(e => { console.error(e.message); process.exit(1); });
