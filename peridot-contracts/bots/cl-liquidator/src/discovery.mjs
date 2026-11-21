// Borrower discovery for the CL group (Mainnet, before public go-live). The contracts keep no enumerable borrower
// list, so the watched set is built from events plus a completeness check against on-chain state:
//
//  1. Events, paged with a persisted cursor (<dataDir>/discovery.json):
//       controller `market_entered(account, market)` and USDC market `borrow_event(borrower)`
//     (borrowing auto-enters the USDC market through track_borrow_market, which emits no market_entered).
//     stellar-rpc scans at most ~10,000 ledgers per getEvents call, so a full 7-day window is ~13 pages (Mainnet,
//     21 Nov: 13 pages in 0.47 s). A cursor older than the RPC window is a recorded GAP (alert), never silently skipped.
//  2. Screening every cycle, batched: one getLedgerEntries per 100 watched accounts reads USDC-market
//     BorrowSnapshots(user) and controller UserMarkets(user), plus the controller instance (MarketUserCounts). Only
//     accounts with a nonzero borrow principal go to the plan (2 simulations each, as for listed borrowers).
//  3. Completeness ("seed from current on-chain state"): the controller's MarketUserCounts[usdcMarket] is the exact
//     number of accounts entered in the USDC market, and every borrower is entered there. If fewer known accounts are
//     entered than that count, some borrower is unknown (history older than the window, a gap): alert every cycle with
//     the missing count until the operator adds the accounts to `borrowers` (the listed set is always watched).
import fs from 'node:fs';
import path from 'node:path';
import {loadSdk} from './chain.mjs';

export const WINDOW_LEDGERS = 120_960;       // stellar-rpc default retention, ~7 days of 5 s ledgers
export const KEYS_PER_CALL = 200;            // getLedgerEntries limit
const ledgerOfCursor = c => { try { return Number(BigInt(String(c).split('-')[0]) >> 32n); } catch { return null; } };
const isAccount = a => /^[GC][A-Z2-7]{55}$/.test(a ?? '');

/** Pure: accounts named by decoded events ({name, topics: [native...]}); bot accounts and non-addresses dropped. */
export function accountsFromEvents(events, cfg) {
  const out = new Set(), bots = new Set([cfg.liquidator, cfg.heartbeatAccount].filter(Boolean));
  for (const e of events) {
    const a = e.name === 'market_entered' ? e.topics[1] : e.name === 'borrow_event' ? e.topics[1] : null;
    if (isAccount(a) && !bots.has(a)) out.add(a);
  }
  return [...out];
}

/** Pure: the plan's borrower set = listed (always) + known accounts with a nonzero borrow principal. */
export function watchSet(listed, screened) {
  return [...new Set([...listed, ...screened.filter(s => s.principal > 0n).map(s => s.user)])];
}

/** Pure: completeness alert when fewer known accounts are entered in the USDC market than the controller counts. */
export function coverage(screened, onChainCount) {
  const known = screened.filter(s => s.enteredUsdc).length;
  return {knownEntered: known, onChainEntered: onChainCount, missing: Math.max(0, Number(onChainCount ?? 0) - known), complete: onChainCount != null && known >= onChainCount};
}

