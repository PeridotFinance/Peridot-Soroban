# Isolated protocol29 CL harness

Native test-only package. `soroban-sdk =29.0.1` pins `soroban-env-host =29.0.1`,
matching the protocol29 preflight host documented by Stellar RPC29.0.1. SDK29.0.0
is intentionally avoided because its release used the protocol28 Wasmi runtime.
The production workspace remains pinned to SDK25; existing WASM artifacts are
loaded unchanged. This package never compiles or publishes contract WASM.

The fixture source is shared with the SDK25 harness using a Rust path module.
Select the compiled tests; the historical SDK25 setup regression is not a
protocol29 compatibility promise. The parent runner supplies validated limits,
profiles, captured cost tables and artifact paths. A protocol29 host alone does
not make fixed-oracle/SAC/controlled-pool-state tests a live-state proof.

Official runtime references:
- https://github.com/stellar/rs-soroban-sdk/releases/tag/v29.0.1
- https://github.com/stellar/stellar-rpc/releases/tag/v29.0.1

Run native compilation/checks from the repository's peridot-contracts directory:

```sh
cargo test --manifest-path harness/cl-runtime29/Cargo.toml --no-run
cargo tree --manifest-path harness/cl-runtime29/Cargo.toml -i soroban-env-host
CL_RESOURCE_PROFILE="$PWD/target/cl-next/captured-network-diagnostic.json" \
  cargo test --manifest-path harness/cl-runtime29/Cargo.toml \
  runtime_integrity_and_captured_cost_models -- --nocapture
```

Use absolute paths in environment variables: Cargo runs tests with this package
as their working directory. The integrity test verifies the resolved host
version, actual default ledger protocol, and every CPU/memory coefficient
against the hash-pinned captured configuration before limits are installed.
The current 29.0.1 SDK does not expose a constructor accepting custom cost
models. The shared validator compares all default coefficients using the pinned
host's public Debug representation and fails closed if they differ; it does not
replace the model or modify private host state.

On 2026-11-02, both 86-entry cost vectors captured at ledger 65264920 exactly
matched host 29.0.1 (revision `ff6ae01b3465ac44185d2824876491e62c9a4cc4`).
This removes the runtime/coefficient mismatch for controlled-state execution.
It does not establish historical live-state replay, envelope-level fees, or
completion of the required lifecycle flows. The integrity test alone is not
a release gate.
