// Margin pass (pooled margin v2, contracts/margin-controller build b1cb1cb2) inside the cash-liquidator process: one
// signer, one sequence. Each cycle: discovery (counter bootstrap + margin_open/close/liquidate events), liquidate every
// position with LTV >= its own liq_ltv (worst first, simulate -> profit check -> journal -> send), settle backstop bad
// debt until zero (self-sized, PooledShortfall #35 aware), clear expired backstop exit requests, reconcile (send only on
// true), then alerts. Never throws past marginPass(); dry run (DRY_RUN || MARGIN_DRY_RUN) never signs.
import * as S from '@stellar/stellar-sdk';
import {convertPass, newLot} from './convert.mjs';
import {E6, expectedLiquidation, maxPayFor, marginProfit, rankLiquidatable, applyMarginEvents, trackLiquidatable,
  isNewLedger, marginAlerts, gateAlerts, simErrorCode, ERR, liquidationFromEvents} from './margin-plan.mjs';

export const DUST_MSG = 'dustLiquidation: liquidating at an expected loss within the $2 dust limit';
const POS_EVENTS = ['margin_open', 'margin_close', 'margin_liquidate', 'margin_impaired'];
const BS_EVENTS = ['bs_request', 'bs_cancel', 'bs_withdraw'];
const keyVec = (name, ...addrs) => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(name), ...addrs.map(a => S.nativeToScVal(a, {type: 'address'}))]);
const big = v => (v == null ? null : BigInt(v));

export function marginState(s) {
  return {initialized: false, lastEventLedger: null, lastCycleLedger: null, active: [], positions: {}, backers: {}, liqSince: {}, alertLast: {}, failures: 0, ...(s || {})};
}

/** Startup/per-cycle pin check of the deployed margin contract. Returns {fees}. Throws on any mismatch. */
export async function verifyMargin(cfg, c) {
  const m = cfg.margin, M = cfg.pins.markets;
  const inst = await c.instance(m.contract);
  if (!m.wasmAllowed.includes(inst.wasm)) throw Error(`margin code not pinned: ${inst.wasm}`);
  const expect = [['Ctrl', [], m.controller], ['Usdc', [], M.USDC.underlying], ['Xlm', [], M.XLM.underlying],
    ['Market', [M.USDC.underlying], M.USDC.receipt], ['Market', [M.XLM.underlying], M.XLM.receipt], ...(m.pool ? [['Pool', [], m.pool]] : [])];
  for (const [k, a, want] of expect) { const got = inst.get(keyVec(k, ...a)); if (got !== want) throw Error(`margin ${k}${a.length ? '(' + a[0].slice(0, 6) + ')' : ''} is ${got}, pinned ${want}`); }
  for (const k of ['Admin', 'Guardian']) if (inst.get(keyVec(k)) === cfg.publicKey) throw Error(`refused: keeper key is the margin ${k}`);
  const fees = inst.get(keyVec('Fees'));
  if (!fees) throw Error('margin Fees not configured');
  return {fees, wasm: inst.wasm};
}

// Pages events from startLedger. The RPC refuses a ledger range together with a cursor ("ledger ranges and cursor cannot
// both be set", Mainnet 25 Nov), so no endLedger is sent; events past `endSeq` are filtered here. Returns the events and
// `through` = min(endSeq, the events node's own latestLedger), so the checkpoint never runs ahead of a lagging events node.
async function pageEvents(c, contractId, names, startLedger, endSeq) {
  const out = [];
  let req = {contractId, names, startLedger, limit: 100}, through = endSeq;
  for (let page = 0; page < 100; page++) {
    const r = await c.rawEvents(req);
    if (r.latestLedger != null && Number(r.latestLedger) < through) through = Number(r.latestLedger);
    const bounded = r.events.filter(e => e.ledger <= through);
    out.push(...bounded);
    if (bounded.length !== r.events.length || r.events.length < 100) return {events: out.filter(e => e.ledger <= through), through};
    if (!r.cursor) throw Error('event page full but RPC returned no cursor');
    req = {contractId, names, cursor: r.cursor, limit: 100};
  }
  throw Error('event paging did not finish in 100 pages');
}
// Only the retention error ("startLedger must be within the ledger range: a - b"); never the cursor/range refusal.
const retention = e => /startLedger must be within|before oldest/i.test(e.message);

