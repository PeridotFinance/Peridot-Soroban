# Linux baseline CI: 627e2e2

PR75 head `627e2e2c926b2f62dbb1b1cd0ef6f9949caf2d6d`, tested merge `1728feeebe092280ffc650c11b8cda1b2e4df3d6`. Stage1 remains incomplete and release approval false. This archive is separate from prior Linux captures and local prototype evidence.

[Completed CI run](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41613837844) failed. Payload `linux-ci-payload.tar.xz` is 5,015,620 bytes, 97 members; SHA256 `cc53ab0679eaaa4f1cad756647c449412f6a483d90d99965dec9b7444d317bf5`.

Every original ZIP size and GitHub digest was checked. Every tar member was independently compared byte-for-byte with its original ZIP or pinned Git source. The index preserves exact hashes, bytes, artifact expiry, complete per-profile gates/errors and WASM identities. Named test outcomes and Cargo totals, proof/report/log hashes and source pins were separately verified. No full checker/XDR replay or new execution is claimed.

| Artifact | ZIP bytes | ZIP SHA256 | Expiry |
| --- | ---: | --- | --- |
| 13222195810 | 200952629 | `403294ffdf9e5d8c2c3ba89b645dd77c2d26c8305a1245ab5a7239d31972d299` | 2026-11-17T10:10:21Z |

Checker job125842187722 passed42 tests. Checkpoint job125842188506 failed its gate step and uploaded evidence successfully. This workflow runs the baseline, not storage, repayment or restoration prototypes. All10 stored baseline Linux WASM hashes match the previous1198078 capture, independently verified.

| Profile | Invocation | Lifecycle | Transaction | Checkpoint | Lifecycle errors | Transaction errors |
| --- | --- | --- | --- | --- | ---: | ---: |
| internal-policy | FAIL | FAIL | FAIL | FAIL | 106 | 303 |
| captured-network-diagnostic | PASS | PASS | PASS | PASS | 0 | 0 |

Captured main36/36 pass, archival3/3 and history archival2/2 pass. Internal main4 pass/32 fail, archival0 pass/3 fail and history archival0 pass/2 fail. Zero missing flow labels does not erase failed assertions or gate errors. Native reward53 pass; default margin110 pass/2 ignored; separate host25 funded captured9 pass and internal5 pass/4 fail.

The capture registry was not edited by this preservation task. Full portable gate replay remains unperformed; do not substitute aggregate exit1 for each profile’s recorded gates and errors.

Verify preservation without network access, signing or contract execution:

```sh
python3 docs/deployment-handoffs/evidence/cl-linux-ci-627e2e2-20261103/verify-archive.py
```

The verification scripts beside the archive retain their original preparation paths; `verify-archive.py` is the portable exact-member check. No old evidence, contracts, resource profiles, transaction stages, handoff00, borrowing configuration or production maintenance margin was modified. No secrets, signing, broadcasting, deployment or Upshift binding occurred.
