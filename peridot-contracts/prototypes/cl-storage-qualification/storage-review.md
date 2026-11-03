# Operation-scoped storage overlay: experiment, not release approval

Stage 1 remains incomplete. This is a new isolated candidate layered after the preserved duplicate-TTL candidate; production contracts, policy limits, transaction stages and handoff 00 are unchanged. No signing, broadcasting, keys, secrets, Upshift binding, production maintenance-margin setting or borrowing enablement is involved.

## Layering and reproducibility

Apply in this order to the pinned baseline source tree:

1. Baseline `9cf43082bfb57334d57dbc58cfa90ccad4d98078`, with the source pins already used by the atomic qualification runner.
2. `prototypes/cl-atomic-qualification/optimization.patch`, SHA256 `bffb4dcf73404f5e98f7f57c90ae5f896dbcd289f6d7734f4687b16121cdd20e`.
3. This directory's `storage.patch`, SHA256 `ecc2da1ee00b833c59b8f2b8311ae660d5daef72c057732db697fb4a43a0def3`.

The new patch is generated against `target/cl-atomic-optimized-v3-build/workspace`, copied to `target/cl-storage-work/workspace`; it is not a patch directly against production HEAD. Pre-overlay source SHA256 values:

| File | SHA256 |
|---|---|
| `contracts/receipt-vault/src/contract.rs` | `2a1d144359d7d9b7e0e686363e8b9a923901b3c7ecd6cbf1520358c5804ae053` |
| `contracts/receipt-vault/src/storage.rs` | `a459cb6509b45aad514b10c1fe3069ec4fadf33c6bd081bffcbccbdf33c6ed34` |
| `contracts/simple-peridottroller/src/contract.rs` | `b71859c1ff4632e2ff5e8032f1a0224736c2b6d82a3a8c8e66af2a276b63fcf8` |
| `contracts/simple-peridottroller/src/storage.rs` | `6db9c1654531353098fa677d7da0bd7a846ff831ab90919543adca347321b6b3` |

Apply with `patch --batch -p1 -i /absolute/path/to/storage.patch` in that isolated workspace. The lead owns WASM builds, which must use `stellar contract build`, and all compiled invocation/transaction measurements and `check_budget.sh` runs. No native test result can satisfy those resource gates.

## What changes

The broad `ensure_initialized` and explicit `bump_ttl` endpoints remain intact for governance and unmodified paths. New `ensure_initialized_for_operation` retains the exact existing initialized/underlying checks via `ensure_initialized_for_snapshot`, then renews a supplied private Rust key list. There is no new selector, mode flag, storage key, initializer, contract stage or layout migration.

Narrow initialization is used by the receipt's deposit, withdrawal implementation, borrowing implementation, margin borrow, repayment, repay-max, partial/full margin repayment, transfer implementation, balance/debt reads, CL custody setup hooks, and margin-withdraw context setup. Underlying/collateral read endpoints and the reserve-policy getters/final assertion are included so they do not reintroduce broad restoration into the same lifecycle call tree.

The private interest helper now owns its required keepalive, rather than relying on prior broad initialization. Public `update_interest` and the earlier candidate's direct withdrawal/margin-borrow helper calls therefore still read the same fresh interest inputs and execute the original accrual body. There is no same-ledger shortcut, cached amount, skipped external rate model, omitted fee split, missing cap check or changed fallback.

`cache_price` replaces the controller's broad core sweep with oracle and max-age maintenance. Existing per-token support, symbol and price-cache maintenance, fresh oracle calls, validation and failure behavior remain. Governance/default liquidation terms and the supported-market registry are not consumed by a token price refresh. Other controller endpoints retain their existing behavior.

## Required keys and intentional TTL differences

These are source-level access changes, not measured savings. A key is avoided only if no other descendant consumes it. Whole-transaction read/write sets must confirm the actual result.

| Path | Retained consumed data / keepalive | No longer restored solely for keepalive |
|---|---|---|
| Public/direct interest accrual | Underlying, last update, borrowed total, borrow index, reserve/admin totals, model and fallback borrow rate; reserve/admin factors when debt is nonzero | Admin, pending admin, supply cap, collateral factor, initial exchange rate, supply yearly rate, legacy accumulated-interest counter; unrelated routing/config |
| Dynamic rate branch | Original live-cash/model query, cached boosted value/timestamp, accounting estimate and managed cash; original fallback rate | No strategy or fee reads are skipped to reduce footprint |
| Receipt balance/debt read | Initialized/underlying; existing per-user/per-position debt snapshot/principal/flag maintenance; explicit global borrow-index maintenance on nonzero margin debt | Other users, governance config, unrelated global yield history |
| Transfer/CL ownership boundary | Margin/controller identity, transfer pause and health checks, exact bypass, actual claim-first custody reassignment, token/reward hooks | Admin, supply cap, legacy yield fields; collateral factor is still renewed in the existing debt-dependent valuation branch |
| Borrow/repay with nonzero cap | BorrowCap and TotalBorrowPrincipal inside the existing `bcap > 0` branches, fresh debt accounting and global/local principal updates | No TotalBorrowPrincipal read introduced for cap zero |
| Deposit/withdraw | Relevant supply/collateral/cap/accounting keys, original reward boundaries and asset accounting | Unrelated governance keepalive; some conservative path-level keys remain |
| Reserve getters / complete-open assertion | Original policy presence/defaults, enabled flag, live and managed cash, reserve/admin liabilities and existing valuation inputs | Governance/default rate fields that do not participate in the actual check |
| Controller `cache_price` | Oracle, max age, instance TTL and original per-token support/symbol/cache reads | Admin, supported-market registry, close factor, liquidation incentive/fee, optional reserve recipient/PERI token |

