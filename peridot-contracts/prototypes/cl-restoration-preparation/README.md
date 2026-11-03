# Explicit restoration preparation experiment

Stage 1 remains incomplete and PR #75 remains draft. This isolated prototype
combines the preserved storage experiment and the separate full-repayment
improvement. Production contracts, financial stages, both resource profiles,
borrowing configuration and handoff 00 are unchanged.

The experiment uses the official protocol-29 `simulate_restore_op`, serializes
unsigned RestoreFootprintOp envelopes, and validates bounded batches against
the captured settings. It projects **only TTL changes** into a cloned controlled
snapshot, then runs fresh compiled financial invocations with finite limits.
It does not apply transactions through Stellar Core, verify account signatures
or establish live-state qualification. Native restore memory is unmeasured;
zero declared instructions do not mean zero native CPU. See `restore-notes.md`.

Each probe starts from an independent archived prestate. Its preparation fees
must not be added to other independent probes as if they formed one sequential
lifecycle. A real journey requires fresh simulation after each confirmed batch
and again before the financial operation. Restoration does not pause interest,
extend deadlines, grant custody rights or guarantee future price availability.

## Reproduce

Prerequisites are the repository's pinned Aquarius WASM at
`target/cl-share-checkpoint/aquarius-pool.wasm`, the existing Node decoder
dependencies, Stellar CLI and Rust. No account identity or secret is needed.
Use fresh paths and do not alter prepared inputs during execution.

```sh
cat prototypes/cl-atomic-qualification/optimization.patch \
  prototypes/cl-storage-qualification/storage.patch \
  prototypes/cl-storage-qualification/repayment.patch > /tmp/cl-restoration-combined.patch
PYTHONDONTWRITEBYTECODE=1 CARGO_BUILD_JOBS=1 python3 \
  prototypes/cl-restoration-preparation/build.py \
  --output target/restoration-clean-repro \
  --optimization /tmp/cl-restoration-combined.patch
python3 prototypes/cl-restoration-preparation/prepare.py \
  --build target/restoration-clean-repro --output target/restoration-final-repro
python3 prototypes/cl-storage-qualification/identify_build.py \
  --build target/restoration-final-repro
CL_ATOMIC_BUILD_DIR=target/restoration-final-repro CARGO_BUILD_JOBS=1 \
  bash prototypes/cl-restoration-preparation/run.sh \
  combined_repayment_cases target/restoration-repayment-repro
```

`build.py` uses `stellar contract build` with a fresh build-specific contract
cache. It retains `funded-liquidation-validation`, including its Mainnet
restriction. Contract SDK is 25.3.1; compiled harness SDK, host and simulation
are pinned to 29.0.1. Captured settings/cost coefficients are ledger65270655,
SHA256 `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`.

Additional focused filters, each in a separate fresh run directory:

```text
test::atomic_qualification::compiled_atomic_qualification_swap_close_64_owners_128_changes_8_unresolved_5pct_trade
test::atomic_qualification::compiled_atomic_qualification_8_independent_liquidations_repeated_both_call_outages
test::atomic_qualification::compiled_atomic_qualification_8_independent_pending_recoveries_repeated_both_call_outages
```

Do not set the older automatic-archive environment variables for these runs:
this prototype explicitly archives required keys, prepares them, and rejects
additional automatic restoration in the final operation. The setup is bounded
by the captured profile; every assessed root then runs independently under both
unchanged profiles. Synthetic external authorizers use narrow recorded trees;
V3, facade, receipt and vault contract authorization executes normally.

`run.sh` preserves test exits and invokes `check_budget.sh` for financial rows,
including the independent pool trade. `restore-checker.py` separately validates
native restore envelopes, rent, batch coverage and journey links. It does not
invent SDK invocation resources for native operations. The default final gate
fails for missing Core application, signature and live-state evidence. A failed
gate propagates out of `run.sh`; expected incompleteness is not CI approval.

```sh
python3 -m unittest discover -s prototypes/cl-restoration-preparation \
  -p 'test_*.py'
python3 prototypes/cl-restoration-preparation/assess.py \
  --run target/restoration-repayment-repro --output target/restoration-assessment.json
python3 prototypes/cl-restoration-preparation/cpu-attribution.py
```

The unsigned preparation payer is independent of the borrower. Interrupted
preparation is serialized/reloaded after each batch; duplicate simulation finds
already-live keys and returns an empty footprint. The recorded partial-prefix
financial attempt remains a failure. Fixture time does not advance during
preparation; observed confirmation latency and restoration after a long outage
remain unmeasured. Funded liquidation repays debt and seizes collateral in one
unchanged financial invocation. No borrowing activation, signing, broadcast,
deployment, helper-contract adoption or policy change is authorized here.
