# Isolated Testnet depth rehearsal

## September23 continuation (in progress)

The genuine-data replay started16:30:44UTC, run
`95f2890e-ace5-45ce-a9bf-8345f631baa5`. Completion is NOT yet established.
Fresh Mainnet public samples pass the unchanged guards. Before starting, the
explicit `calibrate` mode changed ONLY the unused Testnet quote stub to
0.998888741990XLM/yXLM from a fresh validated public sample. Transaction
`f49f557c805bf39614bb32c7d04b5b1b5198997284f0aab6ecfd82450f6727b8`
succeeded at4831487. The mode requires fresh phase, no prior observation/recovery,
the pinned disposable pool, independent fixture audit and venue agreement.
This is mock setup, NOT an independently priced Testnet market; it never follows
the market automatically during a run. No Mainnet or cloud configuration changed.

The first calibration attempt stopped before signing because Testnet RPC returned
a closed-ledger timestamp2seconds ahead of the local clock. The harness and
Testnet-only reporter now wait at most5seconds for that actual timestamp and then
recheck: future timestamps and ledgers older than60seconds still fail closed.
They do not widen price freshness or alter collection cadence.96keeper tests pass,
including no-wait, bounded wait, stalled/jumped clock and stale/future rejection.

Read-only cloud history through16:27:13UTC contains4267attempts since September20:
1249collected,1215agreeing,3018collection/validation failures,0missed slots and
833healthy of4237overlapping mature windows. Failures classify as2686spread,
331ratio and1generic depth guard. Last24hours:1086failures of1440attempts and
203healthy windows; last2hours:120successful agreeing samples and healthy windows.
The longest consecutive collection-failure streak is740minutes. Recent recovery
does NOT establish sufficient availability or liquidation safety for uncapped
lending. Cloud unsigned policy-v1 candidates are not interchangeable with this
policy-v2 Testnet publisher. No old history seeds the fresh local window.

Evidence: `/private/tmp/peridot-observer-{history,summary}-20260923.{log,json}`,
`/private/tmp/peridot-depth-calibrate-retry-20260923.log`,
`/private/tmp/peridot-depth-replay-20260923.log` and the ignored fixture journal.

Mainnet migration remains unsigned:16:34UTC preflight (64579095–97) previews
24.2152411XLM,4.8441242PYUSD and4.8463068USDC full exits. XLM harvest traps
before external calls while last_harvest1790179728 plus3600second cooldown is
still in the future (eligible17:08:48UTC). This matches the source's cooldown
guard; recheck after expiry without disabling it. Stable harvest previews pass.
Independent harvest/withdraw simulations do not prove sequential settlement,
historical-liability reconciliation, or an exit under non-parity market conditions.
No transaction or guard change was made on Mainnet.

## September21–22 results

Status: fixture deployed and independently verified; **live positive publication,
lost-response restart and governed recovery remain incomplete**. Mainnet and both
DigitalOcean workers are unchanged. No Mainnet fees or withdrawals occurred.

September22 continuation: committed/pushed as `9f0d4852388c97c80c93a47e7fcb354c2083b2bd`
on leveraged-fix. Fresh14:31UTC read-only audit again verifies all8setup successes,
all3code hashes, empty observation/recovery and no pending transaction. Fresh2mock
tests and94keeper tests pass. The live collector still rejects `depth/spread_guard`.
At14:32UTC the1000-unit spread was0.6941% and10000-unit spread1.1540%, above the
unchanged1% limit. No new live publisher run or transaction was started. These
point-in-time checks do not establish continuous availability/unavailability.
Current prices are also below the fixture's fixed0.966 quote; before a later
healthy-data replay, explicitly configure/review its Testnet-only mock reference.
Matching a mock quote is not independent real-market validation. No production
guard or price was changed to force a pass.
Almanax scan `ca75706b-5bf1-4961-9a3f-7e1994313518` overaf856a1..9f0d485 completed
with zero findings fetched. Only result documentation changed after scanned code;
this is not proof of live publication, economic safety or Mainnet readiness.

## Isolation and artifacts

The runner is `scripts/depth-testnet-rehearsal.mjs` in this keeper package. It uses
only the literal official Testnet RPC/passphrase and two newly generated identities
stored in the ignored `target/depth-testnet-replay/stellar` configuration directory
with0600permissions. No established Mainnet or shared-Testnet identity is loaded.
Never print/commit the identity files or copy these fixture addresses into frontend
Mainnet configuration.

| Role | Testnet address |
| --- | --- |
| Admin | GCRV2PW4LGWTOGUGQVVQJBRRJZICGPRSKMDFLBP6G2IKCFU4MUTTRT3G |
| Reporter | GCQFJG4JVPI4SLBOAHMQOO27JGA6II6NZWCVUY2B5L6TKLFDPON3HND7 |
| Router | CDTUAUSKRXUUOD44WZVCJUZHVAVEHPGA7PFGR4KWDIO6IS2A7KN4P6KZ |
| Mock asset metadata | CDTQF3ZNVGUX357IXJ5XQT2DXT6IU7EEDS5APQAWFCHHQLMQDHX53R2J |
| Mock quotes/upstream | CD7RGNVHWDDPVWLRTGGTK72WFJYYLW6H55MMTEICYMODZFHP23VLESWL |

