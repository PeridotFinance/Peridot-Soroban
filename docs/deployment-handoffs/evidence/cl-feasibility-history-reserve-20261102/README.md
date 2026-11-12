# Complete local checkpoint evidence

Stage 1 is incomplete and PR75 remains draft. Captured-network invocation/lifecycle/transaction/checkpoint gates PASS; internal-policy all four FAIL. The runner exits1. Release approval remains false. These are compiled controlled-state protocol29 measurements, including actual pinned Aquarius bytecode, synthetic external auth and unsigned full-transaction evidence. They do not validate real signatures or live production state.

`index.json` binds every payload member, separate local WASM hashes, raw profile summaries/errors and the compressed execution summary. `checkpoint-payload.tar.xz` preserves the completed runner, its explicit failures, build/test/budget logs, raw invocation/call/unsigned transaction records, governed evidence documents and the pinned public Aquarius fixture. Supplemental logs include28 checker tests and110 native margin passes/2existing ignores. Earlier ordinary fixture failures are labelled diagnostic-only and are excluded from the proof manifest.

`execution-summary.json.xz` includes complete resources, fee inputs/components, exact restored LedgerKey XDR and decoded history-key names. Its uncompressed SHA256 is in the index. Individual maxima can come from different invocations; SDK entry totals differ from unique serialized footprint keys.

Replay from the source checkout containing this checkpoint (fresh directories required):

```sh
mkdir target/cl-local-evidence-new
tar -xJf ../docs/deployment-handoffs/evidence/cl-feasibility-history-reserve-20261102/checkpoint-payload.tar.xz -C target/cl-local-evidence-new
python3 scripts/replay_cl_evidence.py target/cl-local-evidence-new/checkpoint \
  --output target/cl-local-replay-new
```

Replay validates every package checksum and source/proof-map hash, then runs `check_budget.sh` for each profile and compares gates, error arrays and raw invocation resource/fee/error blocks. Exit1 means failed gates remain failed only if both records in `replay-verification.json` say verified; exit2 means a mismatch. No approval is inferred from exit1. Use the separately preserved Linux archive/wrapper to replay the earlier Linux revision; its hashes are not these local hashes.

The public pool fixture is `fixture/aquarius-pool.wasm` in the payload, SHA256 `12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`. Use it with the full runner command in handoff11 to reproduce execution offline. All WASM builds must use `stellar contract build`; no signing, broadcasting, keys/secrets or Mainnet transaction is authorized.
