// Borrower discovery (public go-live). Offline fake RPC with real XDR keys/values; no network, no keys.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {loadSdk, MAINNET} from '../src/chain.mjs';
import {normalize} from '../src/config.mjs';
import {makeDiscovery, accountsFromEvents, watchSet, coverage, WINDOW_LEDGERS, KEYS_PER_CALL} from '../src/discovery.mjs';
import {botAllow} from '../src/mainnet.mjs';
import {stageCall, mapLimit} from '../src/main.mjs';

const S = loadSdk();
const C = b => S.StrKey.encodeContract(Buffer.alloc(32, b));
const G = n => { const b = Buffer.alloc(32, 0x5a); b.writeUInt32BE(n, 0); return S.StrKey.encodeEd25519PublicKey(b); };
const tmp = t => { const d = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'clliq-disc-'))); t.after(() => fs.rmSync(d, {recursive: true, force: true})); return d; };
const cfgOf = (dir, over = {}) => normalize({networkPassphrase: MAINNET, rpcUrl: 'https://rpc.invalid',
  contracts: {receipt: C(1), vault: C(2), controller: C(3), usdcMarket: C(4), usdc: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', pyusd: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2', oracle: C(7)},
  pins: {receipt: 'a'.repeat(64), vault: 'b'.repeat(64), controller: 'c'.repeat(64), usdcMarket: 'd'.repeat(64)},
  liquidator: G(900), executorKeyFile: '/k/l', heartbeatAccount: G(901), heartbeatKeyFile: '/k/h', heartbeatRpcUrls: ['https://a.invalid', 'https://b.invalid'],
  repayCapRaw: '1000000000', borrowers: [G(1)], dataDir: dir, ...over});

// World: events per ledger, borrow principals and UserMarkets on chain, controller MarketUserCounts.
function world(cfg, {head = 1_000_000, events = [], principal = {}, entered = {}, count = null, pageLedgers = 10_000} = {}) {
  const calls = {getEvents: 0, getLedgerEntries: 0, getLatestLedger: 0, keys: 0};
  const cur = L => `${(BigInt(L) << 32n).toString().padStart(19, '0')}-4294967295`;
  const curL = c => Number(BigInt(c.split('-')[0]) >> 32n);
  const ev = (name, topics, ledger) => ({ledger, topic: [S.xdr.ScVal.scvSymbol(name), ...topics.map(a => new S.Address(a).toScVal())]});
  const all = events.map(e => ev(e.name, e.topics, e.ledger));
  const w = {calls, head,
    rpc: {
      getLatestLedger: async () => { calls.getLatestLedger++; return {sequence: w.head}; },
      getEvents: async ({startLedger, cursor, limit}) => {
        calls.getEvents++;
        const from = cursor ? curL(cursor) + 1 : startLedger;
        if (from < w.head - WINDOW_LEDGERS) throw Error('startLedger must be within the ledger range');
        const to = Math.min(w.head, from + pageLedgers - 1);
        const got = all.filter(e => e.ledger >= from && e.ledger <= to).slice(0, limit);
        const last = got.length === limit ? got.at(-1).ledger : to;
        return {events: got, cursor: cur(last), latestLedger: w.head};
      },
      getLedgerEntries: async (...keys) => {
        calls.getLedgerEntries++; calls.keys += keys.length;
        assert(keys.length <= KEYS_PER_CALL, 'getLedgerEntries over its key limit');
        const entries = [];
        for (const k of keys) {
          const d = k.contractData(), contract = S.Address.fromScAddress(d.contract()).toString(), key = d.key();
          if (key.switch().name === 'scvLedgerKeyContractInstance') {
            if (count == null) { entries.push({val: {contractData: () => ({contract: () => d.contract(), key: () => key, val: () => ({instance: () => ({storage: () => []})})})}}); continue; }
            const m = S.xdr.ScVal.scvMap([new S.xdr.ScMapEntry({key: new S.Address(cfg.contracts.usdcMarket).toScVal(), val: S.xdr.ScVal.scvU32(count)})]);
            const st = [new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('MarketUserCounts')]), val: m})];
            entries.push({val: {contractData: () => ({contract: () => d.contract(), key: () => key, val: () => ({instance: () => ({storage: () => st})})})}});
            continue;
          }
          const name = key.vec()[0].sym().toString(), user = S.Address.fromScVal(key.vec()[1]).toString();
          let val = null;
          if (name === 'BorrowSnapshots' && contract === cfg.contracts.usdcMarket && principal[user] != null)
            val = S.xdr.ScVal.scvMap([new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol('interest_index'), val: S.nativeToScVal(10n ** 18n, {type: 'u128'})}),
              new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol('principal'), val: S.nativeToScVal(BigInt(principal[user]), {type: 'u128'})})]);
          if (name === 'UserMarkets' && contract === cfg.contracts.controller && entered[user]) val = S.xdr.ScVal.scvVec(entered[user].map(m => new S.Address(m).toScVal()));
          if (val) entries.push({val: {contractData: () => ({contract: () => d.contract(), key: () => key, val: () => val})}});
        }
        return {latestLedger: w.head, entries};
      },
    }};
  return w;
}

