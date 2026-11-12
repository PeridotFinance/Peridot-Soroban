# Completed Linux CI archive — 9cf4308

This is a separate baseline archive, not the earlier Linux archives or isolated
split-transactions CI. Stage 1 remains incomplete; release approval is false.

Run [41473099204](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41473099204),
head `9cf43082bfb57334d57dbc58cfa90ccad4d98078`, actual PR merge checkout
`8b7d356e6a5edf9c53540f302cd142084a426c9a`. Checker job passed 32 tests;
checkpoint job failed after executing the required flows, and artifact upload
succeeded. This replaces pending-CI wording for that baseline only.

Artifact 13158618482 expires **2026-11-16 16:18:07 UTC**. All 75 original members
and 22 exact HEAD replay-source members are preserved losslessly. Original ZIP
compression is replaced; member bytes are unchanged. Original ZIP SHA256:
`a9fdb21a4c0327609a09f162859ce31259d64b54905c06948588fe8e711b2d83`.
Durable payload: 2162904 bytes, SHA256
`bd2f7c13727ffeede8a0f46fbcea395b9f03337ff0f309b81ba5cc175d42cd6a`. The original 200.95 MB ZIP need not be committed.

| Profile | Invocation | Lifecycle | Transaction | Checkpoint | Invocation error rows | Lifecycle errors | Transaction errors |
|---|---|---|---|---|---:|---:|---:|
| internal-policy | FAIL | FAIL | FAIL | FAIL | 31 | 106 | 303 |
| captured-network-diagnostic | PASS | PASS | PASS | PASS | 0 | 0 | 0 |

Internal compiled suite: 4 passed / 32 failed; five archival reruns failed.
Captured: 36 passed / 0 failed, plus 3 + 2 passing archival reruns.
Integrity: one pass per profile. Native funded: internal 5 pass / 4 fail;
captured 9 pass. Shared default-margin native suite: 110 pass / 2 ignored;
reward vault: 53 pass. Exact named outcomes, gate summaries, error arrays,
SDK maxima and all ten separate WASM identities are retained in
`completed-tests-and-identities.json`; unmodified raw evidence is in the payload.

`replay-verification.json` confirms both profiles' individual gates, exact error
multisets and invocation resource/error/fee blocks. Historical aggregate exit1
alone was not accepted. All payload members and exact source hashes were checked.
To replay from `peridot-contracts` with the pinned Node dependencies installed:

```sh
python3 scripts/replay_cl_linux_ci.py PATH_TO_THIS_DIRECTORY --output target/cl-linux-9cf-replay-fresh
# Expected exit1 AND both replay-verification records true; mismatch is exit2.
```

This extraction initially used the same replay tool with an in-memory exact
capture registry entry; the lead subsequently registered that same identity.
The extraction task performed no repository gate/source/policy edits or contract
rerun, signing, broadcasting, secret access, or deployment. Controlled-state
protocol29 / synthetic external authorization / unsigned simulation remains
distinct from real signatures and live-state validation. Linux binaries must not
be relabelled as Mac builds or isolated split prototype binaries.

## Separate Linux WASM identities

| Artifact role/path | Bytes | SHA256 |
|---|---:|---|
| artifacts/cl_integration_fixtures.wasm | 3168 | `bff4b6856c0199893515ca688c674847c23eedd097fe143c1b7734691bcd7f7f` |
| artifacts/cl_share_pool_facade.wasm | 7298 | `d3c10c9b2d6e29a66b3b3e2c9ad7077b1dcea7268d284e1da3e306548217a093` |
| artifacts/cl_share_vault.wasm | 85747 | `65e239b6ddb13a395e98ccfb8f04fa46c858b632c42943df78612efd9e13cb63` |
| artifacts/jump_rate_model.wasm | 7644 | `1416b1f0bc441ddcda230d9e18759ffa0d3f24b7fe0b70172ca01bb3aaa3312a` |
| artifacts/margin_controller.wasm | 91657 | `8d4d3ae0d645c2da44c6519385b038268258d9bfda1b1caeb68b4c80e8e789e4` |
| artifacts/receipt_vault.wasm | 87197 | `510481f6ab76f8a7eac3f18cc7fb7ff1583e3c8a40d5f333f18e855ff2bf50dc` |
| artifacts/simple_peridottroller.wasm | 63289 | `4d40bddeda038fa189b6d667861a4690b1a47cb6b8fdfb64eeff41ac8a63ce96` |
| artifacts/swap_adapter.wasm | 10876 | `28816ff4592011626b7b77ae24bde23de1763c13d7e5f14ecc178c163b53ac82` |
| default-artifacts/margin_controller.wasm | 87123 | `2e3db3a49bc7ddab576a1f2d9d72e2b938933f9d32daa31df421556173f28d6c` |
| legacy-receipt/artifacts/receipt_vault_legacy_abi.wasm | 80262 | `8cb3797e21d280922f0fc7b8e5f111668951c8429675dbcc5c234c3e760bc439` |
