# Isolated depth publisher — implementation and release fence

September20,2026. Development/review only; Mainnet activation is not approved.
`depth-publisher-main.mjs` is a separate opt-in process, excluded from the keyless
observer Docker image. No cloud environment, signer, deployment or Mainnet state
was changed. No live Testnet fixture was deployed or transaction submitted in
this work. Local tests use actual SDK transaction assembly/signatures with random
ephemeral test keys and controlled RPC responses, not a real contract execution.

## Explicit Testnet replay configuration

The process takes no arguments. It requires all of:

- `CONFIRM_DEPTH_TESTNET=ISOLATED_DEPTH_REPLAY`.
- `DEPTH_TESTNET_MANIFEST`: path to a public JSON fixture manifest.
- `DEPTH_TESTNET_JOURNAL`: absolute path on durable local storage, in an existing
  operator-owned directory. One writer only; no ephemeral cloud filesystem.
- `DEPTH_TESTNET_REPORTER_SECRET`: dedicated isolated fixture secret, supplied
  securely to this process. Never use the Mainnet deployer or existing keeper.

The manifest contains `kind: "isolated-depth-price-replay-v1"`, the literal
Testnet network passphrase, `reporter`, `router`, `asset`, `quote`, `pool`,
`routerWasmHash`, `poolWasmHash` and `inIndex`(0or1). All addresses and hashes
must come from a separately deployed/reviewed isolated fixture; there are no
defaults to a shared deployment. Quote must be the Testnet native SAC. The mock
asset and quote must have7decimals; the router source must match the exact
reporter/pair/pool and existing1800s/300s/1% policy. Code hashes are mandatory.
Mainnet manifests are rejected before key loading; RPC is fixed to the official
Testnet endpoint and its reported network is checked again before signing.

Run from `bots/aquarius-keeper` only after provisioning that fixture:

```sh
node src/depth-publisher-main.mjs
```

The collector still reads the genuine Mainnet yXLM/XLM public depth and Aquarius
quotes. Publishing them to a **Testnet mock pair** is an explicitly labeled replay,
not discovery of a Testnet yXLM market. The fixture's executable quotes must agree
with the candidate. Do not silently replace the collector with synthetic history
or describe that fixture as production-market validation.

## Before each signature

1. Verify network, fresh non-regressing RPC ledger, pinned router/pool code,
   exact pool token order, decimals and every observation-policy field.
2. Reconcile any persisted public hash first. Read the actual router observation;
   missing/malformed state stops the process, never bootstraps silently.
3. Recompute the candidate from this process's raw bounded history. Failed or
   missed samples are retained. A restart requires a fresh30-minute window.
4. Cross-check both fresh pool quotes even inside the minimum publication
   interval; divergence can invalidate an existing valid observation immediately.
5. Simulate only `publish_observation` or `invalidate_observation`. Reject
   restoration, errors, any signer other than source-account auth, absent/extra
   auth entries, changed invocation arguments and every nested auth invocation.
6. Bound total fee to0.1XLM and lifetime to30seconds; publication expires no later
   than candidate-end+60seconds. Recheck configuration, code, actual observation,
   quote agreement, candidate freshness and transaction lifetime before signing.
7. Append/fsync a public-hash intent before signing/sending. Never persist an
   envelope or secret. Submit once and poll the exact hash. An unknown response,
   timeout or terminal failure stops the process; no automatic resubmission.

The optional observer callback is awaited outside the collection error handler.
It receives a bounded clone, cannot modify observer history, and cannot turn an
uncertain submission into a recoverable sample failure. Slow RPC/publication work
can cause real missed slots, which invalidate coverage; none are backfilled.
This implementation has no demonstrated live timing/SLA yet.
The current300-second minimum update interval equals the300-second on-chain
maximum age: it leaves no inclusion-delay headroom. Even a healthy collector can
have brief expiry gaps before the next report is included. This remains a policy
review item; no interval/expiry guard was silently changed here.

## Restart and failure handling

The append-only journal is scoped to the network, all manifest bindings, reporter
and pinned code hashes. It has an exclusive `.lock` file, fsynced records and a
1MiB size guard. A crashed process leaves its lock behind deliberately. Check the
process is gone and reconcile the public hash before clearing a stale lock. A
partial record, scope mismatch, full journal or recorded FAILED transaction needs
operator review; never delete/truncate it merely to restart. Preserve the original
as evidence and only archive/replace after every intent is independently resolved.
Do not run multiple processes using different journals for the same reporter.

`NOT_FOUND` is **not** proof that submission failed. A crash between journaling
and sending also leaves an unresolved intent by design. Restart checks a pending
hash, never resends it; only a confirmed SUCCESS clears it automatically. If RPC
retention has elapsed, external transaction-history reconciliation is necessary.
See Stellar's [sendTransaction](https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/sendTransaction)
and [getTransaction](https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getTransaction)
semantics. The CLI emits fixed error categories, not raw SDK exceptions.

| Event | Publisher action | Remaining limitation |
| --- | --- | --- |
| Failed/missed sample or restart | Invalidate an existing valid report once | RPC/chain failure can prevent invalidation; on-chain expiry still applies |
| Pool quote diverges | Invalidate once, no replacement report | Does not create executable liquidation depth |
| New complete healthy window, move within1% | Simulate and publish after freshness/replay checks | Honest reporter and executable small probes are still trusted |
| Genuine move exceeds1% | Invalidate/hold; no synthetic ramp | Same-policy reconfiguration preserves history; governed recovery is unresolved |
| Submission outcome unknown | Stop with durable public hash | Manual reconciliation may be required; never resubmit blindly |

## Evidence and remaining work

87keeper tests pass, including the11publisher/journal tests and2new observer
callback tests. The local72-slot end-to-end replay covers normal publication,
collector failure, one invalidation,30-minute recovery and no repeated watermark
movement. Separate9native router tests prove exact auth, expiry, replay checks,
0.5% genuine recovery and a3% move remaining halted even after reconfiguration.
No production Rust changed in this follow-up.

Still required: provision/review an isolated Testnet fixture and exercise actual
publication, restart and on-chain post-state reconciliation; prove live cadence;
review uncapped liquidation losses and a governed depeg-recovery policy; reproduce
the corrected non-parity legacy withdrawal with actual deployed dependencies;
restoration/all-stream/reward-route tests; final production artifacts, complete
fee simulations and explicit Mainnet activation approval. Current work neither
resolves those gates nor spends any of the approved400XLM fee ceiling.
