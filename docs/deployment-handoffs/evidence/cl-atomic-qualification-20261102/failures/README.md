# Historical failed atomic-qualification experiments

This is **negative evidence only**. It preserves earlier fixture, export and
comparison failures without changing their logs or treating successful partial
tests as Stage1 approval. Stage1 remains incomplete; release approval is false.
Current experiments and successful final evidence belong in separate packages.

[index.json](index.json) records the archive SHA-256, compressed/uncompressed
sizes, exact member set, per-file hashes and lengths, and failure type for each
experiment. [failure-payload.tar.xz](failure-payload.tar.xz) uses XZ preset3 and
contains only the explicitly selected files listed in that index. Member paths
are relative to `peridot-contracts/target`; no original file was modified.

## Preserved failures

| Experiment | Historical result and classification |
| --- | --- |
| v1 baseline captured, optimized captured, baseline archived | Each completed 3 tests and failed1 with `Budget/ExceededLimit`; saved gates also report incomplete lifecycle/transaction provenance. These partial results are not a complete acceptance proof. |
| v2 baseline captured, optimized captured, baseline archived, baseline internal | `Auth/ExistingValue` fixture failures followed by terminated test processes. The preserved reason identifies consumed mock nonces restored with ledger data while the mock generator reset. This is a fixture failure, not an adversarial authorization-rejection proof. |
| v3 baseline archived and optimized archived | Each completed all4 compiled tests, but the archive exporter produced false touched-state mismatches. The original failing checker output remains intact; passing functional tests do not erase the evidence-export failure. |
| open diagnostic and diagnostic2 | Each failed its single diagnostic test with `Budget/ExceededLimit`. Logs and available measurements are preserved, including empty files where execution did not reach measurement. |
| v3 differential live and known-peak, initial and `-corrected` JSON outputs | Earlier JavaScript comparison-normalizer false differences. Both historical attempts are retained even where byte-identical. The later successful `-final.json` outputs are deliberately outside this negative package. |

The package also preserves the explicitly listed `cl-atomic-v1-sources` and
`cl-atomic-v2-failed-sources` prototype snapshots. These are historical source
collections, not an assertion that every diagnostic used every file in them.
Each run's original `source-before.json` remains the authority for its recorded
input hashes; this package does not reconstruct missing binaries or claim a
fresh replay. V3 archived runs retain their own source hashes and metadata;
successful final v3 sources are not imported here.

The allowed run files are test/budget logs, JSON/JSONL evidence, profile/network
configuration, proof/hash/build metadata and exit-code texts. Source inclusion
uses an exact path list for our prototype files. The archive excludes active
v4 runs, successful final v3 live run files, final differential outputs, Cargo
caches/build trees, unrelated target data, secrets and signed artifacts.
Unsigned controlled-state transaction records are evidence, not signed
transactions or broadcasts.

## Integrity verification

Packaging rehashed every original selected file after compression and checked
that its size and SHA-256 were unchanged. It then read every archive member,
checked exact set equality, and verified every size and SHA-256 independently.
The following repeats the archive/member check from this directory without
extracting, executing contracts or contacting a network:

```sh
python3 - <<'PY'
import hashlib, json, tarfile
from pathlib import Path
root = Path('.')
index = json.loads((root / 'index.json').read_text())
archive = root / 'failure-payload.tar.xz'
assert archive.stat().st_size == index['archive_bytes']
assert hashlib.sha256(archive.read_bytes()).hexdigest() == index['archive_sha256']
with tarfile.open(archive, 'r:xz') as tar:
    members = tar.getmembers()
    assert len(members) == len({m.name for m in members}) == index['members_count']
    assert {m.name for m in members} == set(index['members'])
    for member in members:
        assert member.isfile()
        data = tar.extractfile(member).read()
        expected = index['members'][member.name]
        assert len(data) == expected['bytes']
        assert hashlib.sha256(data).hexdigest() == expected['sha256']
print('Negative-evidence archive integrity verified; no pass proof or release approval.')
PY
```

A matching archive proves preservation only. It neither reproduces the tests
nor resolves their failures. No limits, transaction stages, production code,
registry, borrowing setting, signing/broadcasting behavior, Upshift binding,
production maintenance margin or shared00 policy changed during packaging.
