# Focused atomic qualification evidence — 2026-11-02

Stage1 incomplete. No policy, production contract or transaction-stage adoption.
The recommendation is to require storage-access/restoration optimization before
another atomic qualification decision. The receipt TTL patch remains isolated.

`qualification-payload.tar.xz`:4,027,688 bytes,
SHA256`197b008abc841be68756d4f5853297fd46d27d044afabfcf931dace3737fd207`.
All364 members were independently verified against [index.json](index.json).
The payload preserves seven final runs, exact baseline/candidate WASMs, generated
harness sources,147 baseline source pins, original and repaired checkers,
commands/configuration, raw unsigned XDR/resource/call reports, proof mappings,
42-test log, paired comparisons, reviews, authorization cases and exact gate replay.

The run source parent is90111e2, atomic reference9cf4308; the bb154ec split stays a
negative result. Baseline/candidate receipt identities are distinct. Source-only
changes to production contract files and profiles are absent. Pinned SDK/host29.0.1
uses all captured cost terms from ledger65,270,655. This is controlled-state
bytecode execution with unsigned recording templates, not live-state or signed
transaction qualification.

## Results and separate gate layers

| Population | Compiled tests | Invocation gate | Lifecycle/transaction/checkpoint gates |
| --- | --- | --- | --- |
| Baseline/candidate live, each |4 PASS|PASS|FAIL|
| Baseline/candidate restored, each |4 PASS|PASS|FAIL|
| Baseline/candidate former peak, each |1 PASS|PASS|FAIL|
| Existing internal policy |0 PASS /4 FAIL|FAIL|FAIL|

Every replay selects the checkpoint gate and exits1. Verification compares each
summary field, missing-proof label and error string, not aggregate exit1.
Combined captured reports retain2 lifecycle/46 transaction errors. Internal
retains18/62. Original restored gate reports had142 transaction errors; the
repaired decoder resolves exactly96 recording-projection errors in each build,
leaving46 missing full Stage1 proof errors. Gate booleans and release refusal
are unchanged. Both original summaries and corrected replay are preserved.

The assessment's `existing_gates` are the original run summaries; use
`gate-replay/index.json` and the top-level index's `replayed_gates` for the audited
post-decoder replay. Internal partial observed rows fitting a comparison boundary
are never a completed workload: that run has0/4 completed tests and two failed
host captures. The separate former-peak cases likewise are not full lifecycle runs.

New baseline swap-open peaks at242,307,129 CPU live/244,149,068 restored. The
candidate reaches237,646,433/239,488,372. Its restored CPU distance to240M is only
511,628 before real authorization. Writes remain81 open,98 activation,89 close,
104 liquidation, exceeding the proposed80 workload boundary.104 also exceeds
the proposed100-write ceiling. No limit was changed to accommodate the result.

151 live,151 restored and17 former-peak baseline/candidate pairs match logical
state, events, authorization and reported TTL changes after normalizing only
pinned receipt executable identity. Three actual Aquarius claims remain in each
measured swap-open. All checked reward histories conserve exactly; exited owners
retain booked claims and unresolved intervals remain quarantined. Full repayment
and release in this matrix uses static interest; accrued funded recovery is separate.

## What the recording repair does and does not prove

Host29 recording can emit a zero-size/no-value/no-TTL placeholder when a nested
failed call is handled. The isolated exporter retains the original logical value
or absence for that precise case. The decoder validates the raw marker, actual
before-entry XDR, footprint membership, RW index and restoration membership,
and rejects malformed/contradictory markers. Execution/resource/fee/footprint/TTL
calculations do not change; all151 rows of each before/after exporter run match
on those quantities. Original failures remain in the separate archive below.

Restoration expires persistent non-instance data for six roles in each disposable
root.150 of151 records contain restoration indices, including expected rejection
probes; this is not150 confirmed network restorations. Code/instance archival and
effective restored TTL/rent for a handled failed subcall remain unqualified.
Fee sums include independent probes/re-expiry and are not a sequential lifecycle
price. No real account signature, network submission, live issuer state or
network latency is claimed.

## Reproduction

From `peridot-contracts`, use the [prototype README](../../../../peridot-contracts/prototypes/cl-atomic-qualification/README.md)
for fresh builds/runs and the [qualification report](../../16-cl-atomic-qualification-20261102.md)
for exact tests, remaining authorization experiment and decision. Fresh directories
refuse overwrite. Baseline bytes are extracted from the verified original archive;
only the isolated receipt candidate uses `stellar contract build`.

After reproducing the README's run paths, replay each profile and compare all gates:

```sh
python3 prototypes/cl-atomic-qualification/replay_gates.py \
  --run baseline-live=target/atomic-baseline-live \
  --run candidate-live=target/atomic-candidate-live \
  --run baseline-restored=target/atomic-baseline-restored \
  --run candidate-restored=target/atomic-candidate-restored \
  --run internal=target/atomic-internal --output target/atomic-gate-replay
```

Fresh repaired runs already contain correct gate summaries; the replay does not
remove arbitrary errors. Historical evidence has original absolute source/profile
paths; reproduce in fresh directories to regenerate portable path bindings, or
preserve the original workspace paths for an exact historical checker replay.
Archive integrity itself is independent of those paths:

```sh
python3 - <<'PY'
import hashlib,json,tarfile
from pathlib import Path
p=Path('../docs/deployment-handoffs/evidence/cl-atomic-qualification-20261102')
i=json.loads((p/'index.json').read_text());a=p/'qualification-payload.tar.xz'
assert hashlib.sha256(a.read_bytes()).hexdigest()==i['archive_sha256']
with tarfile.open(a) as t:
    assert set(t.getnames())==set(i['members'])
    for n,v in i['members'].items():
        b=t.extractfile(n).read()
        assert len(b)==v['bytes'] and hashlib.sha256(b).hexdigest()==v['sha256']
print('Exact archive/member integrity PASS; no release approval')
PY
```

[Failed experiments](failures/README.md) preserve165 files separately; they are not
pass proofs. [Completed Linux CI90111e2](../cl-linux-ci-90111e2-20261102/README.md)
is also separate and executes neither isolated prototype. No signing, broadcasting,
deployment, secrets, Upshift, production maintenance-margin or handoff00 changes.
