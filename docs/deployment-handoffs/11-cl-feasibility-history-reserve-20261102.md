# CL feasibility, larger histories and complete-open reserves — 2026-11-02

**Stage 1 remains incomplete. PR #75 remains draft. No release approval.** This checkpoint preserves commits 64093aa, 14ef4d8, 8f3e5fd and da4e1a5, both resource profiles, their reports and the authoritative shared 00 unchanged. All three claim-first boundaries and existing transaction stages remain. Borrowing preparation is disabled; production maintenance margin remains unset. No signing, broadcasting, keys/secrets, Mainnet transactions or Upshift binding occurred.

## Measured feasibility under unchanged policy

The [durable Linux assessment](evidence/cl-linux-ci-20261102/resource-feasibility.md) and [machine report](evidence/cl-linux-ci-20261102/resource-feasibility.json) analyze the completed prior Linux run using protocol 29 execution and captured cost coefficients. They are explicitly historical, not measurements of this checkpoint's changed controller.

Worst observed swap-open uses 235,455,712 instructions and 24,514,438 memory bytes. The disjoint required Aquarius mutation/claim subtrees alone cost 147,933,713 instructions. Fee collection 53,570,569 plus position deposit 64,865,976 already exceed the 100M internal ceiling before the three required claim calls, swaps or wrappers. Deposit, redeem and swap-close likewise exceed 100M in required venue subtrees in the measured stressed state. Even an unsafe hypothetical deletion of every repeated getter saves only 7,033,065 instructions. No such deletion was implemented.

For this pinned venue path/state, wrapper caching alone cannot achieve internal CPU acceptance. Architectural investigation should separately consider equivalent, cheaper fee-inclusive NAV/position accounting in the venue, compact receipt/controller configuration, and redesigned duplicate storage ownership. Each requires measured before/after authorization, reward, valuation, donation, tick/range, rollback and restoration equivalence. Packing keys can increase write bytes/rent/contention. CPU and entry cardinality currently bind before memory; observed swap memory headroom 17,428,602 bytes is not a bound on arbitrary state.

Changing internal CPU/entry/write policy is a **separate unapproved risk-policy proposal**, not an optimization or an automatic consequence of captured-network success. No limit or stage changes were made. Funded repayment and permitted seizure remain atomic.

## Larger history and eventual attribution

Two new compiled tests construct actual state on the pinned Aquarius bytecode. The first uses 64 direct-share historical beneficiaries, 383 weight checkpoints, 128 additional ownership changes, eight claim outages and eight exited beneficiaries. It settles the first interval for all 64 original owners and later intervals incrementally for exited owners. Remaining verified backing is reserved historical entitlement, not missing-boundary quarantine.

The second creates eight margin positions for one borrower and forces eight funded exits with both pending-view and claim calls failing through controlled pool-state corruption. Restoring the exact pool record allows real later claims, but 16 missing boundary readings stay missing. Previously booked rewards survive; forced liquidation never waives them. Published controlled replay evidence permits governed attribution of the first interval after 86,400 seconds, explicitly marked governed rather than automatically verified. The original exited borrower receives 192,192; the liquidator/newcomer receives 0. Seven intervals totaling 1,076,271 remain quarantined when sufficient evidence is not provided.

The [attribution and archival notes](evidence/cl-feasibility-history-reserve-20261102/ATTRIBUTION.md) specify required governance evidence: pinned venue code, authenticated boundary state/order, emissions/tick/range changes, beneficial custody weights and supply, waiver records, claim receipts and rounding, and a no-double-attribution reconciliation mapping each amount to those records. The contract checks authorization, digest, delay and accounting; it cannot establish whether arbitrary off-chain evidence is true. A later cumulative reading cannot reconstruct an absent boundary. Without sufficient evidence, quarantine is indefinite and excluded from NAV and normal reward accrual.

State construction is labelled fixture setup, with invocation limits and nested authorization enforced but no full-transaction claim. Every assessed mutation is captured. Assertion/index discovery runs on disposable ledger clones, so query APIs that settle rewards or extend TTLs cannot warm or mutate assessed state. Client checkpoint lookup is binary search; runtime settlement is incremental and never enumerates all holders in liquidation/emergency withdrawal.

## Complete-open reserve enforcement

The validation controller now checks the cash borrowing-enabled flag and reserve after open cash costs on activate, execute and expired pending resolution. Tests exercise an actual supplier withdrawal below the floor between swap and activation, and a separate borrowing pause. All three finalizers reject with complete ledger rollback; swap rejects both reserve shortfall and pause. Positive configured swap/activation succeeds.

Risk-reducing pending funded recovery and active full repayment/debt-free release remain available under a 100% floor, disabled borrowing and claim outage. Release moves the receipt claim to free margin custody; it is not wallet redemption. A healthy expired pending position cannot use legacy liquidation conversion to bypass open checks: fresh liquidation eligibility rejects atomically.

This remains behind `funded-liquidation-validation`, whose constructor rejects Mainnet. Default/deployed controller integration is still a separate release dependency. No unavailable production setter was invented, no reserve check skipped in the tested origination route, and no production borrowing enabled.

## Durable Linux evidence and exact replay

