# Bounded low-finding disposition

2026-11-11. Local source review, two focused native getter tests, and read-only
inspection of an already-built default receipt WASM. No network call, signature,
deployment, rate policy change or receipt-source change by this reviewer.

## Rate model: zero net-asset denominator

Source: `contracts/jump-rate-model/src/lib.rs`, `utilization`.

The implementation returns0 immediately when `borrows==0`, before computing
`cash+borrows-reserves`. With valid reserve factors, both exported rate getters
complete without division by zero: borrow rate is the configured base, and supply
rate is0. A base *quote* with no outstanding borrower does not itself create
borrow income. There is no supply-only mispricing or revert demonstrated by the
reported denominator condition.

Two regression tests were added **inside cfg(test)**; no production line changed:

- `zero_denominator_supply_only_zero_debt_has_zero_supplier_income` checks empty
  state `(cash,reserves)=(0,0)`, fully reserved cash `(100,100)`, and ordinary cash
  `(100,0)`, all with debt0. Each returns borrow-rate20,000 (2% annual,1e6 scale)
  and supplier-rate0 for reserve factors0,100,000 and1,000,000.
- `zero_denominator_with_debt_records_existing_rate_discontinuity` exercises the
  distinct debt-bearing stress state. With base20,000, multiplier180,000,
  jump4,000,000, kink800,000 and reserve factor100,000:

|Cash|Borrows|Reserves|Net denominator|Borrow-rate|Supply-rate|
|---:|---:|---:|---:|---:|---:|
|100|50|149|1|964,000 (96.4%)|867,600 (86.76%)|
|100|50|150|0|20,000 (2%)|0|

The second row confirms the existing zero-denominator discontinuity; it is a
diagnostic pin, **not approval of debt-bearing pricing**. It requires nonzero
debt and therefore is outside a fresh group whose ordinary, margin and flash
borrowing are durably disabled and whose debt remains0. Reassess the appropriate
fail-closed/saturation treatment before any borrowing activation; this review
does not silently change that economic policy.

Command executed:

```sh
env CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/cl-storage-native-cargo \
  cargo test --locked -p jump-rate-model zero_denominator -- --nocapture --test-threads=1
```

Result: **2 passed,0 failed,13 filtered**, exit0; test execution0.03s after3.41s
compile. Tests register the native model and explicitly seed its five parameter/
admin entries as test genesis. They install **no mock authorization**, invoke the
actual getter functions and make no initializer/signature proof claim. These are
native arithmetic tests, not compiled financial-flow/resource or Core application
qualification; no new check_budget financial row is invented.

Disposition: **no production rate-model change needed for the bounded supply-only
release on the evidence found**. Retain the debt-enabled discontinuity as a later
activation question. This conclusion depends on the separate durable-borrowing-off
proof; a controller pause alone expires and cannot establish that premise.

## Internal reward modules and actual default WASM scope

Source exclusion is explicit in `contracts/receipt-vault/src/lib.rs`:

- `reward_backing`, `reward_claims`, `reward_ledger`, `reward_lending`,
  `reward_settlement` and `reward_share_hooks` are behind
  `cfg(any(test, feature="hybrid-rewards"))`.
- `cfg(all(feature="hybrid-rewards", target_arch="wasm32"))` emits
  `compile_error!("hybrid-rewards is native integration work, not a deployable release")`.
- `reward_accounting_model`, `reward_backing_test` and `lp_lending_test` are
  `cfg(test)` only. Generic receipt `lp-engine` is separately rejected; it is an
  internal source-consumer feature, not permission to deploy a hybrid generic ABI.

Inspected the already-built default cash receipt with:

```sh
stellar contract info interface \
  --wasm target/lending-release-clean-20261111/patched-artifacts/receipt_vault.wasm \
  --output json
```

Artifact identity at inspection:

- SHA256 `7f2e9f2f2f737699fbdf3defc7e96d44d6e78bd867d608eafe145580fc3a5efd`.
-87,224 bytes;96 exported functions in the contract spec.
- No exposed hybrid claim/reinvestment/settlement entrypoint such as
  `claim`, `reinvest`, `redeem_rewards`, `compound`, `settle_reserved`,
  `finish_receive_partitioned` or `begin_conversion`.

Do **not** broaden this to “all reward code is absent.” The default receipt still
exports `cl_assign_margin_rewards`, `cl_prepare_margin_rewards`,
`cl_reassign_margin_rewards`, `get_cl_custody` and `set_cl_custody`; ordinary
controller reward-accrual calls also remain in default financial paths. Cash-only
release binding must keep CL custody false, margin controller absent, boosted
vault absent and controller PERI token/reward configuration absent. Source gating
of native hybrid modules is distinct from proving those deployed-state settings.

Several internal module function names overlap ordinary API names (`deposit`,
`withdraw`, `borrow`, `transfer`, `initialize`). Name matching alone cannot prove
which implementation was linked. The `cfg`/compile_error boundary supplies the
source-level exclusion; the pinned default WASM spec confirms that no additional
hybrid ABI was exposed. This is build-scope evidence, not a scan clearance or proof
of absence of every possible internal instruction sequence. Reinspect any final
WASM rebuilt after subsequent medium-finding fixes; do not transfer this hash-bound
observation to a different release artifact without verification.

## Final artifact reinspection

Re-ran `stellar contract info interface --wasm target/lending-medium-final-20261111/artifacts/receipt_vault.wasm --output json`. Final receipt SHA4229b7ef9f40028a20e7bace2484f330dbbea625aa98f29f453861e0c498f79e, 89,033 bytes, 98 exported functions. The same excluded hybrid selector set is absent; the five CL custody/reward selectors remain. New permanent-seed/getter ABIs are included. Raw interface saved as `final-receipt-interface.json`. This does not claim all reward hooks absent.