test('pure: events name accounts (market_entered account, borrow_event borrower); bot accounts dropped; listed set is the minimum', () => {
  const cfg = {liquidator: G(900), heartbeatAccount: G(901)};
  const ev = [{name: 'market_entered', topics: ['market_entered', G(2), C(4)]}, {name: 'borrow_event', topics: ['borrow_event', G(3)]},
    {name: 'market_entered', topics: ['market_entered', G(900), C(4)]}, {name: 'redeem', topics: ['redeem', G(5)]}, {name: 'borrow_event', topics: ['borrow_event', C(9)]}];
  assert.deepEqual(accountsFromEvents(ev, cfg).sort(), [G(2), G(3), C(9)].sort());
  assert.deepEqual(watchSet([G(1)], [{user: G(1), principal: 0n}, {user: G(2), principal: 5n}, {user: G(3), principal: 0n}]), [G(1), G(2)]);
  assert.deepEqual(coverage([{enteredUsdc: true}, {enteredUsdc: false}], 3), {knownEntered: 1, onChainEntered: 3, missing: 2, complete: false});
  assert.equal(coverage([{enteredUsdc: true}], 1).complete, true);
});

test('startup backfill pages the whole window; new borrowers join the watched set automatically; listed stays', async t => {
  const d = tmp(t), cfg = cfgOf(d, {discovery: {startLedger: 950_000}});
  const w = world(cfg, {events: [{name: 'market_entered', topics: [G(2), C(1)], ledger: 951_000}, {name: 'borrow_event', topics: [G(2)], ledger: 975_000},
    {name: 'market_entered', topics: [G(3), C(1)], ledger: 999_000}], principal: {[G(2)]: 20_000_000n}, entered: {[G(2)]: [C(1), cfg.contracts.usdcMarket]}, count: 1});
  const alerts = [], disc = makeDiscovery(cfg, {rpc: w.rpc, S, out: o => o.level === 'alert' && alerts.push(o)});
  const r = await disc.cycle({maxPages: 40});
  assert.deepEqual(r.watch, [G(1), G(2)]);                         // listed G(1) + debtor G(2); G(3) known, no debt
  assert.equal(r.known, 3); assert.equal(r.debtors, 1); assert.equal(r.coverage.complete, true); assert.equal(alerts.length, 0);
  assert.equal(r.rpc.getEvents, 5);                                // 50,000 ledgers / 10,000 per page, stops when caught up
  // G(3) borrows later: one steady cycle later it is watched.
  const before = w.calls.getEvents;
  const w2 = world(cfg, {head: 1_000_050, events: [{name: 'borrow_event', topics: [G(3)], ledger: 1_000_020}], principal: {[G(2)]: 1n, [G(3)]: 7n},
    entered: {[G(2)]: [cfg.contracts.usdcMarket], [G(3)]: [cfg.contracts.usdcMarket]}, count: 2});
  const disc2 = makeDiscovery(cfg, {rpc: w2.rpc, S});                // restart: persisted cursor and known set
  assert.equal(Object.keys(disc2.state.known).length, 2);           // G(2), G(3) persisted; G(1) is listed
  const r2 = await disc2.cycle({maxPages: 3});
  assert.deepEqual(r2.watch.sort(), [G(1), G(2), G(3)].sort()); assert.equal(r2.rpc.getEvents, 1, 'resumes from the cursor: one page');
  assert(before >= 5);
});

test('cursor older than the RPC window: gap recorded and alerted, rescan from the window start; coverage exposes missed borrowers', async t => {
  const d = tmp(t), cfg = cfgOf(d);
  fs.writeFileSync(path.join(d, 'discovery.json'), JSON.stringify({cursor: `${(BigInt(500_000) << 32n)}-1`, cursorLedger: 500_000, known: {[G(2)]: {}}, gaps: []}));
  const w = world(cfg, {head: 1_000_000, principal: {[G(2)]: 1n}, entered: {[G(2)]: [cfg.contracts.usdcMarket]}, count: 3});
  const alerts = [], disc = makeDiscovery(cfg, {rpc: w.rpc, S, out: o => o.level === 'alert' && alerts.push(o)});
  const r = await disc.cycle({maxPages: 40});
  assert.equal(disc.state.gaps.length, 1); assert.equal(disc.state.gaps[0].from, 500_000);
  assert(alerts.some(a => /older than the RPC window/.test(a.msg)));
  assert.deepEqual(r.coverage, {knownEntered: 1, onChainEntered: 3, missing: 2, complete: false});
  assert(alerts.some(a => /not known to the bot/.test(a.msg) && a.missing === 2));
  assert.deepEqual(r.watch.sort(), [G(1), G(2)].sort());             // still watches what it knows
});