The prior final Linux artifact is now [durably indexed](evidence/cl-linux-ci-20261102/index.json), including separate Linux WASM hashes, all raw logs, both summaries/errors and exact pinned replay sources. The [archive and replay instructions](evidence/cl-linux-ci-20261102/README.md) preserve every original artifact member before its November 16 expiry. The earlier bootstrap-failure report remains historical evidence, superseded for final-run status by this index. Seven local/Linux WASM hashes differ; cross-host byte-identical builds are not established and pins must not be interchanged.

Replay verifies each profile's four gates, missing flows, lifecycle/transaction error multisets and raw invocation resource/fee/error blocks. It normalizes only explicitly bound log paths. Exit 1 means reproduced failed gates only when both profile verification records exist and pass; exit 2 means mismatch. A regression cannot hide behind aggregate exit 1. The preserved source checker is used because today's required proofs differ from the earlier checkpoint. This historical replay does not approve current code.

## Post-change execution and resources

Both profile gate results are final: captured-network invocation/lifecycle/transaction/checkpoint **PASS**, internal-policy all four **FAIL**. The full runner exits1 and release approval is false. [Detailed test/resource/gate report](evidence/cl-feasibility-history-reserve-20261102/execution-summary.md); [machine checkpoint](../../peridot-contracts/config/cl-feasibility-checkpoint.json).

The complete compiled captured-network suite passes **30 tests**, followed by **3 existing historical** and **2 larger-history archival** reruns. The internal profile passes3 and fails27 main tests; all5 archival tests fail before completing their proofs. Limits remain100M/100SDKentries/50writes internally and400M/400/200 in the captured diagnostic; both retain41,943,040 memory bytes and all other checks.

| Profile | SDK CPU maximum | SDK memory bytes | SDK entry total | Writes |
|---|---:|---:|---:|---:|
| Internal policy, including rejected attempts |100,003,827|16,847,283|136|30|
| Captured-network diagnostic |235,456,785|24,514,601|213|56|

The internal numbers include budget exhaustion and are **not completed-flow estimates**. The captured report contains1490 successful invocation rows. Its2562 unsigned target-runtime records include2494 successes,68 expected rejections with rollback, and733 observed restorations. Complete unsigned maxima are236,693,997CPU,21,678,566memory bytes,158unique footprint keys,57writes and15,092envelope bytes. SDK entry totals and serialized unique footprint counts are different metrics.

The two larger-history runs alone produce1004 unsigned records and436 actual restorations. Including restored variants, their maxima are128,416,150CPU,12,281,537memory bytes,149unique keys,52writes,12disk entries/2200disk bytes and14,012envelope bytes. Some restored variants therefore exceed the internal write limit even where their normal invocation did not. Exact restored HistoryCount/Weight/Segment/Interval/Settled/Proposal keys and baseline/poststate comparisons are retained in the compressed summary. This restores touched history keys, not every unrelated historical entry.

| Unsigned fee scope | Inclusion | Non-refundable | Refundable | Rent | Total |
|---|---:|---:|---:|---:|---:|
| All captured transactions, component maxima |100|162,160|15,147|22,686,852|22,845,642|
| Larger histories including restoration, component maxima |100|140,485|5,049|9,068,453|9,209,408|

Fees are stroops; component maxima can come from different transactions and must not be summed. Total already includes rent. Inclusion100 is a controlled ledger setting, not a live fee quote. Envelopes/auth trees are unsigned: real signature verification and signed-envelope overhead remain unmeasured. Captured limits are enforced; no profiling-only run is presented as acceptance.

## Local regression checks

Checker/replay/summary tests:28 PASS. Native margin regression:110 PASS,2 existing ignores; native vault:53 PASS. These do not replace the compiled protocol29 resource and transaction gates.

## Reproduction and remaining proofs

From `peridot-contracts`, use a fresh output directory:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_cl*py'
python3 scripts/replay_cl_linux_ci.py \
  ../docs/deployment-handoffs/evidence/cl-linux-ci-20261102 \
  --output target/cl-linux-replay-new
python3 scripts/analyze_cl_feasibility.py \
  --durable-evidence ../docs/deployment-handoffs/evidence/cl-linux-ci-20261102 \
  --output-dir target/cl-feasibility-new
CL_CAPTURED_CONFIG="$PWD/../docs/deployment-handoffs/evidence/cl-resource-reconciliation-20261102/network-config.json" \
AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm" \
CL_CHECKPOINT_DIR="$PWD/target/cl-checkpoint-new" \
bash scripts/run_cl_share_checkpoint.sh
```

The runner uses only `stellar contract build` for WASMs, explicit profiles for all acceptance runs, and `check_budget.sh` on compiled and native-prototype reports. It binds all 40 required proof labels to concrete tests/assertions. Larger-history archival reruns are explicit rather than silently omitted. Native host 25 prototype evidence remains separate from compiled host 29 evidence. No required gate failure is swallowed.

Remaining release blockers include the internal resource policy, supported production reserve/controller deployment integration, live PYUSD/USDC/oracle/depeg/capacity state, real signatures/signed-envelope overhead, historical-state growth and retention bounds, governance evidence availability and final review. The shared 00 begin/execute/swap/activate ordering remains flagged for coordinated reconciliation; no independent policy edit was made.
