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

Runs one observation per minute for a bounded32–120minutes (default35), including
the initial sample. Each child invokes the existing read-only price-shadow.mjs;
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
collection, with evidence in `/private/tmp/peridot-yxlm-shadow-soak-20260919.jsonl`.
At this entry’s creation it is RUNNING; no completed window result is claimed.
The [risk review](PRICING_RISK_REVIEW.md) records observed orderbook concentration,
correlated-venue/reporter threats and why quote size cannot establish a safe cap.
Do not treat a passing soak or code scan as approval to activate borrowing.

## Contract and authority

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
pricing service has been deployed yet.

Tests: `node --test test/*.test.mjs`; router Rust tests include exact reporter auth,
replay/invalidation/freshness and both quote directions. The explicit compiled
router/LP regression also tests warm-cache borrowing rejection and repayment.

Horizon field and alignment reference:
https://developers.stellar.org/docs/data/apis/horizon/api-reference/list-trade-aggregations
