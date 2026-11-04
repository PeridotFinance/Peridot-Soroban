# Option1 V3 lifecycle and proof map

Stage 1 is incomplete; PR #75 remains draft. This is a source-reviewed map of
existing entrypoints and isolated research overlays, not production adoption or
release approval. Borrowing preparation stays disabled, production maintenance
margin remains unset, and both resource profiles remain unchanged. No signing,
broadcasting, deployment, secrets or shared handoff00 changes are part of this
document.

The source of transition semantics is [contract.rs](../../contracts/margin-controller/src/contract.rs),
[perps.rs](../../contracts/margin-controller/src/perps.rs),
[pending_recovery.rs](../../contracts/margin-controller/src/pending_recovery.rs),
[funded_liquidation.rs](../../contracts/margin-controller/src/funded_liquidation.rs)
and the six isolated overlays listed below. Public comments are not sufficient:
`add_position_collateral_v3` requires **Open**, despite its pending-open comment;
`execute_open_position_v3` requires an existing swap execution and delegates to
activation. The actual open sequence is **begin → swap → activate**, with
`execute` an alternate activation entrypoint, not an extra pre-swap stage.

## State and evidence notation

These labels describe existing records; they introduce no persisted stage:

| Label | Actual state, debt and custody |
|---|---|
| U | `PendingOpen`, no execution; debt namespace initialized with zero balance. Margin pTokens and open-fee reservation have been removed from free margin but remain in controller custody. |
| P | `PendingOpen` with execution; debt is live after borrow/swap. Controller holds recorded position underlying; position receipt collateral has not been deposited. |
| O | `Open`; debt may be positive or zero after repayment. Controller holds position receipt pTokens for the borrower. |
| O+C | `Open` plus an unwithdrawn pending-close intent. Receipt collateral and debt remain as in O. |
| C-U | `Closing`, withdrawn but unconverted. Controller holds underlying; the pending-close record is the asset authority. The old `position.collateral_ptokens` field alone must not be counted as still-held receipts. |
| C-D | `Closing`, conversion recorded (`pending.debt_amount > 0`); controller holds recorded debt-asset proceeds and any recorded underlying remainder. Actual current debt is read from the debt vault. |
| C-R | C-D after a successful partial `finish_close`: spent proceeds are deducted in storage; residual debt and any remaining assets remain attached to the position. |
| L-S | `Liquidated`, `Started`; controller holds receipt pTokens or inherited underlying, with any recorded proceeds. Original liquidator and takeover deadline remain recorded. |
| L-D | `Liquidated`, `CollateralConverted`; controller holds recorded proceeds, with no remaining position receipts/underlying in the isolated settlement's accepted shape. |
| Removed | Position and associated pending/route/mode records are cleared and user-position index updated. Fee/reward entitlements outside those position records are separate obligations. Removal is not a blanket entitlement waiver. |

**J1–J7** below identify selected tests in the current joined proof map. A J label
means that test contains the stated transition/assertions, not that this document
certifies its completion. Completion requires the exact build's generated-source
pins, named test result, resource rows, unsigned transaction evidence and gate
reports. Setup transitions are not automatically separately qualified financial
roots. Per-profile results and all missing Stage 1 labels remain authoritative.

**B** means earlier, separately identified baseline evidence, principally
[handoff10](../../../docs/deployment-handoffs/10-cl-history-recovery-20261102.md),
[its state map](../../../docs/deployment-handoffs/evidence/cl-history-recovery-20261102/v3-state-machine.md),
[handoff14](../../../docs/deployment-handoffs/14-cl-decisions-reserve-integration-20261102.md)
and [handoff21](../../../docs/deployment-handoffs/21-cl-restoration-preparation-20261103.md).
Those historical results do not become a rerun of the joined WASM. **S** means
source-reviewed behavior without a dedicated assertion in the selected seven
tests. B/S rows must not be reported as new joined successes.

## Public position entrypoints

All owner-authorized rows require the owner at the root; nested contract
authorization remains enforced. “Permissionless” means no borrower signature at
that root, not that account authentication, dependencies, capital or fees disappear.
Deadline comparisons below use ledger timestamp, not TTL/live-until ledger.

