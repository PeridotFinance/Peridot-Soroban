# Current disposition

This file preserves the earlier baseline inventory below. That inventory's uncommitted/legacy statements are historical, not the current release decision. Accepted e3e6f73 closed the flash route; the final default receipt additionally treats absent reserve configuration as 100% cash reserve and keeps absent borrowing permission false. The three current flash tests pass, including controller pause expiry/lift and a missing-policy case. Admin reserve/fee withdrawals and strategy binding are accepted Ledger-only capabilities under the clarified user rule and must not be used during supply-only operation. Final manifests must show no bound strategy, margin controller or reward module. The final fixed source and artifact identities are in RE-AUDIT.md and medium-final-build-hashes.json.

---

# Cash-market outflow review — 2026-11-11

Source review only. Baseline commit: `227d0564e6776b47a01e9545d9093d84b6950534`.
Scope: the deployable `receipt-vault` and `simple-peridottroller` public interfaces,
including indirect underlying transfers, excluding the ordinary holder
`withdraw` payout itself. Assumed replacement-market configuration:
`get_min_cash_reserve_bps() = Some(configured_bps)`,
`get_cash_borrowing_enabled() = false`, `get_boosted_vault() = None`,
`get_margin_controller() = None`. These are required readbacks, not readbacks
performed by this review. Legacy Mainnet contracts remain retired.

**Baseline conclusion:** ordinary and margin borrowing are durably blocked, but
baseline flash borrowing is not. The baseline flash entrypoint only consults
the controller's expiring borrow pause. There is no separate durable
`set_flash_enabled`/`set_flash_paused` selector. A concurrent, uncommitted
candidate adds the shared durable cash-policy check to `flash_loan`; this report
does not treat that source change as a tested, built or deployed release.

## Public receipt routes

References below are to
[receipt contract source](../../contracts/receipt-vault/src/contract.rs);
line numbers are review-time pointers. The candidate inserts four lines in
`flash_loan`, shifting later pointers relative to the baseline.

| Public route | Underlying movement and authority | Result under the stated cash-only configuration |
|---|---|---|
| `borrow(user, amount)` (3768) | Vault to the authenticated borrower. Checks rates, account/collateral policy, controller borrow pause, liquidity/cap and cash policy before the transfer (3956). | **Blocked.** When reserve configuration is present, `check_cash_borrow` requires `CashBorrowingEnabled == true`; false rejects before any transfer or completed debt creation. This does not rely on the controller pause remaining live. |
| `borrow_for_margin(position_id, receiver, amount)` (4009) | Vault to receiver. Requires the configured margin controller's actual authorization; a different receiver also authorizes. Checks rates, controller borrow pause, liquidity/cap and the same cash policy. | **Blocked twice:** absent margin-controller binding rejects; a bound controller would still encounter the durable cash-borrowing gate. No public caller can nominate an arbitrary controller to bypass the stored binding. |
| `borrow_for_margin_to_controller(position_id, amount)` (4136) | Resolves the stored controller and delegates to `borrow_for_margin`. | **Blocked:** missing binding rejects; it does not have a second transfer implementation that omits the cash gate. |
| `flash_loan(initiator, receiver, amount, data)` (4524), **baseline** | Initiator authenticates; receipt transfers principal to callback receiver and requires original live balance plus fee to return atomically. Receiver need not separately authorize. Reentrancy guard, available liquidity and controller borrow pause apply. | **Durable-disable blocker.** Baseline does not read `CashBorrowingEnabled` or call `check_cash_borrow`. Controller pause expiry reopens this public route even with durable ordinary borrowing disabled. This is a supply-only-policy bypass, not a demonstrated unrepaid drain: callback repayment still must succeed or the whole transaction reverts. |
| `flash_loan`, **uncommitted candidate** | Adds `if minimum_cash_reserve_bps.is_some() { check_cash_borrow(...) }` after interest update and before pause/liquidity checks or transfer. | False now rejects for a configured cash market. Enabled calls must fit unencumbered managed local cash above the floor. Missing reserve configuration deliberately retains legacy behavior. Candidate requires its own tests, clean build, hash verification and review. |
| `repay_full_for_margin(position_id, payer, max_amount)` (4369) | Stored margin-controller authorization and payer authorization; receives the authorized maximum, measures actual receipt, requires enough for current debt, and refunds only `received - current_debt` to that same payer. | No controller binding means unavailable. When legitimately bound, the refund is an explicitly funded return of excess payment, not an unbacked public withdrawal. Borrow disable does not block repayment. Zero debt returns without any payment or refund. |
| `reduce_reserves(amount)` (2859) | Stored admin authenticates; amount must not exceed `TotalReserves`; underlying goes only to that admin and managed cash is reduced by measured outflow. | **Admin-authorized outflow remains available.** No cash-borrowing flag or minimum-reserve check. A fresh market initializes the reserve ledger to zero, so a positive withdrawal needs recorded reserves. Token transfer failure rolls the transaction back. This is not an unauthenticated bypass. |
| `reduce_admin_fees(amount)` (2895) | Stored admin authenticates; amount is bounded by `TotalAdminFees`; same admin destination and measured managed-cash reduction. | **Admin-authorized outflow remains available.** Same distinction as reserve withdrawal. Fresh fees start at zero. The borrowing-disabled promise does not revoke this admin authority. |
| `rebalance_idle_cash(admin)` (1497) | Authenticated stored admin can send excess cash to the configured boosted strategy through `deposit_into_boosted`. | With no boosted binding, `deposit_into_boosted` returns without transferring. If admin later binds a strategy, borrowing-disabled **does not independently disable strategy investment**: reserve/idle-target floors constrain investment, but the helper does not read the borrowing-enabled flag. Keep the no-strategy readback separate from the borrow readback. |
| `deposit(user, amount)` (1644), indirect investment only | User supplies underlying. The deposit later calls `deposit_into_boosted` for eligible cash (1752). | No strategy binding means no outgoing strategy transfer. A future admin-authorized binding would allow this public deposit to trigger investment under that configured policy, even while borrowing is disabled. The public user does not choose the strategy destination. |
| Successful `flash_loan`, post-callback investment | Calls `deposit_excess_idle_cash` after repayment. | No strategy binding means no investment. The baseline flash bypass still matters without a strategy; a strategy is not necessary for the temporary principal outflow. |
| `prepare_liquidity(amount)` (1477) | Stored admin authenticates. Pulls cash **into** the receipt from an existing strategy as needed; it does not pay the caller. | No strategy binding creates no alternative outward path. This is not a public sweep. |

