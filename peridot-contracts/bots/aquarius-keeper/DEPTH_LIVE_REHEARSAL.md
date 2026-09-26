# Isolated Testnet depth rehearsal

## September26 verification: expired-report recovery complete

This supersedes the historical running status below. The one-shot finished
September25 at18:51:08UTC after38/38 collected and agreeing samples,0missed slots,
0collection failures and8healthy mature windows. Admin finish transaction
`b3a1ecef8e7a39fdcd4f6f3673bcab3af8d0a19fd4b12742ab9d65f22e1a8fe5`
succeeded at ledger4867736; recovery cleared and the dependent mock price was
available. Independent September26 19:14:08UTC audit verifies all17recorded
transactions, phasecomplete, pendingnull and recoverynull. The price is now absent
because the finished one-shot's report expired, not because recovery failed.
Evidence: `/private/tmp/peridot-depth-final-audit-20260926.log` and the service log
below. Verified launchd exit0; completed job has been unloaded.

`timelyRestartVerified:false` remains correct: the original timely lost-response
restart was NOT demonstrated. This completes only the expired-report governed
recovery in the controlled Testnet fixture, not actual lending positions,
Mainnet migration or uncapped economic/liquidation review. Do not reset this
completed fixture's journal or rerun it as a fresh test. Runtime remains scanned
`85789496eb498f1c8cb53ca0a02d551ea4e1aef6`; no new runtime changes or transactions.

User approved a Droplet costing at most$10/month and secure transfer of the
dedicated Testnet reporter/admin keys for Testnet-only signing. Provisioning is
blocked: the `peridot` DigitalOcean context returns403 for GET /v2/droplets.
No Droplet created, key transferred or new cloud spending initiated. Existing
production keeper and keyless observer are unchanged. Update scoped DO access
before provisioning; preserve durable journals and separate Testnet identities.

## September25 live recovery status

18:14UTC update: the first recovery process stopped without a shutdown record
after16good samples (last18:07:45). No process remained; cause unknown. Independent
audit18:11:58 confirmed all12transactions SUCCESS, no unresolved journal/fixture
hash, unchanged recovery and unavailable price. Stale empty locks were archived
(not the journal) only after that reconciliation. A detached-shell attempt did not
survive. A temporary `launchctl submit` job was removed cleanly after1sample/no
transactions because it did not explicitly disable restart.

The same scanned runtime is now a one-shot user launchd job:
`gui/501/com.peridot.depth-recovery-20260925`, with explicit RunAtLoad=true,
KeepAlive=false (verified OnDemand=true), configured in ignored
`target/depth-testnet-replay/com.peridot.depth-recovery-20260925.plist`.
It starts a NEW30-minute window; no history, approval, guard or price was reset.
Log `/private/tmp/peridot-depth-recovery-service-20260925.log`. Existing recovery
approval expires19:52:42UTC. Completion/failure must be checked, not assumed.
Afterward unload with `launchctl bootout gui/501/com.peridot.depth-recovery-20260925`.
Do not run another copy while active. Laptop lid closure/network loss can still
interrupt collection; the job only prevents idle sleep and terminal-lifetime loss.

Runtime commit `85789496eb498f1c8cb53ca0a02d551ea4e1aef6` is pushed and
remote-verified on leveraged-fix.104keeper tests pass; Almanax scan
`bb4df603-e6f1-4c86-b6d8-2d3575196965` COMPLETE with zero findings fetched.
No runtime changes after that scan.

The resumed Testnet process reconciled the original publication without resending
and recorded `timelyRestartVerified:false`. Invalidation
`d6badccfde92eda145a2a96e50447b665e4048b24a11bd43521c221ab0a959ec`
and admin begin-recovery
`a4669185724f674b7c18ef7bd4e21ec35fb877d9e4114ee2b1ba499f91c7ae7d`
confirmed. New observer run `7bf25542-c2a1-4e38-807a-9c22c5fa281d` began
17:52:43UTC. At18:07:45UTC16/16samples pass,0misses/0failures, still warming.
Pricing remains unavailable while recovery is pending. Completion is NOT yet
established. Log `/private/tmp/peridot-depth-recovery-20260925.log`; the runner
audits on completion. Do not start a second runner or reset the journal.

Read-only cloud refresh through17:51:13UTC:7231attempts,3989collected,
3955agreeing,3242failures,0missed,3361healthy overlapping windows. Latest24h:
1216/1440samples collected/agreed,224failures (220spread,3Aquarius guard,
1depth transport),1004healthy windows, longest failure streak77minutes. Latest2h:
120/120successful agreeing samples and healthy windows. Improvement does not
establish liquidation availability or uncapped economic safety. Existing observer
deployment/configuration remains unchanged; no Mainnet fees or funds moved.

