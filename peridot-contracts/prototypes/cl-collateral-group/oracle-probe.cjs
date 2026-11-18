// READ-ONLY Reflector probe for the CL collateral group (18 Nov 2026). simulateTransaction only.
// No signing, no submission, no keys. Usage: node oracle-probe.cjs > out.json
'use strict';
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const rpc = new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm', {timeout: 60});
const SRC = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP'; // public account, used only as simulation source
const FEEDS = {stellarDex: 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M', external: 'CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN'};
const PYUSD = 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2', USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const asset = a => a.stellar ? S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'), new S.Address(a.stellar).toScVal()]) : S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(a.other)]);
const fmt = v => JSON.parse(JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x));
async function view(c, m, args = []) {
  const tx = new S.TransactionBuilder(new S.Account(SRC, '0'), {fee: '100', networkPassphrase: S.Networks.PUBLIC}).addOperation(new S.Contract(c).call(m, ...args)).setTimeout(0).build();
  const sim = await rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) return {error: String(sim.error).split('\n')[0].slice(0, 160), ledger: sim.latestLedger};
  return {value: fmt(S.scValToNative(sim.result.retval)), ledger: sim.latestLedger};
}
(async () => {
  const out = {observedAt: new Date().toISOString(), rpc: 'https://soroban-rpc.mainnet.stellar.gateway.fm', method: 'simulateTransaction (read-only)', feeds: {}};
  for (const [name, id] of Object.entries(FEEDS)) {
    const f = {id};
    for (const m of ['base', 'decimals', 'resolution', 'last_timestamp']) f[m] = await view(id, m);
    const as = await view(id, 'assets'); f.assets = as.value || as.error; out.ledger = as.ledger;
    const list = JSON.stringify(f.assets);
    f.lists_PYUSD = {stellar_sac: list.includes(PYUSD), other_symbol: list.includes('"PYUSD"')};
    f.lists_USDC = {stellar_sac: list.includes(USDC), other_symbol: list.includes('"USDC"')};
    f.lastprice = {};
    for (const [label, a] of [['Stellar(PYUSD)', {stellar: PYUSD}], ['Stellar(USDC)', {stellar: USDC}], ['Other(PYUSD)', {other: 'PYUSD'}], ['Other(USDC)', {other: 'USDC'}], ['Other(USD)', {other: 'USD'}]]) {
      const r = await view(id, 'lastprice', [asset(a)]); f.lastprice[label] = r.value === undefined ? r.error : r.value;
    }
    // history/twap/x_last_price omitted: they returned empty or MissingValue on 18 Nov and are not cited.
    out.feeds[name] = f;
  }
  console.log(JSON.stringify(out, null, 1));
})().catch(e => { console.error(e.message); process.exit(1); });
