# CL share pool facade: principal-route checkpoint

This is a **local/Testnet validation contract**, not a production implementation
or a passing handoff-07 integration gate. It is excluded from the contract build
unless `validation-harness` is enabled (or native tests are running). Constructor
and configuration-dependent entrypoints reject the Mainnet network identifier.
No deployed configuration is changed.

Base repository: `47858c1a13c2588970395faa2212e7b68bd33060` on `leveraged-fix`.
The controller-auth fixture copies `authorize_controller_pool_swap` from that
commit. It is not the actual V3 controller and implements no positions or risk
checks. The vault fixture is deliberately synthetic, without Aquarius, reward
ownership, or price-router logic. Actual SEP-41 transfers use a Stellar asset
contract. Only account mint/admin authorization is explicitly mocked; contract
authorization is exercised by real nested calls, without blanket auth mocks.

## Implemented here

- Aquarius-compatible `estimate_swap` and `swap` signatures, fixed token indices.
- Configured controller authorization plus exact nested input transfer.
- Separate authorization for the facade's underlying transfer into the vault.
- Checked `u128` to `i128` conversion for the vault/SEP-41 ABI.
- Per-operation balance accounting, full actual output, minimum-output check,
  and donation preservation. Reported vault return amounts are not trusted.
- Deposit-only/full pause distinction and zero-quote rejection at execution.
- Authenticated donation sweep to a fixed treasury, barred during an operation.
- Feature/network guard so incomplete research cannot initialize on Mainnet.

The fixture tests include wrong caller, controller impersonation, missing or
wrong-amount nested authorization, dust, favorable execution above the quote,
directional pauses, stale quotes, and reward-claim failure rollback.

## Commands

From `peridot-contracts/`:

```sh
CARGO_BUILD_JOBS=2 cargo test -p cl-share-pool-facade --features validation-harness
CARGO_BUILD_JOBS=2 stellar contract build --package cl-share-pool-facade --features validation-harness
FACADE_VALIDATION_WASM="$PWD/target/wasm32v1-none/release/cl_share_pool_facade.wasm" cargo test -p cl-share-pool-facade --features validation-harness compiled_facade_roundtrip -- --ignored --nocapture
```

Use Stellar CLI 25.2.0 or newer, as required by the workspace's
`experimental_spec_shaking_v2` SDK feature. Plain `cargo build --target
wasm32v1-none` is rejected by that feature; do not bypass its build-system check.

The last command registers the compiled facade WASM, but the vault and
controller-auth fixture remain native. Its printed resource figures describe
only that fixture, not the production V3 transaction or Mainnet state. No
unlimited-budget override is used.

## Checkpoint results (2026-10-31)

- Rust 1.99.1, Stellar CLI 28.1.0, resolved Soroban SDK 25.3.1.
- Native suite: 8 tests passed; compiled test is deliberately ignored in the
  ordinary command and executed separately with the WASM environment variable.
- Compiled-facade test passed, including missing/wrong nested authorization,
  controller-address impersonation, and failed-claim rollback.
- A final run with `--include-ignored` and the compiled artifact passed all
  9 tests with no skipped tests.
- CLI-processed WASM: 7,360 bytes; SHA-256
  `37a765eef14efc2e02e2e83d63acd6076258e33378615531a3b7b3187f8a73fb`.

| Operation in compiled facade/native fixtures | Instructions | Memory bytes | Memory-read entries | Write entries |
| --- | ---: | ---: | ---: | ---: |
| Underlying to shares | 1,483,838 | 1,341,391 | 10 | 6 |
| Shares to underlying | 1,389,406 | 1,336,977 | 10 | 6 |

These figures exclude actual V3, receipt-market, price-router and Aquarius pool
execution. They cannot establish production transaction capacity.

## Gates still blocked

1. Integrate the actual share-vault implementation. The agreed planned ABI is
   `deposit_underlying(Address, i128, i128) -> i128`,
   `redeem(Address, i128, i128) -> i128`, and `u128` deposit/redeem estimates.
   The vault must independently enforce NAV validity and real execution costs.
2. Add explicit beneficial-owner custody accounting. V3's current pool ABI
   provides only the controller address. It does not provide a position ID or
   user owner; controller shares pending activation and pooled receipt pTokens
   therefore need a separate per-position ledger and checkpoints.
3. Define authenticated, bounded recovery context consumed by receipt/vault
   transfers. A failed claim currently rolls normal operations back. **Recovery
   during reward failure is not implemented here**; no unrestricted bypass is
   substituted. Quarantine and waiver handling belong to the accounting path.
4. Exercise real compiled V3, receipt markets, controller, adapter, share vault
   and actual hash-pinned Aquarius pool together. Existing exact-pool tests pin
   pool hash `12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`
   but are native strategy tests, not the required full stack.
5. Demonstrate opens, closes, every interrupted-open recovery state, funded
   in-kind liquidation, owner reward conservation and resource limits at pinned
   Mainnet state. Synthetic maintenance values in tests do not authorize live
   parameters.
6. Add the production delayed controller/upgrade administration, TTL strategy,
   liability-aware sweep, adversarial sweep-during-operation and recovery tests,
   and independent security review. Do not remove the validation guards before
   these gates pass.
