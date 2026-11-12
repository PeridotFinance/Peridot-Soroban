# Mixed proceeds and external funding companion — measured checkpoint

The separate companion passes **1/1 under each profile** using the unchanged corrected six-overlay V3 `ad9fbb7669ff126ff54e8d9b021d610b9313cb5f3dc595df6e9d7dabc3d7d482` (100,419 bytes). Captured execution took 73.69 seconds; internal execution took 76.35 seconds. Both invocation gates pass; both complete Stage 1 checkpoint gates **fail**, with 46 missing required flows, two lifecycle errors and 46 transaction errors. Release approval remains false. This companion does not replace the frozen seven-test suite or its internal-policy failures.

Final local build: `target/cl-option1-mixed-build-v3`, generated package `cl-storage-ba1cc53b55347f7c-harness`. Final runs: `target/cl-option1-mixed-captured-v3` and `target/cl-option1-mixed-internal-v3`. Exact measurements, artifact hashes and source/log bindings are in [mixed-results.json](mixed-results.json). The Mac artifact identity is explicitly pinned; this preparer is **not a demonstrated portable Linux CI build**. After these frozen runs, the lead added explicit `--rebind-reviewed-platform` for future CI: it validates the exact six-overlay/source/artifact pins and changes only the generated test V3 identity, retaining input and generated hashes. That path is not exercised by this local evidence. The exact measured constructor is preserved at `target/cl-option1-mixed-preportable.py`.

## Concrete proof

The test executes a real converted Long Closing position with positive debt-token proceeds. It does **not** call `finish_close` first. An artificial 30-year interest timestamp, with only a few ledger advances, creates positive current debt above the held proceeds. The saved pre-settlement financial snapshot is `mixed-before-settlement.json`; subsequent test authorization registration may add native mock metadata, so it is not labelled the exact full positive transaction prestate.

| Atomic settlement quantity | Raw synthetic token units |
|---|---:|
| Own proceeds paid first | 49,962,213 |
| External deficit | 33,966,075 |
| Authorized external maximum | 38,966,075 |
| Exact payer refund | 5,000,000 |
| Full current debt repaid | 83,928,288 |
| Remaining debt / collateral seized | 0 / 0 |

The actual compiled root and receipt repayment helpers execute real nested contract authorization. SAC events prove own-proceeds payment, external maximum transfer, then exact refund in that order. Cash-vault underlying increases by full debt, payer balance falls by the deficit, and V3 consumes exactly the held proceeds. Position and pending-close records are removed only after zero debt. Both Aquarius reward calls are faulted through controlled pool state, and existing borrower reward records survive unchanged. External actors use narrow native test authorization; this does not establish real account signatures or production smart-account authorization.

The insufficient-maximum case reaches the root, fails without an authorization error, and compares complete ledger data and TTL rollback. A separate local clone with the Mainnet network ID rejects at the unchanged validation root before nested calls, with complete rollback. It performs no network transaction.

The **33,966,075 external contribution is voluntary loss cover with no collateral compensation**. The artificial interest scenario proves arithmetic and atomic behavior; it does not establish economically incentivized liquidation, recyclable payer capital, a keeper operating horizon or actual re-expiry. Existing guards prohibit CL Short; Short is not an omitted supported route.

## Existing fees and rewards remain backed

All fee funding is created by public operations. A free-margin provider transfers cash receipt pTokens; another trader funds a TEST-only 100bps open fee and completes a ninth actual open/activation. This reserves **500,000 cash pTokens** for the provider. No arbitrary fee storage is injected.

Across mixed settlement, every existing fee epoch/entitlement entry remains byte-identical, public pending-fee totals remain equal, and V3 cash receipt pToken backing remains equal. A separate preview clone checks the entitlement without altering the recovered prestate. Subsequently measured public fee distribution and beneficiary claim each deliver the expected 500,000; free margin rises by 500,000 and backing is unchanged. A repeated claim returns zero. These separate fee roots are verification of continued beneficiary access, not omitted work required to complete settlement.

Before actual distribution, exact scaled reward conservation is: received 100,671; paid 0; pending 100,632; fractions 38,999,999,999,973,007,512; quarantine 0; rounding 26,992,488. After fee claim and later eligible reward accrual: received 9,021,043; paid 0; pending 9,021,019; fractions 23,999,999,999,859,739,688; quarantine 0; rounding 140,260,312. Scale is 10^18 and both equalities are exact. No waiver occurs.

The existing resource label `option1_mixed_late_reward_quarantine` must not be interpreted as physical quarantine receipt: **actual quarantine is zero**. No CL ownership change occurs in this outage segment; later receipts accrue to eligible weights. The test proves booked-claim preservation and exact conservation for this path, not a new historical quarantine redistribution scenario.

## Resource evidence

Finite acceptance limits and captured CPU/memory cost coefficients are enforced on host/SDK 29.0.1, protocol 29, captured ledger 65,270,655, config SHA256 `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`. Contracts retain their locked SDK 25.3.1 dependency. Setup explicitly uses the captured profile; selected measured roots use the selected profile. Diagnostic shadow budgets are not acceptance limits. The following SDK quantities are identical in both profile runs:

