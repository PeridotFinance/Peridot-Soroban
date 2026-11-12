# Linux storage prototype CI: 627e2e2

PR75 head `627e2e2c926b2f62dbb1b1cd0ef6f9949caf2d6d`, tested merge `1728feeebe092280ffc650c11b8cda1b2e4df3d6`. Stage1 remains incomplete and release approval false. This archive is separate from prior Linux captures and local prototype evidence.

[Completed CI run](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41613837952) failed. Payload `storage-ci-payload.tar.xz` is 11,259,004 bytes, 546 members; SHA256 `69d7c40f473caee457bd4f34d8bf0d66a82c4e6358af6a1d9b8da975ebe33849`.

Every original ZIP size and GitHub digest was checked. Every tar member was independently compared byte-for-byte with its original ZIP or pinned Git source. The index preserves exact hashes, bytes, artifact expiry, complete per-profile gates/errors and WASM identities. Named test outcomes and Cargo totals, proof/report/log hashes and source pins were separately verified. No full checker/XDR replay or new execution is claimed.

| Artifact | ZIP bytes | ZIP SHA256 | Expiry |
| --- | ---: | --- | --- |
| 13222091360 | 210174949 | `da93b442026448eff500910bfc91037ecfbec5b3afd1500f5f388b42cb38d95c` | 2026-11-17T10:13:43Z |
| 13221271132 | 52956783 | `5e5fb79fdd4e9b76bb2d1fe80bab59f42b71302765f4fbb3a5a03da82ff0704a` | 2026-11-17T09:58:24Z |

Both profile jobs failed and uploaded their evidence successfully. Each metadata helper suite passed7 tests. Baseline artifacts reuse the pinned atomic baseline WASM; optimized candidate artifacts were freshly compiled in Linux. This workflow does not execute the subsequent repayment correction or explicit restore-batch prototype.

| Profile / build / view | Tests pass/fail | Invocation | Lifecycle | Transaction | Checkpoint | Lifecycle errors | Transaction errors | Missing resource rows |
| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: |
| captured-network-diagnostic / baseline-cold | 4/0 | PASS | FAIL | FAIL | FAIL | 2 | 379 | 28 |
| captured-network-diagnostic / baseline-live | 4/0 | PASS | FAIL | FAIL | FAIL | 2 | 46 | 0 |
| captured-network-diagnostic / baseline-data | 4/0 | PASS | FAIL | FAIL | FAIL | 2 | 46 | 0 |
| captured-network-diagnostic / candidate-cold | 4/0 | PASS | FAIL | FAIL | FAIL | 2 | 379 | 28 |
| captured-network-diagnostic / candidate-live | 4/0 | PASS | FAIL | FAIL | FAIL | 2 | 46 | 0 |
| captured-network-diagnostic / candidate-data | 4/0 | PASS | FAIL | FAIL | FAIL | 2 | 46 | 0 |
| internal-policy / baseline-cold | 0/4 | FAIL | FAIL | FAIL | FAIL | 18 | 92 | 5 |
| internal-policy / baseline-live | 0/4 | FAIL | FAIL | FAIL | FAIL | 18 | 62 | 2 |
| internal-policy / baseline-data | 0/4 | FAIL | FAIL | FAIL | FAIL | 18 | 64 | 2 |
| internal-policy / candidate-cold | 0/4 | FAIL | FAIL | FAIL | FAIL | 18 | 249 | 21 |
| internal-policy / candidate-live | 0/4 | FAIL | FAIL | FAIL | FAIL | 18 | 63 | 2 |
| internal-policy / candidate-data | 0/4 | FAIL | FAIL | FAIL | FAIL | 18 | 64 | 2 |

Live/data views copy their source fixture log; do not sum those counts as additional executed tests. Captured baseline and candidate each pass all4 fixture assertions. Live and data comparisons each match151 complete A/B records, with59 maintenance-changed rows requiring review. Cold comparison explicitly rejects missing complete host evidence at row34; it is not a passing equality proof.

Internal baseline and candidate each fail4/4 fixture tests. Every terminal panic is classified from its recorded diagnostic as Budget/ExceededLimit (CPU and/or SDK entry totals), not a synthetic administrator setup or compiler failure. All three comparisons reject incomplete/mismatched row coverage. Captured cold host/resource failures and missing quantities remain failures even though fixture assertions pass. Full exact summary errors are preserved rather than collapsed into exit1.

Fresh Linux build logs explicitly show receipt-vault and simple-peridottroller compilation for both profiles. Corrected receipt contract source SHA256 `a215cb99cdcb7b5ae54cedc743137d0e3d657fbab378bd59c11b6463362cf58e` produces the same88,329-byte receipt WASM in both profiles: `d36d3620732a2a6b2773cdc27a4e323b2db24d6620f06c086d5b1157a2637974`. Controller63,411-byte SHA256: `9130ee98cc37a38bd2f1fd459d6f03c3d1a0ba0da8b5b322c425fbff267c6870`.

These identities differ from the prior local88,231-byte receipt artifact `292801...`. That older execution remains evidence for its exact bytes; this new Linux compilation cannot retroactively turn it into corrected-source coverage. The new index and verification summary preserve complete manifests, code identities and explicit compilation lines.

Verify preservation without network access, signing or contract execution:

```sh
python3 docs/deployment-handoffs/evidence/cl-storage-ci-627e2e2-20261103/verify-archive.py
```

The verification scripts beside the archive retain their original preparation paths; `verify-archive.py` is the portable exact-member check. No old evidence, contracts, resource profiles, transaction stages, handoff00, borrowing configuration or production maintenance margin was modified. No secrets, signing, broadcasting, deployment or Upshift binding occurred.
