# Handoff 02: Stellar isolated market group XLM + USDC routing into Upshift vaults, with leveraged margin

Audience: the implementation LLM working in `PeridotFinance/Peridot-Soroban` (`peridot-contracts/`).
Written: 2026-10-30.

Read `00-shared-risk-model-and-rollout.md` first; it overrides this file where they differ. Per its rollout order, this group **launches holding cash locally**, as the funding (borrow) group for handoff 07's leveraged LP. The Upshift allocation is built here but stays **disabled** until sections 2a to 2c are complete and tested.

Start from branch **`leveraged-fix`** (newest, 2026-10-12: margin keeper recovery hardening). Read first:

1. `peridot-contracts/CLAUDE.md` (build rules, footprint rules, FIND-039 cross-contract safety, rollout invariants).
2. `peridot-contracts/addresses.md` (mainnet IDs, current settings snapshot).
3. `contracts/receipt-vault/src/contract.rs`: the boosted-vault seam (`set_boosted_vault`, `deposit_into_boosted`, `get_boosted_underlying`, the redemption path around `boosted_underlying_redemption_baseline`).
4. `contracts/aquarius-lp-vault/`: the reference for a **non-DeFindex strategy that implements the boosted ABI** and attaches through the unmodified `set_boosted_vault`, then binds back with `set_receipt_vault`. Copy its shape.
5. `contracts/margin-controller/` and `contracts/swap-adapter/`, plus `margin.md`, `bots/margin-liquidator/`.

