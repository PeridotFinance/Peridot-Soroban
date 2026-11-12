# USDC/XLM cash lending pilot — implementation scope, not a build

11 November 2026; source checkpoint `b7b433c4885db5878f7fa6f25f3d1fad2b5a07b3`.
This bounded document follows shared handoff00 before handoff02. The later user direction
narrows the pilot to fresh cash-only markets, without Upshift, DeFindex or margin/LP
execution. No code, contract build, new test execution, signature, deployment, key access
or public financial operation was performed for this scope.

**Recommendation:** a fresh cash-only supply pilot is a smaller implementation and
qualification project than CL Stage1, but it is not deployable from the current script
and evidence alone. The reserve ABI already exists. The immediate blockers are the
complete cash-root measurement/authentication matrix, production manifest/readback
completion and recovery qualification. Supply-only launch and permission to enable
borrowing are separate decisions. Borrowing remains disabled.

## Scope and exact identities

Five fresh instances: one `simple-peridottroller`, two `receipt-vault` instances and two
`jump-rate-model` instances. Existing audited/reviewed source can be reused only after
pinning the exact selected commit/WASM and confirming its current changes; package
version strings do not identify an artifact. No MarginController, SwapAdapter, Aquarius,
Upshift adapter or external strategy is required for this pilot.

|Market|Underlying identity|Exposure|
|---|---|---|
|USDC|Circle issuer `GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`; SAC `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`|USDC issuer/peg and, after separately approved lending activation, borrower credit/liquidation exposure|
|XLM|Native XLM, no issuer; SAC `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`|XLM price and, after activation, borrower credit/liquidation exposure|

The deliberately explicit configured identities above must be checksum/asset-contract
verified in the release manifest; ticker matching and a regex are insufficient.
All old Peridot Mainnet controller/receipt/strategy deployments remain compromised and
retired. Do not bind them, migrate their admin roles or reuse their addresses. Public
external SAC/oracle identities are a different category and require fresh verification.

“1×” here means unleveraged supplier claims and no LP/margin product. Ordinary lending,
once enabled, still permits borrowing and manual recursive loops within CF/caps. It is
not a promise that the ordinary controller forbids all user leverage. Initially borrowing
is durably disabled: idle local cash produces no external yield; no fixed supplier APR
or unfunded PERI incentive should be promised. Once approved, supplier income is the
borrow-interest share after configured reserve/admin fees. At protocol level this is a
transfer from borrowers, not external strategy yield.

“Cash locally” means all **unborrowed** underlying is held by the receipt contract.
Outstanding loans are debt claims, not cash. Neither target_cash_bps nor the hard reserve
creates additional backing. Supplier withdrawal liquidity, loss-protection assets,
operating XLM and funded-liquidator debt-asset capital must be separately funded.

## What exists and what remains

|Area|Existing source/evidence|Remaining work before pilot|
|---|---|---|
|Receipt accounting|Actual-delta deposit, internal ManagedCash excluding donations, pToken exchange rate, interest indices/principal mirrors, supply/borrow caps, withdraw, transfer and debt state|Compiled fresh two-market fixture with real SAC behavior, nonzero elapsed interest, dust/donation/first-and-final-holder cases and explicit current bytecode identities|
|Cash reserve ABI|`set_min_cash_reserve_bps(admin,bps:u32)`, `get_min_cash_reserve_bps()->Option<u32>`, `get_borrowable_cash()->u128`, `assert_borrowing_reserve()`, `set_cash_borrowing_enabled(admin,bool)` and getter in default receipt source|Pin default WASM/spec with these methods; no feature-only or historical receipt substitution; verify source/live cash and protocol-claim treatment at all relevant boundaries|
|Borrowing pause|Presence of MinCashReserveBps opts into local-cash borrowing; CashBorrowingEnabled defaults false. Both ordinary and margin borrow enforce it when configured. Controller has a separate borrow pause|Keep durable receipt flag false; controller pause alone expires and is insufficient. Test pause expiration and every callable borrow/flash-loan route; no automatic activation|
|Ordinary controller|Account-level CF/health, market entry/exit, collateral-transfer checks, primary price freshness, close factor and liquidation incentive, repay-on-behalf and pToken seizure|Complete two-market tests in both directions, bounded entered-market inventory, manual loops, pause/stale-price matrix, actual nested auth and custom-account reentry negatives|
|Rate model|Separate volatile/stable JumpRateModel instances; planner initializes explicit base/multiplier/jump/kink and binds each receipt|Approved parameters/readback, rate and utilization integer math, nonzero accrual and cash-withdrawal behavior; no inherited historical production settings|
|Liquidation|`SimplePeridottroller.liquidate(borrower,repay_market,collateral_market,repay_amount,liquidator)` atomically calls authorized receipt repay_on_behalf and collateral seize|Borrower-independent funded liquidation, insufficient funds/authorization/failed seize rollback, stressed liquidation depth and operating-capital sizing; fresh prices required; collateral is a pToken claim, not guaranteed immediate cash|
|Full ordinary repayment|`repay(user,amount)` and `repay_max(user)` exist. The former caps repayment before accruing; latter accrues first and recomputes full debt|`repay_max` has **no payer-specified maximum** and dynamically sizes nested transfer. Existing unit test uses blanket auth. Prove actual source and detached authorization after elapsed time; if bounded max/refund is required, review an additive ordinary-account helper using the existing margin-helper design, not an invented setter/mechanism|
|Release/withdrawal|Cash withdrawals can cross the borrowing reserve floor; final exit rejects outstanding market borrows; pToken seize does not synchronously redeem collateral|Prove full repayment then debt-free withdraw/exit with borrowing disabled, reserve shortage, oracle outage and exhausted other-market cash. A cash shortfall may delay payout; never advertise immediate liquidity for all deposits|
|Deployment preparation|`scripts/deploy_funding_group_mainnet.sh` supports only `build` and `plan`; builds only via stellar contract build. `prepare_funding_group.py` creates unsigned actions/readbacks, strategy list empty, margin unset, fresh deployments and disabled borrowing|No upload/deploy/verify executor exists here. `verify_funding_group_mainnet.sh` is absent. Need resumable reviewed transaction manifest, exact ABI argument validation, checksum addresses, post-deploy readbacks and role transfer verification; no execution authorized now|
|Keeper/monitoring|Existing repository TTL/price monitoring patterns can be reused|Key inventory, persistent restoration batching, fee budget, primary freshness/issuer flags, cash utilization/borrow pause/ledger archival alerts, no always-available-keeper assumption; no new service deployment|

