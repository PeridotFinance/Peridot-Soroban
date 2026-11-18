// Read-only: probe Reflector feeds (base, decimals, resolution, assets, lastprice for our tokens). No signing.
'use strict';
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const rpc = new S.rpc.Server(process.env.PROBE_RPC || 'https://soroban-rpc.mainnet.stellar.gateway.fm', {timeout: 60});
const SRC = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
const FEEDS = {external: 'CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN', stellarDex: 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M', fiat: 'CBKGPWGKSKZF52CFHMTRR23TBWTPMRDIYZ4O2P5VS65BMHYH4DXMCJZC'};
const TOK = {USDC: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', XLM: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA', PYUSD: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2', EURC: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV'};
const asset = a => a.stellar ? S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'), new S.Address(a.stellar).toScVal()]) : S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(a.other)]);
async function view(c, m, args = []) {
  const tx = new S.TransactionBuilder(new S.Account(SRC, '0'), {fee: '100', networkPassphrase: S.Networks.PUBLIC}).addOperation(new S.Contract(c).call(m, ...args)).setTimeout(0).build();
  const sim = await rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) return {error: String(sim.error).split('\n')[0].slice(0, 160)};
  return {value: S.scValToNative(sim.result.retval), ledger: sim.latestLedger};
}
const fmt = v => JSON.parse(JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x));
(async () => {
  const out = {observedAt: new Date().toISOString(), feeds: {}};
  for (const [name, id] of Object.entries(FEEDS)) {
    const f = {id};
    for (const m of ['base', 'decimals', 'resolution', 'period', 'last_timestamp', 'version', 'admin']) f[m] = fmt((await view(id, m)).value ?? (await view(id, m)).error);
    const as = await view(id, 'assets'); f.assets = fmt(as.value || as.error);
    f.lastprice = {};
    const tries = [...Object.entries(TOK).map(([k, t]) => [`Stellar(${k})`, {stellar: t}]), ...['USDC', 'XLM', 'EURC', 'PYUSD', 'EUR', 'USD'].map(s => [`Other(${s})`, {other: s}])];
    for (const [label, a] of tries) { const r = await view(id, 'lastprice', [asset(a)]); f.lastprice[label] = fmt(r.value === undefined ? r.error : r.value); }
    out.feeds[name] = f; out.ledger = as.ledger;
  }
  console.log(JSON.stringify(out, null, 1));
})().catch(e => { console.error(e.message); process.exit(1); });