The remaining monetary receipt selectors do not introduce another underlying
outflow outside holder withdrawal: `repay`, `repay_max`, `repay_for_margin` and
`repay_on_behalf` receive debt repayment; `transfer`/`transfer_from` move receipt
pTokens, not vault underlying; `seize` moves pTokens to a liquidator/fee recipient
under stored controller authorization and an amount/shortfall/expiry-bound
context. A subsequent redemption is a separate holder-withdrawal operation.
`absorb_margin_bad_debt` is controller-authorized debt/reserve accounting, not an
underlying transfer. `init_margin_borrow_state`, `begin_margin_withdraw` and CL
custody/reward-context setters are controller-gated state preparation, not
standalone cash payouts. In particular, `begin_margin_withdraw` binds recipient,
maximum pTokens and ledger sequence and cannot be used without the configured
controller's authorization.

No generic `sweep`, `rescue_tokens`, arbitrary-token withdrawal or arbitrary
recipient cash-transfer selector was found in either reviewed deployable ABI.
Native `reward_lending`, `reward_backing` and related modules are explicitly
outside `contractimpl`; `hybrid-rewards` cannot compile to deployable WASM. Their
Rust `pub fn` declarations are not additional receipt contract selectors.

## Controller routes and configuration boundaries

[Controller source](../../contracts/simple-peridottroller/src/contract.rs)
`liquidate` (2600) pulls repayment from the liquidator through the debt market
and seizes collateral **pTokens** through the collateral market. It does not
withdraw collateral underlying. It requires supported, distinct markets,
liquidator authorization, liquidation permission, borrower membership,
determinate account shortfall, current debt/pricing and bounded seizure.
`liquidate_for_margin` (2624) additionally requires an allowlisted controller's
authorization; it has no usable margin route in a fresh group with no such
allowlist/bindings. `repay_on_behalf_for_liquidator` (2950) only arranges inbound
repayment, with account shortfall and close-factor checks. Receipt `seize` still
requires the stored controller's authorization.

