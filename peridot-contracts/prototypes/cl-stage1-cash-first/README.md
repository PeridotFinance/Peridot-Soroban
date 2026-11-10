# Stage 1 cash-first transaction measurement — isolated prototype

Stage 1 remains incomplete; PR75 stays draft. Option1 is permanently closed.
This overlay measures the approved 1× design on the **existing runtime29 harness**.
Production contracts, profiles, borrowing, maintenance margin and handoff00 are unchanged.
No signing, broadcasting, deployment, secrets or legacy Mainnet bindings.

The measured roots include the receipt command, actual vault calls, token transfers,
Aquarius, valuation, reward accounting and real contract authorization in one transaction.
A helper invocation gets no new budget. Receipt→vault→receipt callbacks are prohibited
by Soroban; the receipt passes its stored typed command under actual receipt authorization.
Synthetic external actor authorization is scoped; it is not account-signature evidence.

## Reproduce

From `peridot-contracts`, with the existing pinned fixture dependencies restored:

```sh
python3 prototypes/cl-stage1-cash-first/prepare.py --output target/cl-stage1-rebuild
CL_STAGE_BUILD_DIR="$PWD/target/cl-stage1-rebuild" bash prototypes/cl-stage1-cash-first/run.sh internal-policy compiled_stage1 target/cl-stage1-replay-internal
python3 prototypes/cl-stage1-cash-first/summarize.py target/cl-stage1-replay-internal
node prototypes/cl-stage1-cash-first/footprints.cjs target/cl-stage1-replay-internal > target/cl-stage1-replay-internal/footprints.json
CL_STAGE_BUILD_DIR="$PWD/target/cl-stage1-rebuild" bash prototypes/cl-stage1-cash-first/run.sh captured-network-diagnostic compiled_stage1 target/cl-stage1-replay-captured
python3 prototypes/cl-stage1-cash-first/summarize.py target/cl-stage1-replay-captured
```

`prepare.py` uses only `stellar contract build` for WASMs. Native `cargo test`
executes compiled WASMs. It needs the existing `target/cl-restoration-combined-build/artifacts`
fixture set, `target/cl-share-checkpoint/aquarius-pool.wasm`, pinned Rust dependencies,
OpenZeppelin submodule and the unsigned XDR decoder's locked Node dependencies.
The durable evidence archive contains exact fixture WASMs and build manifests. No
production address or key is required. Outputs must be fresh directories.

Final build: `target/cl-stage1-cash-first-build-v4`; final measurements:
`target/cl-stage1-internal-v6`, `target/cl-stage1-captured-v6`.
Historical v3 measurements precede the mint-valuation correction and are not final evidence.

`run.sh` executes `check_budget.sh` for every measured flow, retaining all global gate
errors. Its exit0 means compiled tests and the **invocation** gate passed, not full
lifecycle/transaction/checkpoint approval. `summarize.py` requires all nine transaction
rows, both complete journeys,13 passing tests, named invariant and actual-backing regression,
then independently decodes envelopes/footprints. It refuses release approval. Per-profile
summaries preserve missing flows and exact errors. Do not pass `--gate invocation` off as
release qualification. No workflow currently executes this isolated prototype.

## Accounting rules and proof mapping

- `compiled_stage1_full_open_close`: five open roots then prepare-close, allocate
  mixture and convert/payout; two actual intervening Aquarius trades; exact scaled rewards.
- `compiled_stage1_full_open_in_kind_close`: five open roots then prepare-close and
  a complete in-kind withdrawal/payout transaction; same accounting assertions.
- Nine `compiled_stage1_isolated_*` tests independently measure each root. Setup is
  explicitly captured-profile fixture construction, including the old atomic seed deposit.
  That seed is not a new production bootstrap proof or an omitted user transaction.
- `compiled_stage1_incumbent_fee_entitlement_allocated_before_share_burn_one_unit`
  proves **INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN** through the vault:
  one Aquarius liquidity unit, zero principal, all fees collected, allocation at old
  supply, retained backing, newcomer balance unchanged, allocation event before share burn.
- `compiled_stage1_cash_mint_uses_actual_fee_inclusive_backing` creates an off-oracle
  pool price and proves cash mint uses fresh actual withdrawable composition, valued
  at the independent oracle, including fees exactly once. Convergence-value accounting
  is reserved for investment cost protection and is not the share mint denominator.

Command funding, reserves, post-burn exit claims and unsolicited donations are excluded
from common backing. Mint releases only contributed cash into common backing and mints
locked shares directly to their beneficiary. Later investment checks fresh prices, current
cap and execution quotes, charges bounded losses to the command reserve, preserves both
actual backing and conservative convergence value, refunds excess and unlocks shares.
TEST ONLY range[-2000,2000], equal token decimals, token0 underlying, cap2,000,000,000raw,
1800-second test deadline (3600 maximum), contribution200,000,003raw/reserve100,000raw.
These values are not approved production settings or proven workload bounds.

Existing claim-first mint/burn/transfer/custody hooks remain. The measured1× path mints
and burns directly held shares; it has no margin/receipt beneficial-custody reassignment.
It does not prove the separate2× transfer/custody lifecycle. Debt is always zero.

## Remaining build prerequisites

The numeric rows permit a Stage1-only build plan, not adoption of this overlay. Complete
command expiry/retry/cancellation and payout-failure rollback tests; qualify reward-outage
exit/waiver/quarantine behavior; qualify legacy entrypoint compatibility and lock guards;
prove valuation rounding/range bounds and concurrent liabilities; measure bounded histories,
first mint/final exit and cold restoration; obtain Fable review and later authorized signature
measurements. No new production build/integration is performed at this checkpoint.

Cancellation before mint returns segregated actual token mixture. After mint it refunds
only unused reserve and unlocks existing shares; **this is not an immediate token-mixture
exit**. Normal staged burn fails closed on upstream reward-claim outage. Therefore complete
in-kind cancellation/recovery during outages remains a blocker, not a passed proof.

Fable high-effort review was attempted with tools disabled and the previously approved
bounded tables/design payload. CLI returned429 `usage_limit_reached` before any model
execution. The internal reviewer found the mint pricing issue and verified its correction;
that is not a substitute claim of Fable review. The review payload and status are preserved.
