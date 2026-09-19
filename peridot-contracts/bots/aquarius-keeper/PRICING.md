# LP pricing observer (not activated)

The September18 user-approved design is implemented separately from the existing
live keeper entrypoint. `node src/price-main.mjs` runs one **read-only** Mainnet
observation; `--loop` repeats every60seconds. It loads no secret in observation
mode, signs nothing and makes no changes to DigitalOcean or the deployed keeper.

The collector resolves and verifies the yXLM SAC issuer against its contract ID,
checks the native-XLM quote and actual pool token order, checks both RPC/Horizon
network identities and Horizon freshness, and requests six CLOSED five-minute
SDEX trade-aggregation buckets. It computes each bucket's volume-weighted
XLM/yXLM ratio with integer arithmetic, then equally weights these six intervals.
Missing buckets are not filled; the current spot quote is never substituted.
Both directions of a100-unit Aquarius executable quote must agree with that
window. The pool is a cross-check, never averaged into the SDEX price.

Candidate policy (requires exposure/liquidity review before Mainnet activation):
30-minute window; each bucket>=3trades and>=10yXLM; window>=1000yXLM; every bucket
and both pool directions within1% of the window; ratio between0.80 and1.05XLM;
report age<=300seconds; updates at least300seconds apart and step<=1%.
Sparse trading, low volume, fast price changes or depegs can deliberately halt
pricing. Do not lower these thresholds merely to obtain a price. The first live
read-only run on September18 stopped with `insufficient bucket volume`.

Follow-up at02:48UTC September19: over the preceding24hours only27/283 overlapping
windows passed SDEX policy checks.181 failed per-bucket volume,8 total volume,
15 trade count and52 missing buckets. This excludes historical Aquarius checks
and is NOT manipulation-resistance evidence. Current candidate policy is not
suitable for reliably available borrowing; additional reference/policy review
is required before activation. Code commit991ffff passed Almanax scan
`a73c307f-c911-4ad6-b255-de0e3825d8b2` with zero findings; that does not validate
the economic safety or availability of a chosen pricing policy.

## Read-only availability and depth research — September19

From this directory:

```sh
node src/price-audit.mjs 7
node src/price-shadow.mjs
```

The first command checks one to seven days of public trade aggregations, using
bounded twelve-hour requests below the page limit. It rejects malformed,
duplicate and out-of-range records; it never fills missing buckets. Its JSON
includes normalized observations and their SHA256 for independent reproduction.
The second command collects **one** two-way classic execution-quote sample at
1000 and10000 units, then simulates unsigned two-way100-unit Aquarius quotes.
Neither command accesses keys, signs, submits, loops or changes the live keeper.

At13:29UTC September19, the seven-day interval September12 13:25 through
September19 13:25 contained1994/2016 buckets and1,690,025.0419071yXLM of volume.
Only544/2011 overlapping windows passed (27.05%); the longest consecutive failing
window run spanned12h10m. The top10% of observed buckets accounted for82.61% of
volume. First-rejection counts:1190 bucket-volume,81 total-volume,89 trade-count,
107 incomplete windows. These results exclude historical Aquarius agreement and
publication step/uptime checks, so actual combined availability may be lower.
The raw-data report is `/private/tmp/peridot-yxlm-pricing-audit-20260919.json`;
normalized observations SHA256:
`5f4f1b2f08c1a0aef314d2a4ba686d69b02d57d39c8910d6597b8b1402a04da2`.

The depth prototype requires exact asset identity, a direct route, positive
two-sided liquidity, <=1% spread, <=0.5% midpoint change between probe sizes,
bounded ratio and coherent fresh ledgers. Classic paths may draw on the orderbook
or classic AMMs; they are not proof of independently owned liquidity, locked
liquidity or actual fills. `depthWindow` evaluates31–64 samples over30minutes,
with <=90-second gaps, increasing ledger IDs, integer trapezoidal time weighting
and <=1% per-sample deviation. It is tested with synthetic history only. All
outputs explicitly retain `publicationEligible:false`; nothing imports this
reference into the publisher or modifies the approved trade-based implementation.

One live combined sample at13:36UTC used the same Horizon/RPC ledger64507836.
The classic large-probe midpoint was0.979304495473XLM/yXLM and both Aquarius
directions agreed within1%. Evidence:
`/private/tmp/peridot-yxlm-depth-aquarius-shadow-20260919.json`. This is NOT a
collected30-minute window or a continuous availability/manipulation-resistance
result. No shadow daemon or pricing service was started. Issuer redemption
claims are not a substitute for an independently validated market price.

Next: continuous depth/Aquarius shadow collection, economic manipulation and
exposure review, then an explicit reviewed policy decision before any publisher
change. Full compiled oracle+pool/gauge/route validation and outage/liquidation,
restoration, funding and migration gates remain. The additional research suite
passes49 keeper tests in total; Rust artifacts and production behavior unchanged.
Code commit `5a38639` is pushed on `leveraged-fix`. Almanax scan
`3246cd80-0bb2-46ce-901d-45b5076f184c` over `b09defc..5a38639` completed with
zero findings and no triage. This is a code-diff result, not validation of the
economic safety or continuous availability of a potential replacement oracle.

