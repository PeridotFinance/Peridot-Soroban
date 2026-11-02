# Completed Linux CI archive — ac63255

This is the completed second Linux archive, separate from `../cl-linux-ci-20261102/`. Stage 1 remains incomplete; PR #75 remains draft. It supersedes pending-CI wording for source ac63255 only; it is not execution evidence for subsequent patches.

Run [41449397462](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41449397462), source `ac6325589a0095084c2df641d9b743860b4795f8`, merge `904f2aa7a106dbcfe6d3af5c392dec434ab9a143`. Checker job succeeded; checkpoint job failed during actual execution/gates; bootstrap and artifact upload succeeded. Artifact13146343183 was preserved before expiry2026-11-16T13:40:11Z.

Original ZIP SHA256: `33dcb99170894384466ebe6ac9d0e3d2fc95dfeaec599fc2066eb682f6a56701`. Durable payload SHA256: `3f1496b938090f43ef248c719a938da8ec7a4705d762fd70380c6dd630f8fd5f`. All50 original members and21 exact replay source members are losslessly preserved; original ZIP container compression is replaced, member bytes are unchanged.

| Profile | Invocation | Lifecycle | Transaction | Checkpoint | Lifecycle errors | Transaction errors |
|---|---|---|---|---|---:|---:|
| internal-policy | FAIL | FAIL | FAIL | FAIL | 97 | 284 |
| captured-network-diagnostic | PASS | PASS | PASS | PASS | 0 | 0 |

Captured:30 main+3 archive+2 history-archive tests pass; internal:3 main pass/27fail and all5 archival tests fail. Integrity1passes per profile. Native funded: captured9pass; internal5pass/4fail. Native vault53pass. Exact completed test sections are in `completed-tests-and-identities.json`, raw logs and profile error arrays are inside the payload.

`replay-verification.json` verifies each profile’s gates, complete error multisets and invocation resource/fee/error blocks. Exit1 means verified historical failures; exit2 is an evidence mismatch. A prior local replay attempt failed for disk space and is not execution evidence.

From `peridot-contracts`, after `npm ci --prefix bots/margin-liquidator --ignore-scripts`:

```sh
python3 scripts/replay_cl_linux_ci.py \
  ../docs/deployment-handoffs/evidence/cl-linux-ci-ac63255-20261102 \
  --output target/cl-linux-ac63255-replay-fresh
# Expected exit1, with BOTH profile verification records true; never accept exit1 alone.
```

| Linux WASM | Bytes | SHA256 |
|---|---:|---|
| cl_integration_fixtures.wasm | 3168 | `bff4b6856c0199893515ca688c674847c23eedd097fe143c1b7734691bcd7f7f` |
| cl_share_pool_facade.wasm | 7298 | `d3c10c9b2d6e29a66b3b3e2c9ad7077b1dcea7268d284e1da3e306548217a093` |
| cl_share_vault.wasm | 85747 | `65e239b6ddb13a395e98ccfb8f04fa46c858b632c42943df78612efd9e13cb63` |
| jump_rate_model.wasm | 7644 | `1416b1f0bc441ddcda230d9e18759ffa0d3f24b7fe0b70172ca01bb3aaa3312a` |
| margin_controller.wasm | 91655 | `bf9c9a09091a9a4b64b2060e8474387356f72f40a2c0a5fa68b4fe4f80bbb7ba` |
| receipt_vault.wasm | 87197 | `510481f6ab76f8a7eac3f18cc7fb7ff1583e3c8a40d5f333f18e855ff2bf50dc` |
| simple_peridottroller.wasm | 63289 | `4d40bddeda038fa189b6d667861a4690b1a47cb6b8fdfb64eeff41ac8a63ce96` |
| swap_adapter.wasm | 10876 | `28816ff4592011626b7b77ae24bde23de1763c13d7e5f14ecc178c163b53ac82` |

Seven Peridot WASMs differ from the prior local ac63255 build; the integration fixture matches. Linux and local identities must not be interchanged. These binaries use synthetic test administrators and controlled state. Recording external authorization is not real signature enforcement. Release approval remains false.
