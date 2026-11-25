// Pooled margin v2 Mainnet ceremony builder (WP8). Modelled on research/cl-collateral-group-20261118/tools/clg-ceremony.cjs
// and reusing the cash tools' read-only lib.cjs (patchRpc: raw getLatestLedger, protocol-30 safe).
// ONE unsigned transaction at a time, built from a fresh live-Mainnet simulation taken right before it is written.
// It never signs, never sends and never reads keys. The operator signs on the Ledger (margin-run.sh `step`).
//
//   addresses                         deterministic margin id (admin + salt; no network)
//   list                              ordered steps (M/C numbers) with live done/pending status
//   simulate-all [--fork F.json] [--out F.json]   read-only: simulate every pending step that can run live now; attach
//                                     the fork-harness rows (harness/margin-fork wp8 test) for the deploy-dependent ones
//   prepare <step> --out F.unsigned.xdr   fresh simulation; every earlier step must read back done
//   readback <step>                   live post-state check (exit 1 if not done)
//   gate <preflight|seed-balance|config|keeper|canary>   non-transaction gates (exit 1 if not ok)
//   headroom                          live cash-market headroom for the pilot (borrow caps, 70 % reserve)
//   gate-template                     PRINTS (never writes) the keeper gate file template for the OPERATOR to fill in
//   exclusion-fragment                prints the cash-liquidator exclusion the WP7 keeper must carry (margin id)
//   emergency <name> [args] --out F   kill switches / K-steps / rollback (see EMERGENCY); never blocked by earlier steps
//   verify-signed F.unsigned.xdr F.signed.xdr   same hash, exactly one valid signature by the expected source
//   status <hash>                     RPC outcome (read-only)
//
// Refusals (tested): any receipt hook / margin hook / deposit-pause method anywhere (operation or auth tree); retired
// or never-touch contracts as target, argument or auth node; any target other than the margin contract (the only
// exception: the conditional FB1 USDC receipt `deposit`); an artifact whose sha256/size is not the pin; the unpause
// (M16) and every resume unless the operator-written keeper gate passes; a guardian that is still a placeholder.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const CASH_TOOLS = '/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/research/cash-borrowing-activation-20261117/tools';
const {S, u, json, addr, u128, u32, bool, key, chain, padResourceFee} = require(path.join(CASH_TOOLS, 'lib.cjs'));

const CFG_FILE = path.join(__dirname, 'margin-mainnet.json');
const sha = b => crypto.createHash('sha256').update(b).digest();
const u64v = x => S.nativeToScVal(u(x), {type: 'u64'});
const E6 = 1_000_000n;
const fmt7 = r => (Number(u(r)) / 1e7).toFixed(7);
const uv = v => (['bigint', 'number', 'string'].includes(typeof v) ? u(v) : 0n); // failed view -> 0
const js = v => JSON.stringify(v, (_, y) => typeof y === 'bigint' ? String(y) : y);

// Every margin entrypoint this tool may call. Anything else on the margin contract is refused.
const MARGIN_METHODS = new Set(['set_market', 'enter_markets', 'set_pool', 'set_guardian', 'set_opens_paused', 'propose_upgrade', 'upgrade',
  'set_risk', 'set_caps', 'set_fees', 'set_max_slippage', 'set_headroom', 'set_backstop_params', 'backstop_deposit', 'backstop_request_withdraw',
  'backstop_cancel_withdraw', 'backstop_withdraw', 'admin_recapitalize', 'settle_backstop_debt', 'reconcile', 'admin_resolve_impairment',
  'open', 'close', 'close_with_repay', 'claim', 'bump_ttl']);

// ---------------------------------------------------------------- pure (unit-tested)
function contractIdFor(network, deployer, label) {
  const preimage = S.xdr.HashIdPreimage.envelopeTypeContractId(new S.xdr.HashIdPreimageContractId({
    networkId: sha(Buffer.from(network, 'utf8')),
    contractIdPreimage: S.xdr.ContractIdPreimage.contractIdPreimageFromAddress(new S.xdr.ContractIdPreimageFromAddress({
      address: S.Address.fromString(deployer).toScAddress(), salt: sha(Buffer.from(label, 'utf8'))})),
  }));
  return S.StrKey.encodeContract(sha(preimage.toXDR()));
}
const marginId = cfg => contractIdFor(cfg.network, cfg.admin, cfg.salt);
const need = (v, name) => { assert(v, `${name} not set in margin-mainnet.json (PLACEHOLDER / owner decision)`); return v; };

// contracttype structs are ScMaps with symbol keys sorted; every field here is a u128 or u64.
function structVal(obj, types = {}) {
  const ks = Object.keys(obj).sort();
  return S.xdr.ScVal.scvMap(ks.map(k => new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol(k),
    val: types[k] === 'u64' ? u64v(obj[k]) : u128(obj[k])})));
}
const feesVal = f => structVal(f);
const capsVal = c => structVal(c);
const headroomVal = h => structVal(h);
const bsParamsVal = p => structVal(p, {cooldown_s: 'u64', window_s: 'u64'});
const sameNum = (got, want) => !!got && Object.keys(want).every(k => String(got[k]) === String(want[k]));

// Call guard: denylist (hooks, deposit pause) first, then retired / never-touch, then the allowlist.
function assertCallAllowed(cfg, target, method, step = {}) {
  assert(!cfg.denyMethods.includes(method), `refused: ${method} is a receipt/controller hook or deposit-pause function (custody-only; no deposit pause)`);
  assert(!/margin_controller|pause_deposit/.test(method), `refused: ${method} looks like a hook / deposit-pause function`);
  assert(!cfg.retired.includes(target), `refused: ${target} is a retired contract`);
  assert(!cfg.neverTarget.includes(target), `refused: ${target} is a never-touch contract`);
  if (step.kind === 'mint') {
    assert(target === cfg.deps.usdcReceipt && method === 'deposit', 'refused: the mint step may only call the USDC receipt deposit');
    return true;
  }
  assert.equal(target, marginId(cfg), `refused: ${target} is not the margin contract ${marginId(cfg)}`);
  assert(MARGIN_METHODS.has(method), `refused: margin method ${method} is not on the ceremony allowlist`);
  return true;
}
// Walk every node of an authorization tree: no hook/deny method, no retired / never-touch contract anywhere.
function scanAuthTree(cfg, inv) {
  const f = inv.function();
  if (f.switch().name === 'sorobanAuthorizedFunctionTypeContractFn') {
    const cf = f.contractFn(), c = S.Address.fromScAddress(cf.contractAddress()).toString(), m = cf.functionName().toString();
    assert(!cfg.denyMethods.includes(m) && !/margin_controller|pause_deposit/.test(m), `refused: ${m} in the authorization tree`);
    assert(!cfg.retired.includes(c) && !cfg.neverTarget.includes(c), `refused: ${c} in the authorization tree`);
    const a = js(cf.args().map(x => S.scValToNative(x)));
    for (const r of [...cfg.retired, ...cfg.neverTarget]) assert(!a.includes(r), `refused: ${r} in authorized arguments`);
  }
  for (const s of inv.subInvocations()) scanAuthTree(cfg, s);
}
function assertNoRetiredArgs(cfg, op) {
  const ic = op.body().invokeHostFunctionOp().hostFunction().invokeContract();
  const s = js(ic.args().map(a => S.scValToNative(a)));
  for (const r of [...cfg.retired, ...cfg.neverTarget]) assert(!s.includes(r), `refused: retired/never-touch contract ${r} in arguments`);
}
function checkArtifact(cfg, wasm) {
  const h = sha(wasm).toString('hex');
  assert.equal(h, cfg.artifact.sha256, `refused: artifact sha256 ${h} is not the pinned ${cfg.artifact.sha256}`);
  assert.equal(wasm.length, cfg.artifact.bytes, `refused: artifact size ${wasm.length} != pinned ${cfg.artifact.bytes}`);
  return h;
}
function deployArgs(cfg) {
  const D = cfg.deps;
  return [addr(cfg.admin), addr(D.controller), addr(need(cfg.signers.guardian.publicKey, 'signers.guardian.publicKey')), addr(D.usdc), addr(D.xlm)];
}

