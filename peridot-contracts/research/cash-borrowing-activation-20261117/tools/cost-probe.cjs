// Read-only Mainnet probes: Reflector history gaps (staleness k), per-call simulated costs of the cross-contract reads the
// controller adds once an oracle is set, and the CL staged receipt's get_account_snapshot cost. No signing.
'use strict';
const {S, MAINNET, json, addr, chain} = require('./lib.cjs');
const c = chain(MAINNET);
const other = s => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(s)]);
async function cost(contract, method, args = []) {
  const tx = new S.TransactionBuilder(new S.Account(MAINNET.admin, '0'), {fee: '100', networkPassphrase: MAINNET.network})
    .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(0).build();
  const sim = await c.rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) return {method, error: String(sim.error).split('\n')[0].slice(0, 160)};
  const d = sim.transactionData.build(), r = d.resources(), fp = r.footprint();
  return {contract, method, cpu: Number(r.instructions()), readOnly: fp.readOnly().length, readWrite: fp.readWrite().length, diskReadBytes: r.diskReadBytes(), memBytes: sim.cost ? Number(sim.cost.memBytes) : null};
}
(async () => {
  const out = {observedAt: new Date().toISOString(), gaps: {}, costs: []};
  for (const s of ['USDC', 'XLM', 'EURC']) {
    const r = (await c.view(MAINNET.oracles.external, 'prices', [other(s), S.nativeToScVal(96, {type: 'u32'})])).value || [];
    const ts = r.map(p => Number(p.timestamp)).sort((a, b) => a - b), gaps = ts.slice(1).map((t, i) => t - ts[i]);
    out.gaps[s] = {records: r.length, first: ts[0], last: ts[ts.length - 1], maxGapSeconds: Math.max(...gaps), gapsOver300: gaps.filter(g => g > 300).length,
      minPrice: r.reduce((m, p) => (m === null || BigInt(p.price) < m ? BigInt(p.price) : m), null), maxPrice: r.reduce((m, p) => (m === null || BigInt(p.price) > m ? BigInt(p.price) : m), null)};
  }
  out.costs.push(await cost(MAINNET.oracles.external, 'lastprice', [other('XLM')]));
  out.costs.push(await cost(MAINNET.oracles.external, 'decimals'));
  out.costs.push(await cost(MAINNET.oracles.external, 'resolution'));
  for (const [n, m] of Object.entries(MAINNET.markets)) out.costs.push({market: n, ...(await cost(m.receipt, 'get_account_snapshot', [addr(MAINNET.admin)]))});
  out.costs.push({market: 'CL_STAGED', ...(await cost(MAINNET.clStagedReceipt, 'get_account_snapshot', [addr(MAINNET.admin)]))});
  out.costs.push({market: 'CL_STAGED', ...(await cost(MAINNET.clStagedReceipt, 'get_underlying_token'))});
  out.ownerEnteredMarkets = await c.tryView(MAINNET.controller, 'get_user_markets', [addr(MAINNET.admin)]);
  out.ownerAccountLiquidity = await c.tryView(MAINNET.controller, 'account_liquidity', [addr(MAINNET.admin)]);
  out.costs.push(await cost(MAINNET.controller, 'account_liquidity', [addr(MAINNET.admin)]));
  console.log(json(out));
})().catch(e => { console.error(e.message); process.exit(1); });
