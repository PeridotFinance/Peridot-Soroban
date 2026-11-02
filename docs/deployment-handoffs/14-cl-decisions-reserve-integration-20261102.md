# Reserve integration and decision checkpoint — 2026-11-02

Stage 1 remains incomplete and PR #75 remains draft. This is a source/test candidate, not production approval. Existing limits, all three claim-first boundaries, transaction stages, borrowing-disabled preparation and production maintenance-margin settings are unchanged. No signing, broadcasting, keys/secrets, Upshift binding, Mainnet transactions or shared00 edits occurred.

## Completed Linux evidence and resource decision

The previous source ac63255 [Linux CI run41449397462](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41449397462) completed: checker success; checkpoint failure during actual gates; bootstrap and evidence upload success. The [new independent archive](evidence/cl-linux-ci-ac63255-20261102/README.md) preserves all50 original members and21 exact replay-source members before November16 expiry. Its per-profile gates, error arrays, test completion and separate WASM identities were extracted and replayed, not inferred from aggregate exit1. Captured gates all PASS/no errors; internal gates all FAIL/97 lifecycle and284 transaction errors. Earlier Linux and local archives remain intact. These results describe ac63255, not the patch below.

The [decision memo](12-cl-resource-decision-20261102.md) establishes that 100M/100/50 first appeared as local checker literals, with no documented quantitative derivation.100M matches the host25 budget default;100/50 match SDK25 invocation entry/write defaults; SDK25 invocation CPU is600M. Current intent is a conservative independent policy, not a verified network ceiling. The captured protocol29 settings are400M instructions/400 unique footprint keys/200 writes/40MiB, with full pinned cost coefficients.

Keeping current limits requires venue/accounting changes: unchanged Aquarius subtrees alone exceed100M on deposit, redeem and both swap legs. The memo quantifies floors, key packing/cache tradeoffs, and correctness/migration/restoration proofs. A separately proposed300M/300SDKentries/300unique keys/100writes/32MiB policy with20% workload headroom is analysis-only and is NOT implemented. It has only3.305M CPU remaining before its proposed workload boundary in prior unsigned measurements; signed and stressed workload costs could invalidate it. SDK entry totals and unique serialized transaction keys remain distinct. No policy or extra stage is selected.

## Production/default source patch and dependency order

The default controller now invokes receipt reserve checks in the same finalization implementation used by activate, execute and expired resolve. Checks remain after collateral receipt deposit, fresh health valuation and open fee accounting. Missing or incorrectly decoded ABI traps and rolls back the whole finalizer. Successfully decoded `None` denotes an explicitly unconfigured ABI-capable legacy market. Configured funding markets check both borrowing permission and the reserve. Atomic liquidation conversion retains its existing fresh eligibility requirement; it cannot activate a healthy pending position as a bypass.

`collect_open_fee` reserves pToken fee entitlement; it does not withdraw receipt cash. The compiled finalizer matrix uses a real10bps/200000-pToken fee, changes pause/floor after swap, and proves complete-ledger rollback plus exact recovery refund. It does not invent a fee-induced cash withdrawal.

The owner-funded swapped-pending recovery ABI is exported in the default controller for ordinary SAC Long and Short positions. It requires the owner, PendingOpen/V3 state and recorded execution; fully repays post-accrual debt up to the authorized maximum; asserts zero debt; returns the execution collateral; refunds reserved fee shares; and clears position state atomically. Short collateral includes original margin plus sale proceeds. Pause/reserve headroom is not a recovery prerequisite. Full repayment and release of active positions remains available; release produces backed free-margin receipt claims, not wallet redemption.

Independent review found an inherited prototype bug: fee shares were refunded before checkpointing the owner's margin-fee entitlement. The old compiled default WASM incorrectly earned400 units from intervening fees. The fix calls `accrue_user_fee` before increasing the refunded balance. A real second-position fee distribution proves the fixed owner receives0, the original provider200000, repeated claim0, and refunded shares200000. Before/fixed logs are preserved; the interrupted pre-fix full run is explicitly incomplete. This fee entitlement is separate from Aquarius reward history, whose existing proofs remain required.