References: [strict-send paths](https://developers.stellar.org/docs/data/apis/horizon/api-reference/list-strict-send-payment-paths)
and [path-payment venues](https://developers.stellar.org/docs/build/guides/transactions/path-payments).

## Bounded continuous shadow runner

```sh
node src/price-soak.mjs 35
```

Schedules one observation per minute for32–120minutes (default35), including
the initial sample. Host suspension can extend elapsed wall time; overdue slots
are recorded as missed after resumption, not sampled retroactively. Each child
invokes the existing read-only price-shadow.mjs;
only PATH is inherited, with no keeper/deployer credentials. A45-second subprocess
timeout kills stalled collection. There is no signing, publishing, deployment or
automatic restart. A clock jump aborts; late slots and failed samples are retained,
never backfilled. Every JSONL record is explicitly non-publishable, and a final
digest covers all preceding JSONL lines. No digest/summary means an incomplete run.

The evaluator retains the boundary point and every attempted slot in a30-minute
window. It rechecks both Aquarius directions for each point and against the window
average. It rejects gaps, replayed ledgers, failed samples and venue divergence;
agreement flags saved in input are never trusted. Summary denominators include
all scheduled mature attempts, including failures and boundary warm-up. Overlapping
windows are not independent statistical trials. Tests now total56 keeper tests.

The first run started September19 at14:31:58UTC, targeting15:06:58UTC plus final
collection, and completed at15:11:43UTC after local scheduling pauses. Process
exited0. Of36 scheduled attempts,23 were collected and all23 agreed;13 were missed
slots (indices8–18 and34–35), not quote rejections. All six mature window checks
failed: four `failed_sample_in_window`, two `latest_sample_unavailable`. The
cause of local scheduling pauses was not established; these are NOT measured
market outages. No uninterrupted30-minute validation was achieved. No collector
remains running. Always-on collection is needed before assessing market liveness.

Evidence: `/private/tmp/peridot-yxlm-shadow-soak-20260919.jsonl`; final digest over
all preceding JSONL lines independently verified:
`9bb3cc587016559377bad02d4932b69ae993f8a9e65eba4601f906d5114530a2`.
Runner commit `b8229ca` was pushed on `leveraged-fix`; Almanax scan
`1ac226b8-7169-4a86-bf08-217dc10c1fe9` over `ac6fb63..b8229ca` completed with zero
findings and no triage. Subsequent changes only document results; runtime unchanged.
The [risk review](PRICING_RISK_REVIEW.md) records observed orderbook concentration,
correlated-venue/reporter threats and why quote size cannot establish a safe cap.
Do not treat a passing soak or code scan as approval to activate borrowing.

## Contract and authority

The independent cloud observer is described below; it does not publish observations
or change the contract/reporter authority described in this section.

`PriceSource::Observed` records a separately authorized reporter, reference asset,
pool, probe and bounds. Reporter may publish/invalidate only that observation;
it cannot set sources, change parameters, transfer administration or upgrade.
On-chain publishing independently checks pool token identity at configuration,
both executable directions at publication, window duration/freshness, bounds,
rate limits and monotonic window ends. The contract cannot prove SDEX history:
the reporter and data service remain trust dependencies, and wash trading is not
eliminated by volume checks. A compromised reporter may deny service or publish
a false bounded observation. This is not a trustless manipulation-proof oracle.

Expiry uses the observation END, not submission time. Invalidation preserves the
old ratio/replay history; polls do not repeatedly advance an invalid watermark.
Only a newer healthy complete window can recover. A large genuine move beyond
the step guard needs reviewed governance recovery, not automatic ramping through
invented intermediate prices. A dead keeper cannot invalidate immediately, so
the last report may remain usable until its300-second expiry. Likewise a pool
move after publication is caught on the next poll, not retroactively.

The router multiplies the observed ratio by fresh upstream XLM/USD and preserves
the oldest timestamp. Configure `set_required_observation` for ALL three LP
settlement assets so absence, expiry or invalidation also gates XLM/PYUSD/USDC
controller pricing. Use address-based yXLM pricing; do not retain a strategy's
old yXLM→Other("XLM") parity alias. Required-observation metadata and reporter
history live in bounded instance storage; missing source configuration fails
the dependency check. Loss/restoration and governance procedures remain release
gates, not permission to initialize a replacement historical state.

The new LP controller always revalidates live oracle prices for risk checks and
rejects static fallback settings. A warm cache cannot bypass an observation
halt. Generic core controller behavior is unchanged. Repayment does not require
a price; collateral transfers, withdrawals and liquidation can still be blocked
by missing price data. Total oracle-outage liquidation liveness remains a release
gate; do not claim this alone solves it.

## Testnet publisher and release controls

`PRICE_MODE=publish` is fenced to `PRICE_NETWORK=testnet` plus
`CONFIRM_PRICE_TESTNET=ISOLATED_ORACLE`. It requires explicit `PRICE_ASSET`,
`PRICE_QUOTE`, `PRICE_POOL`, `PRICE_ROUTER`, and a separately scoped
`PRICE_REPORTER_SECRET` (optional matching `PRICE_REPORTER_PUBLIC_KEY`). Do not
reuse or transmit the deployer/admin seed. `PRICE_RPC_URL`/`PRICE_HORIZON_URL`
override the default network endpoints. Mainnet publication is rejected even
when a seed is supplied; removing this gate requires the release review.

Before signing, the publisher checks the on-chain source configuration, simulates
the exact call, refuses restoration/unexpected signers and caps total fee at
0.1XLM. Only public hashes are logged; an unknown submission exits without
retrying. Reconcile that hash before restarting. An unhealthy collection
invalidates a previously valid report; on-chain invalidation failure cannot be
claimed as a successful halt. Never deploy this process with automatic restarts
that bypass transaction reconciliation. No funded Testnet publisher or Mainnet
pricing publisher has been deployed yet; the independent read-only cloud observer
below does not publish or invalidate on-chain prices.

Tests: `node --test test/*.test.mjs`; router Rust tests include exact reporter auth,
replay/invalidation/freshness and both quote directions. The explicit compiled
router/LP regression also tests warm-cache borrowing rejection and repayment.

Horizon field and alignment reference:
https://developers.stellar.org/docs/data/apis/horizon/api-reference/list-trade-aggregations

## Always-on read-only observer

`node src/price-observer-main.mjs` runs continuously at60-second intervals. It
reuses the shadow point/window checks but keeps at most64 recent records. Long
pauses become one bounded gap marker with the full missed-slot count; no burst
of backfilled quotes. Every sample logs per-process cumulative scheduled,
collected, agreeing, missed and mature/healthy-window counts, plus a unique run ID.
These are observations, not price publications or borrowing approvals.

SIGTERM/SIGINT stop the worker and its unsigned child gracefully. Each child is
limited to45seconds and inherits only PATH. The parent rejects publishing mode
and credential-bearing environment variable names; no keys are needed. Clock
jumps terminate the run. Restarts deliberately discard in-memory history and
require a fresh30-minute window; NEVER stitch separate run IDs into continuous
coverage. This initial research worker does not add a database or promise durable
history across restart/deployment. Export cloud logs before rotation; long-term
external log storage and market-health alert delivery are not configured here.

`Dockerfile.observer` copies only explicitly listed research sources, excludes
main.mjs and price-main.mjs, installs locked production dependencies with lifecycle
scripts disabled, and runs as non-root. The existing Dockerfile/signing worker is
unchanged. No public HTTP ingress is needed.

The separate `.do/aquarius-price-observer.yaml` app spec uses one Frankfurt
512MiB worker (`apps-s-1vcpu-0.5gb`, verified $5/month), no autoscaling and no env
secrets. Deployment/restart-count alerts are configured; they are not proof that
market-price failure notifications reach an operator. The source uses a dedicated
release branch/tag `aquarius-price-observer-v0.1.0`, never advanced after publication;
development stays on leveraged-fix and no automatic deployment from that branch
is enabled. Verify the exact resolved source SHA after deployment. Never apply
this spec to existing signing app `b38d552c-3cd6-4742-82da-ca44222f5a13`.

Deployed September19: app `ab177729-9cf6-43c7-8510-aaa70855c2e3`, deployment
`c09690f5-387b-436a-9c27-f94d936ec2e1`, ACTIVE on exact source
`8b6636f3e058ffd1d0bc53a0695ed932701a01ad`. Both release branch and tag match that
commit. Cloud build completed using the dedicated Dockerfile;62 keeper tests and
local read-only startup/shutdown smoke passed, npm production audit found zero
vulnerabilities. Almanax `6d470ae2-cd31-4daa-af01-c72aef1d1413` over
`2351a60..8b6636f` completed with zero findings, without triage. Config was also
manually reviewed and server-validated; Almanax API does not reveal path settings.

App and worker env counts are both zero, one instance, no ingress. Runtime started
15:32:05UTC, run ID `a263fa6e-9873-4290-949a-9decc50e4a28`. First three samples
through15:34:07UTC were healthy point observations, all agreeing, no misses/failures;
still warming the initial30-minute window. This is startup/cadence evidence,
NOT a completed continuous window or economic safety result. Raw evidence:
`/private/tmp/peridot-cloud-observer-verified-runtime.log`. The original signing
keeper's active deployment and exact spec SHA256 were independently verified
unchanged after observer deployment. Worker remains running.

Inspect without keys or chain writes:

```sh
doctl --context peridot apps logs ab177729-9cf6-43c7-8510-aaa70855c2e3 price-observer --type run --tail 100 --no-prefix
```

Keep different run IDs separate and export logs before rotation. Publishing remains
disabled regardless of any healthy-window result; activation still requires the
remaining economic, contract, liquidation and release checks.
