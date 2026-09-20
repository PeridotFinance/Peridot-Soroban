# LP compiled-stack validation — September 20, 2026

Status: improved validation candidate, **not a Mainnet release approval**.
Mainnet initialization/publication fences remain. No funds moved, no deployment
or keeper configuration changed during this work.

## Defects reproduced and targeted changes

Combining the compiled router, controller, receipt and strategy with the actual
concentrated-pool WASM exposed memory overruns that the separate component tests
did not cover. The second simultaneous stablecoin loan used 48,280,331 bytes;
oracle-recovery liquidation used 47,198,127 bytes. Mainnet read-only configuration
at ledger64526681 confirmed the same 41,943,040-byte limit as the test fixture
(400,000,000 CPU instructions). No limit was increased to make the tests pass.

- The router's new `price_snapshot` returns live price, precision and resolution
  together. Required observations, expiry/invalidation and cross-quote validation
  still run. Failed upstream precision lookup returns unavailable, not the legacy
  decimals fallback. LP controller pricing requires this interface; no legacy
  price/static-price/warm-cache fallback on failure. Generic controller behavior
  is unchanged.
- LP receipts add `get_accrued_account_snapshot`: debt accrual and the resulting
  account snapshot in one contract frame. A failed health snapshot rolls accrual
  back. Cross-market debt is never replaced by stale cached debt. Missing snapshot
  support fails closed in the LP controller.
- `get_liquidation_snapshot` batches the existing balance, LIVE exchange-rate and
  underlying reads; it does not substitute a cached NAV. The unused legacy
  immediate-redeem preview is conservatively zero for LP liquidation. Seizure
  transfers shares; subsequent withdrawal still checks health and liquidity.
- Fresh LP receipts omit the already-no-op zero-PERI controller accrual call.
  Binding is sealed and activation requires the fresh zero-PERI controller.
  AQUA ownership/checkpoints are unchanged. Enabling PERI in a future release
  would require coordinated receipt/controller changes, not just controller code.

The explicit receipt export allowlist now has58 methods. There is no generic
bootstrap, margin, flash-loan or uncheckpointed share-mutation export.

## Test boundary

Three new explicit compiled tests cover simultaneous PYUSD/USDC loans against XLM,
exact borrower auth, repayment, observation invalidation/expiry, atomic refusal
of liquidation during an outage, successful liquidation after recovery, primary
claim outage, historical AQUA ownership, actual conversion and settlement payout.
Reward conversion uses the same hash-pinned concentrated-pool code, initialized
locally as a reward route. It is NOT a snapshot of the deployed AQUA swap route.

Additional ordinary tests cover accrued-snapshot rollback, invalid live snapshot
with still-valid legacy price/cache, strict router metadata and observation checks.
The complete workspace passes676 unit tests and3 doc tests;62 keeper tests pass.
Clippy completes with existing warnings and the existing option_env_unwrap
allowance, not a strict warning-free result. Final explicit-test results are in
the local handoff and `/private/tmp/peridot-lp-full-stack-final-compiled-20260920.log`:
all7 explicit checks pass, including exact borrower and liquidator authorization.

Research limits remain250 entries /200M CPU /40MiB memory; all transaction
resource enforcement remains enabled. Measured representative operations:

| Operation | Entries | CPU instructions | Memory bytes |
| --- | ---: | ---: | ---: |
| Second simultaneous loan, exact borrower auth | 242 | 192,474,037 | 36,674,915 |
| Borrow during actual primary-claim outage | 232 | 185,090,617 | 37,981,100 |
| Primary reward conversion, actual pool-code route | 116 | 106,293,072 | 33,473,874 |
| Converted reward payout | 121 | 111,673,246 | 26,839,380 |

Actual pool code SHA256:
`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.
Upstream oracle contracts, interest model and boost/plane dependencies are still
native controlled fixtures. There are no configured gauges in this fixture.
These results are not a complete deployed-dependency/Mainnet-state simulation,
worst-case four-stream resource proof, restoration test or signed canary.

Validation-only artifacts (test admin, Mainnet fences retained):

- Receipt: `68ce982f34c1784ccc911463b160e5133bbdba1d8da2821dae6df1dccdf52199`
- Controller: `9c7c92d3c161f2a761ceef6bbf40bcefe427b0594375d362a537a17575bcfcdb`
- Strategy: `c23d93b935c4f22dac3c1c33ccebaef40ed9f1e6b6da1b5ba4cfff322ed8ab1e`
- Router: `20114bc89bb2323fdfbb5e1588795bbc88dade5264e2d161e34d2b82c7c3759d`

Run explicit checks with LP_RECEIPT_WASM, LP_CONTROLLER_WASM, LP_STRATEGY_WASM,
LP_PRICE_ROUTER_WASM and AQUARIUS_CONCENTRATED_WASM pointing to these files:
`cargo test -p lp-lending-vault --locked compiled_ -- --ignored --nocapture`.
Use `node scripts/check_lp_lending_exports.mjs <receipt.wasm>` for the ABI check.

## Fresh operational evidence and remaining gates

The cloud shadow observer's single run through September20 15:44UTC recorded
1453 scheduled slots,1110 collected/agreeing,0 missed,343 collection/validation
failures and990/1423 healthy mature windows. Failed runs were one sample on
September19 19:32UTC,315 consecutive samples on September20 05:55–11:09UTC,
and27 samples13:45–14:11UTC. Current sanitized logs do not distinguish endpoint
failures from rejected market data. No historical root cause is established.
This is uninterrupted scheduling, NOT uninterrupted price availability. The
depth observer remains research-only and cannot publish.

Read-only Mainnet migration previews at16:22UTC (ledgers64527109–112) show deployer
ownership of all live pilot shares and reported debt zero in all three receipts.
Both pools report empty gauge maps at that time. PYUSD/USDC full withdrawals
simulate4.8427816/4.8449612 settlement tokens. The XLM withdrawal simulation fails
inside the strategy after the paired-asset swap estimate. No withdrawal was
submitted. Diagnose its precise live guard/configuration before changing anything;
do not treat this as permission to widen protections. No historical-claim audit
is implied merely by current share ownership and zero debt.

User was asked to approve canary-only supply caps30XLM/6PYUSD/6USDC and borrow
principal caps1XLM/1PYUSD/1USDC. These are proposed exposure limits, NOT applied
settings or manipulation-safety bounds. Fee budget still needs a reproducible
production-admin artifact and fresh upload/deployment simulations. Pricing
availability/outage policy, actual dependency/restoration/migration validation and
final scoped security review still gate deployment. Existing CF targets remain
50% XLM /80% stables; live pilot CFs remain0. No debt or frontend activation.