| Entrypoint | Before → after; financial effect | Authority, deadline and recovery availability | Evidence scope |
|---|---|---|---|
| `begin_open_position_v3` | No position → U; reserve margin/open-fee pTokens and initialize zero debt. | Owner; supported route, price, leverage, borrowing and lock checks. Creates open deadline. U can cancel or expire without debt. | J1/J2/J3/J5/J6/J7 setup; B. No exhaustive registry/route boundary proof. |
| `swap_open_position_v3` | U → P; redeem reserved margin, borrow and swap atomically; record held underlying. | Owner; allowed through exact expiry, rejects after expiry and repeat execution. Failure rolls back borrow/custody. | J6 measured swap roots; P setup in J1/J2; B. |
| `activate_open_position_v3`; `execute_open_position_v3` | P → O; deposit held underlying, assign beneficiary receipt custody, reserve open fee. Debt persists. `execute` rejects without prior swap. | Owner; activation itself does not enforce the open deadline. Current health and configured borrowing/reserve checks still apply. Failed activation leaves P recoverable. | J6 activation; execute alias/failed activation are B, not new dedicated J cases. |
| `cancel_pending_open_v3` | U → Removed; refund reserved margin and uncollected open-fee shares to free margin after accruing old fee weight. | Owner; zero debt and absent execution required. No swapped-debt cancellation. | B; S for joined candidate. |
| `expire_pending_open_v3` | Expired U → Removed; same refund. | Permissionless only when `now > expires_at`; execution must be absent and debt zero. P cannot be discarded. | B; S for joined candidate. |
| `resolve_pending_open_v3` | Expired P → O using the ordinary activation implementation. | Permissionless strictly after expiry; still needs deposit, health and configured borrowing/reserve checks. Not a guaranteed recovery when those checks fail. | B; S for joined candidate. |
| `recover_pending_open_funded` | P → Removed; owner pays current full debt within maximum, then receives recorded underlying; uncollected open fee refunded after old-weight accrual. | Owner; no timeout needed. No dependency on successful activation or borrowing reserve headroom. Ordinary-token default route; CL custody requires validation build. | B, including handoff14 fee-refund regression and handoff21 owner-funded recovery. Not the independent J1/J2 ABI. |
| `recover_expired_pending_funded` **overlay** | P → Removed; distinct payer fully pays debt, receives zero-bonus bounded underlying; residual underlying and refundable open fee remain borrower-owned. | Payer auth; `now > expires_at`, fresh values, liquidation pause/lock guards. No activation or venue conversion required. | J1. |
| `liquidate_pending_open_funded` **overlay** | P → Removed with the same full-payment/zero-bonus allocation. | Distinct payer; current configured maintenance condition must be unhealthy; no expiry prerequisite. Healthy pre-expiry state rejects. Fresh values and liquidation pause/lock guards retained. | J2; not an arbitrary stale-price recovery. |
| `add_position_collateral_v3` | O → O; move existing free-margin position pTokens into locked position accounting. Debt unchanged. | Owner; positive amount, enough free margin, no pending close. | S; not a pending-open repair ABI. |
| `repay_margin_position_v3` | O/C → same status; owner repayment reduces debt, collateral remains locked. | Owner; positive amount. Existing receipt repayment caps from pre-accrual balance and can leave newly accrued residual debt. | B; no full-repayment guarantee from this ABI. |
| `repay_full_margin_position_v3` **overlay** | O/C → same status; pay post-accrual debt up to fixed authorized maximum; refund excess. | Owner; positive maximum, sufficient funds, recorded V3 position. Release is a separate authorized call. | J3/J4 exercise O and rejection/repeat paths; C implementation is S in the seven tests. |
| `release_debt_free_position_v3` | O/C → Removed; O receipts become backed free-margin claims; C assets follow close-phase release rules. | Owner; fresh interest and exactly zero debt. Free-margin release is not necessarily wallet redemption. | J3/J4 exercise O release; C release is not a dedicated J proof. |
| `close_position_v3` | O Long → Removed; atomic receipt withdrawal, venue conversion, repayment, contingent fee reservation and residual release. | Owner; no pending close. Debt must clear or whole call rolls back. Short must use its ordinary-token split close. | S/B context; J6 uses staged close, not this atomic ABI. |
| `begin_close_position_v3` | O → O+C; creates close intent/deadline, with no withdrawal or debt repayment. | Owner; no existing close. Intent can be canceled; permissionless expiry clears expired unwithdrawn intent. | Called inside J6/J7 combined preparation; standalone boundary S. |
| `withdraw_close_position_v3` | O+C → C-U; receipt redemption puts underlying in controller custody, debt persists, close deadline refreshed. | Owner; preparation not expired; no repeat withdrawal. Claim/receipt failure rolls back. | Called inside J6/J7 preparation; standalone boundary S. |
| `prepare_close_position_v3` | O → C-U by atomic begin+withdraw. | Owner; creates and immediately consumes preparation in one invocation. | J6/J7 measured roots. |
| `swap_close_position_v3` | C-U Long → C-D; all held Long underlying becomes debt-asset proceeds; debt is not yet repaid. If debt already zero, release path removes position. | Owner; unconverted and not expired; route, quote/oracle floor and debt/slippage floor checks. Refreshes close deadline. | J6/J7 converted paths; already-zero-debt branch S. |
| `swap_close_short_position_v3` | Ordinary-token C-U Short → C-D with caller-bounded input and retained underlying remainder. | Owner; Short only; same timing/debt floor controls. **CL Short is prohibited at origination and is not qualified.** | S for this CL candidate. No inference from Long tests. |
| `finish_close_position_v3` | C-D/C-R → Removed if debt clears; otherwise C-R. Pays from recorded proceeds, persists amount spent, reserves close fee only from debt-free surplus. | Permissionless; no expiry prerequisite. Does not silently discard residual debt. Owner repayment/release or eligible independent settlement remains needed. | J6 normal completion; J7 actual committed partial finish. |
| `cancel_close_position_v3`; `expire_close_position_v3` | O+C → O by clearing intent; C-U → O by redepositing held underlying and updating actual minted receipt amount. | Cancel: owner. Expire: permissionless only strictly after deadline. Both reject converted close; redeposit may itself fail dependency/resource checks. | S in joined seven; source/B map must not imply outage-independent success. |
| `replace_position_pool_v3` | O/C/L → same state; replace recorded route only, custody/debt unchanged. | Admin; old binding must be disabled and replacement validated. Not available for PendingOpen and not a healthy-route override. | S; no joined migration or replacement-venue proof. |
| `liquidate_position_v3` | Unhealthy O, or expired P via atomic activation and fresh liquidation check → Removed. Legacy venue liquidation repays proceeds and may absorb remaining bad debt. | Distinct liquidator; P requires expiry/execution and cannot use activation to bypass final liquidation health. Depends on venue path. | S/B map; different economics and path from J5 funded in-kind settlement. |
| `begin_liquidation_v3` | Unhealthy O or eligible C → L-S/L-D; debt retained. Inherits actual close underlying/proceeds when present and records takeover deadline. | Distinct liquidator; fresh risk. Healthy C allowed only for expired converted residual-debt recovery, with zero incentive. | J7 starts active and inherited-underlying liquidation; healthy residual entry branch S. |
| `swap_liquidation_v3` | L-S → L-D; redeem receipts if needed, convert held collateral, record debt proceeds. Debt remains live. | Authorized original liquidator, or another strictly after takeover deadline; updates liquidator and deadline. Route/valuation/slippage checks retained. | J7 converted fixture transition; successor swap and all failures S. |
| `finish_liquidation_v3` | L-D → Removed; repay proceeds, possibly absorb residual bad debt; incentive only from surplus, remainder to borrower margin. | Original liquidator before deadline; authorized successor after deadline. | S in joined seven. Not covered by new full-payment/no-writeoff claims. |
| `liquidate_funded_v3` **existing validation route** | Unhealthy O → Removed; external full debt payment precedes allowed receipt seizure; residual receipts become borrower free margin. | Distinct liquidator; fresh prices/rate, max repayment/min seizure, liquidation pause/lock guards. Configured active-liquidation incentive applies; not the overlay's blanket zero-bonus rule. | J5; B prior negative/outage evidence remains separately identified. |
| `settle_stalled_funded_v3` **overlay** | C-U/C-D/C-R/L-S/L-D → Removed; recorded proceeds first, then external full payment, then zero-bonus compensation and residual wallet transfers. No new partial stage or bad-debt writeoff. | Distinct payer; C with positive top-up must be expired or unhealthy; C with zero top-up must be expired. L honors original liquidator exclusivity until takeover deadline. Fresh prices required for any positive top-up; liquidation pause/lock guards always retained. | J7's six Long shapes below; remaining combinations S. |

