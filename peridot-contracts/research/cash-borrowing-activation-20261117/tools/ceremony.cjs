// Cash-market borrowing activation ceremony builder. ONE unsigned transaction at a time, built from a fresh simulation
// taken right before it is written, with the admin Ledger account as source. It never signs, never sends, never reads keys.
// The operator signs on the device:
//   stellar tx sign --sign-with-ledger --rpc-url "$RPC" --network-passphrase "$NET" F.unsigned.xdr > F.signed.xdr
// checks it with `verify-signed`, sends it with `stellar tx send`, and confirms with `readback <step>`.
//
//   list                                   ordered steps with live done/pending status
//   simulate-all                           read-only: simulate every pending step against live state (fees, resources)
//   prepare <step> --out F.unsigned.xdr    fresh simulation, all earlier steps must read back done, writes XDR + JSON
//   readback <step>                        live post-state check for one step (exit 1 if not done)
//   gate <step>                            evaluate a non-transaction gate (price readback / liquidator ready)
//   verify-signed F.unsigned.xdr F.signed.xdr   same hash, exactly one valid admin signature
//   status <hash>                          RPC outcome (read-only)
//   kill <USDC|XLM|EURC> --out F           emergency: set_cash_borrowing_enabled(false) (durable, immediate)
//   pause-borrow <USDC|XLM|EURC> --out F   emergency: admin set_pause_borrow(market, true) (expires after 72 h)
//   unpause-borrow <USDC|XLM|EURC> --out F admin set_pause_borrow(market, false): the Ledger lifts a (watcher) pause
//   revoke-guardian --out F                emergency: set_pause_guardian(admin): retire a compromised pause key
//   caps                                   offline: borrow cap x close factor x buffer <= liquidator inventory
//   renew-cl-pause --out F                 admin set_pause_borrow(CL staged receipt, true): re-arm its 72 h pause
//
// Options: --config cfg.json (Testnet rehearsal addresses), --params params.json (default ../params.proposed.json).
// The CL staged receipt listed on the same controller is refused as a target or argument of every transaction.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {S, MAINNET, u, json, addr, u128, u32, bool, sym, key, chain, padResourceFee} = require('./lib.cjs');

const STRKEY = S.StrKey;
const LIMITS = Object.freeze({cpu: 97_000_000, keys: 100, writes: 50, maxFeeStroops: 5_000_000n, bidStroops: 10_000n, window: 40, timeout: 240});