function cachePosition(ms, id, p, cfg) {
  ms.positions[id] = {owner: p.owner, side: p.held_asset === cfg.pins.markets.XLM.underlying ? 'long' : 'short', heldAsset: p.held_asset,
    debtAsset: p.debt_asset, heldPt: String(p.held_ptokens), liqLtv: String(p.liq_ltv)};
}

/** Counter bootstrap (source of truth), then event sync keyed on ledger sequence. Mutates ms. */
export async function discoverMargin(cfg, c, ms, L, log) {
  const m = cfg.margin;
  let impairedSeen = false;
  if (!ms.initialized) {
    const counter = BigInt(await c.view(m.contract, 'get_position_counter'));
    const active = [];
    ms.positions = {};
    for (let id = 1n; id <= counter; id++) {
      const p = await c.view(m.contract, 'get_position', [c.u64(id)]);   // a failed read throws: bootstrap retries next cycle
      if (p && p.open) { active.push(String(id)); cachePosition(ms, String(id), p, cfg); }
    }
    let backers = Object.fromEntries(m.backersSeed.map(b => [b, ms.backers[b] ?? '0']));
    try {
      const {events: ev} = await pageEvents(c, m.contract, BS_EVENTS, Math.max(1, L.sequence - m.backerLookbackLedgers), L.sequence);
      backers = applyMarginEvents({active: [], backers}, ev).backers;
    } catch (e) {
      if (!retention(e)) throw e;
      log('alert', 'margin backer backfill outside RPC retention: list backers with an exit request in margin.backersSeed', {err: e.message});
    }
    Object.assign(ms, {active, backers: {...ms.backers, ...backers}, lastEventLedger: L.sequence, maxSeenId: String(counter), initialized: true});
    log('info', 'margin state bootstrapped', {counter, active: active.length, backers: Object.keys(ms.backers).length, ledger: L.sequence});
    return {impairedSeen};
  }
  // Counter catch-up every cycle (one view): an id above the last one seen is read directly, so an open whose event was
  // missed (lagging events node, lost cursor) is still found. The counter + get_position stay the source of truth.
  const counter = BigInt(await c.view(m.contract, 'get_position_counter'));
  for (let id = BigInt(ms.maxSeenId ?? '0') + 1n; id <= counter; id++) {
    const p = await c.view(m.contract, 'get_position', [c.u64(id)]);
    if (p && p.open && !ms.active.includes(String(id))) { ms.active.push(String(id)); cachePosition(ms, String(id), p, cfg); }
  }
  ms.maxSeenId = String(counter);
  const start = ms.lastEventLedger + 1;
  if (start > L.sequence) return {impairedSeen};
  let ev, through;
  try {
    const a = await pageEvents(c, m.contract, POS_EVENTS, start, L.sequence), b = await pageEvents(c, m.contract, BS_EVENTS, start, L.sequence);
    through = Math.min(a.through, b.through);
    ev = [...a.events, ...b.events].filter(e => e.ledger <= through);
  } catch (e) {
    if (!retention(e)) throw e;
    log('alert', 'margin event cursor outside RPC retention: re-bootstrapping positions from the counter; check margin.backersSeed', {err: e.message});
    ms.initialized = false;
    return discoverMargin(cfg, c, ms, L, log);
  }
  ev.sort((a, b) => a.ledger - b.ledger || String(a.id).localeCompare(String(b.id)));
  const r = applyMarginEvents({active: ms.active, backers: ms.backers}, ev);
  for (const id of Object.keys(ms.positions)) if (!r.active.has(id)) delete ms.positions[id];
  if (through < start - 1) through = start - 1;   // a lagging events node never moves the checkpoint backwards
  Object.assign(ms, {active: [...r.active], backers: r.backers, lastEventLedger: through});
  return {impairedSeen: r.impairedSeen};
}

async function price(cfg, c, token) {
  const p = await c.view(cfg.margin.controller, 'get_price_usd', [c.addr(token)]);
  return p == null ? null : [BigInt(p[0]), BigInt(p[1])];
}