// auth policies: none | root | create | root+transfer
function checkAuth(cfg, auth, op, source, policy, opts = {}) {
  for (const a of auth) scanAuthTree(cfg, a.rootInvocation());
  if (policy === 'none') { assert.equal(auth.length, 0, 'no authorization expected'); return; }
  assert.equal(auth.length, 1, 'exactly one authorization entry required');
  const a = auth[0];
  assert.equal(a.credentials().switch().name, 'sorobanCredentialsSourceAccount', 'only the transaction source may authorize');
  const root = a.rootInvocation(), f = root.function();
  if (policy === 'create') {
    assert.equal(f.switch().name, 'sorobanAuthorizedFunctionTypeCreateContractV2HostFn', 'root must be the contract creation with constructor');
    const cc = f.createContractV2HostFn(), want = op.body().invokeHostFunctionOp().hostFunction().createContractV2();
    assert.equal(cc.toXDR('base64'), want.toXDR('base64'), 'authorized creation differs from the operation');
    assert.equal(cc.executable().wasmHash().toString('hex'), cfg.artifact.sha256, 'deploy: wasm hash is not the pin');
    assert.equal(js(cc.constructorArgs().map(x => S.scValToNative(x))), js(opts.constructorArgs.map(x => S.scValToNative(x))),
      'deploy: constructor args (admin, controller, guardian, USDC, XLM) differ');
    assert.equal(root.subInvocations().length, 0); return;
  }
  assert.equal(f.contractFn().toXDR('base64'), op.body().invokeHostFunctionOp().hostFunction().invokeContract().toXDR('base64'), 'authorized root differs from the operation');
  const subs = root.subInvocations();
  if (policy === 'root') { assert.equal(subs.length, 0, 'this call must not authorize nested calls'); return; }
  if (policy === 'root+transfer') {
    assert.equal(subs.length, 1, 'exactly one nested transfer expected');
    const cf = subs[0].function().contractFn(), args = cf.args().map(x => S.scValToNative(x));
    assert(S.Address.fromScAddress(cf.contractAddress()).toString() === opts.token && cf.functionName().toString() === 'transfer'
      && args[0] === source && args[1] === opts.to && u(args[2]) <= u(opts.max) && subs[0].subInvocations().length === 0,
    `nested authorization is not transfer(${source} -> ${opts.to}, <= ${opts.max}) on ${opts.token}`);
    return;
  }
  throw Error('unknown auth policy ' + policy);
}
function classLimits(cfg, cls) { const l = cfg.limits.classes[cls]; assert(l, 'unknown resource class ' + cls); return l; }
function checkResources(data, lim, cls) {
  const r = data.resources(), fp = r.footprint(), keys = [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64'));
  assert(Number(r.instructions()) <= lim.cpu, `${cls}: CPU ${r.instructions()} above ${lim.cpu}`);
  assert(keys.length <= lim.keys && new Set(keys).size === keys.length, `${cls}: ${keys.length} footprint keys above ${lim.keys} (or duplicates)`);
  assert(fp.readWrite().length <= lim.writes, `${cls}: ${fp.readWrite().length} writes above ${lim.writes}`);
  assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'archived entries: restore first (outside this tool)');
  return {cpu: Number(r.instructions()), keys: keys.length, writes: fp.readWrite().length, diskReadBytes: r.diskReadBytes(), writeBytes: r.writeBytes(),
    resourceFeeStroops: String(data.resourceFee().toString())};
}

// Canary amounts (contract math: usd = amount * price / scale). Long: XLM collateral, USDC debt; short: the reverse.
function canaryAmounts(long, equityUsd7, lev, px, pu, haircutBps) {
  const eq = u(equityUsd7), debtUsd = eq * (u(lev) - E6) / E6 * BigInt(10000 - haircutBps) / 10000n;
  const [hp, hs] = long ? px : pu, [dp, ds] = long ? pu : px;
  return {collateral: eq * u(hs) / u(hp), borrow: debtUsd * u(ds) / u(dp), debtUsd};
}
// SPEC §4: B >= coef_long * long equity + coef_short * short equity + coef_largest * largest position (usd7, rounded up).
function requiredBackstop(h, longEq, shortEq, largest) {
  const up = (a, b) => (u(a) * u(b) + E6 - 1n) / E6;
  return up(h.coef_long, longEq) + up(h.coef_short, shortEq) + up(h.coef_largest, largest);
}
// Pilot maximum debt per side, from the caps: debt <= OI - equity and <= equity * (L - 1), equity <= side cap.
function pilotMaxDebtUsd(caps, lev) {
  // debt = notional - equity, notional <= OI and equity >= notional / L  =>  debt <= OI * (L - 1) / L
  const byLev = u(caps.side_equity_usd) * (u(lev) - E6) / E6, byOi = u(caps.side_oi_usd) * (u(lev) - E6) / u(lev);
  return byLev < byOi ? byLev : byOi;
}
// Cash-market headroom (raw units of the debt asset): min(cap - borrowed, borrowable cash) - pilot max debt.
function cashHeadroom(m, pilotDebtRaw) {
  const capLeft = u(m.borrowCap) - u(m.totalBorrowed), avail = capLeft < u(m.borrowableCash) ? capLeft : u(m.borrowableCash);
  return {capLeftRaw: String(capLeft), borrowableCashRaw: String(m.borrowableCash), effectiveRaw: String(avail), pilotMaxDebtRaw: String(pilotDebtRaw),
    headroomAfterPilotRaw: String(avail - u(pilotDebtRaw)), fits: avail >= u(pilotDebtRaw)};
}

