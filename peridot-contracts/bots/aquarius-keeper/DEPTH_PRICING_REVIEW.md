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
replay/invalidation, >=120-second update spacing, a prorated ratio step and <=60-second
candidate age checks. Existing on-chain maximum age remains300seconds. Bad/newly
unavailable data proposes invalidation if the stored observation is still valid;
otherwise hold. A step violation also invalidates inside the update interval.
Repeated invalidation cannot move the watermark when the report is already invalid.
No synthetic intermediate reports or resetting history after a depeg.
September21 policy v2 prorates100bps over300seconds, capped at100bps per update:
40bps at120seconds. This avoids simply multiplying the old drift allowance by
publishing more often; it is not a formal rolling-window manipulation bound.

There is now a separate **isolated-Testnet-only transaction adapter**, described
in `DEPTH_PUBLISHER_TESTNET.md`. It is not deployed and has not submitted a live
Testnet transaction. Real SDK signing/assembly against controlled RPC responses
is covered locally; this must not be described as a live network canary. The
existing trade publisher, live contracts, required-observation bindings and all
Mainnet fences are unchanged. Router source now implements separately admin-
approved recovery; all four recovery mutations reject Public network execution.
The keyless image still excludes
the signing adapter and its entrypoint. Cloud candidate generation authorizes
neither Mainnet signing nor a policy change.

## Review and tests

The prior74keeper tests included9candidate/planner tests: deterministic evidence,
restart/warm-up, failed/omitted/replayed/reordered slots, forged health flags,
spread/depth/pool mismatch, bounded history, slow/future/overlapping attempts,
stored-state replay/invalidation/rate checks, excessive step without synthetic
ramping, malformed/missing chain state, expiry and recovery only after a new
complete window. A manual review tightened missing-state handling before commit.
The7existing router-observation tests were rerun and pass (native controlled
quotes/upstream, not a newly integrated depth-publisher or Mainnet-state test).
Candidate commit `e890878621ea6bb272110f1d383323d7f9503c5d` passed Almanax scan
`92c2af1d-8a05-43c5-a43f-5aca2863a317` over2ca34e7..e890878: COMPLETE, zero
findings fetched. The subsequent adapter/runtime commit
`fcfb91c3351d90c43c6dee8d888fea492758deed` also passed Almanax scan
`0b74548a-592f-4776-9b58-bf5ecb500673` overcd07141..fcfb91c: COMPLETE, zero
findings fetched. This is code-review evidence, not economic safety or Mainnet
activation approval.

September20 follow-up:87keeper tests pass, including11new publisher/journal tests
and2observer-consumer tests. The end-to-end offline replay executes the actual
collector scheduler, candidate builder, SDK assembly/signing, public hash journal
and publication/invalidation controller against controlled RPC: one failed sample
causes one invalidation and requires30new healthy minutes before recovery. It does
not execute the Rust contract; the separate9native router-observation tests do.
Two new native cases prove honest0.5% recovery works, but a genuine3% move remains
blocked after invalidation and reapplying the same policy. September21 adds a
separate governed path rather than relaxing that ordinary-publication guard.
The full router suite passes39tests. All7explicit compiled LP validation tests
were rerun successfully against the existing pinned validation WASMs and exact
pool WASM. Their earlier limitations remain: controlled upstream/quotes/local
pool state, not final production artifacts or a Mainnet-state migration rehearsal.

At17:58:16UTC the unchanged cloud v0.1.2 run0841699f had38/38collected/agreed,
0missed,0failures,7healthy/8mature attempts. Latest31–32point candidate ratio was
0.966425406394XLM/yXLM. The first mature attempt was still warming due to ledger
timestamp lag. This is short-run evidence, not a sustained availability claim.

## Economic and release conclusion

September21 recovery review: the admin must approve a bounded reference, wait for
a full new30-minute healthy window, then separately complete recovery after at
least300seconds of report-end progression and fresh report/upstream/quote checks.
Cancelled/expired/changed-configuration approvals stay locked; the reporter cannot
finish. Borrowing AND liquidation pricing are unavailable until completion, while
repayment works. An honest real3% move is covered natively; compiled full-stack
tests cover the state machine at parity using actual pool code with controlled
local state. Neither proves live non-parity migration or economic safety.
See `DEPTH_PUBLISHER_TESTNET.md` for exact authority and operational restrictions.

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