async function readPositions(cfg, c, ms, fees, log) {
  const m = cfg.margin, list = [], unavailable = [];
  for (const id of [...ms.active]) {
    try {
      if (!ms.positions[id]) {
        const p = await c.view(m.contract, 'get_position', [c.u64(id)]);
        if (!p || !p.open) { ms.active = ms.active.filter(x => x !== id); continue; }
        cachePosition(ms, id, p, cfg);
      }
      const pc = ms.positions[id];
      const ltv = BigInt(await c.view(m.contract, 'get_position_ltv', [c.u64(id)]));
      const debt = BigInt(await c.view(m.contract, 'get_position_debt', [c.u64(id)]));
      const funding = BigInt(await c.view(m.contract, 'get_position_funding', [c.u64(id)]));
      const pos = {id, side: pc.side, owner: pc.owner, heldPt: BigInt(pc.heldPt), liqLtv: BigInt(pc.liqLtv), ltv, debt, funding};
      list.push({...pos, worstPay: maxPayFor(pos, fees).worst});
    } catch (e) {
      if (simErrorCode(e.message) === ERR.NotOpen) { ms.active = ms.active.filter(x => x !== id); delete ms.positions[id]; continue; }
      unavailable.push(id);
      log('warn', 'margin position read failed', {id, err: e.message});
    }
  }
  return {list, unavailable};
}

async function send(cfg, c, sim, kind, rec, deps) {
  const signed = c.sign(sim.prepared), hash = signed.hash().toString('hex');
  deps.journal(cfg.journalFile, {kind, intent: hash, ...rec});
  const res = await c.submit(signed);
  deps.journal(cfg.journalFile, {kind, result: res.status, hash, ledger: res.ledger});
  deps.log(res.status === 'SUCCESS' ? 'info' : 'alert', `${kind} submitted`, {...rec, ...res, metrics: sim.metrics});
  return res;
}

async function liquidateOne(cfg, c, p, env, deps) {
  const m = cfg.margin, M = cfg.pins.markets, log = deps.log;
  const long = p.side === 'long';
  const [heldName, debtName] = long ? ['XLM', 'USDC'] : ['USDC', 'XLM'];
  const ctx = {rateHeld: env.rates[heldName], priceHeld: env.prices[heldName], priceDebt: env.prices[debtName], fees: env.fees};
  const inv = env.inventory[debtName] ?? 0n;
  const model = expectedLiquidation(p, ctx);
  const {maxPay} = maxPayFor(p, env.fees, {bufferBps: m.payBufferBps, inventory: inv});
  const base = {id: p.id, side: p.side, ltv: p.ltv, liqLtv: p.liqLtv, debt: p.debt, modelPay: model.pay, underwater: model.underwater};
  if (inv < model.pay) { log('alert', `margin liquidation needs more ${debtName} inventory than held`, {...base, haveRaw: inv}); return {...base, skipped: 'inventory'}; }
  const sim = await c.prepare(m.contract, 'liquidate', [c.addr(cfg.publicKey), c.u64(p.id), c.u128(maxPay)], {cpuStop: m.cpuStop, maxFeeStroops: m.maxFeeStroops});
  if (!sim.ok) {
    const code = simErrorCode(sim.error);
    const benign = code === ERR.NotLiquidatable || code === ERR.NotOpen;
    const dryBalance = env.dry && code === 10;   // SAC #10: the keeper does not hold the payment yet (unfunded dry stage)
    log(benign || dryBalance ? 'info' : 'warn', benign ? 'margin position no longer liquidatable' : dryBalance ? 'DRY_RUN margin liquidate needs the payment balance' : 'margin liquidate simulation failed', {...base, maxPay, err: sim.error, metrics: sim.metrics});
    return {...base, skipped: 'simulation', err: sim.error};
  }
  const ev = liquidationFromEvents(sim.events, {margin: m.contract, keeper: cfg.publicKey, debtToken: M[debtName].underlying});
  const seizePt = ev?.seizePt ?? model.seizePt, pay = ev?.paid ?? model.pay;
  const prof = marginProfit({seizePt, rateHeld: ctx.rateHeld, priceHeld: ctx.priceHeld, pay, priceDebt: ctx.priceDebt,
    feeStroops: BigInt(sim.metrics.fee), priceXlm: env.prices.XLM, minProfitBps: m.minProfitBps});
  const rec = {...base, maxPay, pay, seizePt, source: ev ? (ev.paid != null ? 'sim events' : 'sim seize + model pay') : 'model', profitUsd7: prof.profitUsd, profitBps: prof.bps, feeUsd7: prof.feeUsd, metrics: sim.metrics};
  let dust = false;
  if (!prof.ok) {
    // Owner rule (25 Nov): liquidate at a loss when the expected loss (oracle USD, incl. the tx fee) is <= $2 per position.
    const lossUsd7 = prof.profitUsd < 0n ? -prof.profitUsd : 0n;
    if (lossUsd7 > m.dustMaxLossUsd7) {
      log('alert', 'unprofitable margin liquidation: expected loss above the $2 dust limit; not sent', {type: 'unprofitableSkipped', key: `unprofitable:${p.id}`, ...rec, lossUsd7});
      return {...rec, skipped: 'unprofitable', lossUsd7};
    }
    dust = true;
    log('alert', DUST_MSG, {type: 'dustLiquidation', key: `dust:${p.id}`, ...rec, lossUsd7, dryRun: env.dry});
  }
  if (env.dry) { log('info', 'DRY_RUN would liquidate margin position', {...rec, dust}); return {...rec, dust, dryRun: true}; }
  const res = await send(cfg, c, sim, 'margin_liquidate', {...rec, dust}, deps);
  if (res.status === 'SUCCESS' && env.st) {
    env.st.convert = {lots: [], budget: null, ...(env.st.convert || {})};
    env.st.convert.lots.push(newLot({positionId: p.id, side: p.side, seizePt, ledger: res.ledger ?? env.ledger, nowMs: env.nowMs, cfg, inventory: env.inventory}));
  }
  return {...rec, ...res, dust, stop: res.status !== 'SUCCESS'};
}

