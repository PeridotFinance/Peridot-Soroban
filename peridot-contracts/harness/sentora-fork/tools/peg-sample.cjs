// Read-only PYUSD peg sampler for the peg-oracle threshold (DESIGN.md §12).
// Never signs, never reads keys or env secrets. Only simulateTransaction / getLatestLedger.
//   cd harness/sentora-fork/tools && node peg-sample.cjs --samples 50 --every 30 --out ../.scratch/peg-samples.jsonl
// Each sample: two-way executable quotes (estimate_swap) of the Aquarius PYUSD/USDC concentrated pool (the one the LP
// vaults use) at several probe sizes, its slot0 tick, the same probes on the stable pool for comparison, Reflector
// CAFJZQ USDC/USD and the native Reflector CALI2BY PYUSD/USDC feed. The pool is discovered with the router's on-chain
// get_pools, never the Aquarius HTTP /pools/ API.
'use strict';
const fs = require('node:fs');
const S = require('@stellar/stellar-sdk');
const RPC = 'https://soroban-rpc.mainnet.stellar.gateway.fm';
const NETWORK = S.Networks.PUBLIC;
const opt = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const rpc = async (method, params) => { const r = await (await fetch(RPC, {method: 'POST',
  headers: {'content-type': 'application/json', 'user-agent': 'rpc-client/1.0'},
  body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json(); if (r.error) throw Error(JSON.stringify(r.error)); return r.result; };
const J = x => JSON.stringify(x, (_, v) => typeof v === 'bigint' ? String(v) : v);

const VIEW = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP'; // simulation source only (public key)
const ROUTER = 'CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK';
const PYUSD = 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const CONC = 'CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX';
const STABLE = 'CDMH535JSD224YXPET3B4SJOLXTQQ24GRSCWACGYBKSH2DKFJYWI7SUW';
const REFLECTOR = 'CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN';
const REFLECTOR_DEX = 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M';
const PROBES = [10n, 100n, 1_000n, 10_000n].map(x => x * 10_000_000n); // PYUSD/USDC raw (7 decimals)

const sym = s => S.xdr.ScVal.scvSymbol(s), a = x => S.nativeToScVal(x, {type: 'address'});
const u32 = x => S.nativeToScVal(x, {type: 'u32'}), u128 = x => S.nativeToScVal(BigInt(x), {type: 'u128'});
const vec = (n, ...args) => S.xdr.ScVal.scvVec([sym(n), ...args]);
const simulate = async (contract, fn, args) => {
  const tx = new S.TransactionBuilder(new S.Account(VIEW, '0'), {fee: '100', networkPassphrase: NETWORK})
    .addOperation(new S.Contract(contract).call(fn, ...args)).setTimeout(0).build();
  const sim = await rpc('simulateTransaction', {transaction: tx.toXDR()});
  if (sim.error) return {error: String(sim.error).split('\n')[0].slice(0, 200), ledger: sim.latestLedger};
  const data = S.xdr.SorobanTransactionData.fromXDR(sim.transactionData, 'base64'), res = data.resources(), fp = res.footprint();
  return {ledger: sim.latestLedger, entries: fp.readOnly().length + fp.readWrite().length, cpu: Number(res.instructions()),
    keys: [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64')),
    value: sim.results && sim.results[0] ? S.scValToNative(S.xdr.ScVal.fromXDR(sim.results[0].xdr, 'base64')) : null};
};
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function sample(meta) {
  const t = new Date().toISOString();
  const out = {t, conc: {}, stable: {}};
  const slot = await simulate(CONC, 'get_slot0', []);
  out.ledger = slot.ledger; out.tick = slot.value ? slot.value.tick : null;
  for (const [name, pool, idx] of [['conc', CONC, meta.concIdx], ['stable', STABLE, meta.stableIdx]]) {
    for (const p of PROBES) {
      const fwd = await simulate(pool, 'estimate_swap', [u32(idx.pyusd), u32(idx.usdc), u128(p)]); // sell PYUSD
      const rev = await simulate(pool, 'estimate_swap', [u32(idx.usdc), u32(idx.pyusd), u128(p)]); // buy PYUSD
      const row = {probe: String(p / 10_000_000n)};
      if (fwd.error || rev.error) { row.error = fwd.error || rev.error; }
      else {
        // bid: USDC received per PYUSD sold; ask: USDC paid per PYUSD bought (in/out). Both in bps of 1.
        row.bidBps = Number(BigInt(fwd.value) * 10_000_000n / p) / 1000;
        row.askBps = Number(p * 10_000_000n / BigInt(rev.value)) / 1000;
        row.fwdEntries = fwd.entries; row.revEntries = rev.entries; row.fwdCpu = fwd.cpu; row.revCpu = rev.cpu;
      }
      out[name][row.probe] = row;
    }
  }
  const u = await simulate(REFLECTOR, 'lastprice', [vec('Other', sym('USDC'))]);
  out.usdcUsd = u.value ? {price: String(u.value.price), ts: Number(u.value.timestamp)} : {error: u.error || 'none'};
  const d = await simulate(REFLECTOR_DEX, 'lastprice', [vec('Stellar', a(PYUSD))]);
  out.reflectorDexPyusdUsdc = d.value ? {price: String(d.value.price), ts: Number(d.value.timestamp)} : {error: d.error || 'none'};
  return out;
}

(async () => {
  const n = Number(opt('--samples', '50')), every = Number(opt('--every', '30')) * 1000;
  const file = opt('--out', 'peg-samples.jsonl');
  // Discovery through the router (on-chain), both token orders tried.
  let pools = null;
  for (const order of [[PYUSD, USDC], [USDC, PYUSD]]) {
    const r = await simulate(ROUTER, 'get_pools', [S.xdr.ScVal.scvVec(order.map(a))]);
    if (!r.error) { pools = {order, value: r.value}; break; }
  }
  const meta = {schema: 'peg-sample-v1', router: ROUTER, pools: pools ? {order: pools.order,
    pools: Object.entries(pools.value).map(([k, v]) => ({hash: Buffer.from(k).toString('hex'), pool: v}))} : null};
  for (const [name, pool] of [['conc', CONC], ['stable', STABLE]]) {
    const tokens = await simulate(pool, 'get_tokens', []);
    const type = await simulate(pool, 'pool_type', []);
    const fee = await simulate(pool, 'get_fee_fraction', []);
    const killed = await simulate(pool, 'get_is_killed_swap', []);
    const reserves = await simulate(pool, 'get_reserves', []);
    meta[name] = {pool, tokens: tokens.value, type: type.value, feeFraction: fee.value ?? fee.error, killedSwap: killed.value ?? killed.error,
      reserves: reserves.value ? reserves.value.map(String) : reserves.error};
    const idx = {pyusd: tokens.value.indexOf(PYUSD), usdc: tokens.value.indexOf(USDC)};
    meta[name + 'Idx'] = idx;
  }
  meta.concIdx = meta.concIdx; meta.stableIdx = meta.stableIdx;
  const spacing = await simulate(CONC, 'get_tick_spacing', []);
  meta.conc.tickSpacing = spacing.value;
  const listed = meta.pools ? meta.pools.pools.map(p => p.pool) : [];
  meta.concListedByRouter = listed.includes(CONC); meta.stableListedByRouter = listed.includes(STABLE);
  const fp = await simulate(CONC, 'estimate_swap', [u32(meta.concIdx.pyusd), u32(meta.concIdx.usdc), u128(PROBES[2])]);
  meta.concEstimateFootprint = fp.keys;
  fs.writeFileSync(file, J({meta}) + '\n');
  console.log(J(meta));
  for (let i = 0; i < n; i++) {
    const t0 = Date.now();
    try { const s = await sample(meta); fs.appendFileSync(file, J(s) + '\n');
      console.log(i, s.t, s.tick, J(s.conc['1000']), J(s.usdcUsd)); }
    catch (e) { fs.appendFileSync(file, J({t: new Date().toISOString(), error: String(e).slice(0, 200)}) + '\n'); }
    const wait = every - (Date.now() - t0); if (i + 1 < n && wait > 0) await sleep(wait);
  }
})().catch(e => { console.error(e); process.exit(1); });
