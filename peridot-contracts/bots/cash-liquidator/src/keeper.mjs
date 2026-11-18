import fs from 'node:fs';
import path from 'node:path';
import * as S from '@stellar/stellar-sdk';
import {planLiquidation, seizeIsProfitable, depegAlerts, mergeBorrowers} from './plan.mjs';

const log = (level, msg, extra = {}) => console.log(JSON.stringify({t: new Date().toISOString(), level, msg, ...extra}, (_, v) => (typeof v === 'bigint' ? String(v) : v)));

export function loadState(file) {
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return {cursor: null, startLedger: null, borrowers: []}; }
}
export function saveState(file, st) {
  fs.mkdirSync(path.dirname(file), {recursive: true});
  fs.writeFileSync(file + '.tmp', JSON.stringify(st, null, 1)); fs.renameSync(file + '.tmp', file);
}
function journal(file, rec) { fs.mkdirSync(path.dirname(file), {recursive: true}); fs.appendFileSync(file, JSON.stringify({t: new Date().toISOString(), ...rec}, (_, v) => (typeof v === 'bigint' ? String(v) : v)) + '\n'); }

export async function verifyPins(cfg, c) {
  const net = await c.rpc.getNetwork();
  if (net.passphrase !== cfg.network) throw Error('RPC network mismatch');
  if (cfg.pins.controllerWasm && (await c.codeHash(cfg.pins.controller)) !== cfg.pins.controllerWasm) throw Error('controller code changed');
  for (const [n, m] of Object.entries(cfg.pins.markets)) if (cfg.pins.marketWasm && (await c.codeHash(m.receipt)) !== cfg.pins.marketWasm) throw Error(`${n} code changed`);
}

async function positionsOf(cfg, c, user, entered, priceCache) {
  const out = [];
  for (const [market, m] of Object.entries(cfg.pins.markets)) {
    if (!entered.includes(m.receipt)) continue;
    const [pbal, debt, rate] = await c.view(m.receipt, 'get_account_snapshot', [c.addr(user)]);
    if (!priceCache.has(m.underlying)) priceCache.set(m.underlying, await c.view(cfg.pins.controller, 'get_price_usd', [c.addr(m.underlying)]));
    const pr = priceCache.get(m.underlying);
    const cf = BigInt(await c.view(cfg.pins.controller, 'get_market_cf', [c.addr(m.receipt)]));
    out.push({market, receipt: m.receipt, pbal: BigInt(pbal), debt: BigInt(debt), rate: BigInt(rate), price: pr ? BigInt(pr[0]) : null, scale: pr ? BigInt(pr[1]) : null, cf});
  }
  return out;
}

async function inventoryOf(cfg, c) {
  const inv = {};
  for (const [n, m] of Object.entries(cfg.pins.markets)) inv[n] = BigInt(await c.view(m.underlying, 'balance', [c.addr(cfg.publicKey)]));
  return inv;
}

export async function discover(cfg, c, st) {
  const ids = Object.values(cfg.pins.markets).filter(m => m.borrowable).map(m => m.receipt);
  if (!st.cursor && !st.startLedger) {
    const latest = (await c.rpc.getLatestLedger()).sequence;
    st.startLedger = Math.max(1, latest - cfg.eventsLookbackLedgers);
  }
  for (let page = 0; page < 50; page++) {
    let r;
    try { r = await c.borrowEvents(ids, st.cursor ? {cursor: st.cursor} : {startLedger: st.startLedger}); }
    catch (e) {
      if (/startLedger must be|before oldest|out of range/i.test(e.message)) { log('alert', 'event cursor outside RPC retention: backfill borrowers from history before trusting coverage', {err: e.message}); st.cursor = null; st.startLedger = null; return st; }
      throw e;
    }
    st.borrowers = mergeBorrowers(st.borrowers, r.events);
    if (r.cursor) st.cursor = r.cursor;
    if (r.events.length < 200) break;
  }
  return st;
}

// Approve every borrowable market up front (live mode) so the first liquidation does not wait on an approval and the
// ceremony gate G2 can read the allowances. In DRY_RUN this only reports what it would approve.
export async function ensureAllAllowances(cfg, c) {
  for (const [n, m] of Object.entries(cfg.pins.markets)) if (m.borrowable && !cfg.neverTouch.has(m.receipt)) {
    const bal = BigInt(await c.view(m.underlying, 'balance', [c.addr(cfg.publicKey)]));
    if (bal > 0n) await ensureAllowance(cfg, c, m, bal);
  }
}