test('a start ledger before the window is a recorded gap, not an error', async t => {
  const d = tmp(t), cfg = cfgOf(d, {discovery: {startLedger: 10}});
  const w = world(cfg, {count: 0}), alerts = [];
  await makeDiscovery(cfg, {rpc: w.rpc, S, out: o => o.level === 'alert' && alerts.push(o)}).cycle({maxPages: 40});
  assert(alerts.some(a => /before the RPC window/.test(a.msg)));
});

test('a discovered borrower in shortfall may be liquidated with the same repayCapRaw bound; unknown accounts may not', async t => {
  const d = tmp(t), cfg = cfgOf(d);
  const w = world(cfg, {events: [{name: 'borrow_event', topics: [G(7)], ledger: 999_990}], principal: {[G(7)]: 10n ** 9n}, entered: {[G(7)]: [cfg.contracts.usdcMarket]}, count: 1});
  const r = await makeDiscovery(cfg, {rpc: w.rpc, S}).cycle({maxPages: 40});
  cfg.borrowers = r.watch;                                          // what main.mjs does every cycle
  const l3 = (b, repay) => stageCall({stage: 'L3', borrower: b, repay}, cfg, 1);
  const ok = l3(G(7), 10n ** 9n); assert.doesNotThrow(() => botAllow(cfg, ok.contract, ok.fn, ok.args));
  const big = l3(G(7), 10n ** 9n + 1n); assert.throws(() => botAllow(cfg, big.contract, big.fn, big.args), /outside/);
  const unk = l3(G(8), 1n); assert.throws(() => botAllow(cfg, unk.contract, unk.fn, unk.args), /outside/);
  const bot = l3(cfg.liquidator, 1n); cfg.borrowers.push(cfg.liquidator); assert.throws(() => botAllow(cfg, bot.contract, bot.fn, bot.args), /outside/);
});

test('RPC cost per steady cycle with N known accounts (discovery part): 1 getLatestLedger + 1 getEvents + ceil((1 + 2N) / 200) getLedgerEntries', async t => {
  const rows = [];
  for (const N of [1, 10, 50, 100, 500, 1000]) {
    const d = tmp(t), cfg = cfgOf(d, {borrowers: []});
    const users = Array.from({length: N}, (_, i) => G(i + 10));
    const principal = Object.fromEntries(users.slice(0, Math.ceil(N / 10)).map(u => [u, 1n]));     // 10 % hold debt
    const entered = Object.fromEntries(users.map(u => [u, [cfg.contracts.usdcMarket]]));
    const w = world(cfg, {principal, entered, count: N});
    fs.writeFileSync(path.join(d, 'discovery.json'), JSON.stringify({cursor: null, cursorLedger: null, known: Object.fromEntries(users.map(u => [u, {}])), gaps: []}));
    const disc = makeDiscovery(cfg, {rpc: w.rpc, S, listed: []});
    await disc.cycle({maxPages: 40});
    for (const k of Object.keys(w.calls)) w.calls[k] = 0;
    const r = await disc.cycle({maxPages: 3});
    assert.equal(w.calls.getLedgerEntries, Math.ceil((1 + 2 * N) / KEYS_PER_CALL));
    assert.equal(w.calls.getEvents, 1); assert.equal(w.calls.getLatestLedger, 1);
    assert.equal(r.debtors, Math.ceil(N / 10)); assert.equal(r.coverage.complete, true);
    rows.push({N, debtors: r.debtors, getLedgerEntries: w.calls.getLedgerEntries, discoveryCalls: 2 + w.calls.getLedgerEntries, planSimulations: 2 * r.watch.length});
    assert.equal(r.watch.length, r.debtors, 'only debtors reach the plan');
  }
  console.log('\n' + rows.map(x => `N=${x.N} debtors=${x.debtors}: discovery ${x.discoveryCalls} calls (${x.getLedgerEntries} getLedgerEntries), plan +${x.planSimulations} simulations`).join('\n'));
});

test('observe reads borrowers with bounded concurrency (8), in order: 50 debtors take ~7 rounds, not 50', async () => {
  let inFlight = 0, peak = 0, rounds = 0;
  const f = async x => { inFlight++; peak = Math.max(peak, inFlight); await new Promise(r => setTimeout(r, 5)); inFlight--; rounds++; return x * 2; };
  const t0 = Date.now(), out = await mapLimit(Array.from({length: 50}, (_, i) => i), 8, f);
  assert.deepEqual(out, Array.from({length: 50}, (_, i) => i * 2)); assert.equal(peak, 8); assert.equal(rounds, 50);
  assert(Date.now() - t0 < 50 * 5, 'parallel');
  await assert.rejects(mapLimit([1, 2, 3], 2, async x => { if (x === 2) throw Error('rpc'); return x; }), /rpc/);
  assert.deepEqual(await mapLimit([], 8, f), []);
});