async function settlePass(cfg, c, env, deps) {
  const m = cfg.margin, M = cfg.pins.markets, log = deps.log, out = [];
  for (const name of ['USDC', 'XLM']) {
    const tok = c.addr(M[name].underlying);
    for (let i = 0; i < m.maxSettleIters; i++) {
      const shares = BigInt(await c.view(m.contract, 'get_bs_debt_shares', [tok]));
      if (shares === 0n) break;
      const mx = BigInt(await c.view(m.contract, 'settle_max_ptokens', [tok]));
      const rec = {asset: name, bsDebtShares: shares, maxPtokens: mx, iteration: i};
      // max_ptokens = the sizing view (0 is still valid: backstop XLM pTokens cover the debt, or #35 below).
      const sim = await c.prepare(m.contract, 'settle_backstop_debt', [c.addr(cfg.publicKey), tok, c.u128(mx), c.u128(0)], {cpuStop: m.cpuStop, maxFeeStroops: m.maxFeeStroops});
      if (!sim.ok) {
        const code = simErrorCode(sim.error);
        if (code === ERR.PooledShortfall) log('alert', 'margin settle blocked: PooledShortfall #35 (pooled account has no free liquidity): admin_recapitalize, or liquidations / short close_with_repay first', {...rec, err: sim.error});
        else if (code === ERR.BadAmount || code === ERR.BackstopEmpty) log('alert', 'margin settle cannot withdraw backstop USDC pTokens (backstop empty?): admin_recapitalize', {...rec, err: sim.error});
        else log('warn', 'margin settle simulation failed', {...rec, err: sim.error, metrics: sim.metrics});
        out.push({...rec, skipped: sim.error}); break;
      }
      const r2 = {...rec, repaid: sim.retval, metrics: sim.metrics};
      if (env.dry) { log('info', 'DRY_RUN would settle margin backstop debt', r2); out.push({...r2, dryRun: true}); break; }
      const res = await send(cfg, c, sim, 'margin_settle', r2, deps);
      out.push({...r2, ...res});
      if (res.status !== 'SUCCESS') return {out, stop: true};
      const after = BigInt(await c.view(m.contract, 'get_bs_debt_shares', [tok]));
      if (after >= shares) { log('alert', 'margin settle made no progress; stopping this cycle', {...rec, after}); break; }
    }
  }
  return {out};
}