// Operator-written keeper gate file (agents never write it). Pure: returns the list of reasons it fails (empty = ok).
function keeperGateReasons(cfg, g, mid, nowMs) {
  const K = cfg.keeperGate, R = [], t = s => Date.parse(s), h = 3600e3;
  if (!g || typeof g !== 'object') return ['gate file missing or not JSON'];
  if (/PLACEHOLDER|TODO|FILL/i.test(JSON.stringify(g))) R.push('gate file still contains placeholders');
  if (g.gate !== 'margin-keeper-live') R.push('gate != margin-keeper-live');
  if (g.marginController !== mid) R.push(`marginController ${g.marginController} != ${mid}`);
  const k = g.keeper || {};
  if (k.host !== 'droplet' || !k.hostName || /mac|local|laptop|joshua/i.test(k.hostName)) R.push('keeper must run on the Droplet (keeper.host = droplet, real hostName), not the Mac');
  if (!/^[0-9a-f]{40}$/.test(k.commit || '')) R.push('keeper.commit must be a 40-hex git commit');
  if (k.mode !== 'live' || k.dryRun !== false) R.push('keeper must be LIVE (mode live, dryRun false)');
  if (!(t(k.liveSince) <= nowMs)) R.push('keeper.liveSince missing or in the future');
  const d = g.dryStage || {};
  if (!(t(d.to) - t(d.from) >= K.minDryStageHours * h)) R.push(`dry stage shorter than ${K.minDryStageHours} h`);
  if (d.errors !== 0) R.push('dry stage must have 0 errors');
  if (!(t(d.to) <= t(k.liveSince))) R.push('dry stage must end before the live stage starts');
  const a = g.alerts || {};
  const need = K.requiredAlerts || ['liquidatableOver10min', 'impaired', 'keeperDown'];
  const miss = need.filter(n => a[n] !== true);
  if (miss.length) R.push(`alerts not tested: ${miss.join(', ')}`);
  if (K.requiredAlerts && a.channel !== 'telegram') R.push('alerts.channel must be telegram (delivered to the owner bot)');
  if (!(t(a.testedAt) <= nowMs && nowMs - t(a.testedAt) <= K.maxAlertTestAgeDays * 24 * h)) R.push(`alert test older than ${K.maxAlertTestAgeDays} d or missing`);
  if (K.deadman) {
    const dm = g.deadman || {};
    if (dm.service !== K.deadman.service) R.push(`deadman.service must be ${K.deadman.service} (external, not on the keeper Droplet)`);
    if (dm.pauseHostWatcher !== true) R.push('deadman.pauseHostWatcher must be true (second watcher on the pause host)');
    if (dm.alertsReceived !== true) R.push('deadman.alertsReceived must be true (keeper stop test delivered both alerts to Telegram)');
    if (!(t(dm.stopTestAt) <= nowMs && nowMs - t(dm.stopTestAt) <= K.deadman.maxStopTestAgeDays * 24 * h)) R.push(`deadman stop test older than ${K.deadman.maxStopTestAgeDays} d or missing`);
  }
  const x = g.exclusion || {};
  if (x.cashLiquidatorExcludesMargin !== true || !/^[0-9a-f]{7,40}$/.test(x.commit || '')) R.push('cash liquidator exclusion of the margin account not confirmed (exclusion.cashLiquidatorExcludesMargin + commit)');
  if (!g.operator || /claude|agent|bot|\bai\b|assistant|gpt/i.test(g.operator)) R.push('operator must be a named person (agents never write this file)');
  if (g.attestation !== K.attestation) R.push('attestation text differs from keeperGate.attestation');
  if (!(t(g.writtenAt) <= nowMs && nowMs - t(g.writtenAt) <= K.maxFileAgeHours * h)) R.push(`gate file written more than ${K.maxFileAgeHours} h ago (or in the future)`);
  if (!(t(g.writtenAt) >= t(k.liveSince))) R.push('gate file must be written after the keeper went live');
  return R;
}

// ---------------------------------------------------------------- step model
function steps(cfg) {
  const M = marginId(cfg), D = cfg.deps, A = cfg.admin, P = cfg.params, out = [];
  let n = 0, c = 0, fb = 0;
  const add = s => { s.n = s.kind === 'gate' ? null : s.fallback ? `FB${++fb}` : s.canary ? `C${++c}` : `M${++n}`; out.push(s); return s; };
  const inst = k => async x => { try { return (await x.instance(M))[k]; } catch { return undefined; } };
  const view = (m, a = []) => async x => { const v = await x.tryView(M, m, a); return v && typeof v === 'object' && v.error ? undefined : v; };
  const tx = (id, method, args, done, why, extra = {}) => add({id, kind: 'tx', signer: 'admin', target: M, method, args, done, why, auth: 'root', cls: 'setter', ...extra});
  add({id: 'gate.preflight', kind: 'gate', gate: 'preflight', why: 'live code = pins, both receipt hooks unset, no deposit pause, admin balances'});
  add({id: 'upload', kind: 'upload', signer: 'admin', why: `upload margin v2 WASM ${cfg.artifact.sha256.slice(0, 8)} (${cfg.artifact.bytes} B)`, auth: 'none'});
  add({id: 'deploy', kind: 'deploy', signer: 'admin', contract: M, why: `create ${M} with __constructor(admin, controller, guardian, USDC, XLM); opens start PAUSED`, auth: 'create'});
  for (const [a, r, nm] of [[D.xlm, D.xlmReceipt, 'XLM'], [D.usdc, D.usdcReceipt, 'USDC']])
    tx(`set_market.${nm}`, 'set_market', () => [addr(A), addr(a), addr(r)], async x => (await inst(`Market:${a}`)(x)) === r, `${nm} underlying -> receipt ${r.slice(0, 8)} (checks get_underlying_token)`);
  tx('enter_markets', 'enter_markets', () => [addr(A), S.xdr.ScVal.scvVec([addr(D.xlm), addr(D.usdc)])],
    async x => { const m = await x.tryView(D.controller, 'get_user_markets', [addr(M)]); return Array.isArray(m) && m.includes(D.xlmReceipt) && m.includes(D.usdcReceipt); },
    'margin contract enters the XLM and USDC cash markets as its pooled account');
  tx('set_pool', 'set_pool', () => [addr(A), addr(D.pool)], async x => (await inst('Pool')(x)) === D.pool, 'Aquarius XLM/USDC concentrated pool CBBMQ...BUCV (reads get_tokens)');
  tx('set_fees', 'set_fees', () => [addr(A), feesVal(P.fees)], async x => sameNum(await inst('Fees')(x), P.fees), 'fees 0.10 % / 0.10 %, funding +2 % APR, bonus 3 % + 1 % (= constructor default: read-back only)', {default: true});
  tx('set_max_slippage', 'set_max_slippage', () => [addr(A), u128(P.maxSlippage)], async x => String(await inst('MaxSlippage')(x)) === P.maxSlippage, 'max slippage 1.5 % (= constructor default: read-back only)', {default: true});
  for (const [a, nm] of [[D.xlm, 'XLM'], [D.usdc, 'USDC']])
    tx(`set_risk.${nm}`, 'set_risk', () => [addr(A), addr(a), u128(P.maxLeverage[nm])], async x => sameNum(await inst(`Risk:${a}`)(x), {max_leverage: P.maxLeverage[nm]}), `${nm === 'XLM' ? 'long' : 'short'} side max leverage 5x (solvency re-checked on chain)`);
  for (const [a, nm] of [[D.xlm, 'XLM'], [D.usdc, 'USDC']])
    tx(`set_caps.${nm}`, 'set_caps', () => [addr(A), addr(a), capsVal(P.caps[nm])], async x => sameNum(await inst(`Caps:${a}`)(x), P.caps[nm]), `${nm === 'XLM' ? 'long' : 'short'} caps: $25 / position, $100 side equity, $500 side OI, $1 minimum`);
  tx('set_headroom', 'set_headroom', () => [addr(A), headroomVal(P.headroom)], async x => sameNum(await view('get_headroom')(x), P.headroom), 'pool check 1.25 / 1.10 / +-20 %, derived close reserve, B coefficients 3.0 / 2.1 / 4.6 (= constructor default: read-back only)', {default: true});
  tx('set_backstop_params', 'set_backstop_params', () => [addr(A), bsParamsVal(P.backstopParams)], async x => sameNum(await inst('BsParams')(x), P.backstopParams), 'backstop exit cooldown 72 h, window 48 h, min deposit 0.1 pT (= constructor default: read-back only)', {default: true});
  add({id: 'gate.seed-balance', kind: 'gate', gate: 'seed-balance', why: `admin holds >= ${fmt7(P.seedPtokensRaw)} USDC pTokens (else FB1)`});
  add({id: 'mint.usdc-pt', kind: 'mint', signer: 'admin', fallback: true, target: D.usdcReceipt, method: 'deposit', why: 'FALLBACK only if gate.seed-balance fails: admin deposits the missing USDC into the USDC receipt', auth: 'root+transfer', cls: 'setter'});
  tx('backstop_deposit.seed', 'backstop_deposit', () => [addr(A), u128(P.seedPtokensRaw), u128(P.seedPtokensRaw)],
    async x => { const b = await view('get_backstop')(x); return !!b && u(b.ptokens) >= u(P.seedPtokensRaw) && uv(await view('get_backer', [addr(A)])(x)) >= u(P.seedPtokensRaw); },
    `$750 seed: ${fmt7(P.seedPtokensRaw)} USDC pTokens admin -> backstop (1:1 shares + 1e6 dead shares; min_shares = seed)`,
    {auth: 'root+transfer', cls: 'release', transfer: {token: D.usdcReceipt, max: P.seedPtokensRaw}});
  add({id: 'gate.config', kind: 'gate', gate: 'config', why: 'every read-back, hooks unset, opens still paused, backstop value >= required'});
  add({id: 'gate.keeper', kind: 'gate', gate: 'keeper', why: 'OPERATOR gate file: WP7 keeper LIVE on the Droplet after its dry stage, alerts tested, cash-liquidator exclusion live; plus keeper on-chain checks'});
  tx('unpause', 'set_opens_paused', () => [addr(A), bool(false)], async x => (await view('opens_paused')(x)) === false,
    'LAST M-step: opens unpaused. Refused unless gate.keeper and gate.config pass at prepare time', {historical: true, keeperGate: true});
  for (const [long, nm] of [[true, 'long'], [false, 'short']]) {
    add({id: `canary.${nm}.open`, kind: 'canary-open', canary: true, historical: true, signer: 'admin', long, cls: 'release', auth: 'root+transfer',
      why: `$5 ${nm} at 3x: ${long ? 'XLM collateral, USDC debt' : 'USDC collateral, XLM debt'}`});
    add({id: `canary.${nm}.close`, kind: 'canary-close', canary: true, historical: true, signer: 'admin', long, cls: 'release', auth: 'root',
      why: `swap-close the $5 ${nm} (fallback: emergency close-with-repay)`});
  }
  add({id: 'gate.canary', kind: 'gate', gate: 'canary', why: 'no open admin positions, side totals back to 0, Ledger == pToken balance, NAV >= seed'});
  return out;
}

