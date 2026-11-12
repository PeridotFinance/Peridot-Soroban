# Pair-specific depeg guard — specification only

No implementation or production parameter change. Applies to every proposed stable-pair
vault, including a team-only allowlisted prototype. The accepted ABI is retained;
issuer addresses and the configured primary-feed mapping are pair-specific bindings.
This guard limits new exposure. **It does not recover losses on an existing position.**

## Independent values and normalization

Read each leg's fresh positive primary price, timestamp, feed base/decimals and
resolution, with exact issuer mapping. Reject future timestamps, unavailable values,
issuer pauses and values older than the validated per-feed age. Never substitute a
symbolically similar asset (sUSD→USD, USDT0→USDT) or a pool spot as its own oracle.
Read the actual pool slot price with token ordering and decimal correction. Let
r=token1 units per token0; oracle fair ratio=p0/p1. Compare both reciprocal directions
with conservative rounding. Pool-implied leg prices are r*p1 and p0/r; these are two
views of ONE relative observation, not independent proof of both dollar pegs.

Separately compare USD stable prices to their target$1; for EURC compare EURC/USD to
independent primary EUR/USD. EUR/USD movement is currency risk, not EURC depeg.
If a feed is USDC-denominated, convert using independent USDC/USD; do not assume the
USDC base remains$1. Fee/claim proceeds do not repair or override an invalid price.

Initial **test candidates**, not approved production values:50bps warning threshold
and200bps invalidation threshold on both pool/oracle relative discrepancy and each
leg/peg-reference discrepancy. These values are deliberately not calibrated to excuse
sUSD's observed discount. Validate depth, oracle cadence and rounding with the chosen
pair; bind final values explicitly in the deployment manifest. Freshness candidate:
one300-second resolution plus explicit measured publication allowance, bounded above
by600seconds. No last-good fallback authorizes fresh risk. Require positive configured
thresholds and warning<invalidation; no disable-by-zero bypass in this product.

## State/action matrix

|Condition|New deposits, mint, investment, share-backed borrowing|Exit/recovery|
|---|---|---|
|Both prices fresh, issuer flags clear, all deviations<=warning|Allowed only within existing cap, minima, reserve and authorization checks|Normal route with required claim-first boundary|
|Warning exceeded but prices trustworthy and deviation<=invalidation|Pause new exposure, including later keeper investment of minted cash|In-kind exit/recovery available; priced liquidation may continue only at fresh trustworthy values in a separately built2× product|
|Wider bound exceeded, stale/missing price or issuer-price invalidity|Valuation invalid; block mint, investment, conversion using that valuation, new borrowing and value-based collateral release|Only permitted in-kind exit/recovery and debt repayment; no stale-priced seizure or invented zero price|
|Issuer/venue refuses transfer or withdrawal|No new exposure|Preserve ownership/liabilities/rewards; report actual failure, never mark paid|

Recheck at prepare, funding admission where required, conversion, cash mint, keeper
investment and optional close conversion. Checks in an earlier transaction cannot
replace a later execution check. Pure refund/unlock of existing assets and in-kind
withdrawal must not depend on oracle availability. A paused pending command can expire,
return its actual unminted mixture, or release minted shares/execution reserve according
to the approved state machine. After mint, common backing is not refunded as command
cash. Recovery preserves historical reward claims, including when claim/pending views
fail. Ordinary ownership changes still require their successful same-transaction claim.

At1× there is no debt. A future2× route cannot use this rule to release pledged assets
unconditionally: zero debt or authorized funded atomic repayment/seizure is required.
With untrustworthy valuation, ordinary price-based liquidation remains a blocker;
this research does not invent a fair seize amount or promise borrower-independent
priced liquidation during an oracle outage. G1–G4 remain public-access gates under the
latest team-only rollout decision, not assertions that this price dependency disappears.

## Primary-feed coverage found in this study

Existing external/USD Reflector CAFJZQ… supports exact queried USDC and EURC; PYUSD,
sUSD and USDT0 getters returnNone. Same-provider DEX primary CALI2… supports exact-issuer
PYUSD and USDT0, with USDC as base; the FX primary has EUR/USD. No sUSD primary was
found. See `../price-history/oracles.json` and the accompanying report for exact addresses,
readings and request/response evidence. Selecting a different primary mapping/base
conversion is a concrete configuration/qualification dependency, not a secondary
fallback or silently implemented change. The pool itself cannot close sUSD's gap.

## Resource accounting: included baseline versus missing work

Offline extraction `resource_attribution.py` verifies the accepted unsigned archive
and call member hashes. It does not run a new harness. Two synthetic `lastprice` plus
two `resolution` calls already cost1,485,219–1,489,771 modeledCPU across the affected
roots; `get_slot0` adds1,073,738–1,077,981. These calls were already INCLUDED in the
accepted complete transactions. Do not add them a second time. The old oracle WASM is
bff4b685… compiled synthetic fixture, not live Reflector.

Fresh public getter simulations expose transaction resource DECLARATIONS, not measured
full-root costs: externalUSDC807,059 instructions, externalEURC825,303; DEXPYUSD1,289,348,
DEXUSDT01,291,822, DEXEURC1,187,914; FXEUR863,926. Each declared two uniqueRO keys and
zero writes. These are independent simulations and cannot be summed as an exact combined
transaction CPU/memory/SDK footprint. SDK entry totals and memory are absent. They do
establish that missing reads must not be treated as free. Raw declared footprints are
retained in the oracle evidence. Exact standalone key unions for external USDC plus
DEX PYUSD/USDT0, or external USDC+EURC plus FX EUR, are three unique RO keys
(shared code and two instances), zero writes. This is not three incremental financial
root keys or three SDK entries; complete roots and restoration still need measurement.

Reuse fresh values within ONE transaction for ratio and absolute-peg checks; the old
`stage_root` returns only a ratio, so preserving individual prices needs an explicit
implementation change. Do not presume zero incremental CPU from that change. USD-base
and EUR/USD conversion may require additional primary data, code/instance keys, price
records and restoration. Reserve a versioned guard policy in bounded instance state;
no unbounded scan or separate unmeasured refresh prerequisite.

The accepted signed investment maximum95,144,812CPU leaves4,855,188, and the cold
maximum95,205,766 leaves4,794,234. Neither headroom proves this new guard fits. Full
root CPU,41,943,040memory,100SDKtotal,100uniqueSorobankeys,50writes and all existing
byte/event/rent/envelope checks must be re-measured with the selected live Aquarius
WASM and actual configured primary oracle. Include shared versus incremental keys,
source and detached authorization, warm/cold restore and issuer-failure paths. No new
financial execution/build occurred here; new-guard complete-transaction cost is
explicitly UNMEASURED. Private signed table remeasurement is the first package3 task
under the latest instruction, before any team-only deployment ceremony.