`receipt-vault` package0.2.0, `simple-peridottroller`0.1.0 and `jump-rate-model`0.1.0
version labels are insufficient: the historical `leveraged-fix` receipt with the same
0.2.0 version lacks the reserve ABI. Default build must use public pinned init-admin
configuration; never ship `test-default-admin`, CL validation or LP-specific source-sharing
features as the cash artifact. Existing CL validation Mainnet guard remains untouched.

No external strategy is bound. Therefore Upshift's valuation_state/recall/withdrawable
integration, loss policy and adapter are deferred, rather than required for local cash.
The deployed verifier must assert both boosted-vault and margin-controller bindings are
absent, no active margin-liquidation controller registration, zero external-strategy
balance and the exact supported-market inventory. No LP custody mode is enabled.

## Reserve and repayment rules to preserve

For a configured cash market, the current source calculates supplier NAV from managed
assets/debt net of protocol reserves/admin fees and rounds the minimum reserve upward.
Borrowable unencumbered cash is `min(live balance, ManagedCash) - protocol claims - reserve`
with saturating lower bounds. Donation transfers cannot make extra credit available.
The borrow must leave that floor; withdrawal/repayment are not gated by the fresh-risk
floor. Receipt `get_borrowable_cash` reports numeric headroom and does not itself include
the durable disabled flag: UI must combine them. The planner's expected zero is valid
for a freshly empty receipt, not every funded-but-disabled market.

Reserve setter existence was the earlier ABI blocker; it is now removed in source.
Native tests `reserve_ordinary_borrow_and_durable_pause`,
`reserve_debt_examples_and_rounding`,
`reserve_cash_donations_and_protocol_claims_are_not_lendable` and
`reserve_withdrawal_can_cross_floor_then_complete_open_check_fails` document the intended
behavior. These are synthetic SDK25 fixtures, not deployment or network-auth proof.
The margin complete-open default-build proofs in handoff14 are useful dependency evidence
but are not ordinary cash-pilot transaction measurements.

The cash pilot needs no V3 complete-open transaction. If margin is later added, preserve
its post-cost finalization reserve checks and separate activation approval. Do not let a
future path silently reuse this pilot's supply-only approval.

Ordinary `repay(amount)` can leave accrued residual debt by design. The existing
`repay_max` recomputes post-accrual debt, but its blanket-auth native test does not prove
that a detached signature recorded earlier will authorize a later, larger token transfer.
Required later test: real private source-account and detached auth forms; elapsed time;
nonzero interest; maximum spend/refund if that interface is added; inadequate balance;
zero debt; no residual debt; subsequent withdrawal/exit; nested-amount mismatch rollback.
`repay_full_for_margin(position_id,payer,max_amount)` already provides the max/refund
pattern for a different debt namespace. It cannot be called as an ordinary-account full
repayment shortcut and its CL prototype evidence is not borrowed for this pilot.

