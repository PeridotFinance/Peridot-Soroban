# Atomic Option 1 qualification — isolated experiment

Stage 1 remains incomplete. This directory does not change production contracts,
transaction stages, resource profiles, borrowing settings or release approval.
The proposed 240M workload boundary is an analysis constant, not a runnable
profile. Every execution uses an existing validated profile and finite limits.

The report is [handoff 16](../../../docs/deployment-handoffs/16-cl-atomic-qualification-20261102.md).
The negative split and all earlier checkpoints remain separate.

## Reproduce the focused matrix

Run from `peridot-contracts` with the existing pinned Cargo/Node dependencies,
Rust 1.96.0, Stellar CLI 26.1.0 and public Aquarius fixture already available.
Use fresh output paths; preparation and runners refuse overwrites. There is no
network, key, signing, deployment or submission step in these commands.

```sh
python3 prototypes/cl-atomic-qualification/prepare.py \
  --output target/atomic-baseline-build
python3 prototypes/cl-atomic-qualification/prepare.py \
  --output target/atomic-candidate-build \
  --optimization prototypes/cl-atomic-qualification/optimization.patch

CL_ATOMIC_BUILD_DIR="$PWD/target/atomic-baseline-build" \
  bash prototypes/cl-atomic-qualification/run.sh \
  captured-network-diagnostic atomic_qualification target/atomic-baseline-live
CL_ATOMIC_BUILD_DIR="$PWD/target/atomic-candidate-build" \
  bash prototypes/cl-atomic-qualification/run.sh \
  captured-network-diagnostic atomic_qualification target/atomic-candidate-live
CL_ATOMIC_BUILD_DIR="$PWD/target/atomic-baseline-build" \
  bash prototypes/cl-atomic-qualification/run.sh \
  internal-policy atomic_qualification target/atomic-internal
```

The internal run is expected to exit nonzero; retain its failures. The runner
always invokes `check_budget.sh` on the resulting flow evidence, records the
test and selected invocation-gate exit codes separately, and saves every gate
and error. Captured invocation success does not grant lifecycle, transaction,
checkpoint or release approval. The focused proof map deliberately does not
claim the omitted full Stage 1 proofs.

For restored-state measurements, correct the isolated recording exporter first:

```sh
python3 prototypes/cl-atomic-qualification/repair_recording_evidence.py \
  --build target/atomic-baseline-build --output target/atomic-baseline-restored-build
python3 prototypes/cl-atomic-qualification/repair_recording_evidence.py \
  --build target/atomic-candidate-build --output target/atomic-candidate-restored-build

CL_ATOMIC_BUILD_DIR="$PWD/target/atomic-baseline-restored-build" \
CL_TRANSACTION_ARCHIVE_ROLE=vault,cash_receipt,share_receipt,v3,aquarius,lending_controller \
  bash prototypes/cl-atomic-qualification/run.sh captured-network-diagnostic \
  atomic_qualification target/atomic-baseline-restored
CL_ATOMIC_BUILD_DIR="$PWD/target/atomic-candidate-restored-build" \
CL_TRANSACTION_ARCHIVE_ROLE=vault,cash_receipt,share_receipt,v3,aquarius,lending_controller \
  bash prototypes/cl-atomic-qualification/run.sh captured-network-diagnostic \
  atomic_qualification target/atomic-candidate-restored
```

The correction changes only post-execution evidence projection. Host29 recording
adds footprint-only placeholder records for handled failed subcalls; these must
not be interpreted as deletions. Raw classifier fields are exported. Resources,
fees, restoration indices and TTL changes are unchanged. Effective restored TTL
and rent for such a placeholder still require transaction-application evidence.
No contract instance/code restoration or live-network state is claimed.

The separately targeted former peak uses the same runner with filter
`compiled_stress_four_holders_shifted_pool`, once per baseline/candidate build.
This reuses an existing case; it is not the larger-history matrix or a reserve
configuration proof.

```sh
node prototypes/cl-atomic-qualification/compare.cjs \
  target/atomic-baseline-live target/atomic-candidate-live target/atomic-live-diff.json
node prototypes/cl-atomic-qualification/compare.cjs \
  target/atomic-baseline-restored target/atomic-candidate-restored target/atomic-restored-diff.json
node prototypes/cl-atomic-qualification/archive-counts.cjs \
  target/atomic-baseline-restored target/atomic-restoration-counts.json
python3 prototypes/cl-atomic-qualification/assess.py \
  --run baseline-live=target/atomic-baseline-live \
  --run candidate-live=target/atomic-candidate-live \
  --run baseline-restored=target/atomic-baseline-restored \
  --run candidate-restored=target/atomic-candidate-restored \
  --run internal=target/atomic-internal --output target/atomic-assessment.json
```

Analysis outputs include exact remaining authorization cases and hashes. No
signing is performed. Fee sums are aggregates of evidence records, including
probes; restored roots independently expire state and are not a sequential
user journey. Unsigned templates cannot establish final authorization costs.

## Evidence scope

Baseline binaries come from the hash-verified local atomic archive preserved at
`9cf4308`, not a mutable Cargo output directory or the split prototype. The
candidate builds only the isolated receipt-vault patch using
`stellar contract build`. Production sources remain byte-identical to the
147-source baseline pins. SDK/host29.0.1 and all captured cost terms are checked.

Four compiled cases combine 64 owners, 128 extra ownership transfers, at least
407 checkpoints, eight independent borrowers, eight unresolved intervals,
actual custody changes, both reward calls failing, and principal/reward exits.
The open case also tests the existing 64-position vector using 63 ordinary
begin-only pending positions; it does not pretend they carry debt. Reserve
1,000 bps and borrowing enablement are **test only**; production is unchanged.
Dynamic interest covers funded settlement/recovery and aged open. Full
repay/release remains explicitly static-interest evidence.

Fresh SDK hosts retain byte-identical ledger snapshots, TTLs and ledger settings,
plus SDK address/nonce generators. This removes accumulated fixture objects at
transaction boundaries, never work inside a measured contract invocation.
Only external synthetic actors receive narrow `mock_auths`. V3, receipt, facade
and vault contract authorization executes; no blanket bypass is introduced.

See [workload/authentication plan](authorization-and-workloads.md) and the
[isolated patch review](optimization-review.md). Their remaining cases are
qualification gaps, not tests silently counted as completed.