If the HackMeridian market-group factory (branch or sister repo `hackmeridian-2026`, per the operator's notes) is available to you, implement the Upshift strategy as an attachable vault route there as well. It was not visible on the public branches of `Peridot-Soroban` when this was written; ask the operator for the path rather than guessing.

## 0. What the operator asked for

A new **isolated market group** on Stellar mainnet with two markets, **XLM** and **USDC**. Idle supply in each market routes into the matching **Upshift** vault. **Leveraged margin** on the group.

Isolated means: its own `SimplePeridottroller`, its own two `ReceiptVault`s, its own rate models, its own margin controller. No link to the core group (`CCVUFGXK...`) or the LP pilot group.

## 1. Upshift on Stellar: facts that drive the design

Verified from source (`github.com/upshift-protocol/stellar-upshift-vault-contracts`, `contracts/august-vault/src/contract.rs`), its `VERIFY.md`, Upshift docs and stellar.expert; numbers are from the Upshift app on 2026-10-30 and move daily.

| | earnUSDC (Gami) | earnXLM (Gami) |
| --- | --- | --- |
| Vault contract (also the SEP-41 share token) | `CCL3WITWFFXIHV2I52ECV5DPIEOFSTU3PBPR53ILPLF2IP5KHECXRUTY` | `CC6TRAPQD3NK7THUKWPV5SL2JHKQGNXZVB6S6MVYFSLRWAKEFUWZKZ7J` |
| Underlying (confirm with `query_asset()`) | USDC SAC `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75` | XLM SAC `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA` |
| WASM hash (both) | `4b3d9f6b09f7127b0ce81b0ce9d8428f3f960e4e3ce13c11c1cbb446cccc9e73`, reproducible from commit `33a02f4` | same |
| TVL / APY (app claim) | ~$27.3M / ~10% 30d | ~$3.3M / ~5% 7d |
| Allocation (app claim) | Blend ~$10.8M, Templar on NEAR ~$7.7M, idle ~$7.3M, Spectra ~$1.5M | Templar on NEAR ~$1.39M, **earnUSDC ~$1.18M**, idle ~$0.76M |
| Curator | Gami Labs (co-curator StakeCapital), Fordefi MPC custody | same |

Interface (ERC-4626 style on OpenZeppelin `FungibleVault`):

- `deposit(assets, receiver, from, operator) -> shares`, `mint(shares, receiver, from, operator) -> assets`
- `withdraw(assets, receiver, owner, operator) -> shares`, `redeem(shares, receiver, owner, operator) -> assets`
- `operator.require_auth()`; a contract can pass itself as `from` / `owner` / `operator`.
- Views: `total_assets`, `total_supply`, `convert_to_assets`, `convert_to_shares`, `preview_*`, `max_withdraw`, `max_redeem`, `query_asset`, `is_paused`, `get_admin`, `get_operator`, `get_deployed_assets`, `get_strategy_balances`, `get_wallet_deployed_assets`.
- Share decimals = asset decimals + `decimals_offset` (default 6, so 13 for both).

The five properties that matter:

1. **Instant redeem, but only up to the vault's local balance.** `max_withdraw = min(entitled, local_balance)`. Anything larger reverts (`VaultExceededMaxWithdraw` / `VaultExceededMaxRedeem`). Larger exits wait for the curator to pull funds back; the app's "2-day withdrawal period" is an off-chain service level, not contract logic. Local idle was roughly 27% (USDC) and 23% (XLM) of TVL at reading.
2. **NAV is partly operator-reported.** `total_assets = local balance + Σ strategy.get_balance() + deployed_assets`, and `deployed_assets` is set by the operator (Halborn medium 7.1, only partially fixed, residual risk accepted). Bounded by admin-set per-update and windowed bps limits that the admin can widen.
3. **Share price can fall** (operator lowers `deployed_assets`, about 5% per update by default, or a strategy loses). If a strategy contract reverts, `total_assets` and withdrawals freeze (error #21) until the admin removes it.
4. **Admin can pause and upgrade with no timelock.** Pause makes `max_*` return 0 and blocks deposit/withdraw.
5. **earnXLM is not pure XLM exposure.** It holds earnUSDC and cross-chain NEAR positions. XLM suppliers in our market would carry USDC and NEAR curator risk.
6. **Upshift does not remove Blend dependence.** About $10.8M of earnUSDC's ~$27M sat in Blend at reading. If Blend's USDC yield fades, Upshift's yield falls unless the curator reallocates. A Spectra listing also quoted lower base APYs (3.85% USDC, 2.11% XLM) than the app headline, so the 5% to 10% likely includes incentives. Model Upshift yield as a range, not a fixed number.

Fees: none in the contract; the API reports 0 management and 0 performance fee. Audit: Halborn, 2026-04 (0 critical, 0 high, 2 medium, 5 low); the batch wallet reconciliation added later was not audited. No lending protocol on Stellar uses these vaults as a strategy today as far as we found; Upshift is itself a supplier into Blend.

**Consequence:** this is an admin-trust, partially illiquid strategy. That is acceptable for a capped isolated group if the market keeps enough local cash and the keeper can pull out fast, and it is the reason margin needs extra care (section 5).

## 2. Architecture

```
SimplePeridottroller (new, isolated)
 ├── ReceiptVault XLM  ── boosted ──> UpshiftStrategy (XLM) ──> earnXLM
 ├── ReceiptVault USDC ── boosted ──> UpshiftStrategy (USDC) ──> earnUSDC
 ├── JumpRateModel volatile (XLM) / stable (USDC)  (fresh instances, same code)
 └── Reflector oracle CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN
MarginController (new, bound to this controller only)
 └── SwapAdapter (new instance) ──> XLM/USDC venue (Aquarius first)
```

Reuse `receipt-vault`, `simple-peridottroller`, `jump-rate-model`, `margin-controller`, `swap-adapter`. New code is one new crate plus, where an adapter cannot enforce a protection, **small reviewed changes** to `receipt-vault` and `simple-peridottroller` (shared risk model section 11). Expected changes: the collateral-validity flag in the receipt vault's account snapshot and its propagation into the controller's account valuation (2a), and the cash reserve plus allocator rule (2c).

### 2a. New crate `contracts/upshift-strategy`

A thin adapter that holds Upshift shares and exposes the boosted-vault ABI the receipt vault already speaks (copy signatures exactly from the `receipt-vault` test mock around `test.rs:425-530` and from `aquarius-lp-vault`):

| Boosted ABI (called by ReceiptVault) | Implementation |
| --- | --- |
| `balance(id) -> i128`, `total_supply() -> i128`, `decimals`, `name`, `symbol` | The strategy is its own SEP-41 share token (wrapper shares). Only the bound receipt vault may hold shares; `transfer` restricted or disabled |
| `get_asset_amounts_per_shares(shares) -> Vec<i128>` | **Must return `[0]` for `shares == 0` before any external call** (the receipt vault probes the asset-vector shape this way). Otherwise `upshift.convert_to_assets(upshift_shares_held * shares / total_supply)`. Never panic in this view (FIND-039). On Upshift pause, revert or breaker trip, return the last good **accounting** value, but see "Three quantities" below: that value must not authorize borrowing |
| `deposit(amounts_desired, amounts_min, from, invest) -> ...` | `from` must be the bound receipt vault, `from.require_auth()`. Pull `amounts_desired[0]` (the receipt vault pre-authorizes exactly that `transfer`), call `upshift.deposit(assets, self, self, self)`, mint wrapper shares pro-rata to Upshift shares received. Enforce `max_deploy` and the idle-share cap (2c). Reject if Upshift paused, WASM hash changed, or `query_asset()` mismatches |
| `withdraw(shares, min_amounts_out, to) -> i128` | `to` must be the bound receipt vault. Compute the Upshift shares to redeem, check `upshift.max_redeem(self)`; if insufficient, **panic with a named error** so the receipt vault reports its standard "withdraw liquidity shortfall" rather than accepting less. Call `redeem(..., receiver=to, owner=self, operator=self)`, assert received ≥ `min_amounts_out[0]` by balance delta |
| `set_receipt_vault(admin, receipt_vault)` | One-time binding, mirrors `aquarius-lp-vault` |

Admin and keeper functions: `set_max_deploy`, `set_idle_share_cap_bps`, `set_expected_wasm_hash`, `set_paused`, `recall(amount)` (keeper or admin pulls Upshift back to the receipt vault, bounded by `max_withdraw`), `propose_upgrade_wasm` / `upgrade_wasm` with the same delay pattern as other Peridot contracts.

Units: Upshift shares have 13 decimals, the wrapper should keep the asset's 7 decimals and track Upshift shares internally as `i128`. Fuzz the conversion both ways, including rounding against the depositor.

**Three quantities, kept separate** (shared risk model section 6):

| Quantity | Source | Used for |
| --- | --- | --- |
| Accounting value + timestamp | `convert_to_assets` on success; last good value otherwise | Exchange-rate display, reporting |
| `valuation_state()` | New strategy view returning one of `Valid`, `DepositsPaused` (breaker tripped on configuration, but the last successful read is fresh and the share price did not fall beyond the allowed bps), `Untrusted` (read failed beyond the max age, Upshift paused, share price drop beyond the allowed bps, or strategy-revert freeze) | Read by the receipt vault and propagated to the controller (below) |
| `withdrawable()` | `min(our_entitlement, upshift.max_withdraw(self))` | Sizing redemptions and the keeper's recall |

Emitting an event is not enough: the controller does not react to events. Enforcement (shared risk model section 6):

- The receipt vault's account snapshot (`get_account_snapshot`, kept on its narrow footprint path) carries a **validity flag** derived from `valuation_state()`.
- The controller's account-liquidity calculation **reverts** for any risk-increasing action when **any** of the account's entered collateral snapshots is `Untrusted`: borrow from **any** market in the group, redeem/withdraw collateral, pToken transfer, `exit_market`, margin opens.
- `DepositsPaused` does not block anything except new strategy deposits; collateral prices remain trustworthy.
- Repay always works.
- **Liquidation:** with `Valid` or `DepositsPaused`, normal. With `Untrusted`, ordinary seizure of the affected market's pTokens is disabled; the recovery policy is a timelocked **recovery exchange rate** (never above the last trustworthy value, minus a haircut set by the timelock) under which liquidation resumes, or waiting for a trustworthy read. Liquidations seizing other, valid collateral of the same account continue.
- **Confirmed losses always mark down:** if `convert_to_assets` returns a lower value through a successful read, the exchange rate reflects it immediately, even if that drop also trips the breaker. The existing receipt-vault rule "never replace a real loss with a higher accounting value" stays.

Required cross-market test: supply USDC (Upshift-backed) as collateral → make its valuation `Untrusted` → attempt to borrow **XLM** from the other market, transfer the USDC pTokens, `exit_market` USDC, open margin: all revert. Repay XLM debt: succeeds. Liquidation seizing USDC pTokens: reverts until the recovery rate is set; afterwards succeeds at that rate.

### 2b. Upstream-change circuit breaker

The keeper polls every minute and the strategy checks on deposit:

- WASM hash of the Upshift vault equals the pinned `4b3d9f6b...` (read the contract instance's executable hash).
- `is_paused() == false`, `get_admin()` and `get_operator()` equal pinned values.
- `convert_to_assets(1e13)` has not dropped more than a configured bps since the last observation.

**Two policies, not one blanket guard:**

- **Deposit policy:** a configuration mismatch (WASM hash, admin, operator) blocks new deposits into Upshift and sets `valuation_state` to `DepositsPaused` while reads stay fresh and the share price holds; a pause, failed read or share-price drop sets `Untrusted` (2a).
- **Recovery policy:** `recall` and `withdraw` toward the receipt vault are **always attempted**, even when the breaker is tripped. They still enforce the balance-delta check and `min_amounts_out`; they never require the pinned configuration to match. A breaker that blocks its own recovery path is a bug.

Recall is best effort, not an assured exit: Upshift's pause blocks withdrawals too, and redemptions are capped by its local balance. Moving the boosted binding to `DISABLED` stays an admin action.

### 2c. Liquidity sizing: target cash vs minimum reserve

The real exit limit is Upshift's **local balance**, not our share balance. Two caps, enforced in the strategy on deposit:

- `max_deploy`: absolute cap per market, start small (operator sets; propose 25k USDC and 100k XLM).
- `idle_share_cap_bps`: our position may not exceed this share of Upshift's current local balance (propose 2,000 bps = 20%). If Upshift's idle falls, we stop adding; the keeper recalls down toward the cap.

In the receipt vault, **two separate parameters** (an earlier draft used one 40% number for both, which left no borrowing capacity):

| Parameter | Definition | Proposal | Effect |
| --- | --- | --- | --- |
| `target_cash_bps` | Allocation target: deploy to Upshift only the cash above `target_cash_bps × (cash + strategy_value)`, i.e. as a share of **unborrowed** managed assets | 40% | Decides how much goes to Upshift. Outstanding borrows are not counted as deployable |
| `min_cash_reserve_bps` | Hard floor for borrowing: a borrow (or margin open) must leave local cash ≥ `min_cash_reserve_bps × total_supplied_underlying` | 10% | Decides when borrowing stops |

Definitions:

- `supplied_underlying` = local cash + strategy value + total borrows − total reserves − admin fees (the underlying owed to suppliers, as in the exchange rate). If the strategy valuation is not `Valid`, use **the lower of the cached value and the book estimate** for this sum.
- `required_cash = max( ceil(target_cash_bps × (cash + strategy_value)), ceil(min_cash_reserve_bps × supplied_underlying) )`, both rounded **up** in raw units.
- The **allocator** may deploy only `cash − required_cash` (if positive). It must never invest below the borrowing reserve, even when the target term is smaller.
- A **borrow** (or margin open) must leave `cash ≥ ceil(min_cash_reserve_bps × supplied_underlying)` after the transfer.
- When the strategy valuation is not `Valid`: **no deployments**; recalls and withdrawals toward the market are still attempted (2b recovery policy).

Worked examples:

| Supplied | Borrowed | Cash | Strategy | Target term (40% of cash+strategy) | Reserve term (10% of supplied) | Required cash | Allocator may deploy | Borrowable now |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 100,000 | 0 | 40,000 | 60,000 | 40,000 | 10,000 | 40,000 | 0 | 30,000 |
| 100 | 80 | 10 | 10 | 8 | 10 | 10 | **0** (deploying 2 would breach the reserve) | 0 |
| 100 | 80 | 14 | 6 | 8 | 10 | 10 | 4 | 4 |

As borrows grow, the keeper recalls from Upshift toward the target over time. Borrows never wait on an Upshift redemption; when local cash hits the reserve, borrowing stops until the keeper refills it or rates pull deposits in.

This is new receipt-vault logic (small reviewed change). Set the existing idle-cash buffer equal to `target_cash_bps` so there is one allocation knob, and add the three worked examples as tests.

## 3. Group configuration (proposals; the operator decides)

| Setting | XLM | USDC |
| --- | --- | --- |
| Collateral factor (ordinary lending only) | 60% | 66.7%. These bound **ordinary** borrowing and manual looping (2.5x / 3x at zero headroom). Margin V3 leverage is separate (section 5; shared risk model section 5b) |
| Rate model | volatile curve (copy core params) | stable curve (copy core params) |
| Reserve factor | 10% | 10% |
| `target_cash_bps` / `min_cash_reserve_bps` | 40% / 10% | 40% / 10% |
| Supply cap | operator | operator |
| Borrow cap | **set a positive cap**; 0 means uncapped in this codebase, not disabled | same |
| Liquidation close factor / incentive | 50% / 1.08 | same |
| Oracle | Reflector `CAFJZQ...`, staleness k=2 default | same |

Note the CF caveat in `addresses.md`: the receipt-local `CollateralFactorScaled` is separate from the controller's `get_market_cf`. Set and verify both.

Interest on the Upshift portion is not borrower interest; it reaches suppliers through the boosted NAV in the exchange rate. Make sure the frontend shows supply APY as borrow-interest share plus Upshift yield on the deployed fraction, and labels the Upshift part as variable and curator-dependent.

## 4. Deployment script

Two scripts, matching the cash-only launch:

1. `scripts/deploy_funding_group_mainnet.sh` (launch): build with `stellar contract build` only, upload WASMs, deploy controller, rate models and two receipt vaults; wire vault → controller, add markets, set CFs, caps, `target_cash_bps`, `min_cash_reserve_bps`. **No strategy is deployed or bound**: both markets hold cash locally. Modeled on `deploy_aquarius_lp_controller_mainnet.sh` + `deploy_aquarius_lp_market_mainnet.sh`.
2. `scripts/bind_upshift_strategy_mainnet.sh` (later, separately approved): deploy the strategies paused, `set_boosted_vault` then `set_receipt_vault`, verify every binding in both directions, set a tiny `max_deploy`, and leave the strategies paused until the operator flips them.

`verify_funding_group_mainnet.sh` reads back everything, with a flag for the post-binding state. Record IDs in `addresses.md` only after mainnet verification.

## 5. Leveraged margin on the group

Use the existing `margin-controller` (V3 perps entrypoints only; legacy V1 is disabled) as a **new instance** bound to this controller (`set_peridottroller` + `execute_peridottroller_update`, `set_market(XLM, vaultXLM)`, `set_market(USDC, vaultUSDC)`), with its own `swap-adapter` instance.

- **Swap venue.** Start with the deepest XLM/USDC Aquarius pool, discovered with the router's `get_pools(tokens)` (the Aquarius API listing is incomplete per `addresses.md`). Register it with `set_pool_allowed` / `set_pool_binding`. Soroswap via `swap_exact_tokens_for_tokens` is the second route. Sushi or Curve can be added later (handoffs 03 and 05) without contract changes only if they fit the adapter's existing call shapes; otherwise they need an adapter extension and their own review.
- **Leverage (margin V3 semantics, shared risk model section 5b).** V3 borrows use `borrow_for_margin`, which skips the ordinary account-CF calculation; position health uses equity and the pair's maintenance margin, so the CF only gates market eligibility (`get_market_cf > 0`) and ordinary users. Leverage is an **integer** (`margin_value × leverage`, no divisor): start at **2x** with a maintenance margin set from the liquidation-capacity analysis, or build the fractional-leverage change (handoff 07 section 3d) first if 1.5x is wanted. Code cap is 10x (`MAX_LEVERAGE_CAP`), testnet runs 5x. Step up only after stressed liquidation rehearsals pass. The section 3 CFs separately bound ordinary looping. Margin was deliberately deferred elsewhere because of its security surface; keep it behind its own activation batch after the supply-only group has run.
- **Audit history to re-check before activation** (our own past findings): a broken leverage gate in `open_position_no_swap_v2` that allowed insolvent-from-inception positions, a liquidation bonus that came out as 8%/CF instead of 8%, and a missing voluntary close for underwater swap positions. Prove with tests that the code on `leveraged-fix` you deploy has all three fixed.
- **Liquidity interaction with Upshift.** A margin open borrows from the receipt vault. Opens must leave local cash above `min_cash_reserve_bps` (2c), so they never depend on an Upshift redemption.
- **Liquidations and closes must be proven, not assumed.** A close or liquidation can need to **redeem a supplied position asset** (pTokens of XLM or USDC held by the margin account) before swapping into the debt asset. If that market's local cash is below the redemption, the receipt vault would try to pull from Upshift, which can fail. Required tests, with the Upshift mock paying **nothing**: voluntary close of long and short, partial and full liquidation, and repay-to-exit. Each must succeed from local cash alone, or use the **funded in-kind route** (shared risk model section 10): the liquidator supplies the debt asset, repayment and seizure of pTokens settle atomically, the liquidator bears the exit risk; no flash loan on this route. Name the funded liquidator, its debt-asset budget and the route's capacity. Document which path each case uses.
- **Liquidator bot.** Run `bots/margin-liquidator` against the new controller id, separate config, separate key with no admin rights.
- **Caps.** Per-position and aggregate open interest from the liquidation-capacity simulation (shared risk model section 4): correlated liquidations, collateral withdrawn first, stressed third-party liquidity.

## 6. Tests and gates

| Gate | Evidence |
| --- | --- |
| Unit | Strategy against a faithful Upshift mock implementing the real function names, `max_withdraw` limited by local balance, pause, NAV down-move, strategy-revert freeze (#21), WASM-hash change; recall still attempted with the breaker tripped; each `valuation_state` maps to the behavior table in 2a |
| Account-wide validity | The cross-market test in 2a; liquidation under `Untrusted` reverts until the recovery rate is set; confirmed loss marks down while the breaker trips |
| Cash parameters | All three worked examples in 2c; allocator never deploys below the reserve; borrowing stops at the reserve without calling Upshift; no deployments while valuation is not `Valid` |
| Integration (`receipt-vault` + strategy) | Deposit leaves exactly the idle target; withdraw within buffer never calls Upshift; withdraw beyond buffer redeems; withdraw beyond Upshift idle fails atomically with the standard shortfall and changes nothing; NAV loss marks the exchange rate down; zero-share probe returns `[0]` |
| Footprint | `scripts/check_budget.sh`: deposit, withdraw-with-redeem, borrow, liquidation, margin open/close all fit Soroban limits, measured on a mainnet-state simulation |
| Margin | Open/close long and short, liquidation at the boundary, the three historical bugs above as regression tests |
| Mainnet | Upload + deploy + verify script; group live with **cash held locally** and strategies unbound; operator signs. Upshift binding only after every row above is green, with tiny `max_deploy`; keeper monitor-only for 24h before `recall` is enabled |

## 7. Open operator decisions

1. Accept Upshift admin-trust (operator-reported NAV, no-timelock upgrade/pause) and the earnXLM cross-asset exposure for XLM suppliers, or use earnUSDC only and leave XLM idle or in Blend.
2. `max_deploy`, idle share cap, idle buffer, supply and borrow caps.
3. CFs and leverage at margin activation.
4. Whether a small part of Upshift's yield goes to reserves.
5. Whether to ask Upshift/Gami for a dedicated sub-vault or a guaranteed withdrawal allowance for Peridot (worth a BD conversation; it removes the main liquidity risk).
6. If Gami or Sentora later **supply** a Peridot market, that market must **not** route into Upshift (no circular allocation: it double-counts NAV and couples both sides' withdrawals). One direction per market.

Sources: Upshift contract source and `VERIFY.md` (github.com/upshift-protocol/stellar-upshift-vault-contracts); docs.upshift.finance (stellar vaults, curator model, risk and security model, audits incl. the Halborn 2026-04 report); app.upshift.finance vault pages for gami-earnusdc and gami-earnxlm; stellar.expert contract pages for both vaults.