// Emergency / K-steps / rollback. Each is one unsigned transaction; never blocked by earlier steps.
const EMERGENCY = {
  'pause-opens': {signer: a => a === 'guardian' ? 'guardian' : 'admin', build: (cfg, M, who) => ['set_opens_paused', [addr(who === 'guardian' ? need(cfg.signers.guardian.publicKey, 'guardian') : cfg.admin), bool(true)]], cls: 'setter', note: 'K1: stops NEW positions only; close / close_with_repay / liquidate / claim / settle never pause. Guardian (host key) or admin (Ledger)'},
  'resume-opens': {keeperGate: true, build: (cfg) => ['set_opens_paused', [addr(cfg.admin), bool(false)]], cls: 'setter', note: 'K7: admin only; refused unless the keeper gate passes, not Impaired, and no backstop bad debt'},
  'reconcile': {build: (cfg) => ['reconcile', [addr(cfg.admin)]], cls: 'liquidation', note: 'K2: permissionless; sets Impaired on a pToken deficit (returns true only when newly set)'},
  'settle': {build: (cfg, M, asset, maxPt) => ['settle_backstop_debt', [addr(cfg.admin), addr(asset === 'xlm' ? cfg.deps.xlm : cfg.deps.usdc), u128(maxPt), u128(0)]], cls: 'liquidation', sized: true, note: 'K3: self-sized settle of backstop bad debt (max = settle_max_ptokens); repeat until get_bs_debt_shares = 0'},
  'recapitalize': {build: (cfg, M, raw) => ['admin_recapitalize', [addr(cfg.admin), u128(raw)]], cls: 'release', transfer: (cfg, M, raw) => ({token: cfg.deps.usdcReceipt, max: raw}), note: 'K4: USDC pTokens admin -> backstop, no shares minted; works while Impaired'},
  'resolve': {build: (cfg, M, usdcCharge, xlmRestore) => ['admin_resolve_impairment', [addr(cfg.admin), u128(usdcCharge), u128(xlmRestore || 0)]], cls: 'liquidation', transfer: (cfg, M, c, x) => u(x || 0) > 0n ? {token: cfg.deps.xlmReceipt, max: x} : null, note: 'K5: book the USDC deficit against the backstop and/or restore XLM pTokens from the admin; clears Impaired only if whole'},
  'set-guardian': {build: (cfg, M, g) => { assert(S.StrKey.isValidEd25519PublicKey(g || '') && g !== cfg.admin); return ['set_guardian', [addr(cfg.admin), addr(g)]]; }, cls: 'setter', note: 'rotate the pause-only guardian'},
  'set-caps': {build: (cfg, M, side, per, sideEq, oi, min) => ['set_caps', [addr(cfg.admin), addr(side === 'xlm' ? cfg.deps.xlm : cfg.deps.usdc), capsVal({per_position_equity_usd: per, side_equity_usd: sideEq, side_oi_usd: oi, min_equity_usd: min})]], cls: 'setter', note: 'lower (or later raise) one side\'s caps; usd7 values'},
  'close-with-repay': {build: (cfg, M, id) => ['close_with_repay', [addr(cfg.admin), u64v(id)]], cls: 'release', repay: true, note: 'canary fallback: owner repays the debt from the wallet (no swap)'},
  'claim': {build: (cfg, M, asset) => ['claim', [addr(cfg.admin), addr(asset === 'xlm' ? cfg.deps.xlm : cfg.deps.usdc)]], cls: 'liquidation', note: 'owner claim after a liquidation'},
  'bs-request': {build: (cfg, M, shares) => ['backstop_request_withdraw', [addr(cfg.admin), u128(shares)]], cls: 'setter', note: 'rollback R3: start the 72 h exit of the seed'},
  'bs-cancel': {build: (cfg) => ['backstop_cancel_withdraw', [addr(cfg.admin)]], cls: 'setter', note: 'cancel the admin exit request'},
  'bs-withdraw': {build: (cfg, M, minPt) => ['backstop_withdraw', [addr(cfg.admin), u128(minPt || 0)]], cls: 'release', note: 'rollback R4: withdraw at NAV after the cooldown (pool-checked)'},
  'propose-upgrade': {build: (cfg, M, hex) => { assert(/^[0-9a-f]{64}$/.test(hex || '')); return ['propose_upgrade', [addr(cfg.admin), S.xdr.ScVal.scvBytes(Buffer.from(hex, 'hex'))]]; }, cls: 'setter', note: '24 h timelock; a new proposal replaces the pending one'},
  'execute-upgrade': {build: (cfg, M, hex) => { assert(/^[0-9a-f]{64}$/.test(hex || '')); return ['upgrade', [addr(cfg.admin), S.xdr.ScVal.scvBytes(Buffer.from(hex, 'hex'))]]; }, cls: 'setter', note: 'after the ETA; the hash must equal the proposal'},
  'bump-ttl': {build: (cfg, M, ...ids) => ['bump_ttl', [S.xdr.ScVal.scvVec(ids.map(u64v))]], cls: 'setter', auth: 'none', note: 'permissionless instance + position TTL extension'},
};