The original extended TTL values and thresholds are reused. Required fields are renewed where consumed, including nonzero-cap global principal and boosted cached-value/timestamp branches. Fields intentionally omitted from ordinary operations can become archived sooner; that is an explicit maintenance/cost tradeoff. Existing permissionless full keepalive remains available. Operators still need an archival budget and retention index; successful recovery cannot depend on an off-path keepalive having happened. No historical reward entitlement is pruned, consolidated or waived.

For the v1 diagnostic 104-write liquidation, the main candidates are the two receipts' unrelated governance/legacy yield keys and the controller's global sweep-only keys. Removing duplicate TTL calls alone did not remove those keys. This overlay attempts to remove their *entire access* from the actual recovery tree. It does not claim that the 80-write proposed workload, current 50-write policy, or any headroom threshold is met until compiled cold-state measurements establish that fact.

## Safety review and remaining coverage

- Funded repayment and permitted collateral seizure remain one atomic V3 call. This patch does not touch V3, the share vault, facade, escrow, Aquarius or reward accounting.
- All existing external and contract authorization remains; no blanket mock or bypass is introduced. The three claim-first boundaries remain in their original locations.
- Borrowing pauses, configured reserve policy, final reserve enforcement after open costs, debt accrual/caps, freshness/valuation checks, transfer validation and rollback code remain unchanged.
- Missing optional fields retain their previous defaults. Missing required initialization/debt state still fails. No migration or replacement default is invented.
- Borrow-cap principal TTL was caught in source review and preserved conditionally. A first build made before that correction must remain diagnostic; use only the final hash above for measurement. Boosted cached timestamps likewise retain their prior lifetime when consumed.
- This bounded review does not qualify all legacy strategy/reward configurations. Nonzero PERI emissions can still enter controller paths with broad sweeps; arbitrary entered markets and strategy branches remain workload dependencies. No promise is made to keep every possible configuration under a fixed footprint.
- Full compiled positive/adversarial lifecycle, nonzero dynamic interest, multi-borrower/custody history, archive/restore and all fee/envelope/auth evidence remain the lead's acceptance runs. Signed account authentication and authoritative effective TTL for recording-only handled-failure placeholders remain separate gates.

## Native compatibility checks

`receipt_storage_tests.rs` is a reproducible TEST ONLY child module of the isolated receipt `contract.rs`, copied/linked into the temporary workspace using:

```rust
#[cfg(test)]
#[path = "/absolute/path/to/prototypes/cl-storage-qualification/receipt_storage_tests.rs"]
mod storage_qualification_tests;
```

The native fixture seeds raw synthetic state to isolate accounting/TTL behavior; it does not claim to exercise production initialization or Aquarius. The original seven cases were:

1. Public/direct private-helper nonzero interest parity, exact reserve/admin split and repeated same-ledger calls.
2. Near-expiry required-key renewal while Admin/SupplyCap/YearlyRate/AccumulatedInterest TTL stays unchanged.
3. Missing optional model/factors retain defaults and are not fabricated.
4. Invalid fee-factor failure rolls back both ledger data and TTL.
5. Existing manually set flash-loan flag rejects before authorization or token movement, with complete rollback. This is a flag-guard test, not a custom-account callback reentry proof.
6. Public update on missing underlying preserves its existing no-op behavior.
7. Full post-accrual repayment with a nonzero cap renews near-expiry global principal and clears debt with borrowing disabled. An actual native controller call provides its own authorization; only external payer/admin authorization is narrowly mocked. The SAC transfers real fixture balances. This is native synthetic authorization, not a compiled V3 signature proof.

Reproduce after preparing the layered isolated workspace and adding the test module:

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true \
  CARGO_TARGET_DIR="$PWD/target/cl-storage-native-cargo" \
  cargo test --manifest-path target/cl-storage-work/workspace/Cargo.toml \
  -p receipt-vault --lib storage_scoped_ -- --nocapture