// ---------------------------------------------------------------- pure step model (unit-tested)
function steps(cfg, p) {
  const M = cfg.markets, out = [], borrowable = Object.keys(p.markets).filter(k => p.markets[k].borrowable);
  const all = Object.keys(p.markets);
  const raw = (st, name, k) => st.markets[name][k] && st.markets[name][k].value !== undefined ? st.markets[name][k].value : null;
  const rx = (name, method, args, done, why) => out.push({id: `${name}.${method}`, kind: 'tx', target: M[name].receipt, market: name, method, args, done, why});
  for (const n of all) if (p.markets[n].supplyCapRaw != null) rx(n, 'set_supply_cap', () => [u128(p.markets[n].supplyCapRaw)],
    st => String(raw(st, n, 'SupplyCap')) === String(p.markets[n].supplyCapRaw), 'bound supplier NAV (0 = unlimited today)');
  for (const n of borrowable) rx(n, 'set_borrow_cap', () => [u128(p.markets[n].borrowCapRaw)],
    st => String(raw(st, n, 'BorrowCap')) === String(p.markets[n].borrowCapRaw), 'small principal cap (0 = unlimited today)');
  for (const n of borrowable) rx(n, 'set_reserve_factor', () => [u128(p.markets[n].reserveFactorScaled)],
    st => String(raw(st, n, 'ReserveFactorScaled')) === String(p.markets[n].reserveFactorScaled), 'share of interest to protocol reserves');
  for (const n of all) if (p.markets[n].flashLoanFeeScaled != null) rx(n, 'set_flash_loan_fee', () => [u128(p.markets[n].flashLoanFeeScaled)],
    st => String(raw(st, n, 'FlashLoanFeeScaled')) === String(p.markets[n].flashLoanFeeScaled), 'flash loans share the borrow gate; 100% fee is the only config deterrent');
  for (const n of borrowable) rx(n, 'set_borrow_rate', () => [u128(p.markets[n].borrowRateScaled)],
    st => String(raw(st, n, 'BorrowYearlyRateScaled')) === String(p.markets[n].borrowRateScaled), 'static borrow APR (no rate model contract)');
  for (const n of borrowable) rx(n, 'enable_static_rates', () => [addr(cfg.admin)],
    st => raw(st, n, 'RatesReady') === true && raw(st, n, 'InterestModel') == null, 'RatesReady=true; borrowing still blocked by CashBorrowingEnabled=false');
  for (const n of Object.keys(p.oracle.symbols)) out.push({id: `controller.set_oracle_asset_symbol.${n}`, kind: 'tx', target: cfg.controller, market: n,
    method: 'set_oracle_asset_symbol', args: () => [addr(M[n].underlying), S.xdr.ScVal.scvSymbol(p.oracle.symbols[n])],
    done: st => raw(st, n, 'oracleSymbol') === p.oracle.symbols[n], why: `price ${n} as Reflector Other(${p.oracle.symbols[n]})`});
  out.push({id: 'controller.set_oracle', kind: 'tx', target: cfg.controller, method: 'set_oracle', args: () => [addr(p.oracle.feed)],
    done: st => st.controller.get_oracle === p.oracle.feed, why: 'first set is immediate (no timelock); any later change waits 24 h'});
  out.push({id: 'gate.price-readback', kind: 'gate', gate: 'prices', why: 'controller get_price_usd equals the direct feed read; unpriced assets stay None'});
  for (const n of borrowable) out.push({id: `controller.set_market_cf.${n}`, kind: 'tx', target: cfg.controller, market: n, method: 'set_market_cf',
    args: () => [addr(M[n].receipt), u128(p.markets[n].cfScaled)],
    done: st => raw(st, n, 'controllerCF') != null && String(raw(st, n, 'controllerCF')) === String(p.markets[n].cfScaled),
    pre: st => assert.equal(raw(st, n, 'controllerCF'), null, `${n}: CF already set; a change is timelocked 24 h (propose, wait, execute)`),
    why: 'first set is immediate; later changes are 24 h timelocked, so this value must be right'});
  for (const n of borrowable) rx(n, 'set_min_cash_reserve_bps', () => [addr(cfg.admin), u32(p.markets[n].minCashReserveBps)],
    st => st.markets[n].get_min_cash_reserve_bps === p.markets[n].minCashReserveBps, `max utilisation ${(10000 - p.markets[n].minCashReserveBps) / 100}%`);
  // D2: a separate hot pause-only key becomes PauseGuardian (Ledger-signed). Only when params carry the field, so the
  // Testnet rehearsal params (no pauseGuardian) keep their historical step list.
  if ('pauseGuardian' in p) out.push({id: 'controller.set_pause_guardian', kind: 'tx', target: cfg.controller, method: 'set_pause_guardian',
    args: () => [addr(assertPauseKey(cfg, p))],
    done: st => !!st.controller.PauseGuardian && st.controller.PauseGuardian.value === (p.pauseGuardian && p.pauseGuardian.publicKey),
    pre: () => assertPauseKey(cfg, p),
    why: 'D2: hot pause-only key (watcher) may call pause_*_g for 72 h; it cannot unpause or change parameters; the Ledger unpauses'});
  out.push({id: 'gate.liquidator-ready', kind: 'gate', gate: 'liquidator', why: 'funded liquidator inventory and live keeper before the switch'});
  for (const n of borrowable) rx(n, 'set_cash_borrowing_enabled', () => [addr(cfg.admin), bool(true)],
    st => st.markets[n].get_cash_borrowing_enabled === true, 'THE SWITCH: last, one market at a time');
  // The switch is the only gate that should open borrowing; an active (72 h) controller pause would hide a broken setup.
  for (const s of out.filter(x => x.method === 'set_cash_borrowing_enabled'))
    s.pre = st => assert.equal(st.markets[s.market].is_borrow_paused, false, `${s.market}: controller borrow pause still active (expires after 72 h, or set_pause_borrow(false))`);
  return out;
}
// The pause key must be a plain G-address distinct from the admin Ledger and the liquidator.
function assertPauseKey(cfg, p) {
  const k = p.pauseGuardian && p.pauseGuardian.publicKey;
  assert(k, 'params.pauseGuardian.publicKey not set (fill in the pause key public key printed on its host)');
  assert(STRKEY.isValidEd25519PublicKey(k), 'pause key is not a valid G-address');
  assert.notEqual(k, cfg.admin, 'pause key must not be the admin Ledger');
  assert.notEqual(k, p.liquidator && p.liquidator.publicKey, 'pause key must not be the liquidator key');
  return k;
}
// Per-asset borrow cap must be repayable by the liquidator at the close factor, with a buffer for accrued interest
// (caps bound principal, debt includes interest). Aggregate per market == the cap, so the worst single position and the
// all-borrowers-at-once correlated case need the same first-round inventory. Returns rows; throws on a violation.
function checkCapsVsInventory(p) {
  const cf = u(p.controller.closeFactorScaled), buf = u(p.liquidator.capCoverage.bufferScaled), rows = {};
  for (const [n, m] of Object.entries(p.markets)) if (m.borrowable) {
    const need = (u(m.borrowCapRaw) * cf / 1_000_000n) * buf / 1_000_000n, inv = u(p.liquidator.minInventoryRaw[n]);
    rows[n] = {borrowCapRaw: m.borrowCapRaw, firstRoundRepayRaw: String(u(m.borrowCapRaw) * cf / 1_000_000n), withBufferRaw: String(need), inventoryRaw: String(inv), ok: need <= inv};
    assert(need <= inv, `${n}: cap x close factor x buffer ${need} exceeds liquidator inventory ${inv}`);
  }
  return rows;
}
// D1 pins: the live feed must be the USD-based external feed with the decimals/resolution the ceremony expects, and the
// controller's max age (k x resolution) must equal the pinned staleness bound. Pure; inputs come from feedMeta().
function checkFeedMeta(meta, p) {
  const o = p.oracle, errs = [];
  if (!Array.isArray(meta.base) || meta.base[0] !== 'Other' || meta.base[1] !== o.base) errs.push(`base ${JSON.stringify(meta.base)} != Other(${o.base})`);
  if (Number(meta.decimals) !== o.decimals) errs.push(`decimals ${meta.decimals} != ${o.decimals}`);
  if (Number(meta.resolution) !== o.resolution) errs.push(`resolution ${meta.resolution} != ${o.resolution}`);
  if (Number(meta.k) !== o.maxAgeMultiplier) errs.push(`stored max-age multiplier ${meta.k} != ${o.maxAgeMultiplier}`);
  if (Number(meta.k) * Number(meta.resolution) !== o.maxAgeSeconds) errs.push(`max age ${Number(meta.k) * Number(meta.resolution)} s != ${o.maxAgeSeconds} s`);
  for (const [n, s] of Object.entries(o.symbols)) { const lp = meta.prices && meta.prices[n]; if (!lp || !lp.price) errs.push(`no live Other(${s}) price`); }
  for (const n of o.unpriced) if (meta.prices && meta.prices[n]) errs.push(`${n} unexpectedly priced`);
  return {ok: errs.length === 0, errs};
}
async function feedMeta(cfg, c, p, st) {
  const F = p.oracle.feed, meta = {feed: F, prices: {}};
  for (const m of ['base', 'decimals', 'resolution']) meta[m] = (await c.view(F, m)).value;
  for (const [n, s] of Object.entries(p.oracle.symbols)) meta.prices[n] = (await c.view(F, 'lastprice', [S.xdr.ScVal.scvVec([sym('Other'), sym(s)])])).value;
  for (const n of p.oracle.unpriced) meta.prices[n] = (await c.view(F, 'lastprice', [S.xdr.ScVal.scvVec([sym('Stellar'), addr(cfg.markets[n].underlying)])])).value;
  const kv = st.controller.OracleMaxAgeMultiplier; meta.k = kv ? Number(kv.value) : 2;
  return meta;
}
// Emergency builders (pure). The admin's own pause path is used for K2 because after D2 the guardian is the hot key.
function emergencyOp(cfg, p, cmd, n) {
  if (cmd === 'revoke-guardian') return new S.Contract(cfg.controller).call('set_pause_guardian', addr(cfg.admin));
  assert(cfg.markets[n] && p.markets[n].borrowable, 'borrowable market required');
  if (cmd === 'kill') return new S.Contract(cfg.markets[n].receipt).call('set_cash_borrowing_enabled', addr(cfg.admin), bool(false));
  if (cmd === 'pause-borrow') return new S.Contract(cfg.controller).call('set_pause_borrow', addr(cfg.markets[n].receipt), bool(true));
  if (cmd === 'unpause-borrow') return new S.Contract(cfg.controller).call('set_pause_borrow', addr(cfg.markets[n].receipt), bool(false));
  throw Error('unknown emergency command');
}
// Every address in the operation and the target must not be the CL staged receipt.
function assertNotCl(cfg, op) {
  if (!cfg.clStagedReceipt) return;
  const ic = op.body().invokeHostFunctionOp().hostFunction().invokeContract();
  const target = S.Address.fromScAddress(ic.contractAddress()).toString();
  assert.notEqual(target, cfg.clStagedReceipt, 'refused: CL staged receipt is never a ceremony target');
  const walk = v => { const n = S.scValToNative(v); const s = JSON.stringify(n, (_, x) => typeof x === 'bigint' ? String(x) : x); assert(!s.includes(cfg.clStagedReceipt), 'refused: CL staged receipt appears in arguments'); };
  ic.args().forEach(walk);
}
function checkAuth(auth, op, admin) {
  assert.equal(auth.length, 1, 'exactly one authorization entry required');
  const a = auth[0];
  const cred = a.credentials().switch().name;
  assert(cred === 'sorobanCredentialsSourceAccount' || (cred === 'sorobanCredentialsAddress' && S.Address.fromScAddress(a.credentials().address().address()).toString() === admin),
    'only the admin may authorize');
  const root = a.rootInvocation(), f = root.function().contractFn();
  assert.equal(f.toXDR('base64'), op.body().invokeHostFunctionOp().hostFunction().invokeContract().toXDR('base64'), 'authorized root differs from the operation');
  assert.equal(root.subInvocations().length, 0, 'admin setters must not authorize nested calls');
}
function checkResources(data) {
  const r = data.resources(), fp = r.footprint(), keys = [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64'));
  assert(Number(r.instructions()) <= LIMITS.cpu, `CPU ${r.instructions()} above ${LIMITS.cpu}`);
  assert(keys.length <= LIMITS.keys && new Set(keys).size === keys.length, 'more than 100 or duplicate keys');
  assert(fp.readWrite().length <= LIMITS.writes, 'more than 50 writes');
  assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'archived entries: restore first (outside this tool)');
  return {cpu: Number(r.instructions()), keys: keys.length, writes: fp.readWrite().length, diskReadBytes: r.diskReadBytes(), writeBytes: r.writeBytes(), resourceFeeStroops: String(data.resourceFee().toString())};
}

