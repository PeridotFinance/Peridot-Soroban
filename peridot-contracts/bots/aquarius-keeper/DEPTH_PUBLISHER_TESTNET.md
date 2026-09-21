# Isolated depth publisher — implementation and release fence

Updated September21,2026. Development/review only; Mainnet activation is not approved.
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
reporter/pair/pool and1800s window,120s minimum update,300s expiry and1% quote
deviation policy. Normal step allowance is prorated over300s (40bps at120s),
capped at100bps even after an outage. Code hashes are mandatory.
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
2. Reconcile any persisted public hash first. Read the actual router observation
   and recovery approval (including its exact source-configuration snapshot);
   missing/malformed state stops the process, never bootstraps silently.
3. Recompute the candidate from this process's raw bounded history. Failed or
   missed samples are retained. A restart requires a fresh30-minute window.
4. Cross-check both fresh pool quotes even inside the minimum publication
   interval; divergence can invalidate an existing valid observation immediately.
5. Simulate only `publish_observation`, `publish_recovery_observation` or
   `invalidate_observation`. Recovery requires an existing admin approval and a
   complete post-approval window. Never sign an admin action. Reject
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
The development policy now permits publication every120seconds with unchanged
300-second expiry: nominal180-second inclusion headroom, NOT an availability SLA.
Faster publication prorates the old price-step allowance instead of granting1%
per120seconds. Existing live deployments and cloud schedules are unchanged.

## Governed large-move recovery (validation networks only)

These four mutation entrypoints explicitly reject the Public network. The reporter
cannot approve a new reference, change guard settings or resume lending itself.

1. The existing router admin calls `begin_observation_recovery(caller, asset,
   reference_ratio)`, using the1e12 ratio scale. Prior published history is required;
   this is not bootstrap. Approval snapshots the full observation configuration,
   expires after7200seconds and immediately locks dependent pricing.
2. Collect an entirely new1800-second healthy window starting at/after approval.
   The reporter calls `publish_recovery_observation` once, retaining all window,
   bounds, auth, replay and real two-way pool quote checks. The truthful new price
   must be within1% of the approved reference. Only the old-price step check is
   bypassed; no invented intermediate prices or widened guards.
3. Continue ordinary guarded publication. Dependent prices remain unavailable,
   including after the first successful recovery report. At least300seconds of
   report-end progression must follow that first accepted report. This is a
   review delay, not proof of continuous on-chain observations during the delay.
4. The same approver, still the current admin, calls `finish_observation_recovery`.
   Latest report must be valid and<=60seconds old, upstream fresh, current pool
   quotes agreeing, source configuration unchanged and approval unexpired. Only
   successful completion removes the pricing lock. With120s publication spacing,
   the earliest normal scheduled completion is typically360seconds after step2.

`cancel_observation_recovery` and expiry NEVER resume pricing. A new admin proposal
restarts the full window. Configuration changes require a new approval. Admin
rotation cannot inherit completion authority; the new admin must re-propose.
Admin governance retains its existing power to replace sources/dependencies/code;
these checks do not protect against a malicious fully privileged admin.

**Borrowing AND liquidation/health-dependent actions are blocked during recovery.**
Repayment remains available. This intentionally avoids liquidating from an
unreviewed replacement price but can increase bad debt during a depeg. It does
not solve uncapped liquidation-risk or oracle availability. The keeper has no
admin key; proposal/completion are separate governance operations, not automated.

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
| New complete healthy window, move within prorated step allowance | Simulate and publish after freshness/replay checks | Honest reporter and executable small probes are still trusted |
| Genuine move exceeds allowance | Invalidate/hold; admin-approved recovery can replace it after a new full window | Remains paused until separate admin completion; Mainnet recovery is fenced |
| Submission outcome unknown | Stop with durable public hash | Manual reconciliation may be required; never resubmit blindly |

## Evidence and remaining work

September20 baseline:87keeper tests passed, including the11publisher/journal tests and2new observer
callback tests. The local72-slot end-to-end replay covers normal publication,
collector failure, one invalidation,30-minute recovery and no repeated watermark
movement. Separate9native router tests prove exact auth, expiry, replay checks,
0.5% genuine recovery and a3% move remaining halted even after reconfiguration.
No production Rust changed in this follow-up.
The full router suite passes39tests, and all7existing compiled LP validation
tests were rerun successfully (pinned validation artifacts, not final production
or Mainnet-state validation). Commitfcfb91c3351d90c43c6dee8d888fea492758deed is
pushed to leveraged-fix. Almanax scan0b74548a-592f-4776-9b58-bf5ecb500673 over
cd07141..fcfb91c completed with zero findings fetched. That scan covers the
September20 code, not the September21 recovery changes; it does not certify
economic safety or remove the release fences.

September21 validation:94keeper tests,45native router tests and8explicit compiled
LP tests pass. Added native3% governed recovery, exact authorization, cancellation,
expiry/configuration/quote/freshness rollback and Public-network rejection tests.
Compiled repayment/recovery/resumed borrowing uses actual Aquarius pool WASM with
controlled local state at parity, not a3% live-market or Mainnet migration test.
New router validation WASM SHA256:
`cf6a9ec566350b74be7787eabab6c2cfd669c58d07762bdb188e79578f58ffd8`,
isolated under `target/depth-recovery-validation/artifacts/price_router.wasm`.
The existing receipt/controller/strategy validation artifacts were not replaced.
All compiled tests keep250entry/200MCPU/40MiB limits enabled. Recovery publication
uses11entries/15,800,694CPU/4,623,595bytes; completion13/15,955,862/4,700,406;
resumed borrowing230/184,843,956/34,765,881. Full workspace regression passes;
router clippy passes with the pre-existing `option_env_unwrap` lint allowance
(an unqualified `-D warnings` fails that existing initialization macro).
No new live Testnet deployment/signature, cloud change or Mainnet action occurred.

Still required: provision/review an isolated Testnet fixture and exercise actual
publication, restart and on-chain post-state reconciliation; prove live cadence;
review uncapped liquidation losses and this governed depeg-recovery policy; reproduce
the corrected non-parity legacy withdrawal with actual deployed dependencies;
restoration/all-stream/reward-route tests; final production artifacts, complete
fee simulations and explicit Mainnet activation approval. Current work neither
resolves those gates nor spends any of the approved400XLM fee ceiling.
