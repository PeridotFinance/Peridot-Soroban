# Durable final Linux CI evidence

This preserves the **completed** Linux run [41434868065](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41434868065), rather than the earlier bootstrap failure. Stage 1 remains incomplete; PR75 remains draft. The execution job failed its enforced internal-policy/prototype gates. Checker tests passed. Captured-network invocation/lifecycle/transaction/checkpoint gates passed; internal-policy gates all failed. Neither profile grants release approval.

`index.json` records the source/merge commits, artifact ID13136354990, original ZIP SHA256, original November16 expiry, exact per-profile gate summaries and errors, every archived member's SHA256/size, and the **Linux** WASM hashes. Seven Peridot hashes differ from the separately preserved Darwin build; do not substitute either set's hashes. Aquarius remains pinned to its original hash.

`linux-ci-payload.tar.xz` losslessly repacks all47 original artifact members and adds19 exact source files from commit `da4e1a598b7c652c79745836417d9943e561c6f2` for historical replay. Every original payload byte is preserved; the archive container differs from the original104,658,129-byte ZIP. The original ZIP digest is provenance, not a claim that tar.xz has that digest. Committing this archive removes dependence on GitHub's expiring download.

From `peridot-contracts`, with the pinned `bots/margin-liquidator` package dependencies installed (`npm ci --prefix bots/margin-liquidator --ignore-scripts` if needed):

```sh
python3 scripts/replay_cl_linux_ci.py \
  ../docs/deployment-handoffs/evidence/cl-linux-ci-20261102 \
  --output target/cl-linux-ci-replay-new
```

Output must be new. This invokes each profile's `check_budget.sh` from the exact preserved source, validates unsigned XDR/fees and lifecycle evidence, and compares each gate, missing-flow list, lifecycle/transaction error multiset, plus every invocation resource/fee/error block with the saved results. Only explicitly bound test-log paths are normalized. `replay/replay-verification.json` must contain verified records for **both** profiles, including `invocation_resources_and_errors_verified=true`. Exit1 preserves the reproduced failed gates; exit2 means mismatch/incomplete verification. Aggregate exit1 alone is insufficient. No historical replay result approves the new source or production release.

```sh
python3 scripts/analyze_cl_feasibility.py \
  --durable-evidence ../docs/deployment-handoffs/evidence/cl-linux-ci-20261102 \
  --output-dir target/cl-feasibility-replay-new
```

This is controlled pool state, actual pinned Aquarius bytecode, compiled synthetic oracle/plane, SDK/host29.0.1 with captured protocol29 cost parameters. It is not live-state or real account-signature evidence. No signing, broadcasting, keys, secrets or network transaction is involved in these commands.
