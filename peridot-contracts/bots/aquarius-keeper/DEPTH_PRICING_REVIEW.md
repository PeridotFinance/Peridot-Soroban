# Depth-TWAP replacement candidate — September20

User approved development and security review, **not Mainnet activation**.
This is a scoped engineering review, not an independent economic audit.

## Implementation and trust boundary

`depth-pricing.mjs` constructs an unsigned, method-tagged candidate from the
continuous observer's bounded raw history. It recomputes all validations rather
than trusting logged `healthy` or `agrees` flags. The observer records the result
as `depthCandidate`, always with `publicationEligible:false`.

- Six closed trade buckets are replaced by a time-weighted30-minute series of
  executable classic venue quotes, sampled every60seconds. This is a different
  economic reference, not a relaxation of the old trade-volume checks.
- Both1000/10000-unit directions, direct exact-pair routes, <=1% bid/ask spread,
  <=0.5% midpoint depth impact, fresh/coherent ledgers and ratio0.80–1.05 remain.
  Small-probe100-unit Aquarius quotes must agree both per-sample and with the
  resulting window within1%; they do not influence the reference midpoint.
- The window uses31–64 observations, increasing ledger/timestamps and <=90second
  gaps. Every attempted slot in the window must exist and pass. Failed/missing
  samples, clock errors and restarts cannot be bridged or forward-filled.
- The candidate window starts no earlier than this process's start; pre-restart
  observations cannot warm-start it. With normal chain timestamp lag this can
  require31minutes, not exactly30. Attempt duration must be<=45seconds, matching
  the subprocess timeout. Slow/future/overlapping attempts fail closed.
- The evidence SHA256 covers selected raw observations and attempt timestamps.
  It supports reproduction, NOT proof that quotes were executable, independent,
  continuously available or honestly collected by an uncompromised reporter.

`planDepthPublication` is a **pure proposal planner**, not a publisher. Freshly
queried router state is mandatory; missing/malformed state halts (never interpreted
as permission to bootstrap). A valid candidate can propose publication only after
replay/invalidation, >=300-second update spacing, <=1% ratio step and <=60-second
candidate age checks. Existing on-chain maximum age remains300seconds. Bad/newly
unavailable data proposes invalidation if the stored observation is still valid;
otherwise hold. A step violation also invalidates inside the update interval.
Repeated invalidation cannot move the watermark when the report is already invalid.
No synthetic intermediate reports or resetting history after a depeg.

There is intentionally **no transaction adapter connected to this planner**.
The existing trade publisher, price-router contract, required-observation bindings
and all Mainnet fences are unchanged. The keyless image still excludes signing
and publishing entrypoints. Before connecting a publisher it must validate the
configured source/reporter/asset/network/policy, read fresh chain state and both
pool quotes, simulate and enforce fee/auth guards, recheck freshness, and stop on
an unresolved submission hash. Cloud candidate generation authorizes none of this.

## Review and tests

74keeper tests pass, including9new candidate/planner tests: deterministic evidence,
restart/warm-up, failed/omitted/replayed/reordered slots, forged health flags,
spread/depth/pool mismatch, bounded history, slow/future/overlapping attempts,
stored-state replay/invalidation/rate checks, excessive step without synthetic
ramping, malformed/missing chain state, expiry and recovery only after a new
complete window. A manual review tightened missing-state handling before commit.
The7existing router-observation tests were rerun and pass (native controlled
quotes/upstream, not a newly integrated depth-publisher or Mainnet-state test).
Almanax review of this new candidate is pending; previous scans cover earlier code.

## Economic and release conclusion

Keep borrowing activation blocked. This method avoids requiring traded volume in
every bucket, but does not establish reliable availability or manipulation cost.
The v0.1.1 cloud observer's first sample failed the unchanged1% spread guard.
Do not widen it to create a candidate. The earlier ~71% near-price orderbook
concentration, revocable quotes, correlated venues and large-exit slippage remain
relevant; see `PRICING_RISK_REVIEW.md`. No finite attack-cost lower bound or safe
unlimited exposure was established. The user-approved no-cap target is retained.

Next gates: sustained recorded candidate availability; independent economic and
liquidation-loss review under uncapped exposure; reviewed/fenced publisher
integration and outage recovery; actual-dependency/restoration/full-stack tests;
production artifacts and final review. The current XLM migration also needs a
validated non-parity price/recovery path. Neither the400XLM approved fee ceiling
nor an Almanax zero-finding scan removes those gates.