```

Initial native compilation was cancelled with exit 130 at the lead's request because simultaneous fresh builds caused severe host memory pressure. Its log is preserved at `target/cl-storage-work/native-tests.log`. No test result or resource success is inferred from that interrupted compilation. The subsequent single-job retry completed successfully: **7 passed, 0 failed, 0 ignored, 193 filtered out**, using default native build features and SDK 25.3.1 / host 25.0.1. Execution took 0.17 seconds after compilation. Log: `target/cl-storage-work/native-tests-retry-1.log`. This is not a protocol29 compiled/resource acceptance result.

Native evidence SHA256:

- `prototypes/cl-storage-qualification/storage.patch`: `ecc2da1ee00b833c59b8f2b8311ae660d5daef72c057732db697fb4a43a0def3`
- `target/cl-storage-work/receipt_storage_tests-7pass.rs` (preserved exact original seven-test source): `69aed1cb6746d6fa55196d9268785a9751e8130abf7e3900bf4ba2b36cbb0f64`
- `target/cl-storage-work/native-tests.log`: `12e586f89bea6a0905af2b53be4b56d13efa9b0997dc6bad1b60f84e7274fe76`
- `target/cl-storage-work/native-tests-retry-1.log`: `1eff501ea178abf8a372105d85b67abd20fe33cdc80f6f1d7265057fbbd88927`

## Follow-up: actual callsites, custom authorization callback and post-write rollback

The final native source contains **11 tests, all passing** in `target/cl-storage-work/native-tests-retry-6.log` (0 failed, 0 ignored, 193 filtered, 0.26 seconds after compilation). The same single-job command above reproduces this bounded suite. The storage patch remains unchanged at `ecc2da1ee00b833c59b8f2b8311ae660d5daef72c057732db697fb4a43a0def3`.

| Additional test | Concrete assertion |
|---|---|
| `storage_scoped_elapsed_withdraw_actual_callsite` | Real receipt withdrawal accrues 1B debt to 1.1B through its direct-helper callsite, books 10M reserves and 5M fees, and pays exactly 102,833,300 underlying for 100M pTokens; actual SAC and receipt balances checked. |
| `storage_scoped_elapsed_margin_borrow_actual_callsite` | Actual native controller calls receipt `borrow_for_margin`; accrued global debt becomes 1.1B before the extra 100M loan. Receiver gets 100M SAC, position debt is 100M and global debt is 1.2B. Configured TEST ONLY reserve1000/borrowingtrue is isolated fixture state. No controller authorization is mocked. |
| `storage_scoped_failure_after_accrual_writes_rolls_back` | Exact scoped external auth succeeds. Failed withdrawal's host event buffer contains a failed-call `AccrueInterest` with `interest_accumulated=100,000,000`, proving real accrual preceded the insufficient-pToken failure. Exact numerical state and full ledger-entry/TTL snapshot equal the pre-call values afterward. |
| `storage_scoped_custom_account_check_auth_reentry_is_host_rejected` | An actual registered native custom account's `__check_auth` runs through `set_auths`, without mocking that callback. It calls the already-active receipt's `update_interest`; a native observation records exactly host `Context/InvalidAction`. The account deliberately rejects authorization and all ledger/TTL changes roll back. |

The callback test uses an unsigned authorization template for a deliberately adversarial native account contract. It is not Ed25519 account-signature acceptance, a compiled custom-account proof, or a replacement for target-protocol authorization evidence. The original manual flash flag test is retained and explicitly distinct.

Evidence development failures are retained honestly. Retry2 had 11 passing generic assertions, but its generic failure assertion did not yet prove post-accrual execution. Retry3 failed the stronger event assertion because inspection reset the last-invocation event buffer. Retry4 captured the actual earlier authorization rejection: pre-call `as_contract` inspection after installing narrow mock auth had consumed/reset its invocation context. Moving numerical inspection before the scoped auth setup corrected that fixture. Retry5 reached the intended insufficient-pToken failure, but queried `get_diagnostic_events`, which excludes contract events. Retry6 captures `get_events` immediately after the failure and passes the exact nonzero failed-event assertion. No authorization scope was broadened, no failure swallowed and no production code changed during these corrections. Only retry6 is credited with the strengthened post-write rollback proof.

Final native evidence SHA256:

- `prototypes/cl-storage-qualification/receipt_storage_tests.rs`: `6d5fe8911b1feafd95909351a668266571c8247cbbfb8c595dc576c3800841c1`
- `target/cl-storage-work/native-tests-retry-2.log`: `cf6864f186416155954b15ef6a61b2e48f83bfff7a28686fe5cbdf3b16c8f5e4`
- `target/cl-storage-work/native-tests-retry-3.log`: `8e45dceb331eae4095ed0dce26396cbdc030a75bcc8b9f2ccb5e0bebb3528467`
- `target/cl-storage-work/native-tests-retry-4.log`: `1c4870bd2b402edfdcca0791a3b28f4dd16950aae9fbce00884217463f918567`
- `target/cl-storage-work/native-tests-retry-5.log`: `f0c968db803e5da1bad267e54e6598f53c8436db0daa84cbc00aa055e4196a53`
- `target/cl-storage-work/native-tests-retry-6.log`: `164ca2d9c3e89a683c846b8c929ed162b4cdf11d030244225e173c6cde7c1b4d`
