# Lending storage qualification

This isolated SDK/host 29.0.1 fixture invokes the compiled receipt, controller
and rate model. Storage/cap tests use explicitly labelled controlled receipt
genesis; the separate v6 fresh lifecycle invokes the actual compiled initializer
with exact host source-account authorization. Neither proves live eviction
discovery, account signatures, or Core transaction application. No network access
is required.

The unsigned transaction simulator is copied from
`harness/cl-runtime29/src/transaction_evidence.rs`. The only archive-selection
changes permit an exact contract-instance selection using the `-instance` role
suffix and exact `SupplyCap`/`BorrowCap` selection using the cap-role suffixes.
Entry data is retained and its TTL changed to the preceding ledger.
Persistent policy keys use the original archive selector. Every restored reading
must equal its warm reading, with nonempty restored-entry indices. The missing
pause map and uninitialized rate fixture are separately created, never deleted
to imitate expiry. The keeper uses a still-live short instance TTL and asserts
the exact renewal from 1100 to 1000100.

```sh
CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$PWD/target/lending-cap-caller-build" \
  stellar contract build \
  --manifest-path prototypes/lending-archive-qualification/margin-caller/Cargo.toml \
  --out-dir target/lending-cap-caller-artifacts
CARGO_TARGET_DIR="$PWD/target/lending-archive-cargo" python3 prototypes/lending-archive-qualification/run.py \
  --artifacts target/lending-medium-final-20261111/artifacts \
  --cap-caller target/lending-cap-caller-artifacts/lending_cap_test_caller.wasm \
  --network-config target/cl-next-full/network-config.json \
  --output target/lending-archive-replay-NEW
```

The output directory must not already exist. Both profiles execute. The runner
returns nonzero while any test or global checker gate fails; missing Stage 1
flows are not substituted by these storage getters. Complete unsigned preflight
resources, serialized envelopes, restoration keys, rent and fee components are
in the JSON reports. Warm SDK invocation rows are separately recorded, without
pretending they measure archived SDK execution. The global checker receives
those measured rows; no proof manifest or complete Stage 1 transaction proof is
fabricated.

No production contract source, policy, cost model, resource ceiling or transaction
stage is changed by this prototype. WASMs must be supplied from a separate
`stellar contract build` output. Generated ledger snapshots remain controlled
genesis only.

The v5 financial-cap suite adds a real SAC, explicitly funded permanent seed,
ordinary deposit/borrow/repay/withdraw, and a tiny compiled **synthetic** margin
caller. The caller invokes `init_margin_borrow_state` and `borrow_for_margin`
with real contract-invoker authorization. It is not the V3 controller and must
never be deployed as one. Borrowing is enabled only in controlled test genesis.
The synthetic binding is explicitly cleared by the actual receipt admin setter
before the separate unbound cash withdrawal proof; no collateral-lock proof is
claimed. Cap expiry changes only the selected cap's TTL. A malformed missing-cap
genesis is separately classified and is never presented as archival evidence.
Optimized WASM strips panic text: negative proofs assert typed contract traps,
successful paired controls, cap-boundary amounts and complete state rollback,
not fabricated panic messages.

V6 additionally invokes the exact compiled initializer with the pinned public
admin using a narrowly scoped `SourceAccount` host authorization tree. It never
registers a mock contract at that G-address. Synthetic classic account/trustline
genesis enables actual SAC minting; transaction signatures and Core account
verification remain unproved. A contiguous fresh seed/deposit/withdraw/next
deposit journey passes with borrowing disabled. Reinitialization and separately
constructed nonempty/unseeded legacy-state deposit/seeding attempts reject.
The latter is an unsupported-upgrade counterexample, not migration support or
an actual upgrade/expiry experiment. The additional report is `*.json.fresh.json`.
Final v6 evidence is `target/lending-archive-qualification/v6-run2`: three passing
Rust tests per profile, 44 unsigned preflights and 23 warm SDK roots. SDK maxima
are 16,008,521 CPU, 1,666,770 memory bytes, 58 entries and 23 writes; unsigned
preflight maxima are 17,524,364 CPU, 3,045,578 memory bytes, 43 unique keys and
14 writes. The fresh initializer alone measures 4,619,414 CPU, 1,449,345 memory,
58 entries and 23 writes. Both profiles retain failed global lifecycle,
transaction and checkpoint gates; the runner exits 1.