## Resource evidence — no complete cash row qualifies yet

Required boundaries remain **100,000,000 CPU /41,943,040 memory bytes /100 SDK entries /
100 unique Soroban keys /50 writes**, plus byte/event/envelope/rent/fee checks. SDK entries
are memory reads + disk reads + writes; they are not unique serialized transaction keys.
No changed limit, projected savings or inferred pass is authorized.

|Required complete pilot root|CPU|Memory|SDK entries|Unique keys|Writes|Qualification|
|---|---:|---:|---:|---:|---:|---|
|Supply: receipt.deposit with fresh controller/rate model/real SAC|Not captured|Not captured|Not captured|Not captured|Not captured|FAIL: required complete evidence missing|
|Borrow: both USDC→XLM and XLM→USDC collateral directions|Not captured|Not captured|Not captured|Not captured|Not captured|FAIL: required complete evidence missing|
|Repay/repay_max with elapsed interest|Not captured|Not captured|Not captured|Not captured|Not captured|FAIL: required complete evidence missing|
|Liquidate: controller repay + seize, both directions|Not captured|Not captured|Not captured|Not captured|Not captured|FAIL: required complete evidence missing|
|Withdraw/release and cold restoration journeys|Not captured|Not captured|Not captured|Not captured|Not captured|FAIL: required complete evidence missing|

There is one relevant historical **native synthetic reserve borrow** observation,
retained in `docs/deployment-handoffs/evidence/cl-stage1-20261102/reserve-resources-final.jsonl`:
3,063,343 instructions;322,223 memory bytes;41 memory reads+1 disk read+9 writes =
**51 SDK entries**;92 disk-read bytes;1,360 write bytes;712 contract-event bytes.
Unique transaction keys, serialized/signed transaction, exact full fee and target-Core
application are absent. It is single-receipt local CF/static-rate synthetic accounting,
not the two-market default compiled pilot. Therefore it is an indicative lower-complexity
measurement, **not a passing borrow row above**. Native test uses budget reset_unlimited;
it does not establish acceptance execution under the selected CPU cap.

Native `test_borrow_budget_peridottroller_same_market` asserts CPU≤5.3M/memory≤950k;
`test_borrow_budget_with_two_cross_market_collateral_positions` asserts CPU≤10M/memory≤3M,
read entries≤90 and writes≤20. These are assertions, not retained actual values or complete
100-SDK-entry proof; the latter also uses three boosted mock markets and blanket auth.
Controller `allow_heavy_liquidation_functional_test` explicitly disables resource limits.
Its tests prove functional intent, not liquidation feasibility. Do not reuse CL O1/O5,
V3 active liquidation, LP-engine or closed-Option1 rows as cash-pilot evidence.

The known structural footprint is two token SACs (for liquidation), up to two entered
receipt markets, controller, selected rate models, primary oracle, per-user debt/pToken
records, interest/reserve/cap state, controller price/health/reward records and all
contract code/instances. Broad initialization/TTL sweeps can touch extra keys even with
no strategy. Exact physical inventory and restored writes must be enumerated by actual
complete transactions, not estimated from function names or WASM byte sizes.

## Exact qualification matrix and deployment ordering

Future work, not executed here:

1. Freeze a **fresh cash-only** manifest: approved admin/control model, exact default
   source/spec/WASM hashes, SDK/Core versions, network settings/cost coefficients, XLM
   and Circle-USDC identities, primary oracle metadata/mapping, positive finite supply
   and borrow caps, both controller/local CFs, rate curves, reserve/admin/flash fees,
   liquidation incentive/close factor and receiver. No parameter is approved by this
   document. Zero cap means uncapped in this code; it cannot mean disabled.
2. Extend the existing unsigned planner, then later implement reviewed execution and
   independent verification. It presently omits explicit reserve-factor/admin-fee/
   liquidation-policy configuration and many cap/rate/admin/binding readbacks; defaults
   cannot silently stand in for operator decisions. Validate checksums, exact argument
   types and bounds, and distinguish expected Option values from missing ABI failures.
3. Build only with stellar contract build when that phase is approved; private compiled
   fixtures for controller+two receipts+two models+real SACs and primary-oracle-shaped
   data. No mock_all_auths or mocked own-contract auth. Public test admins are synthetic,
   not production identities. Initial configuration must never allow borrow while
   wiring is incomplete; durable guard false before rate readiness and controller pause.
4. Measure supply, withdraw, borrow both directions, accrued partial/full repay and
   atomic liquidate both directions from warm, near-expiry and fully archived prestates.
   Include valid/invalid source and detached auth, two entered markets, zero-PERI/no
   external strategy, partial/maximum amounts, donating attacker and restored code/
   instance/data. Run every row through check_budget.sh; exact per-gate errors and missing
   flow failures remain nonzero. Include cost-model pins, signed envelope/auth overhead,
   actual Core fees/sequence/debits and restoration rent/journey total.
