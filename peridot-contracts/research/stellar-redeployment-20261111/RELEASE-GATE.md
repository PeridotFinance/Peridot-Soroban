# Fresh lending release evidence — scoped disposition

Both original medium findings have executable fixes/tests. The accepted flash guard remains in place. Final source snapshot e3eab76a74028a394b0cde4e48c2ec42662811cc has a completed Almanax rescan: **one HIGH legacy-upgrade incompatibility and one LOW fail-closed missing pause map**, no medium. Raw JSON and all earlier findings are preserved. The HIGH is a real unsupported upgrade path; the user forbids old-contract reuse/upgrade. Compiled fresh initialize→permanent seed→deposit→full withdrawal→next deposit passes, while synthetic nonempty/unseeded legacy state explicitly reproduces rejection. This is the required test-backed fresh-only non-applicability disposition, not a scan with zero high findings or an API dismissal.

## Five requested checks

| Check | Local evidence | Mandatory deployment readback |
|---|---|---|
| Clean WASMs | PASS: final three artifacts independently reproduced from an empty target directory, exact SHA256 match | Each upload and fresh instance must match medium-final-build-hashes.json |
| Borrowing/flash disabled durably | PASS: explicit false at initialization, missing permission false, missing reserve configuration retains all cash; actual-controller expiry/lift tests and cap/restoration tests | Every market false, reserve Some(10000), debt0; sealed code identity establishes the shared flash guard |
| Ledger roles / deployer no rights | Ledger-pinned initializer build, compiled source-account initializer test, native wrong/missing-auth tests; not a hardware-signature test | All admin/guardian values exact Ledger, no pending admin/upgrade; throwaway absent from every role |
| Local cash, no allocation | Default fresh state, zero-rate no-oracle native controller journey, final ABI inspected; no hybrid reward module | No BoostedVault, MarginController, InterestModel, Oracle, PERI or router; ClCustody false |
| Later CL listing without redeploy | Existing add_market ABI; native fresh controller add/bind succeeds, default CF0 | Exact fresh controller binding and supported-market readback; later CL remains separate admin action |

Native: 5 accounting/configuration + 13 reserve/flash tests PASS; 2 rate denominator tests PASS for their stated cases. Compiled pinned host29: getter/TTL, financial cap restoration, and actual fresh initialization lifecycle all PASS under both profiles. The report distinguishes SDK entries, unique transaction keys, writes, fees/rent and source-account host authorization. No signing or Core application is claimed. The legacy full unit suite was not broadly rerun.

The global CL checker retains invocation PASS and lifecycle/transaction/checkpoint FAIL for missing required global CL proofs. Those failures are not relabelled, swallowed or presented as CI approval. They are distinct from this scoped fresh cash-market remediation, and do not authorize CL deployment or borrowing activation.

The command list is prepared in COMMANDS-DRAFT.md. New addresses, deployer fee funding and separate per-market permanent/withdrawable seed amounts remain operator inputs. Every actual transaction still requires current simulation, review, operator signature, one submission and post-transaction readback; none was performed here. No old contract was invoked. Ledger-only reserve/fee withdrawal and strategy binding remain accepted powers, explicitly not used in supply-only mode. Maintenance margin stays unset.

## Measured pinned-host envelope

The final `v6-run2` executes 3 Rust tests per profile, containing 44 preflights (30 successful, 14 expected rejections, 15 restoring) and 23 warm SDK roots. Maxima below are independent observations, not a summed transaction. The actual compiled initializer is among the SDK roots (4,619,414 CPU, 58 entries, 23 writes); preflight transaction maxima are separate.

| Metric | Internal profile | Captured diagnostic | Internal ceiling |
|---|---:|---:|---:|
| SDK CPU | 16,008,521 | 16,008,521 | 100,000,000 |
| SDK memory bytes | 1,666,770 | 1,666,770 | 41,943,040 |
| SDK entry total | 58 | 58 | 100 |
| SDK writes | 23 | 23 | 50 |
| Unsigned host CPU | 17,524,364 | 17,524,364 | 100,000,000 |
| Unsigned host memory bytes | 3,045,578 | 3,045,578 | 41,943,040 |
| Unique transaction keys | 43 | 43 | 100 |
| Unsigned transaction writes | 14 | 14 | 50 |

Both invocation gates PASS. Both lifecycle/transaction/checkpoint gates FAIL with the same 46 missing global labels and recorded errors. The runner exits 1 honestly. No signature/application or live four-market capacity claim follows from this bounded controlled-state suite. Fee/rent rows, exact footprint/XDR, state comparisons, failed fixture attempts and the three final WASMs are retained in `evidence/medium-final-evidence.tar.xz` with a per-member manifest. The earlier archive remains separate and unchanged.