export async function cycle(cfg, c, st) {
  await discover(cfg, c, st); saveState(cfg.stateFile, st);
  const closeFactor = BigInt(await c.view(cfg.pins.controller, 'get_close_factor_scaled'));
  const li = BigInt(await c.view(cfg.pins.controller, 'get_liquidation_incentive_scaled'));
  const xlm = BigInt(await c.view(cfg.pins.markets.XLM.underlying, 'balance', [c.addr(cfg.publicKey)]));
  if (xlm < cfg.minFeeXlmRaw) log('alert', 'liquidator XLM below fee floor', {xlm});
  if (cfg.pins.depeg) await depegCheck(cfg, c);
  if (cfg.pins.oracle) await refreshDriftedPrices(cfg, c);
  const priceCache = new Map(), results = [];
  let inventory = await inventoryOf(cfg, c);
  for (const user of st.borrowers) {
    let liq;
    try { liq = await c.view(cfg.pins.controller, 'account_liquidity', [c.addr(user)]); } catch (e) { log('warn', 'account_liquidity failed', {user, err: e.message}); continue; }
    const shortfall = BigInt(liq[1]);
    if (shortfall === 0n) continue;
    const entered = await c.view(cfg.pins.controller, 'get_user_markets', [c.addr(user)]);
    const positions = await positionsOf(cfg, c, user, entered, priceCache);
    const plan = planLiquidation(positions, inventory, {closeFactor, li, minProfitBps: cfg.minProfitBps, neverTouch: cfg.neverTouch});
    if (plan.action !== 'liquidate') { log(plan.reason.includes('alert') || plan.reason.includes('bad debt') ? 'alert' : 'warn', 'skip', {user, shortfall, reason: plan.reason}); continue; }
    const M = cfg.pins.markets, rm = M[plan.repayMarket], cm = M[plan.collateralMarket];
    if (cfg.neverTouch.has(rm.receipt) || cfg.neverTouch.has(cm.receipt)) throw Error('refused: never-touch market selected');
    // repay_on_behalf pulls the repayment with transfer_from on the liquidator's allowance (receipt-vault 4764-4772).
    if (!(await ensureAllowance(cfg, c, rm, plan.repay))) continue;
    const args = [c.addr(user), c.addr(rm.receipt), c.addr(cm.receipt), c.u128(plan.repay), c.addr(cfg.publicKey)];
    const sim = await c.prepare(cfg.pins.controller, 'liquidate', args);
    if (!sim.ok) { log(cfg.dryRun && /Contract, #9/.test(sim.error) ? 'info' : 'warn', cfg.dryRun && /Contract, #9/.test(sim.error) ? 'DRY_RUN liquidate needs the repay allowance (approved when live)' : 'liquidate simulation failed', {user, plan, err: sim.error}); continue; }
    const coll = positions.find(p => p.market === plan.collateralMarket);
    // liquidate() returns nothing (controller 2603-2623); the controller's own preview gives the seize for this repay.
    const seize = sim.retval != null ? BigInt(sim.retval) : BigInt(await c.view(cfg.pins.controller, 'preview_seize_ptokens', [c.addr(rm.receipt), c.addr(cm.receipt), c.u128(plan.repay)]));
    sim.retval = seize;
    const prof = seizeIsProfitable(seize, coll, plan.repayUsd, cfg.minProfitBps);
    if (!prof.ok) { log('warn', 'unprofitable seize', {user, plan, seizePtokens: sim.retval, seizeUsd: prof.seizeUsd}); continue; }
    const rec = {user, plan, seizePtokens: sim.retval, metrics: sim.metrics};
    if (cfg.dryRun) { log('info', 'DRY_RUN would liquidate', rec); results.push({...rec, dryRun: true}); continue; }
    const signed = c.sign(sim.prepared), hash = signed.hash().toString('hex');
    journal(cfg.journalFile, {kind: 'liquidate', intent: hash, ...rec});
    const res = await c.submit(signed);
    journal(cfg.journalFile, {kind: 'liquidate', result: res.status, hash, ledger: res.ledger});
    log(res.status === 'SUCCESS' ? 'info' : 'alert', 'liquidate submitted', {...rec, ...res});
    results.push({...rec, ...res});
    if (res.status === 'SUCCESS' && cfg.withdrawSeized) await withdrawSeized(cfg, c, cm, plan.collateralMarket, results);
    inventory = await inventoryOf(cfg, c);
  }
  return results;
}

// Approve the repay market as spender of the underlying (SAC approve, expiring). Approves the whole current balance,
// valid ~29 days; re-approves when the allowance is short. Returns true when the allowance covers `need`.
async function ensureAllowance(cfg, c, m, need) {
  const have = BigInt(await c.view(m.underlying, 'allowance', [c.addr(cfg.publicKey), c.addr(m.receipt)]));
  if (have >= need) return true;
  const bal = BigInt(await c.view(m.underlying, 'balance', [c.addr(cfg.publicKey)]));
  const expiry = (await c.rpc.getLatestLedger()).sequence + 500_000;
  const amt = S.nativeToScVal(bal > need ? bal : need, {type: 'i128'});
  const sim = await c.prepare(m.underlying, 'approve', [c.addr(cfg.publicKey), c.addr(m.receipt), amt, S.nativeToScVal(expiry, {type: 'u32'})]);
  if (!sim.ok) { log('warn', 'approve simulation failed', {receipt: m.receipt, err: sim.error}); return false; }
  if (cfg.dryRun) { log('info', 'DRY_RUN would approve repay allowance', {receipt: m.receipt, amount: bal}); return true; }
  const signed = c.sign(sim.prepared);
  journal(cfg.journalFile, {kind: 'approve', intent: signed.hash().toString('hex'), receipt: m.receipt});
  const res = await c.submit(signed);
  log(res.status === 'SUCCESS' ? 'info' : 'alert', 'approved repay allowance', {receipt: m.receipt, ...res, metrics: sim.metrics});
  return res.status === 'SUCCESS';
}

async function withdrawSeized(cfg, c, cm, market, results) {
  const bal = BigInt(await c.view(cm.receipt, 'get_ptoken_balance', [c.addr(cfg.publicKey)]));
  if (bal === 0n) return;
  const sim = await c.prepare(cm.receipt, 'withdraw', [c.addr(cfg.publicKey), c.u128(bal)]);
  if (!sim.ok) { log('warn', 'withdraw of seized pTokens failed in simulation (cash may be lent out); retry next cycle', {market, bal, err: sim.error}); return; }
  if (cfg.dryRun) return;
  const signed = c.sign(sim.prepared);
  journal(cfg.journalFile, {kind: 'withdraw', intent: signed.hash().toString('hex'), market, bal});
  const res = await c.submit(signed);
  journal(cfg.journalFile, {kind: 'withdraw', result: res.status, hash: res.hash});
  log('info', 'withdrew seized collateral', {market, bal, ...res, metrics: sim.metrics});
  results.push({kind: 'withdraw', market, bal, ...res, metrics: sim.metrics});
}

// The controller serves its cached price until it is k*resolution old (600 s), even when the feed has a newer round
// (simple-peridottroller 3504-3516). cache_price(token) is permissionless and rewrites the cache from the live feed.
// Refresh only when the feed has drifted from the cached value, so a fast move is visible to liquidate() at once.
async function refreshDriftedPrices(cfg, c) {
  const other = s => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(s)]);
  for (const [n, symbol] of Object.entries(cfg.pins.oracle.symbols)) {
    const tok = cfg.pins.markets[n].underlying;
    try {
      const live = await c.view(cfg.pins.oracle.feed, 'lastprice', [other(symbol)]);
      const cached = await c.view(cfg.pins.controller, 'get_price_usd', [c.addr(tok)]);
      if (!live || !cached) continue;
      const drift = Number(((BigInt(live.price) - BigInt(cached[0])) * 10_000n) / BigInt(cached[0]));
      if (Math.abs(drift) < (cfg.pins.oracle.refreshDriftBps ?? 30)) continue;
      const sim = await c.prepare(cfg.pins.controller, 'cache_price', [c.addr(tok)]);
      if (!sim.ok) { log('warn', 'cache_price simulation failed', {market: n, err: sim.error}); continue; }
      if (cfg.dryRun) { log('info', 'DRY_RUN would refresh price cache', {market: n, drift}); continue; }
      const res = await c.submit(c.sign(sim.prepared));
      log('info', 'refreshed price cache', {market: n, drift, ...res, metrics: sim.metrics});
    } catch (e) { log('warn', 'price refresh check failed', {market: n, err: e.message}); }
  }
}

async function depegCheck(cfg, c) {
  const other = s => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(s)]);
  try {
    const usdc = await c.view(cfg.pins.depeg.external, 'lastprice', [other('USDC')]);
    const eurc = await c.view(cfg.pins.depeg.external, 'lastprice', [other('EURC')]);
    const eur = await c.view(cfg.pins.depeg.fiat, 'lastprice', [other('EUR')]);
    const alerts = depegAlerts({usdc: usdc && BigInt(usdc.price), eurc: eurc && BigInt(eurc.price), eurFiat: eur && BigInt(eur.price)});
    for (const a of alerts) log('alert', 'DEPEG: consider kill switch set_cash_borrowing_enabled(false)', a);
  } catch (e) { log('warn', 'depeg check failed', {err: e.message}); }
}

export {log};
