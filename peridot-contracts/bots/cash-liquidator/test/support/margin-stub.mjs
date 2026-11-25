// Local stub chain answering the margin pass with fork-derived responses (test/fixtures/margin-fork.json). It records
// every call and never touches the network. The contract-side effects of live sends (settle reducing shares, liquidate
// closing a position) are modelled just enough to exercise the keeper's loops.
import fs from 'node:fs';
import * as S from '@stellar/stellar-sdk';

export const FIX = JSON.parse(fs.readFileSync(new URL('../fixtures/margin-fork.json', import.meta.url), 'utf8'));
export const PINS = JSON.parse(fs.readFileSync(new URL('../../config/mainnet.json', import.meta.url), 'utf8'));
export const MARGIN = 'CADQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQP5KR';
export const KEEPER = 'GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQDZ7H';
export const OWNER = 'GABAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEJXA';
export const BACKER = 'GABQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQHGPC';
const M = PINS.markets;
const nat = v => (v instanceof S.xdr.ScVal ? S.scValToNative(v) : v);
const contractErr = n => `HostError: Error(Contract, #${n})`;

export function marginCfg(over = {}) {
  return {
    pins: PINS, publicKey: KEEPER, dryRun: false, journalFile: '/dev/null', stateFile: '/dev/null',
    margin: {enabled: true, contract: MARGIN, wasmAllowed: [FIX.source.marginWasmSha256], controller: PINS.controller, dryRun: true,
      cpuStop: 289_000_000, maxFeeStroops: 30_000_000, minProfitBps: 0, payBufferBps: 50, maxSettleIters: 8, hfDownMin: 1_100_000n,
      liquidatableAlertMs: 600_000, realertMs: 600_000, backerLookbackLedgers: 100_000, backersSeed: [], roundPollMs: 5_000, ...over},
  };
}