## Supporting financial entrypoints and grouped getters

These methods do not create an additional position stage. Their role matters to
custody and fee accounting, but exercise during fixture setup does not qualify
every branch under both profiles.

| Public group | Effect and authority | Evidence boundary |
|---|---|---|
| `deposit_collateral`, `withdraw_collateral` | Owner-authorized receipt deposit/withdrawal; withdrawal checks remaining wallet receipts against locks. No direct V3 debt discharge. | Baseline custody flows; separate from permissionless recovery. |
| `transfer_spot_to_margin`, `transfer_margin_to_spot` | Owner-authorized transfer between wallet receipts and controller/free-margin custody; accrue fee weight before balance changes. Validation margin-to-spot retains CL beneficiary reassignment. | Fixture and J3 exit paths, B custody evidence; no full cross-product proof. |
| `distribute_margin_fees`, `claim_margin_fees`, `sweep_orphan_fees` | Respectively permissionless conversion/settlement of actually reserved fees, owner claim into free margin, and admin transfer of orphan accounting into recipient free margin. Position debt/status unchanged. | Handoff14 has separate fee-refund/eligibility evidence. J7 does not seed and compare nonzero prior fee reserves through every settlement. |
| `preview_open_fees_v3`, `get_position_fee_terms`, `get_undistributed_margin_fees`, `get_claimable_margin_fees`, `get_margin_fee_index` | Quotes or reads; may maintain TTL. No authorization to take collateral or release debt. | J7 inspects nonzero contingent terms and rejection preservation; not all fee paths. |
| `get_pending_perps_open`, `get_pending_perps_open_execution`, `get_pending_perps_close`, `get_perps_position`, `get_pending_liquidation`, `get_liquidation_takeover_after` | Read phase-specific records/deadlines; may renew TTL. | Used for assertions; a view does not prove an executable recovery. |
| `get_position`, `get_user_positions`, `get_position_counter`, `get_margin_balance_ptokens`, `get_margin_balance_underlying`, `locked_ptokens_in_market` | Read position/index/custody accounting, sometimes with TTL effects. Underlying quote uses receipt rate; status-specific lock view is not a recovery route. | J assertions and B; full maximal registry combinations unqualified. |
| `get_health_factor`, `preview_liquidation_v3` | Risk/quote reads can refresh interest, price caches or TTL; no financial state transition or standalone seizure permission. | Synthetic fresh-price fixtures only; no production oracle integration proof. |
| `compact_user_positions` | Permissionless bounded cleanup of user-position index; no asset or debt release. | Existing bound, not proof of every registry layout. |
| `get_admin`, `get_perps_pair_config`, `get_perps_pair_execution_config`, `get_perps_pair_exit_config` | Configuration reads, no recovery transition. | Source only for complete configuration surface. |
| `initialize`, admin setters/acceptance, pair/execution/exit/slippage settings, fee setters, market/controller/adapter changes, upgrade proposal/execution, `set_cl_share_asset` | Initialization/governance changes configuration or executable identity, not a financial exit. Existing authorization/timelocks/validation-network restrictions apply; no permission to use them is implied here. | No production configuration, upgrade or recovery-by-governance qualification. |