async function clearExpired(cfg, c, ms, L, env, deps) {
  const m = cfg.margin, log = deps.log, out = [];
  const nowSec = BigInt(L.closeTime ?? Math.floor(env.nowMs / 1000));
  for (const backer of Object.keys(ms.backers)) {
    const req = await c.view(m.contract, 'get_withdraw_req', [c.addr(backer)]);
    if (!req) { delete ms.backers[backer]; continue; }
    ms.backers[backer] = String(req.expires_at);
    if (nowSec <= BigInt(req.expires_at)) continue;   // contract: expired iff now > expires_at (CAP-88: equal is live)
    const sim = await c.prepare(m.contract, 'backstop_clear_expired', [c.addr(cfg.publicKey), c.addr(backer)], {cpuStop: m.cpuStop, maxFeeStroops: m.maxFeeStroops});
    const rec = {backer, expiresAt: req.expires_at, shares: req.shares};
    if (!sim.ok) {
      const code = simErrorCode(sim.error);
      if (code === ERR.NoRequest) delete ms.backers[backer];
      log(code === ERR.BadParams || code === ERR.NoRequest ? 'info' : 'warn', 'backstop_clear_expired not applicable', {...rec, err: sim.error});
      continue;
    }
    if (env.dry) { log('info', 'DRY_RUN would clear expired backstop exit request', rec); out.push({...rec, dryRun: true}); continue; }
    const res = await send(cfg, c, sim, 'margin_clear_expired', rec, deps);
    out.push({...rec, ...res});
    if (res.status === 'SUCCESS') delete ms.backers[backer]; else return {out, stop: true};
  }
  return {out};
}

async function reconcilePass(cfg, c, env, deps) {
  const m = cfg.margin, log = deps.log;
  const sim = await c.prepare(m.contract, 'reconcile', [c.addr(cfg.publicKey)], {cpuStop: m.cpuStop, maxFeeStroops: m.maxFeeStroops});
  if (!sim.ok) { log('warn', 'margin reconcile simulation failed', {err: sim.error}); return {error: sim.error}; }
  if (sim.retval !== true) return {newlyImpaired: false};
  log('alert', 'margin reconcile found a pToken ledger deficit: sending reconcile sets Impaired and pauses opens', {metrics: sim.metrics, dryRun: env.dry});
  if (env.dry) return {newlyImpaired: true, dryRun: true};
  return {newlyImpaired: true, ...(await send(cfg, c, sim, 'margin_reconcile', {}, deps))};
}

async function tryView(c, contract, method, args, log) {
  try { return await c.view(contract, method, args); } catch (e) { log('warn', `margin ${method} failed`, {err: e.message}); return undefined; }
}

/** Emits alert lines (level 'alert'); `extra` tags them (e.g. {alertTest: true}). */
export function emitMarginAlerts(alerts, log, extra = {}) {
  for (const {msg, ...rest} of alerts) log('alert', msg, {...rest, ...extra});
}

