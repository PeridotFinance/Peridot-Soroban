// Public read-only contract-code retrieval; no identity, signer or transaction API.
const { rpc, xdr } = require('../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const fs = require('node:fs');
const crypto = require('node:crypto');
const hash = '12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6';
async function main() {
  const server = new rpc.Server('https://rpc.lightsail.network/', { timeout: 20 });
  const network = await server.getNetwork();
  if (network.passphrase !== 'Public Global Stellar Network ; September 2015') throw Error('wrong network');
  const result = await server.getLedgerEntries(xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({ hash: Buffer.from(hash, 'hex') })));
  if (result.entries.length !== 1) throw Error('pinned pool code unavailable');
  const bytes = result.entries[0].val.contractCode().code();
  if (crypto.createHash('sha256').update(bytes).digest('hex') !== hash) throw Error('pool hash mismatch');
  const dir = 'target/cl-share-checkpoint';
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(`${dir}/aquarius-pool.wasm`, bytes);
  const metadata = { scope: 'public code only; no live pool state imported', hash, bytes: bytes.length, ledger: result.latestLedger, network };
  fs.writeFileSync(`${dir}/pool-code-source.json`, JSON.stringify(metadata, null, 2) + '\n');
  console.log(JSON.stringify(metadata));
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