Exact dependencies, regardless of unchanged package version strings:

| Component | Required ABI / identity |
|---|---|
| Default margin-controller0.1.0 candidate | Adds `recover_pending_open_funded(user:Address,position_id:u64,max_repay:u128)->u128`; pins distinct default WASM/spec. |
| Debt receipt-vault0.2.0 | `get_min_cash_reserve_bps()->Option<u32>`, `get_cash_borrowing_enabled()->bool`, `assert_borrowing_reserve()->()`; missing entrypoints cannot be caught as opt-out. |
| Funding preparation | Actual `set_min_cash_reserve_bps(admin,bps:u32)`, `set_cash_borrowing_enabled(admin,enabled:bool)`, getters/readback and `get_borrowable_cash()->u128`. |
| Funded recovery | `repay_full_for_margin(position_id:u64,payer:Address,max_amount:u128)->u128`, `get_margin_borrow_balance(position_id)->u128`, correct controller binding and existing position debt. |

Pinned leveraged-fix source47858c1 already supports full repayment but lacks reserve getters. Its separately compiled historical receipt fixture rejects finalization and still supports funded recovery. Both current and old receipt manifests say0.2.0, so versions alone cannot establish compatibility. The offline helper records original source hashes, modified standalone manifest, SDK25.3.1 lock dependency and synthetic administrator. This is historical-source compilation in controlled state, not proof of an exact deployed binary or storage migration.

Proposed deployment order, not executed: keep borrowing disabled; audit/pin receipt ABI upgrades and storage compatibility first; verify all bindings and full-repay behavior; configure approved funding reserves with borrowing explicitlyfalse and verify getter/cash readbacks; upgrade the reviewed default controller only after dependencies and target-runtime proofs; recheck old pending/active recovery and post-cost finalization; leave borrowing disabled. Deployment/readbacks and enabling borrowing require separate release decisions. Production maintenance margin stays unset.

CL custody and third-party funded liquidation remain validation-only. The validation constructor/entrypoints still reject Mainnet. Default recovery rejects a persisted `ClShareAsset` marker to prevent a known unsupported downgrade from dropping reward hooks. Do not deploy default over validation CL state; this patch does not promote Stage1 CL support or authorize a downgrade. Both binaries have synthetic test identities and are not deployable production-approved artifacts.

## Proof mapping and interpretation

[The complete46-label map](../../peridot-contracts/config/cl-proof-map.json) binds concrete test functions, assertions, measured flows and unsigned transaction cases. Existing40 proofs remain required; six default-build tests add:

| Compiled test suffix in `test::default_reserve_tests` | Proof |
|---|---|
| `compiled_default_reserve_active_repay_release` | Configured swap/activation; active full repayment/release with pause and100%reserve; no debt, absent position, backed free-margin claim. |
| `compiled_default_reserve_post_cost_finalizers_rollback_recover` | Activate/execute/expired resolve reject pause and floor after fee accounting; complete rollback; funded recovery/refund; repeated execution rejection. |
| `compiled_default_pending_recovery_adversarial` | Wrong owner, inadequate maximum/funding, missing nested external authorization, persisted CL downgrade marker and failed actual SAC collateral transfer; complete rollback and eventual recovery. |
| `compiled_default_short_pending_recovery_paused_shortfall` | Atomic full repayment and return of original margin plus sale proceeds under pause/shortfall. |
| `compiled_default_missing_receipt_abi_fails_closed` | Actual historical-source receipt WASM missing reserve ABI rejects finalization; its full-repay ABI remains usable. |
| `compiled_default_pending_fee_refund_preserves_historical_fee_owner` | Intervening actual fee distribution cannot accrue retroactively to refunded fee shares; original provider entitlement and repeat behavior preserved. |

Only generated external fixture accounts use scoped authorization. V3/receipt/adapter/Aquarius contract authorization remains actual nested execution. Missing nested external auth is an SDK rejection/rollback proof; its unsigned recording companion measures positive execution costs and is explicitly NOT a transaction signature/auth rejection proof. Signed/account/contract-wallet execution and serialization remain unmeasured release gates. Actual Aquarius bytecode with synthetic ordinary SAC state and fixed oracle/plane is separate from CL-facade integration, production assets and live venue state.