## Exact joined test selection

The six inputs are [atomic TTL](../cl-atomic-qualification/optimization.patch),
[storage](../cl-storage-qualification/storage.patch),
[full repayment](../cl-storage-qualification/repayment.patch),
[expired pending](pending-recovery.patch), [unhealthy pending](pending-unhealthy.patch)
and [stalled settlement](closing-recovery.patch). Their join does not change the
required profiles or adopt a new policy. [build-closing.py](build-closing.py) and
[run-closing.py](run-closing.py) bind the generated test sources and exact WASMs;
[joined-proof-map.json](joined-proof-map.json) is the machine mapping.

Every name below has prefix `test::closing_qualification::`:

| ID | Exact remaining test name | What its assertions target |
|---|---|---|
| J1 | `expired_pending::compiled_option1_expired_pending_independent_funding_outage_trade_and_rollback` | Expired P, distinct payer, actual intervening venue trade, borrowing disabled, exact maximum/refund, bounded underlying seizure/residual, both reward calls failed, missing root/nested auth and bounds/timing/replay rollback, historical rewards and late quarantine. |
| J2 | `early_pending::compiled_option1_unhealthy_pending_before_expiry_independent_recovery` | Unhealthy P before expiry; healthy rejection, insufficient cash/maximum and auth rejection, independent full funding, no waiver, replay and late quarantine. |
| J3 | `combined_regressions::compiled_combined_repayment_aged_full_max_refund_release_and_beneficiary_claims` | Aged owner full repayment, excess refund, zero-debt repeat, release, beneficiary claims/outage/quarantine and receipt exit. |
| J4 | `combined_regressions::compiled_combined_repayment_insufficient_max_unauthorized_and_closed_rollback` | Seven repayment/release rejection conditions with complete ledger/TTL rollback, followed by successful repayment/release. |
| J5 | `combined_regressions::compiled_combined_funded_liquidation_without_borrower_authorization` | Active independent funded liquidation, actual preceding Aquarius trade and historical reward conservation. |
| J6 | `compiled_atomic_qualification_swap_close_64_owners_128_changes_8_unresolved_5pct_trade` | Stressed begin/swap/activation and staged close; real claim-first boundaries, historical ownership, unresolved intervals and conservation. Finite fixture, not unlimited tick/history capacity. |
| J7 | `closing_cases::compiled_option1_closing_and_liquidated_six_states_independent_full_settlement` | Six reachable Long asset shapes, actual transitions, exact debt clearing and contribution accounting, historical reward preservation during both upstream call outages, invalid minimum rollback, nonzero-fee live-close rejection and successor timing. |

