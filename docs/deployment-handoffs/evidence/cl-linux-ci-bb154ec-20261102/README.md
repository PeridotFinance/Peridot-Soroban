# Completed Linux CI archive — bb154ec

This completed CI is separate from earlier Linux archives and local isolated
prototype experiments. Stage 1 remains incomplete; release approval is false.

Run [41484260698](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41484260698).
Head `bb154ec8f03889aabfdb8dd5ea120f22c46a47f5`; actual merge checkout `966d39f6771473a3e724524beed5d65f8e1deee7`.
Checker job passed 32 tests. Checkpoint job concluded failure after required flow
execution; artifact upload succeeded. Exact completed jobs are recorded separately.

Artifact 13163838118 expires **2026-11-16T17:25:37Z**.
Original ZIP SHA256: `5299122088d4efa52194de035aac76b6bedc6f9801a03c5c73694af71e200a43`.
All 75 original
members and 22 exact
HEAD replay-source members are preserved losslessly. Container compression is
replaced; original member bytes are unchanged. Durable payload SHA256:
`cb3e20a0aa62a94ad7312ecf8733e7cc992ed27532339932af054b8b6a0e56ca` (2162796 bytes).

## Profile results from completed CI

| Profile | Invocation | Lifecycle | Transaction | Checkpoint | Invocation error rows | Lifecycle errors | Transaction errors |
|---|---|---|---|---|---:|---:|---:|
| internal-policy | FAIL | FAIL | FAIL | FAIL | 31 | 106 | 303 |
| captured-network-diagnostic | PASS | PASS | PASS | PASS | 0 | 0 | 0 |

| Profile | Suite | Passed | Failed | Ignored |
|---|---|---:|---:|---:|
| internal-policy | integrity | 1 | 0 | 0 |
| internal-policy | harness | 4 | 32 | 0 |
| internal-policy | archive | 0 | 3 | 0 |
| internal-policy | history-scale-archive | 0 | 2 | 0 |
| internal-policy | native-funded | 5 | 4 | 0 |
| captured-network-diagnostic | integrity | 1 | 0 | 0 |
| captured-network-diagnostic | harness | 36 | 0 | 0 |
| captured-network-diagnostic | archive | 3 | 0 | 0 |
| captured-network-diagnostic | history-scale-archive | 2 | 0 | 0 |
| captured-network-diagnostic | native-funded | 9 | 0 | 0 |
| shared native | logs/default-margin-tests.log | 110 | 0 | 2 |
| shared native | logs/reward-tests.log | 53 | 0 | 0 |

Exact error arrays, named completed test outcomes, SDK cumulative-entry maxima,
unique unsigned-transaction footprints, and WASM identities are retained in
`completed-tests-and-identities.json`. The payload retains original raw logs,
proof manifests, configuration, resources and unsigned transaction records.

Verification checked original ZIP digest/CRC, every archived member hash, pinned
source/proof-map hashes, bound test-log hashes, named test outcome counts, each
profile's individual gate booleans and exact listed errors against the raw CI
logs, and all separate WASM hashes. It did not infer success from aggregate exit1.

**Full offline checker/XDR replay was not performed for this capture.** No
`replay-verification.json` is claimed. This artifact verification is distinct from
the earlier captures' full offline replay. No contracts or tests were rerun.

The exact index identity is registered in `scripts/cl_ci_captures.py`. For an
optional later full offline replay, install pinned Node dependencies, then from
`peridot-contracts` run:

```sh
python3 scripts/replay_cl_linux_ci.py PATH_TO_THIS_DIRECTORY --output target/cl-linux-bb154ec-replay-fresh
# Expect historical exit1 AND both per-profile verification records true.
# Exit1 alone is insufficient; mismatch is exit2.
```

Native funded tests are separate host25 evidence. Compiled acceptance and
unsigned simulation use protocol29.

Controlled-state protocol29 / actual pinned Aquarius bytecode / synthetic
external authorization / unsigned simulation is not real signature enforcement
or live-state approval. Linux binaries must not be relabelled as local Mac or
isolated prototype binaries. No signing, broadcasting, secrets or deployment.

## Separate Linux WASM identities

| Artifact role/path | Bytes | SHA256 |
|---|---:|---|
| artifacts/cl_integration_fixtures.wasm | 3168 | `bff4b6856c0199893515ca688c674847c23eedd097fe143c1b7734691bcd7f7f` |
| artifacts/cl_share_vault.wasm | 85747 | `65e239b6ddb13a395e98ccfb8f04fa46c858b632c42943df78612efd9e13cb63` |
| artifacts/cl_share_pool_facade.wasm | 7298 | `d3c10c9b2d6e29a66b3b3e2c9ad7077b1dcea7268d284e1da3e306548217a093` |
| artifacts/jump_rate_model.wasm | 7644 | `1416b1f0bc441ddcda230d9e18759ffa0d3f24b7fe0b70172ca01bb3aaa3312a` |
| artifacts/margin_controller.wasm | 91657 | `8d4d3ae0d645c2da44c6519385b038268258d9bfda1b1caeb68b4c80e8e789e4` |
| artifacts/receipt_vault.wasm | 87197 | `510481f6ab76f8a7eac3f18cc7fb7ff1583e3c8a40d5f333f18e855ff2bf50dc` |
| artifacts/simple_peridottroller.wasm | 63289 | `4d40bddeda038fa189b6d667861a4690b1a47cb6b8fdfb64eeff41ac8a63ce96` |
| artifacts/swap_adapter.wasm | 10876 | `28816ff4592011626b7b77ae24bde23de1763c13d7e5f14ecc178c163b53ac82` |
| default-artifacts/margin_controller.wasm | 87123 | `2e3db3a49bc7ddab576a1f2d9d72e2b938933f9d32daa31df421556173f28d6c` |
| legacy-receipt/artifacts/receipt_vault_legacy_abi.wasm | 80262 | `8cb3797e21d280922f0fc7b8e5f111668951c8429675dbcc5c234c3e760bc439` |