export function makeDiscovery(cfg, {rpc, S = loadSdk(), dir = cfg.dataDir, out = () => {}, listed = [...cfg.borrowers]} = {}) {
  const file = path.join(dir, 'discovery.json');
  const load = () => { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return {cursor: null, cursorLedger: null, known: {}, gaps: []}; } };
  const save = s => { fs.mkdirSync(dir, {recursive: true}); fs.writeFileSync(file + '.tmp', JSON.stringify(s, null, 1)); fs.renameSync(file + '.tmp', file); };
  const st = load();
  const sym = s => S.xdr.ScVal.scvSymbol(s).toXDR('base64');
  const filters = [
    {type: 'contract', contractIds: [cfg.contracts.controller], topics: [[sym('market_entered'), '*', '*']]},
    {type: 'contract', contractIds: [cfg.contracts.usdcMarket], topics: [[sym('borrow_event'), '*']]},
  ];
  const decode = e => {
    const t = e.topic.map(x => { try { return S.scValToNative(typeof x === 'string' ? S.xdr.ScVal.fromXDR(x, 'base64') : x); } catch { return null; } });
    return {name: String(t[0]), topics: t.map(String), ledger: e.ledger};
  };

  /** Page events from the cursor (or the start ledger) toward the head. Returns {added, pages, head}. */
  async function poll({maxPages = 3, limit = 200} = {}) {
    let added = [], pages = 0, r;
    // raw JSON when available (makeRpc): SDK 16 may not decode newer ledger headers
    const head = rpc.rawCall ? Number((await rpc.rawCall('getLatestLedger')).sequence) : (await rpc.getLatestLedger()).sequence;
    const oldest = head - WINDOW_LEDGERS + 1_000;                 // stay inside the retention window with margin
    if (!st.cursor) {
      const want = cfg.discovery?.startLedger ?? oldest;
      const from = Math.max(want, oldest);
      if (want < from && !st.gaps.some(g => g.to === from)) { st.gaps.push({from: want, to: from, reason: 'start ledger before the RPC window'}); out({level: 'alert', msg: 'discovery: history before the RPC window is not scanned; the coverage check reports any missed borrower', from: want, to: from}); }
      r = await rpc.getEvents({startLedger: from, filters, limit}); pages++;
    } else {
      if (st.cursorLedger != null && st.cursorLedger < oldest) {
        st.gaps.push({from: st.cursorLedger, to: oldest, reason: 'cursor older than the RPC window (bot was down)'});
        out({level: 'alert', msg: 'discovery: cursor older than the RPC window: gap recorded, rescanning from the window start', from: st.cursorLedger, to: oldest});
        st.cursor = null; st.cursorLedger = null; save(st); return poll({maxPages, limit});
      }
      r = await rpc.getEvents({cursor: st.cursor, filters, limit}); pages++;
    }
    for (;;) {
      for (const a of accountsFromEvents(r.events.map(decode), cfg)) if (!st.known[a]) { st.known[a] = {since: r.latestLedger}; added.push(a); }
      st.cursor = r.cursor; st.cursorLedger = ledgerOfCursor(r.cursor); save(st);
      const caughtUp = r.events.length < limit && (st.cursorLedger == null || st.cursorLedger >= (r.latestLedger ?? head) - 1);
      if (caughtUp || pages >= maxPages) break;
      r = await rpc.getEvents({cursor: st.cursor, filters, limit}); pages++;
    }
    if (added.length) out({level: 'info', msg: 'discovery: new accounts', added});
    return {added, pages, head, cursorLedger: st.cursorLedger};
  }

  const key = (contract, parts) => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(contract).toScAddress(),
    key: S.xdr.ScVal.scvVec(parts.map(p => (typeof p === 'string' && /^[A-Z][A-Za-z]*$/.test(p) && p.length < 32 ? S.xdr.ScVal.scvSymbol(p) : new S.Address(p).toScVal()))),
    durability: S.xdr.ContractDataDurability.persistent()}));
  const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(),
    key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));

  /** Batched screen of `users`: [{user, principal, enteredUsdc}] plus the controller's USDC-market user count. */
  async function screen(users) {
    const rows = new Map(users.map(u => [u, {user: u, principal: 0n, enteredUsdc: false}]));
    const keys = [instanceKey(cfg.contracts.controller), ...users.flatMap(u => [key(cfg.contracts.usdcMarket, ['BorrowSnapshots', u]), key(cfg.contracts.controller, ['UserMarkets', u])])];
    let onChainCount = null, calls = 0;
    for (let i = 0; i < keys.length; i += KEYS_PER_CALL) {
      const r = await rpc.getLedgerEntries(...keys.slice(i, i + KEYS_PER_CALL)); calls++;
      for (const e of r.entries) {
        const d = e.val.contractData(), contract = S.Address.fromScAddress(d.contract()).toString(), k = d.key();
        if (k.switch().name === 'scvLedgerKeyContractInstance') {
          for (const it of d.val().instance().storage() ?? []) {
            const kk = it.key();
            if (kk.switch().name === 'scvVec' && kk.vec().length === 1 && kk.vec()[0].switch().name === 'scvSymbol' && kk.vec()[0].sym().toString() === 'MarketUserCounts') {
              const m = S.scValToNative(it.val()); onChainCount = Number(m?.[cfg.contracts.usdcMarket] ?? 0);
            }
          }
          if (onChainCount == null) onChainCount = 0;               // no map yet: nobody entered
          continue;
        }
        const parts = k.vec(), name = parts[0].sym().toString(), user = S.Address.fromScVal(parts[1]).toString(), row = rows.get(user);
        if (!row) continue;
        const v = S.scValToNative(d.val());
        if (name === 'BorrowSnapshots' && contract === cfg.contracts.usdcMarket) row.principal = BigInt(v.principal ?? 0);
        if (name === 'UserMarkets' && contract === cfg.contracts.controller) row.enteredUsdc = (v ?? []).map(String).includes(cfg.contracts.usdcMarket);
      }
    }
    return {screened: [...rows.values()], onChainCount, calls};
  }

  /** One discovery cycle: poll events, screen listed + known, alert on incomplete coverage, return the plan's set. */
  async function cycle(opts = {}) {
    const p = await poll(opts);
    const users = [...new Set([...listed, ...Object.keys(st.known)])];
    const s = await screen(users);
    const cov = coverage(s.screened, s.onChainCount);
    if (!cov.complete) out({level: 'alert', msg: 'discovery: USDC-market accounts not known to the bot (add them to borrowers)', ...cov, gaps: st.gaps.length});
    const watch = watchSet(listed, s.screened);
    return {watch, known: users.length, debtors: s.screened.filter(x => x.principal > 0n).length, coverage: cov, rpc: {getEvents: p.pages, getLedgerEntries: s.calls, getLatestLedger: 1}};
  }
  return {poll, screen, cycle, state: st, listed};
}