// ---------------------------------------------------------------- chain-facing
async function codeExists(c, hex) {
  const k = S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(hex, 'hex')}));
  const r = await c.rpc.getLedgerEntries(k); return !!(r.entries && r.entries.length);
}
async function contractWasm(c, id) { try { return await c.codeHash(id); } catch { return null; } }
async function simulateBuild(cfg, c, op, source, {cls, policy, authOpts, cpuMax, feeCapStroops, hostSigned}) {
  const acc = await c.rpc.getAccount(source), latest = await c.rpc.getLatestLedger(), L0 = cfg.limits;
  // Host-signed (guardian) transactions travel Mac -> pause host -> Mac: a longer validity window.
  const L = hostSigned ? {...L0, window: L0.hostWindow, timeout: L0.hostTimeout} : L0;
  const tx = new S.TransactionBuilder(acc, {fee: String(L.classicFeeStroops), networkPassphrase: cfg.network}).addOperation(op)
    .setLedgerbounds(latest.sequence + 1, latest.sequence + L.window).setTimeout(L.timeout).build();
  const sim = await c.rpc.simulateTransaction(tx);
  assert(S.rpc.Api.isSimulationSuccess(sim), 'simulation failed: ' + String(sim.error || '').split('\n')[0].slice(0, 300));
  assert(!sim.restorePreamble, 'restoration needed: outside this tool');
  checkAuth(cfg, (sim.result && sim.result.auth) || [], tx.toEnvelope().v1().tx().operations()[0], source, policy, authOpts);
  const minFee = BigInt(sim.minResourceFee);
  const prepared = S.rpc.assembleTransaction(tx, padResourceFee(sim)).build();
  const data = prepared.toEnvelope().v1().tx().ext().sorobanData();
  const lim = cpuMax ? {...classLimits(cfg, 'setter'), cpu: cpuMax, keys: 100} : classLimits(cfg, cls);
  const metrics = {cls: cpuMax ? 'upload' : cls, ...checkResources(data, lim, cls || 'upload'), minResourceFeeXlm: Number(minFee) / 1e7};
  assert(u(prepared.fee) <= feeCapStroops, `fee ${prepared.fee} above the cap ${feeCapStroops}`);
  assert.equal(prepared.signatures.length, 0); assert.equal(prepared.source, source);
  const retval = sim.result && sim.result.retval ? S.scValToNative(sim.result.retval) : undefined;
  return {prepared, metrics, retval, events: (sim.events || []).length, simulatedLedger: sim.latestLedger, validLedgers: {min: latest.sequence + 1, maxExclusive: latest.sequence + L.window}};
}
async function prices(cfg, c) {
  const p = async t => (await c.view(cfg.deps.controller, 'get_price_usd', [addr(t)])).value.map(x => u(x));
  return {px: await p(cfg.deps.xlm), pu: await p(cfg.deps.usdc)};
}
async function adminPositions(cfg, c, held) {
  const M = marginId(cfg), n = Number((await c.view(M, 'get_position_counter')).value), out = [];
  for (let id = n; id >= 1 && id > n - 50; id--) {
    const p = await c.tryView(M, 'get_position', [u64v(id)]);
    if (p && !p.error && p.owner === cfg.admin && (!held || p.held_asset === held)) out.push({id, ...p});
  }
  return out;
}

async function opFor(cfg, c, s) {
  const M = marginId(cfg), D = cfg.deps, L = cfg.limits, src = cfg.signers[s.signer].publicKey;
  const inv = (target, method, args, extra = {}) => {
    assertCallAllowed(cfg, target, method, s);
    const op = new S.Contract(target).call(method, ...args); assertNoRetiredArgs(cfg, op);
    return {op, cls: s.cls || 'setter', feeCapStroops: BigInt(L.invokeMaxFeeStroops), policy: s.auth, ...extra};
  };
  if (s.kind === 'upload') {
    assert(cfg.artifact.frozen === true, 'artifact not frozen: upload refused');
    const wasm = fs.readFileSync(path.resolve(__dirname, cfg.artifact.path)), h = checkArtifact(cfg, wasm);
    assert(!(await codeExists(c, h)), `code ${h} already on chain: skip`);
    return {op: S.Operation.uploadContractWasm({wasm}), cpuMax: L.uploadCpuMax, feeCapStroops: BigInt(Math.round(L.uploadMaxFeeXlm * 1e7)), policy: 'none', summary: {sha256: h, bytes: wasm.length}};
  }
  if (s.kind === 'deploy') {
    const h = cfg.artifact.sha256, args = deployArgs(cfg);
    assert(await codeExists(c, h), `code ${h} not uploaded yet (M1 first)`);
    assert.equal(await contractWasm(c, M), null, `${M} already exists`);
    const op = S.Operation.createCustomContract({address: S.Address.fromString(src), wasmHash: Buffer.from(h, 'hex'), salt: sha(Buffer.from(cfg.salt, 'utf8')), constructorArgs: args});
    return {op, cls: 'setter', feeCapStroops: BigInt(L.invokeMaxFeeStroops), policy: 'create', authOpts: {constructorArgs: args}, summary: {contract: M, wasm: h, guardian: cfg.signers.guardian.publicKey}};
  }
  if (s.kind === 'tx') {
    const o = inv(s.target, s.method, s.args());
    if (s.auth === 'root+transfer') o.authOpts = {token: s.transfer.token, to: M, max: s.transfer.max};
    return o;
  }
  if (s.kind === 'mint') {
    const bal = u((await c.view(D.usdcReceipt, 'balance', [addr(src)])).value), seed = u(cfg.params.seedPtokensRaw);
    assert(bal < seed, `admin already holds ${fmt7(bal)} USDC pT >= seed: FB1 not needed`);
    const rate = u((await c.view(D.usdcReceipt, 'get_exchange_rate')).value), amount = ((seed - bal) * rate + E6 - 1n) / E6 + 10n;
    const o = inv(D.usdcReceipt, 'deposit', [addr(src), u128(amount)]);
    o.authOpts = {token: D.usdc, to: D.usdcReceipt, max: String(amount)}; o.summary = {depositUsdcRaw: String(amount)}; return o;
  }
  if (s.kind === 'canary-open') {
    const {px, pu} = await prices(cfg, c), C = cfg.canary;
    const a = canaryAmounts(s.long, C.equityUsd7, C.leverage, px, pu, C.debtHaircutBps);
    const [held, debt] = s.long ? [D.xlm, D.usdc] : [D.usdc, D.xlm];
    const already = (await adminPositions(cfg, c, held)).filter(p => p.open);
    assert.equal(already.length, 0, `refused: an admin ${s.long ? 'long' : 'short'} is already open (id ${already.map(p => p.id)}): never open a second canary; close it (next C-step)`);
    const wallet = u((await c.view(held, 'balance', [addr(src)])).value);
    assert(a.collateral < wallet, `wallet ${fmt7(wallet)} below the canary collateral ${fmt7(a.collateral)}`);
    const o = inv(M, 'open', [addr(src), addr(held), addr(debt), u128(a.collateral), u128(a.borrow), u128(1), u128(C.maxFeeUsdcRaw)]);
    o.authOpts = {token: held, to: M, max: String(a.collateral)};
    o.summary = {collateralRaw: String(a.collateral), borrowRaw: String(a.borrow), debtUsd7: String(a.debtUsd), priceXlm: String(px), priceUsdc: String(pu)};
    return o;
  }
  if (s.kind === 'canary-close') {
    const held = s.long ? D.xlm : D.usdc, open = (await adminPositions(cfg, c, held)).filter(p => p.open);
    assert.equal(open.length, 1, `expected exactly one open admin ${s.long ? 'long' : 'short'}, found ${open.length}`);
    const o = inv(M, 'close', [addr(src), u64v(open[0].id), u128(1)]); o.summary = {positionId: open[0].id}; return o;
  }
  throw Error('no builder for ' + s.kind);
}