The runner explicitly excludes the inherited eight-independent-liquidations,
eight-owner-funded-pending-recoveries and eight-repay/release test functions.
Their earlier results are B evidence. They are not extra joined test successes
and are not eight simultaneous shared-ledger incidents. The separate shared
scenario and restoration companions retain their own identities/results.

## J7's six Long shapes

| Case / resource label | Real prestate | Settlement and proof boundary |
|---|---|---|
| 0 / `option1_closing_withdrawn_funded` | Actual prepare → expired C-U, held underlying, no debt proceeds. | External full funding, direct underlying compensation/residual; impossible minimum rejects with typed error 9101 and ledger/TTL rollback. |
| 1 / `option1_closing_converted_proceeds` | Actual prepare+swap → C-D with sufficient debt proceeds and a nonzero contingent close fee. | Live zero-top-up attempt rejects typed error 9102, preserving fee terms/debt/proceeds/rewards; after expiry proceeds pay debt and excess returns, with no payer seizure. The uncharged contingent fee is an explicit unadopted policy. |
| 2 / `option1_liquidated_started_funded` | Actual unhealthy O → L-S with receipt collateral. | Original liquidator fully funds debt and receives bounded receipts; residual receipts transfer to borrower wallet under this overlay. |
| 3 / `option1_liquidated_converted_proceeds` | Actual begin+swap liquidation → L-D with sufficient proceeds. | Own proceeds fully cover debt, no external contribution/seizure; surplus returns. This invokes new settlement, not legacy `finish_liquidation_v3`. |
| 4 / `option1_closing_partial_finish_residual_funded` | Actual prepare+swap; artificial long timestamp advance; actual successful partial finish → C-R with debt and no remaining collateral/proceeds. | Funder pays residual debt, seizure zero. This permanently spends voluntary loss cover; it is not profitable liquidation or reusable liquidator capital. Fresh synthetic prices are installed explicitly. |
| 5 / `option1_liquidated_inherited_underlying_successor_funded` | Actual C-U → unhealthy L-S; inherited underlying and zero receipts. | Different successor rejects before original liquidator deadline with ledger/TTL rollback, then succeeds after deadline; full external payment precedes underlying seizure. Early failure is a paired generic business-state rejection, not a signature-verification test. |