/** One margin cycle. deps: {log, journal, inventoryOf, nowMs}. */
export async function marginCycle(cfg, c, st, deps) {
  const m = cfg.margin, log = deps.log;
  if (!m.enabled) return {skipped: 'disabled'};
  const nowMs = deps.nowMs ?? Date.now();
  const dry = cfg.dryRun || m.dryRun;
  const ms = st.margin = marginState(st.margin);
  const L = await c.latestLedger();
  if (!isNewLedger(ms.lastCycleLedger, L.sequence)) return {skipped: 'same ledger', ledger: L.sequence};
  const {fees} = await verifyMargin(cfg, c);
  const {impairedSeen} = await discoverMargin(cfg, c, ms, L, log);
  const M = cfg.pins.markets;
  const prices = {USDC: await price(cfg, c, M.USDC.underlying), XLM: await price(cfg, c, M.XLM.underlying)};
  const rates = {USDC: BigInt(await c.view(M.USDC.receipt, 'get_exchange_rate')), XLM: BigInt(await c.view(M.XLM.receipt, 'get_exchange_rate'))};
  const {list, unavailable} = await readPositions(cfg, c, ms, fees, log);
  const ranked = rankLiquidatable(list);
  ms.liqSince = trackLiquidatable(ms.liqSince, ranked.map(p => p.id), L.sequence, nowMs, unavailable);
  let inventory = await deps.inventoryOf(cfg, c);
  const env = {dry, fees, prices, rates, inventory, nowMs, st, ledger: L.sequence};
  const actions = {liquidations: [], conversions: [], settles: [], cleared: [], reconcile: null};
  let stop = false;
  if (prices.USDC && prices.XLM) {
    for (const p of ranked) {
      const r = await liquidateOne(cfg, c, p, env, deps);
      actions.liquidations.push(r);
      if (r.stop) { stop = true; break; }
      if (r.status === 'SUCCESS') env.inventory = inventory = await deps.inventoryOf(cfg, c);
    }
  } else if (ranked.length) log('alert', 'margin liquidations blocked: controller price unavailable', {usdc: !!prices.USDC, xlm: !!prices.XLM});
  if (!stop) { const s = await convertPass(cfg, c, st, env, deps); actions.conversions = s.out; stop = !!s.stop; }
  if (!stop) { const s = await settlePass(cfg, c, env, deps); actions.settles = s.out; stop = !!s.stop; }
  if (!stop) { const s = await clearExpired(cfg, c, ms, L, env, deps); actions.cleared = s.out; stop = !!s.stop; }
  if (!stop) actions.reconcile = await reconcilePass(cfg, c, env, deps);
  // State after the actions, for alerts.
  const h = await tryView(c, m.contract, 'pooled_health', [], log);
  const headroom = await tryView(c, m.contract, 'get_headroom', [], log);
  const impaired = await tryView(c, m.contract, 'is_impaired', [], log);
  const nav = await tryView(c, m.contract, 'backstop_nav', [], log);
  const bsShares = {};
  for (const n of ['USDC', 'XLM']) bsShares[n] = big(await tryView(c, m.contract, 'get_bs_debt_shares', [c.addr(M[n].underlying)], log)) ?? 0n;
  const snap = {positions: list, liqSince: ms.liqSince, health: h ? {hf: BigInt(h[0]), hfDown: BigInt(h[1]), hfUp: BigInt(h[2]), freeUsd: BigInt(h[3])} : null,
    headroom: headroom ?? null, impaired: impaired === true, impairedSeen: impairedSeen || actions.reconcile?.newlyImpaired, bsShares, inventory, unavailable};
  const alerts = marginAlerts(snap, {liquidatableAlertMs: m.liquidatableAlertMs, hfDownMin: m.hfDownMin}, nowMs);
  const g = gateAlerts(ms.alertLast, alerts, nowMs, m.realertMs);
  ms.alertLast = g.last;
  emitMarginAlerts(g.fire, log, {margin: m.contract, ledger: L.sequence});
  for (const key of g.resolved) log('info', 'alert resolved', {key, resolved: true, margin: m.contract, ledger: L.sequence});
  ms.lastCycleLedger = L.sequence;
  const view = {hf: snap.health?.hf, hfDown: snap.health?.hfDown, freeUsd7: snap.health?.freeUsd, impaired: snap.impaired, bsDebtShares: bsShares,
    backstopNavPt: nav ? nav[0] : undefined, backstopShares: nav ? nav[1] : undefined, backers: Object.keys(ms.backers).length, active: ms.active.length};
  return {ledger: L.sequence, dryRun: dry, view, positions: list.length, liquidatable: ranked.map(p => p.id), actions, alerts: alerts.map(a => a.key), fired: g.fire.map(a => a.key)};
}

/** Wrapper for the main loop: never throws; consecutive failures escalate to an alert. Margin failures never stop the cash pass. */
export async function marginPass(cfg, c, st, deps) {
  if (!cfg.margin.enabled) return {skipped: 'disabled'};
  st.margin = marginState(st.margin);
  try {
    const r = await marginCycle(cfg, c, st, deps);
    st.margin.failures = 0;
    if (!r.skipped) deps.log('info', 'margin heartbeat', {ledger: r.ledger, dryRun: r.dryRun, positions: r.positions, liquidatable: r.liquidatable.length, alerts: r.alerts, ...r.view});
    return r;
  } catch (e) {
    st.margin.failures = (st.margin.failures ?? 0) + 1;
    deps.log(st.margin.failures >= 3 ? 'alert' : 'warn', 'margin cycle failed', {err: e.message, failures: st.margin.failures});
    return {error: e.message};
  }
}

export {E6};