async function gate(cfg, c, name, opts = {}) {
  const M = marginId(cfg), D = cfg.deps, P = cfg.params;
  if (name === 'preflight') {
    const code = {controller: await contractWasm(c, D.controller), usdcReceipt: await contractWasm(c, D.usdcReceipt), xlmReceipt: await contractWasm(c, D.xlmReceipt)};
    const hooks = {usdc: await c.tryView(D.usdcReceipt, 'get_margin_controller'), xlm: await c.tryView(D.xlmReceipt, 'get_margin_controller')};
    const depPause = {usdc: await c.tryView(D.controller, 'is_deposit_paused', [addr(D.usdcReceipt)]), xlm: await c.tryView(D.controller, 'is_deposit_paused', [addr(D.xlmReceipt)])};
    const acct = await (await fetch(`${cfg.horizon}/accounts/${cfg.admin}`)).json();
    const xlmBal = Number((acct.balances.find(b => b.asset_type === 'native') || {}).balance || 0);
    const usdcPt = await c.tryView(D.usdcReceipt, 'balance', [addr(cfg.admin)]);
    const codeOk = Object.entries(code).every(([k, v]) => v === cfg.liveCode[k]);
    const ok = codeOk && hooks.usdc === null && hooks.xlm === null && depPause.usdc === false && depPause.xlm === false;
    return {gate: name, ok, code, codeOk, hooksUnset: hooks.usdc === null && hooks.xlm === null, depositPaused: depPause, adminXlm: xlmBal, adminUsdcPt: String(usdcPt),
      adminXlmWarning: xlmBal < 120 && !(await codeExists(c, cfg.artifact.sha256)) ? 'below 120 XLM before M1: the upload needs up to ~112 XLM' : null,
      marginExists: (await contractWasm(c, M)) !== null};
  }
  if (name === 'seed-balance') {
    const b = uv(await c.tryView(D.usdcReceipt, 'balance', [addr(cfg.admin)]));
    return {gate: name, ok: b >= u(P.seedPtokensRaw), adminUsdcPt: String(b), seed: P.seedPtokensRaw};
  }
  if (name === 'config') {
    const list = steps(cfg).filter(s => s.kind === 'tx' && !s.keeperGate && s.id !== 'backstop_deposit.seed' || s.kind === 'deploy');
    const rows = {}; for (const s of list) rows[s.id] = await isDone(cfg, c, s);
    const hooks = [await c.tryView(D.usdcReceipt, 'get_margin_controller'), await c.tryView(D.xlmReceipt, 'get_margin_controller')];
    const req = uv(await c.tryView(M, 'required_backstop')), [nav] = (await c.tryView(M, 'backstop_nav')) || [0n];
    const {pu} = await prices(cfg, c), rate = u((await c.view(D.usdcReceipt, 'get_exchange_rate')).value);
    const valueUsd7 = u(nav) * rate / E6 * pu[0] / pu[1];
    const impaired = await c.tryView(M, 'is_impaired'), bsU = await c.tryView(M, 'get_bs_debt_shares', [addr(D.usdc)]), bsX = await c.tryView(M, 'get_bs_debt_shares', [addr(D.xlm)]);
    const fullBook = requiredBackstop(P.headroom, P.caps.XLM.side_equity_usd, P.caps.USDC.side_equity_usd, P.caps.XLM.per_position_equity_usd);
    const ok = Object.values(rows).every(v => v === true) && hooks.every(h => h === null) && impaired === false && uv(bsU) === 0n && uv(bsX) === 0n && typeof bsU === 'bigint'
      && valueUsd7 >= fullBook && (opts.allowUnpaused || (await c.tryView(M, 'opens_paused')) === true);
    return {gate: name, ok, readbacks: rows, hooksUnset: hooks.every(h => h === null), impaired, bsDebtShares: {usdc: String(bsU), xlm: String(bsX)},
      requiredNowUsd7: String(req), requiredFullPilotBookUsd7: String(fullBook), backstopValueUsd7: String(valueUsd7)};
  }
  if (name === 'keeper') {
    let g = null, fileErr = null;
    try { g = JSON.parse(fs.readFileSync(cfg.keeperGate.gateFile, 'utf8')); } catch (e) { fileErr = e.code || e.message; }
    const reasons = fileErr ? [`gate file ${cfg.keeperGate.gateFile}: ${fileErr} (the OPERATOR writes it; see gate-template)`] : keeperGateReasons(cfg, g, M, Date.now());
    const K = cfg.keeperGate, kt = await (await fetch(`${cfg.horizon}/accounts/${K.keeperAccount}/transactions?order=desc&limit=1`)).json();
    const last = kt._embedded && kt._embedded.records[0] ? Date.parse(kt._embedded.records[0].created_at) : 0, ageMin = (Date.now() - last) / 60e3;
    if (!(ageMin <= K.maxKeeperTxAgeMinutes)) reasons.push(`keeper account's last transaction is ${ageMin.toFixed(1)} min old (> ${K.maxKeeperTxAgeMinutes})`);
    const {px, pu} = await prices(cfg, c), bal = async t => uv(await c.tryView(t, 'balance', [addr(K.keeperAccount)]));
    const maxLong = u(P.caps.XLM.per_position_equity_usd) * (u(P.maxLeverage.XLM) - E6) / E6, maxShort = u(P.caps.USDC.per_position_equity_usd) * (u(P.maxLeverage.USDC) - E6) / E6;
    const needUsdc = maxLong * pu[1] / pu[0], needXlm = maxShort * px[1] / px[0] + u(K.minKeeperXlmRaw);
    const inv = {usdcRaw: String(await bal(D.usdc)), needUsdcRaw: String(needUsdc), xlmRaw: String(await bal(D.xlm)), needXlmRaw: String(needXlm)};
    if (u(inv.usdcRaw) < needUsdc) reasons.push('keeper USDC below the largest long debt');
    if (u(inv.xlmRaw) < needXlm) reasons.push('keeper XLM below the largest short debt + fee floor');
    return {gate: name, ok: reasons.length === 0, reasons, keeperLastTxMinutes: Number(ageMin.toFixed(1)), inventory: inv, gateFile: K.gateFile};
  }
  if (name === 'canary') {
    const pos = await adminPositions(cfg, c), open = pos.filter(p => p.open), sides = {};
    for (const [a, nm] of [[D.xlm, 'long'], [D.usdc, 'short']]) sides[nm] = await c.tryView(M, 'get_side', [addr(a)]);
    const led = {usdc: await c.tryView(D.usdcReceipt, 'get_ptoken_balance', [addr(M)]), xlm: await c.tryView(D.xlmReceipt, 'get_ptoken_balance', [addr(M)])};
    const inst = await c.instance(M), navv = await c.tryView(M, 'backstop_nav'), nav = Array.isArray(navv) ? navv[0] : 0n;
    const ledgerEq = String(inst[`Ledger:${D.usdc}`] || 0) === String(led.usdc) && String(inst[`Ledger:${D.xlm}`] || 0) === String(led.xlm);
    const ok = pos.length >= 2 && open.length === 0 && Object.values(sides).every(s => s && u(s.open_positions) === 0n) && ledgerEq && u(nav) >= u(P.seedPtokensRaw);
    return {gate: name, ok, canaryPositions: pos.map(p => ({id: p.id, open: p.open})), sides, ledgerEq, navPt: String(nav)};
  }
  throw Error('unknown gate ' + name);
}
async function isDone(cfg, c, s) {
  const M = marginId(cfg);
  if (s.kind === 'gate') { try { return (await gate(cfg, c, s.gate, {allowUnpaused: true})).ok; } catch { return false; } }
  if (s.kind === 'upload') return codeExists(c, cfg.artifact.sha256);
  if (s.kind === 'deploy') {
    if ((await contractWasm(c, M)) !== cfg.artifact.sha256) return false;
    const i = await c.instance(M), D = cfg.deps;
    return i.Admin === cfg.admin && i.Guardian === cfg.signers.guardian.publicKey && i.Ctrl === D.controller && i.Usdc === D.usdc && i.Xlm === D.xlm;
  }
  if (s.kind === 'tx') { try { return !!(await s.done(c)); } catch { return false; } }
  if (s.kind === 'mint') return (await gate(cfg, c, 'seed-balance')).ok;
  if (s.kind === 'canary-open' || s.kind === 'canary-close') {
    try { const p = await adminPositions(cfg, c, s.long ? cfg.deps.xlm : cfg.deps.usdc); return s.kind === 'canary-open' ? p.length > 0 : p.length > 0 && p.every(x => !x.open); }
    catch { return false; }
  }
  return null;
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
  const signer = cfg.signers[s.signer], src = need(signer && signer.publicKey, `signers.${s.signer}.publicKey`);
  const o = await opFor(cfg, c, s);
  const res = await simulateBuild(cfg, c, o.op, src, {cls: o.cls, policy: o.policy, authOpts: o.authOpts, cpuMax: o.cpuMax, feeCapStroops: o.feeCapStroops, hostSigned: signer.hdPath == null});
  if (s.kind === 'deploy') assert.equal(String(res.retval), s.contract, `host would create ${res.retval}, not the pinned ${s.contract}`);
  return {res, summary: {step: s.id, n: s.n, signer: s.signer, hdPath: signer.hdPath, call: s.method || s.kind, target: s.target || marginId(cfg), why: s.why, ...(o.summary || {})}};
}
async function requireKeeperGate(cfg, c) {
  const k = await gate(cfg, c, 'keeper');
  assert(k.ok, 'refused: keeper gate not passed (operator file + on-chain): ' + k.reasons.join('; '));
  const g = await gate(cfg, c, 'config', {allowUnpaused: true});
  assert(g.ok, 'refused: config gate not passed: ' + js(g).slice(0, 400));
}

