# Controlled unsigned Core29 fixture

These are test sources for official Stellar Core v29.0.0 at commit `a9d72b0cac3a89ebbcf926af449ac73089e0f8f7`. They are not production contracts or deployment tools. Four final native fixture modes pass (2,581 assertions, twelve applied operations). Exact results and limitations are in `../core-research.md`; this does not approve financial execution or release.

Apply `core-fixture.patch` to that exact checkout after initializing its pinned submodules. It adds the fixture, a mandatory fail-closed signing refusal, and a bounded test-helper build list. Every production translation unit and every authorization/fee/TTL verification path remains present. The guard throws before `SecretKey::sign` whenever `CL_CORE_FORBID_SIGNING` is set. Never run ordinary upstream tests in this qualification: their setup helpers sign.

Stage the preserved unsigned `receipt-code-input.{txt,json}`, `receipt-state-input.{txt,json}`, `maintenance-input.{txt,json}`, `partial-input.{txt,json}`, `captured-network-config.json`, these helper scripts, and the checkout named `build-core` together under a fresh work directory. From the repository root, the input regeneration scripts check the exact preserved v7 source and captured network configuration identities. Their original source records are evidence dependencies, not network reads. The state input deliberately contains only actual compiled receipt code, its matching instance and observed persistent entries; native SDK authorizer placeholders are excluded.

Verify the staged input bytes with `python3 core/verify-staged-inputs.py WORK`. The preserved input metadata retains its original absolute provenance paths. Replay uses the staged bytes and does not need those paths. Full regeneration requires the exact referenced original source capture; an explicit path rebase must be documented and must retain its original SHA256, not silently relabel provenance.

Run only:

```sh
bash WORK/run-restore-test.sh code > WORK/restore-code-test.log 2>&1
bash WORK/run-restore-test.sh state > WORK/restore-state-test.log 2>&1
bash WORK/run-restore-test.sh chain > WORK/restore-chain-test.log 2>&1
bash WORK/run-restore-test.sh partial > WORK/restore-partial-test.log 2>&1
```

The runner forces the signing guard, warning-level logging and this fixture's exact Catch tag. It never starts application/network services, broadcasts, signs, or reads configured identities. The fixture validates empty envelope signatures, checks normal preAuthTx authorization, validates a normal transaction set and uses the ordinary LedgerManager application path. Synthetic source funding and preauthorization are supplied directly at controlled genesis; no setup transaction is signed. The wrong-preauthorization section must reject normally.

The local functional build uses isolated Rust 1.95.0, workspace GNU bootstrap tools, PostgreSQL disabled, and bundled sqlite. All nine official protocol21–29 hosts use their exact locked release builds. C++ objects use the recorded diagnostic O2/O0 flags; native timing is not qualified. `target/cl-option1-core` records exact bootstrap/configure commands, source identities, compiler flags, host rlib hashes and final-link argv. A bridge-only no-LTO attempt failed: Apple ld could not read Rust LLVM bitcode, and matching ld64.lld exposed unresolved host internal symbols. The separately bounded ThinLTO bridge succeeded in 11m20s and retained the same nine host rlibs. Its matching Rust ld64.lld final link and guarded application tests succeeded; exact runtime identity is in the evidence. Do not silently apply bridge flags when rebuilding hosts.

The fixture-only overlay factory seam omits unused network construction and throws if accessed, because stock overlay construction otherwise signs a certificate before startup. SDK zero ledger-modification metadata is explicitly imported at controlled genesis ledger279. Raw inputs and all LedgerEntryData remain unchanged. The partial case is seeded mixed genesis, not an interrupted multi-transaction journey.

A successful controlled restore establishes only the tested native operation, fee, source-sequence/signer changes and exact entry/TTL transitions. It does not establish financial Soroban address credentials, real account signatures, live-state eviction history, all required archived keys, full lifecycle acceptance or release approval. No resource policy, production stage or operational restriction changes.
