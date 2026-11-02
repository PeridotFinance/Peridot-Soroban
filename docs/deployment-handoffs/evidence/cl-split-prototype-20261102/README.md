# Durable local split-transaction evidence

Stage1 incomplete; PR75 draft; no release approval. This archive is separate from
all baseline and Linux archives. It contains the actual prototype and baseline
WASMs, actual pinned Aquarius, raw SDK/call/unsigned-transaction evidence, source
pins, build logs, completed test logs, exact gate errors and earlier failed attempts.

- Captured profile:21 tests pass; invocation gate passes; lifecycle, transaction
  mapping and checkpoint gates fail because the isolated experiment does not
  replace the full Stage1 proof map.
- Internal profile:8 tests pass/13 resource failures; all four gates fail.
- Scope: host29 controlled synthetic state and real Aquarius bytecode. No signed
  account-auth cost, live state, or new escrow archival/restoration approval.

`index.json` binds all266 members and every before/after source/WASM pin.
`execution-summary.json.xz` contains complete stage tables and test measurements.
The raw archive includes all fee components and unsigned envelopes with zero
signatures. No keys or transaction-signing artifacts are included.

Rebuild and execute from `peridot-contracts` (fresh output directories):

```sh
python3 prototypes/cl-split-transactions/prepare.py --output target/split-rebuild
CL_SPLIT_BUILD_DIR="$PWD/target/split-rebuild" bash prototypes/cl-split-transactions/run.sh captured-network-diagnostic split_tests target/split-captured
CL_SPLIT_BUILD_DIR="$PWD/target/split-rebuild" bash prototypes/cl-split-transactions/run.sh internal-policy split_tests target/split-internal
```

Run individually, preserving statuses. Internal returns1 (Cargo101); captured
returns0 for the explicitly selected invocation gate, while the full checkpoint
remains failed. Offline replay verifies each profile's exact gates and error lists,
raw invocation rows, completed-test outcomes and separate WASM identities:

```sh
python3 prototypes/cl-split-transactions/replay.py ../docs/deployment-handoffs/evidence/cl-split-prototype-20261102 --output target/split-replay
```

Replay returns1 when the incomplete checkpoint is reproduced exactly;2 indicates
mismatch. Check `replay-verification.json`; exit1 alone is not verification.

The earlier shared-output build mislabel is preserved under `diagnostic-only`.
The corrected builder uses separate fresh Cargo output directories; baseline
vault hash5e52ff4e…bf4e1f differs from prototype b549cb93…a28df. The validation
baseline V3 f59f592b…7a36 differs from prototype0b7a9e7f…b601. Full identities
are in the index, alongside escrow6e0babdb…84c91 and the other compiled contracts.

Archive SHA256: `3a6760f42ae337c731b6025f7855ecd259433e14d197760036cdef97e60075a7`; 1764556 bytes.

The preserved `replay-verification.json` records exact successful verification of both profiles. Replay exit1 reflects their incomplete checkpoint, not a comparison mismatch.
