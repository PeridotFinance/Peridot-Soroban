# Linux baseline CI evidence: 90111e2

This preserves completed baseline CI for PR75 head
`90111e2dc1a3cc9ffd941eb3661459e6441e1280`, checked out as PR merge commit
`57d27fb5abd6244e67a8f74a1dc0f1e5b79fc044`.
The [workflow run](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41492773441)
completed with **failure**. Checker job `125439781352` succeeded; checkpoint job
`125439781056` failed its gate step and successfully uploaded the evidence.
Stage 1 remains incomplete, PR75 draft, and release approval false.

This workflow executes the existing baseline checkpoint runner. It does **not**
execute either the split-transaction prototype or the atomic-qualification
prototype. No result here qualifies their changes.

## Preserved identities

| Item | Identity |
| --- | --- |
| Original artifact ID | `13167198650` |
| Original artifact name | `cl-stage1-evidence-57d27fb5abd6244e67a8f74a1dc0f1e5b79fc044` |
| Original GitHub expiry | `2026-11-16T18:21:25Z` |
| Original ZIP size | `200952610` bytes, 75 members |
| Original ZIP SHA-256 | `a041000a38963ceed67bcb7b13f501622f97ba77fbb36817ca3ebf6adee760f9` |
| Durable TAR.XZ size | `2162664` bytes, 97 members |
| Durable TAR.XZ SHA-256 | `5f1743a5529e698b24e930f7768b1c7d621e0273ba479ab93b2f48adbc45c0fe` |
| Index SHA-256 | `fd85cd854eeb4a604cc4923b77a465da0cf9ed23db24f8e64fd3f2ea4a28f1f2` |
| Captured configuration SHA-256 | `a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd` |

[index.json](index.json) contains the exact identities of all **10 Linux WASMs**,
including the separate default V3 and historical receipt ABI fixture, and hashes
and lengths of every archive member. These Linux identities are separate from
local builds and older Linux captures. [linux-ci-payload.tar.xz](linux-ci-payload.tar.xz)
losslessly preserves every original ZIP member plus the pinned checker/proof
source needed for offline replay. The original ZIP need not remain available
on GitHub to recover this evidence.

[github-metadata.json](github-metadata.json) preserves the read-only GitHub
artifact/job response; [capture-identity.json](capture-identity.json) records the
exact capture registration. [checker-job.log](checker-job.log) preserves the
completed checker job's raw log, including the checked-out merge identity.
[supporting-checksums.json](supporting-checksums.json) hashes these supporting
files and the verification summary. The original index and TAR.XZ were not
modified when adding these supporting files.

## Gate results and completed tests

| Profile | Invocation | Lifecycle | Transaction | Checkpoint | Missing required flows | Lifecycle errors | Transaction errors |
| --- | --- | --- | --- | --- | ---: | ---: | ---: |
| `captured-network-diagnostic` | PASS | PASS | PASS | PASS | 0 | 0 | 0 |
| `internal-policy` | FAIL | FAIL | FAIL | FAIL | 0 | 106 | 303 |

Zero missing flow labels does not mean their proofs passed. Internal-policy
tests and measurements failed, and those failures remain visible in the
separate error arrays and raw logs. Aggregate exit 1 is not used as a substitute
for the per-profile gates and errors.

| Executed suite | Captured profile | Internal profile |
| --- | --- | --- |
| Compiled baseline harness | 36 passed, 0 failed | 4 passed, 32 failed |
| Compiled archive reruns | 3 passed, 0 failed | 0 passed, 3 failed |
| Compiled history/archive reruns | 2 passed, 0 failed | 0 passed, 2 failed |
| Runtime integrity | 1 passed, 0 failed | 1 passed, 0 failed |
| Native funded cases, separate host25 evidence | 9 passed, 0 failed | 5 passed, 4 failed |

Shared suites: checker **32 passed**; native vault reward tests **53 passed**;
default margin regression **110 passed, 2 ignored**. Archival reruns repeat
selected compiled tests and must not be added to the 36 as distinct tests.
Each profile has 46 mapped proof labels. Actual per-test terminal outcomes and
complete gate/error arrays are retained in [verified-summary.json](verified-summary.json).

Captured-profile measurements comprise 1,652 SDK rows and 2,741 unsigned
target-runtime records. Maximum SDK CPU/memory/entry-total/write values are
235,456,305 instructions / 24,514,601 bytes / 213 / 56. Maximum unsigned CPU,
memory, unique footprint and writes are 236,694,863 / 21,678,566 / 158 / 57;
maximum serialized unsigned envelope is 15,092 bytes. Maximum total estimated
fee is 22,845,643 stroops; 733 records contain restoration indices. These maxima
need not occur in the same transaction. SDK entry totals and unique transaction
footprint keys are different measurements. Real signatures and live-state
validation remain outside this controlled-state evidence.

## Verification performed and replay command

The preservation check independently matched GitHub's ZIP digest and length,
verified the durable archive's digest and every member's digest/length, and
compared each archived artifact member byte-for-byte with the original ZIP.
It checked the saved gate values and every error against the corresponding
checkpoint log; bound proof-map/source hashes to the exact source commit;
verified resource, call and test-log hashes; and parsed completed libtest
outcomes. **A full checker replay was not performed during this preservation
task.** The summary explicitly records `replay_performed: false`.

After `90111e2` is registered with these exact fields in
`peridot-contracts/scripts/cl_ci_captures.py`, run from `peridot-contracts`
with the repository's pinned Node dependencies already installed:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/replay_cl_linux_ci.py \
  ../docs/deployment-handoffs/evidence/cl-linux-ci-90111e2-20261102 \
  --output target/cl-linux-ci-90111e2-replay
```

The output directory must be new. The replay verifies each profile's gate
summary, exact error multiset and invocation resource/error rows; exit **1**
means the recorded failure was reproduced, while exit **2** means verification
failed. Inspect `replay/replay-verification.json` and both profile results;
exit 1 alone is insufficient. This README supplies the command, not a claim
that it has already been executed for this capture.

All prior archives remain independent. No signing, broadcasting, secrets,
Upshift binding, borrowing enablement, production maintenance-margin setting,
policy change or handoff00 edit is authorized or implied by this evidence.
