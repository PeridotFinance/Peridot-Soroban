'use strict';
// Offline Stellar RPC + vault world for keeper v5 tests. Public identifiers only; no key material anywhere.
//  - virtual clock (discrete events; nothing waits in real time) and a seeded RNG;
//  - ledgers close every `ledgerMs` (with jitter); every RPC call takes a sampled latency, reads the world state of the
//    ledger that is latest at a sampled instant inside the call, and reports that ledger as latestLedger (stellar-rpc
//    simulates AT the latest closed ledger: Revision 4);
//  - the frozen contract rules the keeper depends on: phase preconditions per stage_* step, stage_invest only when
//    guard.inside && count == 3 && O < L <= O+3, observations count up only in the next ledger (vault.rs:142-162);
//  - submitted transactions land in the next ledger if their bounds and preconditions hold there.
const assert = require('node:assert/strict');
const S = require('../../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');

function rng(seed) { let a = seed >>> 0; return () => { a = (a + 0x6D2B79F5) >>> 0; let t = a; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; }
function lognormal(random, median, sigma) { const u = Math.max(1e-12, random()), v = random(); return median * Math.exp(sigma * Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v)); }

function makeClock() {
  let now = 0; const timers = []; let seq = 0;
  const sleep = ms => new Promise(resolve => timers.push({at: now + Math.max(0, ms), seq: seq++, resolve}));
  async function drive(promise, limitMs = 3_600_000) {
    let done = false, value, error;
    promise.then(v => { done = true; value = v; }, e => { done = true; error = e; });
    while (true) {
      for (let i = 0; i < 4; i++) await new Promise(r => setImmediate(r));
      if (done) break;
      assert(timers.length > 0, 'virtual clock deadlock');
      timers.sort((a, b) => a.at - b.at || a.seq - b.seq);
      const t = timers.shift(); now = t.at; assert(now <= limitMs, 'virtual time limit'); t.resolve();
    }
    if (error) throw error;
    return value;
  }
  return {now: () => now, sleep, drive};
}

// Latency profiles (ms). TASK: the figure given in the task (2.7 s median for every call). MEASURED: 17 Nov Alchemy,
// read-only (fast methods 0.13-0.45 s; getLatestLedger 1.9-15.8 s; getLedgers 1.6-21.4 s).
const PROFILES = {
  TASK: {default: [2700, 0.25]},
  FIXED: {default: [200, 0]},
  // LIVE: calibrated to the v5 live read-only runs at ~16:10 CET (in-burst simulations 0.6-0.9 s p50, getEvents 1.5 s in
  // bursts, inspect ~2.1 s, inspect+prepare+fresh ~3.2 s), keeping the earlier slow getLatestLedger/getLedgers.
  LIVE: {default: [600, 0.45], getLedgerEntries: [180, 0.35], getNetwork: [130, 0.2], getEvents: [1500, 0.15], getAccount: [360, 0.2],
    getTransaction: [200, 0.3], sendTransaction: [300, 0.3], getLatestLedger: [6000, 0.5], getLedgers: [6500, 0.6]},
  MEASURED: {default: [220, 0.45], getLatestLedger: [6000, 0.5], getLedgers: [6500, 0.6]},
};