The router contains the reviewed528d95c runtime, rebuilt with this dedicated
Testnet initialization admin. Its hash is
`3bbd58e9d571ab5fe4fa25b9accae6d00eeac7c1c42af110cdbbb164e3b08098`.
The new `mock-depth-fixture` hash is
`88cf3b042b447da722a73eac945615426626614339b710ef8b38397d2a413fe3`;
a separate post-format build matches. This stub is NOT a token or executable AMM:
it exposes decimal metadata, admin-set arithmetic quotes and a controlled0.25USD
upstream price. Its constructor and quote setter reject non-Testnet networks.
Two native tests cover quotes, authorization and Mainnet rejection; strict mock
library clippy and all94existing keeper tests pass.

## What actually ran

Eight setup transactions succeeded: two uploads, three deployments, router
initialization, observation configuration and required-observation binding. Public
hashes/ledgers are preserved in `target/depth-testnet-replay/state.json` and
`evidence.jsonl`. Independent RPC reads verify all eight SUCCESS statuses and all
three deployed WASM hashes. Bootstrap observation remains invalid with end0;
dependent price is unavailable, recovery absent, pending transaction null.

The real collector ran13:47:04–13:49:59UTC with idle sleep prevented. Three
minute-spaced attempts failed validation, with zero missed slots, zero healthy
samples/windows, and zero submitted oracle transactions. The publisher returned
hold. We stopped gracefully rather than replacing the real data with synthetic
history or relaxing guards. This short failed attempt is NOT a completed30-minute
availability soak. The original harness logged generic validation failures; the
follow-up preserves the collector's allowlisted diagnostic categories.

Independent direct quote diagnostics at13:50UTC:

| Probe | Bid XLM/yXLM | Ask XLM/yXLM | Spread |
| --- | --- | --- | --- |
|1000units|0.9635328049|0.9697872596|0.6470%|
|10000units|0.9595765201|0.9697876317|1.0585%|

The larger probe exceeds the unchanged1% spread maximum. An earlier direct
collector invocation explicitly returned `depth/spread_guard`. These are short
public snapshots, not a claim that the market will remain unavailable. Do not
change the guard merely to obtain a passing rehearsal.

The initial1-faucet-XLM fixture setup cap stopped initialization BEFORE signing
because its long storage TTL simulated at49.35faucet XLM. It was raised to a
bounded1000faucet-XLM setup-operation cap, strictly Testnet. No failed/unknown
transaction resulted from those preflight stops. The actual reporter adapter's
0.1XLM transaction cap is unchanged. Mainnet's conditional400XLM budget is untouched.

## Resuming and completion criteria

From the repository's `peridot-contracts` directory, after checking for another
running process:

```sh
node bots/aquarius-keeper/scripts/depth-testnet-rehearsal.mjs audit
CONFIRM_DEPTH_TESTNET=ISOLATED_DEPTH_REPLAY caffeinate -i \
  node bots/aquarius-keeper/scripts/depth-testnet-rehearsal.mjs run
```

`caffeinate` is a macOS idle-sleep inhibitor, not protection from lid closure or
network loss. Each process is bounded to approximately2hours; each restart creates
a NEW30-minute history. Unknown submissions require manual hash reconciliation;
never remove a journal/lock to force a retry. Secrets and signed envelopes never
enter public evidence. Preserve the original state rather than redeploying on
every attempt. Deployment is resumable and does not replay confirmed labels.

Intended live sequence (NOT yet passed):

1. Collect a complete genuine window, publish to the controlled Testnet pair,
   deliberately discard the send response, then stop with the public hash durable.
2. Run `run` again as a new Node process. The publisher reconciles that exact hash
   without resubmission. Restart promptly enough to verify the first price before
   its300-second expiry; otherwise stop for operator review.
3. Inject explicitly unavailable collector input: invalidate once and verify the
   dependent price disappears. Fixture admin begins governed recovery at the
   actual last accepted reference. Collect a NEW full30-minute real window.
4. Reporter publishes recovery and ordinary follow-ups. Dependent pricing remains
   unavailable until the separate fixture-admin completion after300seconds of
   report-end progression and all freshness/quote checks. Verify pricing resumes.
5. Independently audit transaction hashes, oracle post-state and journal, then
   record cadence and failure metrics. No Testnet lending positions are created;
   borrow/repay integration remains the separately documented compiled test.

The harness automates privileged steps only for this fixed disposable Testnet
fixture. It is not a production governance keeper. It does not invent a3% real
market move; that case remains covered natively, not by this live attempt.