These are branches of one controlled test. The partial-finish timestamp jump is
about 30 years with only a small number of ledger ticks; it isolates accrual and
committed residual-debt semantics. It is not elapsed Core archival, realistic
latency, or keeper economics.

## Unclosed proof obligations

The separately identified **M1 companion** now closes two review gaps for its
actual converted Long Closing path on the same corrected V3 bytes. Both profiles
complete1/1 test with invocation PASS; full gates remain failed. It applies
49,962,213 raw own proceeds and33,966,075 external funds in one root, refunds
5,000,000 and clears83,928,288 debt. Insufficient maximum rolls back completely.
Nonzero500,000 previously reserved fee shares and backing survive; subsequent
public distribution/claim pays that beneficiary exactly once. See
[mixed-results.md](mixed-results.md) for source/build identities and limitations.
This companion does not turn J7 into a different test, qualify every other
mixed asset shape, or demonstrate an economically willing loss-cover payer.

- **CL Short remains prohibited.** The validation origination guard permits
  cash-funded Long with CL as base and non-CL margin. Ordinary-token Short code,
  formulas or native tests cannot qualify CL Short or authorize removing that
  guard.
- **Recovery remains conditional.** Independent funding avoids absent borrower
  authorization and obligatory venue conversion in the tested states. Fresh
  primary prices and explicit liquidation pauses can still block settlement.
  Even zero-collateral residual cover currently reads both fresh prices whenever
  external top-up is positive. No stale/cached fallback or secondary oracle is
  added. Borrowing disabled is not equivalent to liquidation paused.
- **Mixed proceeds and external funding have a scoped companion proof.** J7's converted
  cases use zero external funding, while its direct-funded cases have zero
  proceeds. Case4 pays proceeds in an earlier committed call. No selected case
  proves positive proceeds repayment and positive external contribution in the
  same new-settlement invocation. M1 separately proves it for converted Long
  Closing and insufficient-maximum rollback; delivery/reentry failures and other
  mixed asset shapes still need their own evidence.
- **Fee obligations are distinct.** Nonzero contingent-fee rejection is tested.
  Prior reserved fee entitlements/backing occupy separate storage and appear
  preserved by source inspection in J7. M1 now seeds nonzero such reserves and
  proves their exact preservation and subsequent beneficiary payment through
  its converted Closing settlement. It does not cover every route. Historical Aquarius
  reward conservation cannot substitute for that margin-fee regression. Healthy
  timeout fee waiver is a policy choice requiring review, not earned income.
- **Other joined branches remain untested.** These include live unhealthy C
  settlement versus healthy positive-top-up rejection, C full-repay/release,
  successor conversion, standalone close-intent cancellation/expiry, disabled
  route replacement, exact liquidation deadline boundary, and complete
  pause/issuer/oracle/resource-failure combinations. Baseline proof labels must
  retain their own build identity when used to describe those behaviors.
- **Preparation and re-expiry are conditional.** RestoreFootprint sizing and
  projected TTL installation in controlled snapshots, including synthetic
  re-expiry, do not establish elapsed Core expiry or native application of this
  entire financial lifecycle. Fresh discovery/re-simulation is required after
  re-expiry or changed dependencies. A separate unsigned local Core receipt
  restoration fixture, even if successful, cannot qualify all combined roots.
- **Account and operational evidence remain separate.** Narrow external test
  auth plus real nested contract authorization is not Ed25519 verification,
  signed-envelope cost, multisig/custom-wallet support, live liquidity, actual
  fee payment, concurrent throughput or a funded willing liquidator. Partial
  numerical rows and aggregate expected exit1 do not establish test completion.

All financial claim-first boundaries and historical/quarantine obligations must
remain intact. No new cap, maintenance margin, resource limit or production
stage follows from this map; missing lifecycle, transaction and release gates
stay failed.
