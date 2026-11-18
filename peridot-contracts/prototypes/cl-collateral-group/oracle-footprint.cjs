// READ-ONLY: simulateTransaction footprints of the controller's three oracle reads (lastprice, decimals,
// resolution) on the two candidate Reflector feeds for USDC. No signing, no submission, no keys.
'use strict';
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const rpc = new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm', {timeout: 60});
const SRC = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const asset = a => a.stellar ? S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'), new S.Address(a.stellar).toScVal()]) : S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(a.other)]);
async function fp(c, m, args) {
  const tx = new S.TransactionBuilder(new S.Account(SRC, '0'), {fee: '100', networkPassphrase: S.Networks.PUBLIC}).addOperation(new S.Contract(c).call(m, ...args)).setTimeout(0).build();
  const sim = await rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) return {error: String(sim.error).slice(0, 160)};
  const r = sim.transactionData.build().resources(); const f = r.footprint();
  return {ledger: sim.latestLedger, cpu: r.instructions(), readOnly: f.readOnly().length, readWrite: f.readWrite().length,
    keys: f.readOnly().map(k => k.switch().name + (k.switch().name === 'contractData' ? ':' + S.StrKey.encodeContract(k.contractData().contract().contractId()) + ':' + k.contractData().durability().name : ''))};
}
(async () => {
  const out = {observedAt: new Date().toISOString(), method: 'simulateTransaction footprint (read-only)'};
  out.CALI_lastprice_Stellar_USDC = await fp('CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M', 'lastprice', [asset({stellar: USDC})]);
  out.CALI_lastprice_Stellar_PYUSD = await fp('CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M', 'lastprice', [asset({stellar: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2'})]);
  out.CAFJ_lastprice_Other_USDC = await fp('CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN', 'lastprice', [asset({other: 'USDC'})]);
  for (const [n, id] of [['CALI', 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M'], ['CAFJ', 'CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN']])
    for (const m of ['decimals', 'resolution']) out[`${n}_${m}`] = await fp(id, m, []);
  console.log(JSON.stringify(out, null, 1));
})().catch(e => { console.error(e.message); process.exit(1); });