function makeWorld({config, seed = 1, profile = 'MEASURED', ledgerMs = 5500, jitterMs = 600, startLedger = 1000, baseTime = 1794920000,
  commands = [], guard = null, boundaries = [], sealed = [], claim = true, lag = null, hook = null, tweak = null, failMethods = [], memoryLimit = 41943040, missingInterval = false, absentInstances = false} = {}) {
  const clock = makeClock(), random = rng(seed), lat = PROFILES[profile];
  assert(lat, 'unknown latency profile');
  // close instants of ledgers startLedger+1, +2, ... (startLedger itself closed at t=0)
  const closes = []; let t = 0;
  for (let i = 0; i < 4000; i++) { t += ledgerMs + (random() * 2 - 1) * jitterMs; closes.push(t); }
  const ledgerAt = time => { let lo = 0, hi = closes.length; while (lo < hi) { const m = (lo + hi) >> 1; if (closes[m] <= time) lo = m + 1; else hi = m; } return startLedger + lo; };
  const closeTimeOf = L => L <= startLedger ? baseTime - Math.round((startLedger - L) * ledgerMs / 1000) : baseTime + Math.floor(closes[L - startLedger - 1] / 1000);
  const state = {commands: structuredClone(commands), guard: guard && {...guard}, boundaries: [...boundaries], sealed: new Set(sealed), claim};
  const timeline = []; // [{ledger, apply(state)}] applied lazily in ledger order
  const stateAt = L => { const s = {commands: structuredClone(state.commands), guard: state.guard && {...state.guard}, boundaries: [...state.boundaries], sealed: new Set(state.sealed), claim: state.claim};
    for (const m of timeline.filter(x => x.ledger <= L).sort((a, b) => a.ledger - b.ledger || a.seq - b.seq)) m.apply(s); return s; };
  let mutationSeq = 0;
  const mutate = (ledger, apply) => timeline.push({ledger, apply, seq: mutationSeq++});
  let keeperSeq = 7n; const landed = new Map(); // hash -> {ledger, status, envelope, fee}
  const stats = {calls: {}, latencies: []};
  const keeper = config.keeperPublicAddress;
  async function call(method, build, payload = null) {
    const [median, sigma] = lat[method] || lat.default;
    const total = lognormal(random, median, sigma), at = random();
    stats.calls[method] = (stats.calls[method] || 0) + 1; stats.latencies.push([method, total]);
    if (hook) await hook(method, payload);
    await clock.sleep(total * at);
    let L = ledgerAt(clock.now());
    if (lag) L -= lag(method, L) || 0;
    const value = build(L, stateAt(L));
    await clock.sleep(total * (1 - at));
    if (value instanceof Error) throw value;
    return value;
  }
  const addr = a => new S.Address(a).toScVal();
  const u32 = n => S.nativeToScVal(n, {type: 'u32'}), u64 = n => S.nativeToScVal(BigInt(n), {type: 'u64'});
  const map = o => S.xdr.ScVal.scvMap(Object.entries(o).sort(([a], [b]) => a < b ? -1 : 1).map(([k, v]) => new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol(k), val: v})));
  const guardVal = g => g ? map({observed_ledger: u32(g.observed_ledger), healthy_count: u32(g.healthy_count), inside: S.xdr.ScVal.scvBool(g.inside), observed_tick: S.nativeToScVal(g.observed_tick ?? -2, {type: 'i32'})}) : S.xdr.ScVal.scvVoid();
  const jobVal = c => map({kind: u32(c.kind), phase: u32(c.phase), deadline: u64(c.deadline)});
  const TERMINAL = new Set([4, 12, 22, 99]);
  const ok = (L, retval, auth = []) => ({_parsed: true, latestLedger: L, events: [], minResourceFee: '500',
    transactionData: new S.SorobanDataBuilder().setResources(1000000, 100, 0).setResourceFee('500'), result: {auth, retval}});
  const fail = (L, why) => ({_parsed: true, latestLedger: L, error: 'HostError: Error(WasmVm, InvalidAction) ' + why});
  const PHASE = {stage_convert: [0, 1], stage_mint: [0, 2], stage_invest: [0, 3], stage_finalise: [0, 5], stage_withdraw: [1, 10], stage_close_convert: [1, 11], stage_emergency_pay: [2, 21]};
  const NEXT = {stage_convert: 2, stage_mint: 3, stage_invest: 5, stage_finalise: 4, stage_withdraw: 11, stage_close_convert: 12, stage_emergency_pay: 22};
  // The contract's own precondition for a step executed in ledger L (null = would succeed).
  function rule(method, args, s, L) {
    if (method === 'stage_checkpoint_investment') return s.guard && !(L > s.guard.observed_ledger) ? 'one guard observation per ledger' : null;
    if (method === 'checkpoint_rewards') return null;
    const id = String(args[0]), c = s.commands.find(x => String(x.id) === id);
    if (!c) return 'missing job';
    const [kind, phase] = PHASE[method];
    if (c.kind !== kind || c.phase !== phase) return 'phase precondition';
    if (method === 'stage_invest') { const g = s.guard; if (!(g && g.inside && g.healthy_count === 3 && g.observed_ledger < L && L - g.observed_ledger <= 3)) return 'investment guard immature or stale'; }
    return null;
  }
  function apply(method, args, s, L) {
    if (method === 'stage_checkpoint_investment') {
      const p = s.guard; const count = p && p.inside && L === p.observed_ledger + 1 ? Math.min(3, p.healthy_count + 1) : 1;
      s.guard = {observed_ledger: L, healthy_count: count, inside: true, observed_tick: -2}; return;
    }
    if (method === 'checkpoint_rewards') { s.sealed.add(s.boundaries[s.boundaries.length - 1]); return; }
    const c = s.commands.find(x => String(x.id) === String(args[0])); c.phase = NEXT[method];
    if (TERMINAL.has(c.phase)) s.commands = s.commands.filter(x => x !== c);
  }
  function view(L, s, contract, method, args) {
    switch (method) {
      case 'stage_config': return ok(L, map({vault: addr(config.vault), guardian: addr(config.admin)}));
      case 'stage_receipt': return ok(L, addr(config.receipt));
      case 'get_admin': case 'stage_guardian': return ok(L, addr(config.admin));
      case 'stage_investment_keeper': return ok(L, addr(keeper));
      case 'stage_investment_guard': return ok(L, guardVal(s.guard));
      case 'stage_funding_asset': return ok(L, addr(config.fundingAsset));
      case 'stage_commands': { const owner = S.Address.fromScVal(args[0]).toString();
        return ok(L, S.xdr.ScVal.scvVec(s.commands.filter(c => (c.owner ?? config.pilotOwners[0]) === owner && !TERMINAL.has(c.phase)).map(c => map({id: u64(c.id), job: jobVal(c)})))); }
      case 'reward_boundary': { const id = Number(S.scValToNative(args[0]));
        return ok(L, id < s.boundaries.length ? map({holder: addr(config.admin), interval: u64(s.boundaries[id]), waived: S.xdr.ScVal.scvBool(false)}) : S.xdr.ScVal.scvVoid()); }
      case 'reward_interval': { const id = Number(S.scValToNative(args[0])); if (missingInterval) return ok(L, S.xdr.ScVal.scvVoid()); return ok(L, map({sealed: S.xdr.ScVal.scvBool(s.sealed.has(id)), segments: u32(1)})); }
      default: return null;
    }
  }
  const accountValue = () => ({accountId: () => S.xdr.AccountId.publicKeyTypeEd25519(S.StrKey.decodeEd25519PublicKey(keeper)),
    numSubEntries: () => 0, signers: () => [], thresholds: () => Buffer.from([1, 0, 0, 0]), ext: () => ({switch: () => 0}), seqNum: () => keeperSeq, balance: () => 100000000n});
  const entry = key => {
    const kind = key.switch().name;
    if (kind === 'account') return {val: {switch: () => ({name: 'account'}), account: accountValue}};
    if (kind === 'configSetting') { const name = key.configSetting().configSettingId().name;
      return {val: {switch: () => ({name: 'configSetting'}), configSetting: () => ({switch: () => ({name}), toXDR: () => Buffer.from(name),
        contractCompute: () => ({txMemoryLimit: () => memoryLimit}), contractEvents: () => ({txMaxContractEventsSizeBytes: () => 16384})})}}; }
    if (kind === 'contractData') { const contract = key.contractData().contract(), id = S.Address.fromScAddress(contract).toString();
      const hash = id === config.receipt ? config.wasmSha256.receipt : config.wasmSha256.vault;
      return {val: {switch: () => ({name: 'contractData'}), contractData: () => ({contract: () => contract, val: () => ({instance: () => ({executable: () => S.xdr.ContractExecutable.contractExecutableWasm(Buffer.from(hash, 'hex'))})})})}}; }
    throw Error('unexpected key');
  };
  const header = s => ({ledgerSeq: () => s, scpValue: () => ({closeTime: () => BigInt(closeTimeOf(s))}), baseFee: () => 100, baseReserve: () => 5000000});
  const rpc = {
    getNetwork: () => call('getNetwork', () => ({passphrase: config.network, protocolVersion: 29})),
    getHealth: () => call('getHealth', L => ({status: 'healthy', latestLedger: L})),
    getLatestLedger: () => call('getLatestLedger', L => ({sequence: L, closeTime: String(closeTimeOf(L))})),
    getLedgers: ({startLedger}) => call('getLedgers', L => startLedger > L ? Error('start after latest') : ({latestLedger: L, ledgers: [{sequence: startLedger, headerXdr: {header: () => header(startLedger)}}]})),
    getEvents: ({startLedger}) => call('getEvents', L => startLedger > L ? Error('start after latest') : ({latestLedger: L, latestLedgerCloseTime: String(closeTimeOf(L)), events: []})),
    getLedgerEntries: (...keys) => call('getLedgerEntries', L => ({latestLedger: L, entries: keys.filter(k => !(absentInstances && k.switch().name === 'contractData')).map(entry)}), keys),
    getAccount: id => call('getAccount', () => new S.Account(id, '7')),
    simulateTransaction: tx => call('simulateTransaction', (L, s) => {
      assert.equal(tx.signatures.length, 0);
      const invoke = tx.operations[0].func.invokeContract(), method = invoke.functionName().toString(), args = invoke.args();
      const contract = S.Address.fromScAddress(invoke.contractAddress()).toString();
      if (method === 'checkpoint_rewards') { const r = ok(L, S.xdr.ScVal.scvBool(s.claim === true)); return tweak ? tweak(method, r, {invoke, tx}) : r; }
      const v = view(L, s, contract, method, args); if (v) return v;
      const why = rule(method, method === 'stage_checkpoint_investment' ? [] : [S.scValToNative(args[0])], s, L);
      if (why || failMethods.includes(method)) return fail(L, why || 'injected contract failure');
      const root = new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(invoke), subInvocations: []});
      const auth = method === 'stage_checkpoint_investment' ? [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(), rootInvocation: root})] : [];
      const result = ok(L, S.xdr.ScVal.scvVoid(), auth);
      return tweak ? tweak(method, result, {invoke, root, tx}) : result;
    }, tx),
    sendTransaction: signed => call('sendTransaction', L => {
      const hash = signed.hash().toString('hex');
      const invoke = signed.operations[0].func.invokeContract(), method = invoke.functionName().toString();
      const args = method === 'stage_checkpoint_investment' || method === 'checkpoint_rewards' ? [] : [S.scValToNative(invoke.args()[0])];
      const target = L + 1, b = signed.ledgerBounds;
      assert.equal(BigInt(signed.sequence), keeperSeq + 1n, 'stub: bad sequence');
      if (b && (target < b.minLedger || (b.maxLedger && target >= b.maxLedger))) { stats.expired = (stats.expired || 0) + 1; return {status: 'PENDING', hash}; }
      const why = rule(method, args, stateAt(target), target);
      landed.set(hash, {ledger: target, status: why ? 'FAILED' : 'SUCCESS', envelope: signed.toEnvelope(), method});
      keeperSeq += 1n;
      if (!why) mutate(target, s => apply(method, args, s, target));
      return {status: 'PENDING', hash};
    }),
    getTransaction: hash => call('getTransaction', L => {
      const r = landed.get(hash);
      if (!r || r.ledger > L) return {status: 'NOT_FOUND'};
      return {status: r.status, ledger: r.ledger, envelopeXdr: r.envelope, resultXdr: {feeCharged: () => '1234'}};
    }),
  };
  return {rpc, clock, stats, random, mutate, stateAt, ledgerNow: () => ledgerAt(clock.now()), closeTimeOf, landed, baseTime, setKeeperSequence: v => { keeperSeq = v; }};
}
module.exports = {makeWorld, makeClock, PROFILES, rng, lognormal};
