# JS SDK and Protocol 30 — note for the frontend (24 Nov 2026, updated 25 Nov)

Stellar Mainnet and Testnet run **protocol 30** since 23 Nov. Its new ledger-header format breaks
`@stellar/stellar-sdk` **16.3.x and older** on every call that decodes a ledger header:
`rpc.getLatestLedger()`, `rpc.getLedgers()`, `getAssetBalance()` for G-addresses, `AssembledTransaction.signAuthEntries()`
without an explicit expiration, and any `LedgerHeader` / `LedgerCloseMeta` `fromXDR` (incl. Horizon `header_xdr`).
Error: `XDR Read Error: unknown StellarValueType member for value 3`.
SDK 13.x is worse: `getTransaction(s)` / `pollTransaction` already fail since protocol 23.

## What to use

| You are on | Do |
|---|---|
| **17.4.0** (the live `peridot-markets` app today) | **Nothing.** Verified: `getLatestLedger` header decodes. Don't downgrade. |
| 17.0–17.2.x | Upgrade to **17.4.0** (17.3.0 is the first fixed 17.x). |
| 16.x | Upgrade to **16.4.1** — drop-in, same XDR API (16.4.0 is the fix; 16.4.1 adds an axios security bump). |
| 13.x–15.x (e.g. the legacy `peridot-dashboard`) | Move to **16.4.1** or 17.4.0; test `getTransaction` / `pollTransaction` paths. |

Don't mix: 17.x rebuilt `xdr` (plain objects instead of `.switch()` / `.header()` accessors, `Uint8Array` instead of
`Buffer`, Node ≥ 22.12, auth v2 default). Code written against 16.x `xdr.*` accessors needs changes on 17.

## Workaround if you can't upgrade yet

Read the latest ledger as raw JSON-RPC (no header decode):

```js
async function latestLedger(rpcUrl) {
  const r = await (await fetch(rpcUrl, {method: 'POST', headers: {'content-type': 'application/json'},
    body: JSON.stringify({jsonrpc: '2.0', id: 1, method: 'getLatestLedger'})})).json();
  if (r.error) throw new Error(JSON.stringify(r.error));
  return {sequence: r.result.sequence, closeTime: Number(r.result.closeTime), protocolVersion: r.result.protocolVersion};
}
```

`simulateTransaction`, `assembleTransaction` / `prepareTransaction`, `sendTransaction`, `getTransaction`,
`getEvents`, `getLedgerEntries`, `getAccount`, `getNetwork`, `getFeeStats` work on 16.3.x.

## Two protocol-30 details for the UI

- **Muxed addresses (CAP-84):** accept only **G…** addresses as the user account. Reject `M…` / `W…` with a clear
  message — every lending call (deposit, withdraw, borrow, repay, enter_market) takes a plain address and rejects
  muxed ones. SDK 16.4.x `Address` / `scValToNative` also throw on `W…`.
- **Timestamps (CAP-88):** two consecutive ledgers can now have the same whole-second `closeTime`. Don't use
  `closeTime` to detect "a new ledger" or as a unique key — use the ledger `sequence`.

## Update 25 Nov: verified status

- **App (`peridot-markets`): OK.** `app/package-lock.json` locks 17.4.0 on `main` and every remote branch; the wallet kit
  (`@creit.tech/stellar-wallets-kit` 2.7.1) dedupes to the same 17.4.0; `@stellar/freighter-api` 6.x carries no SDK.
  A read-only Mainnet probe with the app's own install decodes the protocol-30 header (`baseReserve` 5000000,
  `baseFee` 100, as `trustline.ts:82` needs). Header call sites `rpc.ts:193,238`, `control-client.ts:77`,
  `trustline.ts:72/82` are fine on 17.4.0. User addresses are already G-only; `closeTime` is only a 120 s staleness bound.
- **Group-1 keeper (`keeper/liquidator/margin`): broken by code.** It locks 16.3.1 on main, demo-merge,
  keeper-all-groups and keeper-inclusion-fee, and calls `getLatestLedger()` every cycle (`stellar.mjs:61`). Reproduced
  with its own install. Fix: 16.4.1, or the raw `getLatestLedger` above. Check its journal first.
- **Legacy `peridot-dashboard`** is on 13.3.0 (also breaks `getTransaction`/`pollTransaction`); no deployment found.