## September25 restart repair and explicit expired recovery

The September23 first process collected32/32valid agreeing samples, no failures
or missed slots, and published a genuine30-minute window. Transaction
`c6f60bdab87005a5f5399da4e5f80fb2187af2c39980d945361bfa9b59ef41d6`
is independently confirmed SUCCESS at4831865,17:01:52UTC. The supervisor's new
process had already failed an assertion at17:01:51.734UTC. This supports an
inclusion/ingestion race; the original log did not preserve the exact assertion.
It did NOT complete restart verification or recovery. No processes/locks remained
on September25. The retained observation was expired and dependent pricing absent.

The Testnet publisher now polls ONLY the recorded hash for at most20reads with
1second waits, and stops initiating reads after30seconds elapsed (an in-flight
RPC request remains subject to its own timeout). It never resends, rebuilds or
switches hashes. Timeout keeps the intent and stops the process; FAILED resolves
as failure and blocks further work. RPC errors/hash mismatches still stop safely.
Regressions cover reopened durable journal/delayed SUCCESS, delayed FAILED,
permanent NOT_FOUND, RPC/hash errors and elapsed-time bounds.104keeper tests pass.

An explicit `recover-expired` mode is separate from the normal timely-restart
test. It verifies the exact successful envelope/hash/reporter/router/method/args
against the retained observation, Testnet network, absent recovery, expired report
and unavailable dependent price. It records `timelyRestartVerified:false`, then
invalidates and starts governed recovery without altering history, mock quote,
pricing guards or policy. A NEW real30-minute window and subsequent report
progression/admin finish remain mandatory. Ordinary `run` still refuses to claim
a timely restart from an expired report. Fixed diagnostic stage labels preserve
failure location without logging raw SDK/key errors. Completion now audits itself.

After review, from peridot-contracts (only one runner):

```sh
CONFIRM_DEPTH_TESTNET=ISOLATED_DEPTH_REPLAY caffeinate -i \
  node bots/aquarius-keeper/scripts/depth-testnet-rehearsal.mjs recover-expired
```

As of implementation, no resumed transaction yet. Read-only exact-envelope
verification passes on the confirmed transaction. Fresh17:46UTC actual collector
passes unchanged guards at0.997773697119XLM/yXLM; that is one point, not a window.
Current scoped scan and live recovery results must be recorded before claiming
completion. Mainnet/cloud unchanged;400XLM ceiling untouched.

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

The clock/calibration change is committed/pushed as
`e1d0ab3c3dc9d8e955fd7c940a482b5e5a4360a9` on leveraged-fix. Almanax scan
`823f7a3b-48a7-42be-9138-3ed7081bd24d` completed with zero findings fetched.
All8explicit compiled LP checks pass again against pinned validation artifacts
and freshly downloaded exact Mainnet pool code12fca5a7...ee6. Controlled local
state/dependencies remain: these are not production artifacts or live positions.

### Optional bounded automatic new-process handoff

While the first `run` is active, an operator may attach ONE supervisor:

```sh
CONFIRM_DEPTH_TESTNET=ISOLATED_DEPTH_REPLAY caffeinate -i \
  node bots/aquarius-keeper/scripts/supervise-depth-testnet-replay.mjs
```

It pins this disposable fixture's network/accounts/addresses/code, takes its own
exclusive lock and waits up to7500seconds for `response_lost` AND release of the
first process's lock. It starts one NEW runner process, which independently
reconciles the recorded hash before outage/recovery. It never imports history,
retries a stopped/failed run, deletes another process's lock, resets a journal or
signs itself. After successful completion it invokes the independent read-only
audit. Unexpected state/config/pending setup hash, missing lost hash, clock
regression or timeout stops it.99keeper tests include three supervisor regressions.
Do not attach a manual second runner while this supervisor is active.

Supervisor commit `b563f0315de973e5b41bce6cffd9bdb950c3b774` was pushed and
independently remote-verified. Almanax scan
`adb1f587-5abf-4489-8214-4eaee1885262` completed with zero findings fetched.
The supervisor attached16:43:50UTC to the existing first runner; log
`/private/tmp/peridot-depth-supervisor-20260923.log`. It is RUNNING, not a
completed rehearsal. First process has14/14healthy samples at16:43:48UTC,
0missed/0failures; no publication yet. New-process recovery and the final audit
will appear in the supervisor log, also appended to the fixture public evidence.
Latest full workspace:686unit+3doc pass,13intentional skips. No code changed
after the two scans; subsequent edits record evidence only.

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