// Fee estimate for fork-measured rows (non-refundable part only; rent and events are set by the live sim at prepare).
async function feeConfig(c) {
  const ids = ['configSettingContractComputeV0', 'configSettingContractLedgerCostV0', 'configSettingContractLedgerCostExtV0', 'configSettingContractHistoricalDataV0', 'configSettingContractBandwidthV0', 'configSettingContractEventsV0'];
  const r = await c.rpc.getLedgerEntries(...ids.map(i => S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[i]()}))));
  const o = {};
  for (const e of r.entries) { const v = e.val.configSetting(); const nm = v.switch().name;
    if (nm === 'configSettingContractComputeV0') o.instr = Number(v.contractCompute().feeRatePerInstructionsIncrement());
    if (nm === 'configSettingContractLedgerCostV0') { const l = v.contractLedgerCost(); o.readEntry = Number(l.feeDiskReadLedgerEntry()); o.writeEntry = Number(l.feeWriteLedgerEntry()); o.read1kb = Number(l.feeDiskRead1Kb()); }
    if (nm === 'configSettingContractLedgerCostExtV0') o.write1kb = Number(v.contractLedgerCostExt().feeWrite1Kb());
    if (nm === 'configSettingContractHistoricalDataV0') o.hist1kb = Number(v.contractHistoricalData().feeHistorical1Kb());
    if (nm === 'configSettingContractBandwidthV0') o.tx1kb = Number(v.contractBandwidth().feeTxSize1Kb());
    if (nm === 'configSettingContractEventsV0') o.ev1kb = Number(v.contractEvents().feeContractEvents1Kb());
  }
  return o;
}
function feeEstimateStroops(f, row, txBytes = 1500) {
  const kb = (x, rate) => Math.ceil(x * rate / 1024);
  return Math.ceil(row.cpu * f.instr / 10000) + (row.diskReadEntries || 0) * f.readEntry + row.writeEntries * f.writeEntry + kb(row.diskReadBytes || 0, f.read1kb)
    + kb(row.writeBytes, f.write1kb) + kb(txBytes + 300, f.hist1kb) + kb(txBytes, f.tx1kb) + kb(row.eventsBytes || 0, f.ev1kb);
}

