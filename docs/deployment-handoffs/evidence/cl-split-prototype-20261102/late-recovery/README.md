# Supplemental late-recovery evidence

Two complete recovery transactions executed after the main21-case suite.
Captured:2 pass; invocationPASS, other Stage1 gatesFAIL. Internal:0 pass/2 fail;
all gatesFAIL. Minted recovery127 SDK entries; delivered recovery123. Both exceed
the unchanged100-entry internal ceiling, despite CPU/memory fitting.

The original main21 test functions are byte-identical; only the late_recovery
module declaration was appended. This archive preserves the later source pins,
same WASM identities, raw reports and all gate errors independently. It is not a
replacement for the main archive, a full Stage1 proof map or release approval.

From peridot-contracts, rerun the `late_recovery` filter with the same prepare.py
and run.sh commands. The current full `split_tests` filter contains all23 tests.
Replay the preserved supplement with:

```sh
python3 prototypes/cl-split-transactions/replay.py ../docs/deployment-handoffs/evidence/cl-split-prototype-20261102/late-recovery --output target/split-late-replay
```

Return1 means exactly reproduced incomplete gates;2 means evidence mismatch.
The index binds267 archived files, source/artifact hashes and both profile errors.
