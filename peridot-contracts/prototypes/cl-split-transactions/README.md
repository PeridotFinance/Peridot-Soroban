# Isolated split-transaction experiment

MEASURED INCOMPLETE PROTOTYPE. Production contracts, transaction stages, resource policy and
shared handoff00 are unchanged by this experiment. PR75 stays draft; Stage1 is
incomplete. No signing, broadcasting, deployment, keys/secrets or Upshift.

The baseline is the separate corrected reserve/history checkpoint. Prototype
source is compiled from a hash-bound copy of that source, with explicit overlays;
no prototype ABI is added to production contract files. All WASMs use
`stellar contract build`. The runtime is SDK/host29.0.1 with captured cost models.

Each operation is a separate top-level invocation, with separately serialized
unsigned transaction evidence and real nested contract authorization. Synthetic
external users are narrowly authorized; this is not real account-signature proof.
The internal limits stay100M CPU/41943040memory/100SDKentries/50writes, including
all other existing checks. Captured-network runs are diagnostics, not approval.

Fixture construction for pre-existing holders uses an explicitly recorded captured
profile. Measurements of each prototype transaction use the selected profile.
Successful fixture construction does NOT establish a complete internal-policy
lifecycle, and a later setup or resource failure cannot stand for an authorization
rejection. Missing flows remain failed gates.

Current prototype: Long-only funded escrow, conversion THEN actual fee collection,
fenced fee-inclusive mint with explicit borrower beneficial ownership, separate
claim-first delivery/assignment, and unchanged activation. All original three
claims remain; escrow beneficiary assignment adds another actual claim. No global
ownership lock. Owner-funded full repayment and in-kind release are atomic.
Pre-delivery third-party liquidation is an explicit unresolved architecture gap.

The main captured suite passes21 tests. Internal policy passes8 and fails13 on
resources. Two supplemental complete recovery tests pass captured limits and fail
internal limits (127/123 SDK entries). The current full filter runs all23 cases.
Separate conversion28.93M and preparation79.97M fit; mint131.86M and
activation181 SDK entries fail. Prepare-close120 entries and swap-close187.77M
also fail. The tested split is not suitable for adoption. Aggregate venue sums
do not prove every possible split exceeds100M.

See [the decision and resource tables](../../../docs/deployment-handoffs/15-cl-split-transaction-feasibility-20261102.md)
and [durable evidence](../../../docs/deployment-handoffs/evidence/cl-split-prototype-20261102/README.md).
The baseline is commit9cf43082bfb57334d57dbc58cfa90ccad4d98078. Preparation checks
every baseline source hash and uses distinct fresh Cargo output directories for
baseline and overlay builds. The old `run-filter.sh` is a diagnostic helper;
the reproducible path is:

```sh
python3 prototypes/cl-split-transactions/prepare.py --output target/split-rebuild
CL_SPLIT_BUILD_DIR="$PWD/target/split-rebuild" bash prototypes/cl-split-transactions/run.sh captured-network-diagnostic split_tests target/split-captured
CL_SPLIT_BUILD_DIR="$PWD/target/split-rebuild" bash prototypes/cl-split-transactions/run.sh internal-policy split_tests target/split-internal
```

Run each command individually and preserve its status. Internal execution returns1;
raw Cargo exit101 is retained. The captured command selects the **invocation**
gate and returns0; its lifecycle/transaction/checkpoint gates remain false,
because this isolated suite does not replace the full Stage1 proof map. Check
`gates.json`, `tests.log` and `budget.log`, not aggregate status alone. Replaying
the durable archive verifies exact per-profile errors, completed tests, invocation
rows and all WASM identities, and returns1 for the incomplete checkpoint:

```sh
python3 prototypes/cl-split-transactions/replay.py ../docs/deployment-handoffs/evidence/cl-split-prototype-20261102 --output target/split-replay
```

Prerequisites: the pinned Rust/Stellar toolchain, local Cargo dependencies,
OpenZeppelin sibling source, pinned Aquarius fixture, and the existing
`bots/margin-liquidator/package-lock.json` dependencies for offline XDR decoding.
No account identity or credential is needed. Fresh output paths are mandatory.

Missing gates include absent-owner pre-delivery liquidation, new escrow restoration,
larger concurrent histories, full pause/config mutation coverage, signed costs and
live state. Existing baseline historical evidence is preserved separately.

`patch-encoding-verification.json` proves the published zero-context patch creates
the exact same nine overlay source files used by the measured build. The archives
retain the executed patch encoding; this formatting change removes blank diff
context markers from Git whitespace checks and changes no contract source.
