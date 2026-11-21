// READ-ONLY rent quote for extendFootprintTtl on Mainnet (simulateTransaction only; no signing, no sending).
//   node ttl-quote.cjs '<json [{label, code?:sha256, instance?:C...}]>' <extendTo ledgers> [--out f.json]
'use strict';
const fs = require('node:fs');
const S = require(process.env.STELLAR_SDK_DIR || '/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const RPC = 'https://soroban-rpc.mainnet.stellar.gateway.fm', NET = S.Networks.PUBLIC, ADMIN = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
const rpc = new S.rpc.Server(RPC, {timeout: 60});
const codeKey = h => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(h, 'hex')}));
const instKey = c => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(c).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
async function latest() { const r = await (await fetch(RPC, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method: 'getLatestLedger'})})).json(); return r.result.sequence; }
async function quote(row, extendTo) {
  const key = row.code ? codeKey(row.code) : instKey(row.instance);
  const e = await rpc.getLedgerEntries(key); if (!e.entries.length) return {...row, error: 'entry absent'};
  const size = e.entries[0].xdr ? Buffer.from(e.entries[0].xdr, 'base64').length : null, liveUntil = e.entries[0].liveUntilLedgerSeq;
  const acc = await rpc.getAccount(ADMIN), seq = await latest();
  const data = new S.SorobanDataBuilder().setReadOnly([key]).build();
  const tx = new S.TransactionBuilder(acc, {fee: '10000', networkPassphrase: NET}).setSorobanData(data)
    .addOperation(S.Operation.extendFootprintTtl({extendTo})).setTimeout(0).build();
  const sim = await rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) return {...row, error: String(sim.error).slice(0, 200)};
  const added = Math.max(0, seq + extendTo - liveUntil);
  return {...row, ledger: sim.latestLedger, entryXdrBytes: size, liveUntil, remaining: liveUntil - seq, extendTo, ledgersAdded: added,
    minResourceFeeXlm: Number(sim.minResourceFee) / 1e7, stroopsPerKbPerMillionLedgers: added && size ? Number(sim.minResourceFee) / (size / 1024) / (added / 1e6) : null};
}
(async () => {
  const rows = JSON.parse(process.argv[2]), extendTo = Number(process.argv[3]), out = [];
  for (const r of rows) { out.push(await quote(r, extendTo)); await new Promise(z => setTimeout(z, 300)); }
  const i = process.argv.indexOf('--out'); if (i > 0) fs.writeFileSync(process.argv[i + 1], JSON.stringify({observedAt: new Date().toISOString(), rows: out}, null, 1) + '\n');
  console.log(JSON.stringify(out, null, 1));
})().catch(e => { console.error('FAILED', e.message); process.exit(1); });