5. Prove stale/future/zero primary prices block new cross-market risk and priced seizure;
   normal borrowing pauses must not block repayment or permitted funded seizure; reserve
   shortage blocks new borrow but not repay/authorized exit. Test pToken transfer/allowance
   bypass, recursive borrowing, invalid/mismatched controller, unauthorized admin/init,
   insufficient liquidator funds, failed collateral transfer, replay, reentry and complete
   rollback. Quote a liquidator repayment-capital budget and cash-shortfall limitation.
6. Prepare fresh upload/deploy/init/wiring/verification batches only after proofs. Deploy
   controller and rate models, then receipts with pinned public admin; configure disabled
   reserves, bind/list markets, set explicit risk/rate settings and verify all directions.
   No old market IDs. No strategy, margin controller or fallback-price binding. Keep
   borrowing false in both receipt instances; supply-only readiness is a separate gate.
7. Separate operator decision for ordinary borrowing activation after the complete stress,
   oracle, accounting/auth, resource, operating-budget and liquidation-capacity gates.
   No production maintenance-margin value is needed or set. No margin deployment implied.

## Primary oracle, depeg and recovery boundary

Handoff02 names Reflector `CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN`;
that is a candidate external dependency, not a newly verified production pin here. Default
controller supports lastprice/decimals/resolution, freshness multiplier, token-symbol
mapping and cached prices. It also contains a static fallback path. For the primary-only
pilot, fallback values must remain absent and verified absent; do not add a publisher,
paid provider or fallback integration. Freshness/correct units and issuer restrictions
must be qualified against the exact selected primary ABI.

No pool-implied guard belongs to the cash-only USDC/XLM roots: these contracts do not
hold an LP. XLM is volatile, not a dollar peg. USDC must use a trustworthy current primary
price rather than an unmonitored hardcoded dollar sentinel. Non-USDC stablecoin guard
work is part of the separate CL pair choice, not a justification to add Aquarius reads
inside this cash pilot. Extra oracle/issuer checks, if required, must be in the metered
cash-root test, not in an unmeasured keeper prerequisite.

**Stale-price liquidation remains a blocker:** ordinary price-based seizure calls
require_price for both legs. A fresh cache lasts only its accepted freshness period;
with no trustworthy primary data, fair seizure is unavailable. Repayment is available
without price valuation; eventual zero-debt exit still requires liquid cash and valid
accounting/issuer transfers. There is no approved stale-price haircut/recovery valuation
for this pilot. Do not value unavailable collateral at zero or seize it at a stale quote.
A pool/depeg guard limits new exposure; it cannot restore loss in an existing position.

## Effort estimate and independent work

Estimate for an engineer familiar with this repo, reusing existing source and private
Core tools. No deployment, external audit waiting, infrastructure purchase, parameter
approval or CL work is included.

|Work package|Engineer-days|Parallel ownership|
|---|---:|---|
|Cash ABI/config/proof inventory, current source review, fresh manifest|1–2|Lead; document scope above already supplies starting inventory|
|Planner/readback completion and no-strategy/no-legacy guards|2–3|One scripts agent|
|Compiled cash functional/auth matrix and needed ordinary repay correction|3–5|One receipt/controller test owner; lead reviews any accounting change|
|Complete resource/Core signature/restoration measurements|3–5|One isolated fixture agent after exact artifact freeze|
|Primary oracle/pause/issuer and liquidation/cash-shortfall rehearsal|2–3|Test/review agent; no secondary oracle|
|Keeper inventory/UI correctness, CI artifacts and release review|2–3|Keeper/docs/CI owner; lead integrates|
|**Total**|**13–21**|Approximately2–3 working weeks with effective parallelism, dependencies permitting|

Separate ordinary-failure contingency3–5 engineer-days. A proven footprint overflow,
unsupported default-source repayment authorization or stale-price recovery requirement
can require an explicit rescope; do not disguise it as a fixed estimate. No borrowing
activation or safe cap is implied by finishing these days. Supply-only launch may need
less financial functionality, but must still pass supply/withdraw/cold/auth tests and
cannot promise a lending yield while borrowing is disabled.

## Reproduction of this scoping evidence

`inventory.json` pins all code/evidence used; `resource-evidence.json` reproduces the
native row's SDK-entry arithmetic and marks complete pilot rows missing. Run:

```sh
python3 research/cl-cash-pilot-scope-20261111/verify.py
```

This is an offline evidence/source audit only, not new tests or resource qualification.
