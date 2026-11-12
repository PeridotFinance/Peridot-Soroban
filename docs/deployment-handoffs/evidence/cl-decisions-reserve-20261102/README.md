# Preserved default reserve comparison baseline

Stage1 incomplete, PR75 draft. Captured all4 gatesPASS/noerrors; internal all4FAIL/106 lifecycleerrors/303 transactionerrors. Native default110PASS/2ignored, vault53PASS, checker32PASS. Maincompiled36PASS captured; internal4PASS32FAIL. Archivedreruns5PASS captured/5FAIL internal. No signing, broadcasting, production stage/policy changes or release approval.

The hash-indexed payload preserves100 files, separate default/validation/historical-source WASMs, all reports/logs/proof mappings, the public Aquarius fixture, and diagnostic old-binary fee-regression failure. The index binds every payload member. `execution-summary.json.xz` contains complete fee inputs/components, restored key identities and errors. See handoff14 for commands, ABI dependencies and interpretation.

Replay from this checkpoint's source with installed node dependencies, fresh directories:

```sh
mkdir target/cl-decisions-replay-input
tar -xJf ../docs/deployment-handoffs/evidence/cl-decisions-reserve-20261102/checkpoint-payload.tar.xz -C target/cl-decisions-replay-input
python3 scripts/replay_cl_evidence.py target/cl-decisions-replay-input/checkpoint --output target/cl-decisions-replay
```

Expected exit1 requires BOTH per-profile verification records true, exact gates/errors and invocation resource/fee/error blocks. Exit2 is a mismatch. No aggregate-exit shortcut. This baseline is distinct from the subsequent isolated split-transaction experiment and from either Linux archive.