// ---------------------------------------------------------------- chain-facing
async function stateOf(cfg, c) { return require('./read-state.cjs').readState(cfg, c); }
async function build(cfg, c, op, {source = cfg.admin} = {}) {
  assertNotCl(cfg, op);
  const acc = await c.rpc.getAccount(source), latest = await c.rpc.getLatestLedger();
  const tx = new S.TransactionBuilder(acc, {fee: String(LIMITS.bidStroops), networkPassphrase: cfg.network}).addOperation(op)
    .setLedgerbounds(latest.sequence + 1, latest.sequence + LIMITS.window).setTimeout(LIMITS.timeout).build();
  const sim = await c.rpc.simulateTransaction(tx);
  assert(S.rpc.Api.isSimulationSuccess(sim), 'simulation failed: ' + String(sim.error || '').split('\n')[0].slice(0, 240));
  assert(!sim.restorePreamble, 'restoration needed: outside this tool');
  checkAuth(sim.result.auth || [], tx.toEnvelope().v1().tx().operations()[0], source);
  const prepared = S.rpc.assembleTransaction(tx, padResourceFee(sim)).build();
  const data = prepared.toEnvelope().v1().tx().ext().sorobanData(), metrics = checkResources(data);
  assert(u(prepared.fee) <= LIMITS.maxFeeStroops, `fee ${prepared.fee} above the 0.5 XLM cap`);
  assert.equal(prepared.signatures.length, 0); assert.equal(prepared.source, source);
  return {prepared, metrics, simulatedLedger: sim.latestLedger, validLedgers: {min: latest.sequence + 1, maxExclusive: latest.sequence + LIMITS.window}};
}
function write(cfg, out, res, summary) {
  assert(out && out.endsWith('.unsigned.xdr'), '--out FILE.unsigned.xdr required');
  const meta = out.replace(/\.unsigned\.xdr$/, '.json');
  assert(!fs.existsSync(out) && !fs.existsSync(meta), 'output exists: never overwrite a prepared transaction');
  fs.writeFileSync(out, res.prepared.toXDR() + '\n', {mode: 0o600, flag: 'wx'});
  const record = {status: 'UNSIGNED_PREPARED_SIGN_ON_LEDGER', signing: false, submission: false, network: cfg.network, file: out,
    hash: res.prepared.hash().toString('hex'), source: res.prepared.source, sequence: res.prepared.sequence, maxFeeStroops: res.prepared.fee,
    validLedgers: res.validLedgers, simulatedLedger: res.simulatedLedger, metrics: res.metrics, ...summary,
    next: `stellar tx sign --sign-with-ledger --rpc-url ${cfg.rpc} --network-passphrase "${cfg.network}" ${out} > ${out.replace(/\.unsigned\.xdr$/, '.signed.xdr')}`};
  fs.writeFileSync(meta, json(record) + '\n', {mode: 0o600, flag: 'wx'});
  return record;
}
async function gate(cfg, c, p, name, st) {
  if (name === 'prices') {
    const rows = {}; let ok = true;
    for (const [n, m] of Object.entries(cfg.markets)) {
      const got = await c.tryView(cfg.controller, 'get_price_usd', [addr(m.underlying)]);
      const symbol = p.oracle.symbols[n];
      if (!symbol) { rows[n] = {expected: 'None (unpriced, supply-only)', got}; ok = ok && got === null; continue; }
      const direct = (await c.view(p.oracle.feed, 'lastprice', [S.xdr.ScVal.scvVec([sym('Other'), sym(symbol)])])).value;
      const dev = got && direct ? Number((u(got[0]) - u(direct.price)) * 10000n / u(direct.price)) : null;
      const age = direct ? Math.floor(Date.now() / 1000) - Number(direct.timestamp) : null;
      const good = !!got && u(got[1]) === 10n ** BigInt(p.oracle.decimals) && Math.abs(dev) <= p.oracle.readbackToleranceBps;
      rows[n] = {controller: got, direct, deviationBps: dev, ageSeconds: age, ok: good}; ok = ok && good;
    }
    // No static/fallback price may exist for any market token (set_price_fallback is never part of this plan).
    const st2 = st || await stateOf(cfg, c);
    for (const [n] of Object.entries(cfg.markets)) { const fb = st2.markets[n] && st2.markets[n].fallbackPrice; rows[n].fallbackPrice = fb ? fb.value : null; if (fb) ok = false; }
    if (p.oracle.maxAgeSeconds != null) { const fm = checkFeedMeta(await feedMeta(cfg, c, p, st2), p); rows.feed = fm; ok = ok && fm.ok; }
    return {gate: 'prices', ok, rows};
  }
  if (name === 'liquidator') {
    const lq = p.liquidator.publicKey; if (!lq) return {gate: 'liquidator', ok: false, reason: 'params.liquidator.publicKey not set'};
    const rows = {}; let ok = true;
    for (const [n, min] of Object.entries(p.liquidator.minInventoryRaw)) {
      const bal = await c.tryView(cfg.markets[n].underlying, 'balance', [addr(lq)]);
      // XLM is both repay inventory and the fee/reserve balance: require inventory + fee floor + account reserves.
      const need = n === 'XLM' ? u(min) + u(p.liquidator.minFeeXlmRaw) + u(p.liquidator.accountReserveXlmRaw || 0) : u(min);
      const good = typeof bal !== 'object' || bal === null ? u(bal || 0) >= need : false;
      // repay_on_behalf pulls with transfer_from: the liquidator must have approved each repay market.
      const alw = await c.tryView(cfg.markets[n].underlying, 'allowance', [addr(lq), addr(cfg.markets[n].receipt)]);
      const alwOk = typeof alw === 'bigint' && alw >= u(min);
      rows[n] = {balanceRaw: String(bal), minRaw: String(need), allowanceRaw: String(alw), ok: good && alwOk}; ok = ok && good && alwOk;
    }
    // Role separation: the liquidator holds no admin or guardian power.
    const st3 = await stateOf(cfg, c), guardian = st3.controller.PauseGuardian && st3.controller.PauseGuardian.value;
    rows.roles = {admin: cfg.admin, guardian, ok: lq !== cfg.admin && lq !== guardian && (!('pauseGuardian' in p) || guardian === p.pauseGuardian.publicKey)};
    ok = ok && rows.roles.ok;
    return {gate: 'liquidator', ok, liquidator: lq, rows, manual: 'also confirm keeper heartbeat/log shows DRY_RUN=false, cursor before activation ledger, zero errors'};
  }
  throw Error('unknown gate');
}
function renewClPauseOp(cfg) {
  assert(cfg.clStagedReceipt, 'no CL staged receipt configured');
  const op = new S.Contract(cfg.controller).call('set_pause_borrow', addr(cfg.clStagedReceipt), bool(true));
  const ic = op.body().invokeHostFunctionOp().hostFunction().invokeContract();
  assert.equal(S.Address.fromScAddress(ic.contractAddress()).toString(), cfg.controller);
  assert.equal(ic.functionName().toString(), 'set_pause_borrow');
  assert.deepEqual(ic.args().map(a => S.scValToNative(a)), [cfg.clStagedReceipt, true]);
  return op;
}
async function main(argv) {
  const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; };
  const cfg = opt('--config') ? JSON.parse(fs.readFileSync(opt('--config'), 'utf8')) : MAINNET;
  const p = JSON.parse(fs.readFileSync(opt('--params', path.join(__dirname, '..', 'params.proposed.json')), 'utf8'));
  const c = chain(cfg), cmd = argv[0], list = steps(cfg, p);
  if (cmd === 'caps') return {caps: checkCapsVsInventory(p)};
  // Planning commands refuse params whose caps the liquidator cannot cover; emergency and renew commands are never blocked.
  if (p.liquidator.capCoverage && ['list', 'simulate-all', 'prepare', 'gate', 'readback'].includes(cmd)) checkCapsVsInventory(p);
  const find = id => { const s = list.find(x => x.id === id); assert(s, `unknown step ${id}`); return s; };
  if (cmd === 'list' || cmd === 'simulate-all') {
    const st = await stateOf(cfg, c), rows = [];
    for (const s of list) {
      const row = {id: s.id, kind: s.kind, why: s.why};
      if (s.kind === 'tx') { row.done = s.done(st); if (cmd === 'simulate-all' && !row.done) {
        try { const op = new S.Contract(s.target).call(s.method, ...s.args()); const r = await build(cfg, c, op); row.sim = r.metrics; } catch (e) { row.simError = e.message; } } }
      rows.push(row);
    }
    return {network: cfg.network, ledger: st.ledger, steps: rows};
  }
  if (cmd === 'readback') { const s = find(argv[1]); const st = await stateOf(cfg, c); const done = s.kind === 'tx' ? s.done(st) : (await gate(cfg, c, p, s.gate, st)).ok;
    if (!done) process.exitCode = 1; return {step: s.id, done, ledger: st.ledger}; }
  if (cmd === 'gate') { const s = find(argv[1]); assert.equal(s.kind, 'gate'); const r = await gate(cfg, c, p, s.gate); if (!r.ok) process.exitCode = 1; return r; }
  if (cmd === 'prepare') {
    const s = find(argv[1]); assert.equal(s.kind, 'tx', 'gates are evaluated with `gate`');
    const st = await stateOf(cfg, c);
    for (const prev of list.slice(0, list.indexOf(s))) {
      if (prev.kind === 'tx') assert(prev.done(st), `earlier step ${prev.id} not read back as done`);
      else { const g = await gate(cfg, c, p, prev.gate, st); assert(g.ok, `gate ${prev.id} not satisfied: ${json(g)}`); }
    }
    assert(!s.done(st), `${s.id} already done`);
    if (s.pre) s.pre(st);
    // set_oracle is immediate on first set and 24 h to correct: verify the live feed against the D1 pins first.
    if (s.id === 'controller.set_oracle' && p.oracle.maxAgeSeconds != null) { const fm = checkFeedMeta(await feedMeta(cfg, c, p, st), p); assert(fm.ok, 'feed check failed: ' + fm.errs.join('; ')); }
    const op = new S.Contract(s.target).call(s.method, ...s.args());
    const res = await build(cfg, c, op);
    return write(cfg, opt('--out'), res, {step: s.id, call: s.method, target: s.target, why: s.why, readback: `node ceremony.cjs readback ${s.id}`});
  }
  if (['kill', 'pause-borrow', 'unpause-borrow', 'revoke-guardian'].includes(cmd)) {
    const n = cmd === 'revoke-guardian' ? null : argv[1];
    const op = emergencyOp(cfg, p, cmd, n);
    const ic = op.body().invokeHostFunctionOp().hostFunction().invokeContract();
    const res = await build(cfg, c, op);
    return write(cfg, opt('--out'), res, {step: n ? `${cmd}.${n}` : cmd, call: `${ic.functionName().toString()}(${ic.args().map(a => JSON.stringify(S.scValToNative(a))).join(', ')})`,
      target: S.Address.fromScAddress(ic.contractAddress()).toString(), emergency: true});
  }
  if (cmd === 'renew-cl-pause') {
    // The ONLY transaction that may name the CL staged receipt: re-arm its 72 h controller borrow pause (paused=true, admin).
    const op = renewClPauseOp(cfg);
    const res = await build({...cfg, clStagedReceipt: null}, c, op);
    return write(cfg, opt('--out'), res, {step: 'renew-cl-pause', call: `set_pause_borrow(${cfg.clStagedReceipt}, true)`, target: cfg.controller,
      readback: 'node read-state.cjs (CL_STAGED is_borrow_paused true, PauseBorrowUntil = close time + 259200)'});
  }
  if (cmd === 'verify-signed') {
    const a = S.TransactionBuilder.fromXDR(fs.readFileSync(argv[1], 'utf8').trim(), cfg.network), b = S.TransactionBuilder.fromXDR(fs.readFileSync(argv[2], 'utf8').trim(), cfg.network);
    assert.equal(a.hash().toString('hex'), b.hash().toString('hex'), 'signed transaction differs from the prepared one');
    assert.equal(b.signatures.length, 1, 'exactly one signature');
    const kp = S.Keypair.fromPublicKey(b.source);
    assert(kp.verify(b.hash(), b.signatures[0].signature()), 'signature does not verify for the source');
    return {ok: true, hash: b.hash().toString('hex'), source: b.source};
  }
  if (cmd === 'status') { const r = await c.rpc.getTransaction(argv[1]); return {hash: argv[1], status: r.status, ledger: r.ledger}; }
  throw Error('usage: see header');
}
module.exports = {steps, assertNotCl, renewClPauseOp, checkAuth, checkResources, LIMITS, assertPauseKey, checkCapsVsInventory, checkFeedMeta, emergencyOp};
if (require.main === module) main(process.argv.slice(2)).then(r => console.log(json(r))).catch(e => { console.error('REFUSED/FAILED:', e.message); process.exit(1); });