`claim` (3026), `claim_self` (4112) and permissionless `claim_all` (4100) can move
the configured reward token from the **controller's own prefunded balance** to
the recorded beneficiary, capped by accrued entitlement and available balance.
They do not authorize spending a receipt's underlying balance. `claim_all` is
an intentional public trigger of already-accrued payouts; it cannot nominate a
different destination for an owner's entitlement. An admin can configure the
reward token to an asset also used as market underlying, but any resulting
payout still comes from the separately funded controller balance.

`set_boosted_vault`, `set_margin_controller`, `set_peridottroller`,
`set_cash_borrowing_enabled`, reserve/fee/rate setters, controller market/risk
configuration and admin/upgrade selectors are **retained governance authority**.
They can change the assumptions of this review through authorized transactions.
They are not public methods that silently bypass an unchanged disabled policy.
Receipt upgrades require staged hash/timelock/admin authorization and, when
controller-bound, deposit/redeem/borrow pauses. The retained upgrade authority
means no current policy should be represented as immutable.

## What makes the borrowing gate durable

`MinCashReserveBps` and `CashBorrowingEnabled` are instance-storage configuration
([storage definitions](../../contracts/receipt-vault/src/storage.rs), 57–60).
The reserve setter is admin-authenticated and bounds the value to 0–10,000.
There is no public unset-reserve selector: even `Some(0)` selects cash policy.
First configuration leaves the absent borrowing-enabled flag false. The
enabled setter requires reserve configuration and admin authorization. The
guard has no wall-clock expiry; archived-instance restoration is not a new
authorization to enable borrowing.

Both readbacks are essential. `get_cash_borrowing_enabled() == false` by itself
is insufficient when `get_min_cash_reserve_bps() == None`, because the legacy
borrow paths only invoke the policy when the reserve option is present.
`get_borrowable_cash` and `assert_borrowing_reserve` report/check reserve
arithmetic; they do not themselves prove that borrowing is enabled or disabled.

The controller's `set_pause_borrow` and guardian variant use `set_pause_state`
(1314), which stores current timestamp plus
[`MAX_PAUSE_DURATION_SECS = 72 * 60 * 60`](../../contracts/simple-peridottroller/src/constants.rs).
`is_pause_active` (1401) removes expired flags and returns false after expiry.
These pauses are useful additional controls, not durable supply-only switches.
`FlashLoanActive` is a per-call reentrancy guard, not an administrative pause.
`set_flash_loan_fee` changes price only; even the maximum allowed fee does not
make the entrypoint uncallable. Borrow cap zero means **cap disabled**, not
borrowing disabled; the baseline flash path does not use that cap anyway.
`RatesReady == false` blocks ordinary/margin borrowing but is not the baseline
flash-loan gate.

## Remaining blockers and evidence boundary

1. **Published baseline must not be described as durably supply-only.** The
   flash-loan gap remains in `227d0564`; the concurrent source candidate is the
   specific correction under review. It requires a configured, funded receipt
   test after controller-pause expiry, exact failure/rollback assertions, and
   normal deposit/withdrawal availability. No new regression or resource run
   was executed by this read-only reviewer.
2. **Fresh deployment configuration must be verified per market:** reserve
   option present, borrowing false, no boosted strategy, no margin controller,
   fresh controller identity and no margin-liquidation allowlist. No legacy
   address may be reused to satisfy those checks. Missing configuration is not
   equivalent to a disabled configured market.
3. **Cash-only is a configuration promise with retained admin powers.** Admin
   reserve/fee payouts and future rebinding/upgrades remain authorized routes.
   A stronger claim that all outflows except holder withdrawals are impossible
   is not established by these controls and should not be made.
4. **Review scope is source and ABI classification.** Existing ordinary/margin
   reserve tests (`reserve_ordinary_borrow_and_durable_pause`,
   `reserve_margin_uses_same_floor_and_controller_auth`) do not by themselves
   prove the new flash behavior. Existing flash repayment tests prove a
   different invariant. Clean release builds, candidate-specific regressions,
   resource gates and the requested lending-only security scan remain separate
   evidence gates; no scan pass or deployment approval is claimed here.

No code, production parameters, keys, secrets or network state were changed by
this review. No Mainnet transaction, signing, broadcasting, build or test run
was performed. Only this review document was written.