export function stubChain(name, o = {}) {
  const sc = structuredClone(FIX.scenarios[name]);
  const calls = [];
  const state = {
    seq: o.seq ?? sc.latestLedger, closeTime: o.closeTime ?? 1_795_000_000,
    positions: Object.fromEntries(sc.positions.map(p => [String(p.id), {...p, open: true}])),
    bs: {USDC: BigInt(sc.bsDebtShares.USDC), XLM: BigInt(sc.bsDebtShares.XLM)},
    reconcile: sc.reconcile, impaired: sc.impaired, counter: o.counter ?? sc.positions.length,
    events: o.events ?? [], withdrawReq: o.withdrawReq ?? {},
    shortfall: o.shortfall ?? false, wasm: o.wasm ?? FIX.source.marginWasmSha256,
    adminKey: o.adminKey ?? 'GCAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
  };
  const prices = {[M.USDC.underlying]: FIX.pricesP0.usdc.map(BigInt), [M.XLM.underlying]: sc.priceXlm.map(BigInt)};
  const pos = id => state.positions[String(id)];
  const posView = p => ({owner: OWNER, held_asset: p.side === 'long' ? M.XLM.underlying : M.USDC.underlying, debt_asset: p.side === 'long' ? M.USDC.underlying : M.XLM.underlying,
    held_ptokens: BigInt(p.heldPt), debt_shares: 1n, collateral: 1n, equity_usd: 249_000_000n, notional_usd: 1_238_000_000n, liq_ltv: BigInt(p.liqLtv), opened_at: 1n, funding_ts: 1n, open: p.open});
  const need = id => { const p = pos(id); if (!p || !p.open) throw Error(`get_position_ltv: ${contractErr(13)}`); return p; };
  const views = {
    get_position_counter: () => BigInt(state.counter),
    get_position: ([id]) => (pos(id) ? posView(pos(id)) : null),
    get_position_ltv: ([id]) => BigInt(need(id).ltv),
    get_position_debt: ([id]) => BigInt(need(id).debt),
    get_position_funding: ([id]) => BigInt(need(id).funding),
    get_bs_debt_shares: ([a]) => state.bs[a === M.USDC.underlying ? 'USDC' : 'XLM'],
    settle_max_ptokens: ([a]) => (state.shortfall ? 0n : BigInt(sc.settleMaxPtokens?.[a === M.USDC.underlying ? 'USDC' : 'XLM'] ?? '1000000')),
    pooled_health: () => sc.pooledHealth.map(BigInt),
    get_headroom: () => Object.fromEntries(Object.entries(FIX.headroom).map(([k, v]) => [k, BigInt(v)])),
    is_impaired: () => state.impaired,
    backstop_nav: () => [7_496_255_671n, 7_497_255_671n],
    get_withdraw_req: ([b]) => state.withdrawReq[b] ?? null,
    get_price_usd: ([t]) => prices[t] ?? null,
    get_exchange_rate: () => 1_000_000n,
    balance: () => 0n,
  };
  const fees = Object.fromEntries(Object.entries(FIX.fees).map(([k, v]) => [k, BigInt(v)]));
  const sym = s => S.xdr.ScVal.scvSymbol(s);
  const ik = (n, ...a) => S.xdr.ScVal.scvVec([sym(n), ...a.map(x => S.nativeToScVal(x, {type: 'address'}))]).toXDR('base64');
  const inst = new Map([[ik('Ctrl'), PINS.controller], [ik('Usdc'), M.USDC.underlying], [ik('Xlm'), M.XLM.underlying],
    [ik('Market', M.USDC.underlying), M.USDC.receipt], [ik('Market', M.XLM.underlying), M.XLM.receipt], [ik('Fees'), fees], [ik('Admin'), state.adminKey]]);
  let n = 0;
  const fakeTx = (method, args) => ({method, args, hash: () => Buffer.from(String(++n).padStart(64, '0'), 'hex')});
  function simulate(method, args) {
    const a = args.map(nat);
    if (method === 'liquidate') {
      const p = pos(a[1]);
      if (!p || !p.open) return {ok: false, error: contractErr(13)};
      if (BigInt(p.ltv) < BigInt(p.liqLtv)) return {ok: false, error: contractErr(14)};
      if (o.noEvents) return {ok: true, retval: null, events: []};
      const heldTok = p.side === 'long' ? M.XLM.receipt : M.USDC.receipt;
      const debtTok = p.side === 'long' ? M.USDC.underlying : M.XLM.underlying;
      const pay = BigInt(o.liqPay?.[a[1]] ?? FIX.checks.L5_underwater_minus20.fork.paid);
      const seize = BigInt(p.heldPt);
      if (pay > BigInt(a[2])) return {ok: false, error: contractErr(32)};
      return {ok: true, retval: null, events: [
        {contract: debtTok, topics: ['transfer', a[0], MARGIN, 'USDC:G...'], data: pay},
        {contract: heldTok, topics: ['transfer', MARGIN, a[0]], data: seize},
        {contract: MARGIN, topics: ['margin_liquidate'], data: [BigInt(a[1]), OWNER, a[0], pay, seize, 0n]},
      ]};
    }
    if (method === 'settle_backstop_debt') {
      const k = a[1] === M.USDC.underlying ? 'USDC' : 'XLM';
      if (state.shortfall) return {ok: false, error: contractErr(35)};
      return {ok: true, retval: BigInt(sc.settleRepaid?.[k] ?? '1')};
    }
    if (method === 'backstop_clear_expired') {
      const r = state.withdrawReq[a[1]];
      if (!r) return {ok: false, error: contractErr(28)};
      if (BigInt(state.closeTime) <= BigInt(r.expires_at)) return {ok: false, error: contractErr(6)};
      return {ok: true, retval: null};
    }
    if (method === 'reconcile') return {ok: true, retval: state.reconcile};
    return {ok: false, error: `unexpected ${method}`};
  }
  function apply(tx) {   // contract effects of a successful send
    const a = tx.args.map(nat);
    if (tx.method === 'liquidate') { pos(a[1]).open = false; state.counter = state.counter; }
    if (tx.method === 'settle_backstop_debt') { const k = a[1] === M.USDC.underlying ? 'USDC' : 'XLM'; const step = o.settleStep ?? state.bs[k]; state.bs[k] = state.bs[k] > step ? state.bs[k] - step : 0n; }
    if (tx.method === 'backstop_clear_expired') delete state.withdrawReq[a[1]];
    if (tx.method === 'reconcile') { state.impaired = true; state.reconcile = false; }
  }
  return {
    calls, state,
    addr: x => S.nativeToScVal(x, {type: 'address'}), u64: x => S.nativeToScVal(BigInt(x), {type: 'u64'}), u128: x => S.nativeToScVal(BigInt(x), {type: 'u128'}),
    async latestLedger() { calls.push(['latestLedger']); return {sequence: state.seq, closeTime: state.closeTime}; },
    async instance(contract) { calls.push(['instance', contract]); return {wasm: state.wasm, get: k => inst.get(k.toXDR('base64')) ?? null}; },
    // Mirrors the Mainnet RPC: a ledger range and a cursor together are refused; offset cursors page by `limit`;
    // the events node's latestLedger may lag (o.eventsLag ledgers) behind getLatestLedger.
    async rawEvents(req) {
      calls.push(['rawEvents', req.names[0], req.startLedger ?? 'cursor']);
      if (req.cursor && (req.startLedger != null || req.endLedger != null)) throw Error('{"code":-32602,"message":"ledger ranges and cursor cannot both be set"}');
      const latestLedger = state.seq - (o.eventsLag ?? 0);
      const all = state.events.filter(e => req.names.includes(e.name)).filter(e => e.ledger <= latestLedger);
      const from = req.cursor ? Number(req.cursor) : all.findIndex(e => e.ledger >= req.startLedger);
      const rest = from < 0 ? [] : all.slice(from);
      const page = rest.slice(0, req.limit);
      return {events: page, cursor: String((from < 0 ? all.length : from) + page.length), latestLedger};
    },
    async view(contract, method, args = []) {
      calls.push(['view', method]);
      if (o.failView?.[method]) throw Error(`${method}: ${o.failView[method]}`);
      if (!views[method]) throw Error('unexpected view ' + method);
      return views[method](args.map(nat));
    },
    async prepare(contract, method, args, opts) {
      calls.push(['prepare', method, opts]);
      const r = simulate(method, args);
      return r.ok ? {...r, prepared: fakeTx(method, args), metrics: {cpu: 100_000_000, fee: '2000000'}} : {...r, metrics: {cpu: 0, fee: '0'}};
    },
    sign(tx) { calls.push(['sign', tx.method]); return tx; },
    async submit(tx) { calls.push(['submit', tx.method]); apply(tx); return {hash: tx.hash().toString('hex'), status: o.submitStatus ?? 'SUCCESS', ledger: state.seq}; },
  };
}

export const quietLog = () => { const lines = []; const log = (level, msg, extra = {}) => lines.push({level, msg, ...extra}); return {log, lines}; };