## History operating decision

The [operating plan](13-cl-history-operating-plan-20261102.md) defines authenticated boundary capture, intratransaction replay, an append-only beneficiary/custody/key index, gap watermarks, raw evidence mirrors/rebuild, archival funding and governance evidence review. No indexer, funded service or governance workflow is claimed deployed. No unresolved entitlement is pruned. If authentic evidence never arrives, backing stays quarantined indefinitely, outside NAV; later cumulative readings, elapsed time or a vote without evidence cannot establish missing boundaries. One unresolved segment can delay verified settlement elsewhere in its interval. Forced exits remain independent of attribution, subject to actual state restoration, fee funding and execution limits.

## Reproduction

From `peridot-contracts` with the pinned public Aquarius fixture and dependencies installed, use a fresh output directory:

```sh
CARGO_INCREMENTAL=0 \
CL_CAPTURED_CONFIG="$PWD/../docs/deployment-handoffs/evidence/cl-resource-reconciliation-20261102/network-config.json" \
AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm" \
CL_CHECKPOINT_DIR="$PWD/target/cl-decisions-fresh" \
bash scripts/run_cl_share_checkpoint.sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p 'test_cl*py' -v
```

All WASMs use `stellar contract build`. Historical receipt build requires git object47858c1; CI now checks out full history before the offline helper. The runner builds separate validation/default/historical artifacts, exercises both unchanged profiles, executes required archive/history reruns, binds source/logs and runs `check_budget.sh` for each profile and separate native prototype. Missing measurements, test failures and policy failures remain nonzero. Use the evidence README for exact gate/error replay, not exit1 alone.

## Completed baseline results

The corrected baseline runner completed with exit1. Captured invocation/lifecycle/transaction/checkpoint all PASS with no errors; internal all four FAIL with106 lifecycle and303 transaction errors. This is the preserved comparison baseline for the subsequently authorized isolated split prototype, not adoption of a policy or production stage change.

| Profile | Main compiled | Archival reruns | Native funded | SDK CPU max | SDK memory bytes | SDK entry total max | Writes max |
|---|---|---|---|---:|---:|---:|---:|
| internal-policy | 4 pass / 32 fail | 0 pass / 5 fail | 5 pass / 4 fail | 100,003,731 | 16,847,283 | 137 | 30 |
| captured-network-diagnostic | 36 pass / 0 fail | 5 pass / 0 fail | 9 pass / 0 fail | 235,456,689 | 24,514,601 | 213 | 56 |

Internal maxima include recorded failed/rejected partial executions; they are not completed lifecycle costs. Captured evidence contains1652 SDK rows and2741 unsigned target-runtime transactions. Unique transaction footprint reaches158 keys (distinct from213 SDK entries), with57 writes including restoration. Unsigned CPU maximum236693999, memory21678566bytes, envelope15092bytes. Complete fee component independent maxima: inclusion100, nonrefundable162160, refundable15147, rent22686852, total22845642stroops. Components are independent maxima and must not be summed; rent is already included in total. Real signed overhead and live-state fees remain unmeasured.

Native vault53 PASS; native default margin110 PASS/2 existing ignores; checker/replay/summary32 PASS. The new default compiled subset has6 PASS captured and1 PASS/5 FAIL internal. Reward proofs retain actual-bytecode22732 received/22731 paid/1 rounding backing; exact scaled historical conservation and quarantine are asserted in the complete suites. Separate margin-fee refund regression gives Alice0/Bob200000/repeat0 versus old-WASM erroneous400. All three claim boundaries remain.

[The durable baseline index](evidence/cl-decisions-reserve-20261102/index.json) pins separate validation/default/historical-source WASMs, all raw reports/logs, full fee inputs, proof mappings, diagnostic failures and the compressed [execution report](evidence/cl-decisions-reserve-20261102/execution-summary.md). It is compiled controlled-state evidence, not real signatures, live venue state or release approval. This source checkpoint's new CI status must be reported separately from the completed ac63255 Linux artifact.