| Measured flow | CPU | Memory bytes | SDK entries | Writes | Captured / internal |
|---|---:|---:|---:|---:|---|
| Insufficient maximum rollback | 16,469,961 | 5,683,197 | 58 | 6 | PASS / PASS |
| Local Mainnet guard rejection | 810,275 | 1,319,619 | 2 | 0 | PASS / PASS |
| Mixed own/external full repayment | 46,980,924 | 9,091,183 | 97 | 28 | PASS / PASS |
| Reserved fee distribution | 2,810,868 | 2,742,861 | 14 | 3 | PASS / PASS |
| Beneficiary fee claim | 5,199,429 | 1,634,521 | 32 | 6 | PASS / PASS |
| Repeated fee claim | 3,587,760 | 1,491,944 | 28 | 3 | PASS / PASS |
| Later reward checkpoint | 8,370,193 | 4,276,060 | 27 | 9 | PASS / PASS |

The positive root's unsigned official-host companion measures 46,792,386 CPU, 8,281,220 memory bytes, 70 unique footprint keys (42 read-only / 28 read-write), zero disk reads, 2,544 write bytes, 6,560 serialized envelope bytes and 556 authorization bytes. Return is 92 bytes; contract events are 2,120 bytes (2,212 combined resource bytes). The modeled fee is **165,610 stroops = 0.0165610 XLM**: inclusion 100, nonrefundable 65,018, refundable 5,401 and rent 95,091. This is unsigned controlled-state modeling, not a charged Core fee or a complete operator budget. SDK 97 entries leave only three entries before the internal 100 ceiling; real signatures are unmeasured.

Actual account signatures, live pool/oracle state, full Core transaction application, custom-account reentry, all delivery failures, full pause combinations and restored/re-expired versions of this mixed case remain unproved. Borrowing stays disabled and production maintenance margin unset. No production contracts, shared policy, stages, profiles or frozen seven-test sources were edited.

## Reproduction and gates

Use fresh paths; preparation refuses overwriting. The original v1 compile API mismatch and v2 test-event parser panic remain in their separate build/run directories. The v2 panic happened after the insufficient-maximum rollback assertions; it was not an authorization proof or a successful full case. Final source SHA256: `028918edf50e8598cdb4e275c81025e36ceeb7b7b69afb07c171ee6654727f4c`; measured preportable preparer SHA256: `fcfe315e70f86a0cc6912046e6b64ee0391aed0b0e4704d8977ac8ba8628ec5d`.

```sh
python3 prototypes/cl-option1-qualification/prepare-mixed.py \
  --build target/cl-option1-joined-build-v3 \
  --output target/cl-option1-mixed-build-repro
CL_ATOMIC_BUILD_DIR="$PWD/target/cl-option1-mixed-build-repro" CARGO_BUILD_JOBS=1 \
  bash prototypes/cl-storage-qualification/run_variant.sh captured-network-diagnostic \
  compiled_option1_mixed_proceeds_and_external_deficit_preserve_reserved_fees \
  target/cl-option1-mixed-captured-repro
CL_ATOMIC_BUILD_DIR="$PWD/target/cl-option1-mixed-build-repro" CARGO_BUILD_JOBS=1 \
  bash prototypes/cl-storage-qualification/run_variant.sh internal-policy \
  compiled_option1_mixed_proceeds_and_external_deficit_preserve_reserved_fees \
  target/cl-option1-mixed-internal-repro
```

The existing runner records the actual test exit and invokes `check_budget.sh` on every emitted measurement, but its default map is the older generic map. Bind this companion's reviewed map to the exact copied generated source and actual test exit before evaluating full coverage. For each run (replace only the two paths):

```sh
mixed_run=target/cl-option1-mixed-captured-repro
mixed_build=target/cl-option1-mixed-build-repro
python3 - "$mixed_run" "$mixed_build" <<'PY'
import json, pathlib, sys
run, build = map(pathlib.Path, sys.argv[1:])
reviewed = json.loads(pathlib.Path('prototypes/cl-option1-qualification/mixed-proof-map.json').read_text())
source = str((build / 'harness/src/mixed-funding-tests.rs').resolve())
reviewed['sources'] = {test: source for test in reviewed['sources']}
out = run / 'mixed-proof-map.json'
assert not out.exists()
out.write_text(json.dumps(reviewed, indent=2) + '\n')
PY
python3 scripts/make_cl_proof_manifest.py \
  --map "$mixed_run/mixed-proof-map.json" --profile "$mixed_run/profile.json" \
  --report "$mixed_run/resources.jsonl" --test-log "$mixed_run/tests.log" \
  --test-exit-code "$(cat "$mixed_run/test-exit-code.txt")" \
  --call-profile "$mixed_run/calls.jsonl" --output "$mixed_run/mixed-proof-manifest.json"
bash scripts/check_budget.sh --local-report "$mixed_run/resources.jsonl" \
  --profile "$mixed_run/profile.json" --transaction-evidence "$mixed_run/transactions.jsonl" \
  --proof-manifest "$mixed_run/mixed-proof-manifest.json" --gate checkpoint \
  --summary-json "$mixed_run/mixed-gates.json"
```

The final checker command intentionally returns **1**, preserving missing full-Stage-1 proofs. CI should retain separate test and checker statuses, upload all reports, and propagate failed required gates. No Linux CI approval is asserted here.
