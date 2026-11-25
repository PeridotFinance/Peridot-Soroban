// Read-only (WP8): the admin- and guardian-specific Mainnet entries the ceremony touches, for the wp8 fork test.
// Never signs, never reads keys. Only simulateTransaction / getLedgerEntries. The footprints of these read-only
// simulations (admin as source) give the exact keys: admin USDC-receipt deposit, admin USDC-pToken transfer to the
// pinned margin id, the same two on the XLM receipt (the canary returns XLM pTokens to the admin) (the seed's nested transfer), admin XLM / USDC SAC transfers to it (canary collateral), and the
// admin / guardian accounts. Entries are fetched at the CURRENT ledger (the fork may be a little older).
//   cd tools && node fetch-wp8-entries.cjs --out ../.scratch/wp8-entries.json
'use strict';
const fs = require('node:fs');
const S = require('@stellar/stellar-sdk');
const RPC = 'https://soroban-rpc.mainnet.stellar.gateway.fm', NETWORK = S.Networks.PUBLIC;
const ADMIN = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP', GUARDIAN = 'GBSK3FYCGNYZLHHPSYRJNE53NZXDFL6ED24KLT3GUUETLFKDAUD7PQX3';
const MARGIN = 'CA6Q7HULJTGFMTB7YLJWFKPQVIIWAP46PQHAY3H5V2EZ2R7HBCW6N7NZ';
const UR = 'CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F', XR = 'CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS', USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const rpc = async (method, params) => { const r = await (await fetch(RPC, {method: 'POST', headers: {'content-type': 'application/json'},
  body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json(); if (r.error) throw Error(JSON.stringify(r.error)); return r.result; };
const a = x => S.nativeToScVal(x, {type: 'address'}), n = (x, t) => S.nativeToScVal(BigInt(x), {type: t});
const accountKey = g => S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: S.Keypair.fromPublicKey(g).xdrAccountId()}));
(async () => {
  const out = process.argv[process.argv.indexOf('--out') + 1];
  const acc = new S.Account(ADMIN, '0'), keys = new Map([[accountKey(ADMIN).toXDR('base64'), 1], [accountKey(GUARDIAN).toXDR('base64'), 1]]), sims = [];
  for (const [c, f, args] of [[UR, 'deposit', [a(ADMIN), n(10_000_000, 'u128')]], [UR, 'transfer', [a(ADMIN), a(MARGIN), n(7_500_000_000, 'i128')]],
    [XLM, 'transfer', [a(ADMIN), a(MARGIN), n(410_000_000, 'i128')]], [XR, 'deposit', [a(ADMIN), n(10_000_000, 'u128')]], [XR, 'transfer', [a(ADMIN), a(MARGIN), n(10_000_000, 'i128')]], [USDC, 'transfer', [a(ADMIN), a(MARGIN), n(50_000_000, 'i128')]]]) {
    const tx = new S.TransactionBuilder(acc, {fee: '100', networkPassphrase: NETWORK}).addOperation(new S.Contract(c).call(f, ...args)).setTimeout(0).build();
    const r = await rpc('simulateTransaction', {transaction: tx.toXDR()});
    if (r.error) throw Error(`${f} on ${c}: ${r.error.slice(0, 200)}`);
    const fp = S.xdr.SorobanTransactionData.fromXDR(r.transactionData, 'base64').resources().footprint();
    for (const k of [...fp.readOnly(), ...fp.readWrite()]) keys.set(k.toXDR('base64'), 1);
    sims.push({contract: c, fn: f, latestLedger: r.latestLedger});
  }
  const ks = [...keys.keys()], entries = [];
  for (let i = 0; i < ks.length; i += 100) {
    const r = await rpc('getLedgerEntries', {keys: ks.slice(i, i + 100)});
    for (const e of r.entries || []) entries.push({key: e.key, xdr: e.xdr, lastModified: e.lastModifiedLedgerSeq, liveUntil: e.liveUntilLedgerSeq ?? null});
  }
  const res = {schema: 'wp8-entries-v1', fetchedAt: new Date().toISOString(), latestLedger: (await rpc('getLatestLedger')).sequence, requestedKeys: ks.length, sims, entries};
  fs.writeFileSync(out, JSON.stringify(res, null, 1) + '\n');
  console.log(JSON.stringify({out, requestedKeys: ks.length, found: entries.length, latestLedger: res.latestLedger}));
})().catch(e => { console.error(e.message); process.exit(1); });