// `prepare` preconditions (exported for the offline state-walk test): preflight pins, every earlier step read back
// done (gates and history excepted as marked), the step itself not yet done, and the keeper gate for M16 / the canary.
async function checkPreconditions(cfg, c, list, s, {retryMs = 3000} = {}) {
  const pre = await gate(cfg, c, 'preflight'); assert(pre.codeOk && pre.hooksUnset, 'refused: preflight (live code pins / receipt hooks): ' + js(pre).slice(0, 300));
  for (const prev of list.slice(0, list.indexOf(s))) {
    if (prev.historical || prev.fallback || (prev.kind === 'gate' && ['keeper', 'preflight'].includes(prev.gate))) continue;
    let d = await isDone(cfg, c, prev);
    for (let i = 0; d === false && i < 2 && retryMs; i++) { await new Promise(r => setTimeout(r, retryMs)); d = await isDone(cfg, c, prev); }
    if (prev.kind === 'gate' && prev.gate === 'seed-balance' && !d) d = await isDone(cfg, c, list.find(x => x.id === 'backstop_deposit.seed'));
    assert(d !== false, `earlier step ${prev.n || ''} ${prev.id} not read back as done`);
  }
  if (['deploy', 'tx', 'upload'].includes(s.kind)) assert.notEqual(await isDone(cfg, c, s), true, `${s.id} already done${s.default ? ' (constructor default: read-back only, no transaction)' : ''}`);
  if (s.keeperGate || s.canary) await requireKeeperGate(cfg, c);
  return true;
}
async function main(argv, deps = {}) {
  const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; };
  const cfg = JSON.parse(fs.readFileSync(CFG_FILE, 'utf8'));
  assert.equal(cfg.network, S.Networks.PUBLIC);
  const cmd = argv[0], M = marginId(cfg);
  if (cmd === 'addresses') return {margin: M, admin: cfg.admin, salt: cfg.salt};
  if (cmd === 'exclusion-fragment') return {note: 'WP7: the cash liquidator must never liquidate this pooled account (alert only). neverTouch is a list of RECEIPTS and does not do this.', excludeBorrowers: [M]};
  if (cmd === 'gate-template') return {gate: 'margin-keeper-live', marginController: M, keeper: {host: 'droplet', hostName: 'FILL', commit: 'FILL', mode: 'live', dryRun: false, liveSince: 'FILL ISO'},
    dryStage: {from: 'FILL ISO', to: 'FILL ISO', errors: 0}, deadman: {service: 'healthchecks', pauseHostWatcher: true, stopTestAt: 'FILL ISO', alertsReceived: true}, alerts: {testedAt: 'FILL ISO', channel: 'telegram', liquidatableOver10min: true, pooledHealthLow: true, impaired: true, inventoryLow: true, opensRefusedBadDebt: true, keeperDown: true, conversionFailed: true, dustLiquidation: true},
    exclusion: {cashLiquidatorExcludesMargin: true, commit: 'FILL'}, operator: 'FILL', attestation: cfg.keeperGate.attestation, writtenAt: 'FILL ISO',
    _: `The OPERATOR writes this to ${cfg.keeperGate.gateFile} by hand after checking the Droplet. Agents never write it.`};
  if (cmd === 'verify-signed') {
    const a = S.TransactionBuilder.fromXDR(fs.readFileSync(argv[1], 'utf8').trim(), cfg.network), b = S.TransactionBuilder.fromXDR(fs.readFileSync(argv[2], 'utf8').trim(), cfg.network);
    assert.equal(a.hash().toString('hex'), b.hash().toString('hex'), 'signed transaction differs from the prepared one');
    assert.equal(b.signatures.length, 1, 'exactly one signature');
    assert(S.Keypair.fromPublicKey(b.source).verify(b.hash(), b.signatures[0].signature()), 'signature does not verify for the source (wrong Ledger account / HD path?)');
    return {ok: true, hash: b.hash().toString('hex'), source: b.source};
  }
  const c = deps.chain || chain(cfg), list = steps(cfg);
  const find = id => { const s = list.find(x => x.id === id); assert(s, `unknown step ${id}`); return s; };
  if (cmd === 'headroom') {
    const {px, pu} = await prices(cfg, c), P = cfg.params, out = {ledger: (await c.rpc.getLatestLedger()).sequence, priceXlm: String(px), priceUsdc: String(pu)};
    for (const [nm, R, lev, capsSide, p] of [['USDC', cfg.deps.usdcReceipt, P.maxLeverage.XLM, P.caps.XLM, pu], ['XLM', cfg.deps.xlmReceipt, P.maxLeverage.USDC, P.caps.USDC, px]]) {
      const m = {borrowCap: (await c.raw(R, key('BorrowCap'))).value, totalBorrowed: (await c.view(R, 'get_total_borrowed')).value, borrowableCash: (await c.view(R, 'get_borrowable_cash')).value,
        supply: (await c.view(R, 'get_total_underlying')).value, reserveBps: (await c.view(R, 'get_min_cash_reserve_bps')).value};
      const debtUsd = pilotMaxDebtUsd(capsSide, lev), debtRaw = debtUsd * p[1] / p[0];
      out[nm] = {...m, ...cashHeadroom(m, debtRaw), pilotMaxDebtUsd7: String(debtUsd)};
    }
    out.XLM.capBindsBelowXlmUsd = Number(pilotMaxDebtUsd(P.caps.USDC, P.maxLeverage.USDC)) / Number(u(out.XLM.capLeftRaw));
    return out;
  }
  if (cmd === 'list' || cmd === 'simulate-all') {
    const marginLive = await contractWasm(c, M), fork = opt('--fork') ? JSON.parse(fs.readFileSync(opt('--fork'), 'utf8')) : null, fc = fork ? await feeConfig(c) : null, rows = [];
    for (const s of list) {
      const row = {n: s.n, id: s.id, kind: s.kind, signer: s.signer || null, why: s.why, done: await isDone(cfg, c, s)};
      const needsMargin = !['upload', 'deploy', 'gate', 'mint'].includes(s.kind);
      if (cmd === 'simulate-all' && needsMargin && marginLive === null) row.liveSkipped = 'margin contract not deployed yet: deploy-dependent, see the [FORK] row';
      else if (cmd === 'simulate-all' && row.done === false && s.kind !== 'gate') {
        try { const r = await buildStep(cfg, c, s); row.live = {...r.res.metrics, maxFeeXlm: Number(r.res.prepared.fee) / 1e7, ledger: r.res.simulatedLedger}; }
        catch (e) { row.liveError = e.message.slice(0, 220); }
      }
      const fr = fork && fork.rows.find(x => x.step === s.id);
      if (fr) row.fork = {...fr, estNonRefundableFeeXlm: feeEstimateStroops(fc, fr) / 1e7};
      rows.push(row);
    }
    if (cmd === 'simulate-all') {
      // Pre-deploy proxies: the seed's nested pToken transfer (admin -> the not-yet-created margin id) and FB1's deposit.
      const proxy = {};
      for (const [k, target, method, args, tok, to, max] of [
        ['seedTransfer', cfg.deps.usdcReceipt, 'transfer', [addr(cfg.admin), addr(M), S.nativeToScVal(u(cfg.params.seedPtokensRaw), {type: 'i128'})], null],
        ['mintDeposit1Usdc', cfg.deps.usdcReceipt, 'deposit', [addr(cfg.admin), u128(10_000_000n)], cfg.deps.usdc, cfg.deps.usdcReceipt, '10000000']]) {
        try { const op = new S.Contract(target).call(method, ...args);
          const r = await simulateBuild(cfg, c, op, cfg.admin, {cls: 'setter', policy: tok ? 'root+transfer' : 'root', authOpts: tok ? {token: tok, to, max} : undefined, feeCapStroops: 50_000_000n});
          proxy[k] = {...r.metrics, maxFeeXlm: Number(r.prepared.fee) / 1e7, ledger: r.simulatedLedger, note: 'PROXY simulation only; never prepared or written'}; }
        catch (e) { proxy[k] = {error: e.message.slice(0, 220)}; }
      }
      const res = {network: cfg.network, ledger: (await c.rpc.getLatestLedger()).sequence, at: new Date().toISOString(), margin: M, artifact: cfg.artifact.sha256, steps: rows, proxy,
        fork: fork ? {test: fork.test, forkLedger: fork.forkLedger, marginWasmSha256: fork.marginWasmSha256, deployedId: fork.deployedId} : null, feeConfig: fc};
      if (opt('--out')) fs.writeFileSync(opt('--out'), json(res) + '\n', {flag: 'wx'});
      return res;
    }
    return {network: cfg.network, ledger: (await c.rpc.getLatestLedger()).sequence, margin: M, steps: rows};
  }
  if (cmd === 'readback') { const s = find(argv[1]); const d = await isDone(cfg, c, s); if (d !== true) process.exitCode = 1; return {step: s.id, n: s.n, done: d}; }
  if (cmd === 'gate') { const r = await gate(cfg, c, argv[1]); if (!r.ok) process.exitCode = 1; return r; }
  if (cmd === 'prepare') {
    const s = find(argv[1]); assert.notEqual(s.kind, 'gate', 'gates are evaluated with `gate`');
    await checkPreconditions(cfg, c, list, s);
    const {res, summary} = await buildStep(cfg, c, s);
    return write(cfg, opt('--out'), res, {...summary, readback: `node margin-ceremony.cjs readback ${s.id}`});
  }
  if (cmd === 'emergency') {
    const name = argv[1], e = EMERGENCY[name]; assert(e, 'emergency: ' + Object.keys(EMERGENCY).join('|'));
    const rest = argv.slice(2).filter((x, i, a) => x !== '--out' && a[i - 1] !== '--out');
    const signer = e.signer ? e.signer(rest[0]) : 'admin';
    if (e.keeperGate) {
      await requireKeeperGate(cfg, c);
      for (const a of [cfg.deps.usdc, cfg.deps.xlm]) assert.equal(uv(await c.tryView(M, 'get_bs_debt_shares', [addr(a)])), 0n, 'refused: backstop bad debt outstanding (K3 first)');
      assert.equal(await c.tryView(M, 'is_impaired'), false, 'refused: Impaired (K4/K5 first)');
    }
    let args = rest;
    if (e.sized) { const mx = u((await c.view(M, 'settle_max_ptokens', [addr(rest[0] === 'xlm' ? cfg.deps.xlm : cfg.deps.usdc)])).value); assert(mx > 0n, 'settle_max_ptokens = 0: nothing to settle, or a pooled shortfall (K4 first)'); args = [rest[0], String(mx)]; }
    const [method, cargs] = e.build(cfg, M, ...args);
    let transfer = e.transfer ? e.transfer(cfg, M, ...args) : null;
    if (e.repay) { const id = rest[0], p = (await c.view(M, 'get_position', [u64v(id)])).value, d = u((await c.view(M, 'get_position_debt', [u64v(id)])).value);
      const long = p.held_asset === cfg.deps.xlm, f = u((await c.view(M, 'get_position_funding', [u64v(id)])).value);
      const amt = long ? d + (u(p.notional_usd) * u(cfg.params.fees.close_fee) + E6 - 1n) / E6 + f : d; transfer = {token: p.debt_asset, max: String(amt + amt / 1000n + 1000n)}; }
    const s = {id: `emergency.${name}`, kind: 'tx', signer, target: M, method, args: () => cargs, auth: e.auth || (transfer ? 'root+transfer' : 'root'), cls: e.cls, transfer: transfer && {token: transfer.token, max: transfer.max}};
    const {res, summary} = await buildStep(cfg, c, s);
    return write(cfg, opt('--out'), res, {...summary, emergency: true, note: e.note});
  }
  if (cmd === 'status') { const r = await c.rpc.getTransaction(argv[1]); return {hash: argv[1], status: r.status, ledger: r.ledger}; }
  throw Error('usage: see header');
}
module.exports = {contractIdFor, marginId, steps, EMERGENCY, MARGIN_METHODS, assertCallAllowed, scanAuthTree, checkAuth, checkArtifact, deployArgs,
  checkPreconditions, requireKeeperGate, gate, isDone, opFor, canaryAmounts, requiredBackstop, pilotMaxDebtUsd, cashHeadroom, keeperGateReasons, feesVal, capsVal, headroomVal, bsParamsVal, structVal, feeEstimateStroops, main};
if (require.main === module) main(process.argv.slice(2)).then(r => console.log(json(r))).catch(e => { console.error('REFUSED/FAILED:', e.message); process.exit(1); });
