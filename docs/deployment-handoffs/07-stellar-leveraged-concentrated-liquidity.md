# Handoff 07: Leveraged concentrated liquidity on Stellar (Aquarius CL)

Audience: the implementation LLM working in `PeridotFinance/Peridot-Soroban` (`peridot-contracts/`).
Written: 2026-10-30 (revised after design review). **Priority 1 on Stellar.** Read `00-shared-risk-model-and-rollout.md` first (it overrides this file where they differ), then handoff 04 for Aquarius mechanics and handoff 02 for the funding group.

## 0. What the operator wants and the staged plan

Users hold keeper-managed Aquarius CL positions through Peridot, first at 1x, then with leverage borrowed from Peridot markets. Lenders earn the borrow interest. This creates borrow demand on Stellar and a yield source for suppliers.

Stages:

1. **Transferable share vault, 1x pilot on PYUSD/USDC** (provisional choice: it reuses the existing Peridot PYUSD/USDC strategy pool and price-router groundwork, so less new integration lands at once).
2. **Funding group** (handoff 02) live with **cash held locally**; Upshift allocation is optional and disabled at this stage.
3. **Comparison gate, then share collateral market + capped leveraged LP through margin V3 at 2x integer leverage.** Activate only after reward accounting (shared model section 8a), the share execution facade (3d), post-cost position health and funded liquidation capacity pass their tests. If 2x fails, stay at 1x. Fractional leverage is deferred. The gate (section 1) decides whether PYUSD/USDC or USDC/sUSD becomes the leveraged product.
4. Optional capped Upshift allocation in the funding group (handoff 02 protections complete).
5. Second stable pair, then XLM/yXLM.

**Funding and withdrawals:** apply 00 section 6a and 02 section 2c to the funding markets. Their local cash above the borrowing reserve funds actual loans; lender redemptions consume cash first and attempt an Upshift recall only after that optional strategy is enabled and only within its synchronous exit capacity. The 1x CL share vault is a separate product: its LP capital is not simultaneously available as funding-market cash, and it does not inherit a 50/40/10 lending allocation. Share redemption follows the vault/facade and in-kind recovery rules below. The harness must distinguish a lending-market liquidity shortfall from a share-vault unwind failure and exercise recovery with the funding strategy paying zero. Keep stage 1 at 1x and all margin activation gates intact.

## 1. Candidates

The rule from the shared model applies: concentration and leverage multiply the net; they do not create it. LVR grows with the volatility of the price ratio between the two tokens, so stable pairs come first.

| Pool (Aquarius CL) | Id | TVL | Fee APY | Base reward APY | Notes |
| --- | --- | --- | --- | --- | --- |
| USDC/sUSD 0.1% | `CBV6LZAB...WGWQ` (resolve via `router.get_pools`) | ~$213k | ~6.0% | ~7.6% | Highest fee flow; sUSD issuer and depeg risk; needs a reliable sUSD price |
| PYUSD/USDC 0.1% | `CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX` | ~$276k | ~0% | ~8.3% | Reward-only, AQUA price risk; existing Peridot strategy pool and price-router setup for PYUSD |
| XLM/yXLM 0.1% | `CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F` | ~$313k | ~0% | ~4.8% | Stage 5 |
| XLM/USDC 0.1% | `CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV` | ~$1.24M | ~7.9% | ~3.7% | No leverage; 1x only after handoff 04's backtest |
| USDC/EURC, AQUA pairs | see 04 | | | | No leverage |

Numbers from `amm-api.aqua.network` (2026-10-30, undocumented volume window); reproduce them. Rewards are base rewards (a contract cannot hold ICE, so the boost is 1x) paid in AQUA. Rough pot per year today: USDC/sUSD ~$29k, PYUSD/USDC ~$23k, XLM/yXLM ~$15k. Capacity is the binding constraint; the profitable-capacity sentence (shared model section 2) sets the caps.

**Comparison gate before any collateral activation (stage 3):** for PYUSD/USDC and USDC/sUSD, document:

- **Exact token identities**: contract ids, issuers, SAC vs custom token, admin/clawback/freeze flags.
- **Live depeg detection**: which price-router source prices each stable, its bounds, and a test that a real depeg (simulated on the observed source) lowers collateral value and does not get clamped back to $1. Existing price-router configuration is groundwork; it does not by itself prove suitability for leveraged collateral.
- **Liquidation capacity after withdrawals** (shared model section 4) and the funded in-kind route's capacity (section 10).
- **Net return with substantially reduced AQUA rewards**: one P&L ledger at today's AQUA, at −50% and at −80% AQUA price or emissions. PYUSD/USDC is reward-only today, so this test decides whether it has a leveraged case at all.

The pilot pair stays at 1x until it passes. USDC/sUSD can become the leveraged product if it shows better sustainable returns and adequate pricing and exits. No need to commit before the evidence exists.

## 2. Why the existing pieces are not enough as-is

- `aquarius-lp-vault` is gated to one ReceiptVault (`only receipt vault may deposit`) and its shares are non-transferable. It cannot be collateral or a margin position token.
- **Looping a boosted market does not lever the LP:** borrowed cash is recycled into the same market and the deployed LP amount does not grow. Leverage needs a position token the user holds.
- `lp-peridottroller`, `lp-lending-vault`, `lp-receipt-vault` are validation candidates (mainnet init blocked; activation blocked by "depeg-aware pricing, reviewed migration, compiled full-stack validation, final security review" per `config/lp-lending-mainnet.json`). Reuse their depeg-aware pricing and reward-checkpoint work; do not activate them for this.

## 3. Architecture

```
cl-share-vault (new crate, fork of aquarius-lp-vault)   e.g. "pcl-PYUSD-USDC"
  open deposits, transferable SEP-41 shares, nav_root oracle valuation, range policy
price-router: new source ShareNav { vault }
Funding group controller (handoff 02): USDC and XLM markets, cash held locally at first
  rpCL-<pair> (new, collateral only, borrowing disabled)
margin-controller V3 (group instance) + swap-adapter with zap routes
```

### 3a. `cl-share-vault`

Fork `aquarius-lp-vault`. Keep: single-underlying model with the paired leg swapped at deposit, `nav_root` oracle valuation, `max_pool_divergence_bps`, range policy, harvest and reward routes, upgrade delay pattern. Change and add:

- **Open deposits and redemptions** for any address, with a minimum deposit and minimum remaining balance so dust holders cannot create unbounded persistent entries.
- **Transferable shares** (SEP-41), TTL bumps on balance entries.
- **Fee accounting for partial withdrawals.** Aquarius `withdraw_position` pays out the position's **entire** accrued fee balance on any withdrawal, including a partial one. The vault must book those fees into vault-level accounting and pay the withdrawing holder only their **pro-rata principal plus pro-rata share of booked fees**; the rest stays for remaining holders. Same for `claim_position_fees`. Test with many holders withdrawing in different orders.
- **Every proof in shared model section 8**, including: collect swap fees into NAV before any mint or burn; donations do not change NAV; depositors pay their own zap cost.
- **AQUA ownership by entitlement index** (shared model section 8a), not by common NAV. **Claim-first:** before every ordinary mint, burn and transfer, the vault calls the pool's `claim` in the same transaction, credits the received AQUA at the old supply and settles affected holders at their old balances. If the claim fails, **ordinary deposits and ordinary transfers pause**; receivables are not used on the normal path. **Recovery exceptions** (authorized callers only): liquidation, repay-and-release and emergency redemption keep working, record a proven ledger entry when `get_user_reward(vault)` is readable, and otherwise quarantine the amount (forced liquidation) or require `accept_reward_waiver` (voluntary emergency redemption). Holders claim AQUA in kind or "claim and swap my AQUA"; optional auto-compound mints shares to that holder only. Shares posted as collateral: **pass-through** to the position owner. Required: the Alice/Bob test with unclaimed upstream AQUA at the new deposit, and the 10/90 emission-change case.
- **Separate reward checkpointing from conversion.** `checkpoint_rewards()` calls the pool's `claim` and only updates the index. Any conversion is a separate action with its own price guard. A failed or killed swap must never block checkpointing, principal withdrawal or liquidation. Unconverted AQUA is never in NAV.
- **Rewards are not collected unconditionally before burns.** A normal burn checkpoints rewards first when `claim` works. When claims are killed or failing, the burn proceeds on the recovery paths only. With a readable `get_user_reward(vault)`, the holder's AQUA is a **proven** ledger entry settled from the next successful claim. Without it: a voluntary emergency redemption requires `accept_reward_waiver = true` (contract-enforced) or reverts; a forced liquidation proceeds and the borrower's unprovable amount is **quarantined** for later attribution (shared model section 8a).
- **Emergency in-kind redemption** for unencumbered shares: withdraw the holder's pro-rata liquidity and pay both tokens in kind, with no swap, no oracle dependency and no reward claim (entitlement ledger entry recorded as above). Aquarius documents no kill switch on principal withdrawal, so this path remains available when swaps or claims are killed. Shares used as collateral cannot use it; they exit through repay or the liquidation routes in 3d.
- **Capacity cap**: our position ≤ a share of the pool's active liquidity (start 30%) plus an absolute cap from the profitable-capacity sentence.
- **Range-cross checkpoint** on every in/out-of-range transition (Aquarius rewards are binary and re-checkpointed only on deposit, withdraw or claim).
- **20-position cap per owner per pool**: one active range; recenter withdraws the old range fully.
- Exact-in swap remainder edge case: estimate first, strict `out_min`, sizes far below depth.

### 3b. Pricing

- `price-router` source `ShareNav`: `price(share) = booked NAV / total_supply × price(underlying)`; fail closed when `nav_root` is older than its max age, the underlying price is stale, or a pool kill flag affecting valuation is set.
- **Depeg-aware stables:** price sUSD and PYUSD from bounded observed prices (price-router), never a hard $1. This is the same blocker the LP lending config lists; solve it here once.
- Validity is separate from accounting (shared model section 6) and checked **across the whole account**: an untrusted share price makes the controller revert any risk-increasing action of an account holding those shares (borrow from any market, redeem, transfer, `exit_market`, margin opens). Repay always works. Ordinary seizure of untrusted collateral is disabled until a trustworthy price returns or the recovery policy applies; a zero-price sentinel never feeds seize maths.

### 3c. Collateral market

ReceiptVault instance with underlying = share token in the funding group controller; borrowing disabled; borrow cap the smallest positive value if zero means unlimited; supply cap = share-vault capacity. Set and verify both the receipt-local CF and the controller CF.

**Margin V3 rules, not ordinary-CF rules** (shared model section 5b):

- V3 borrows go through `borrow_for_margin` without the ordinary account-CF calculation; V3 position health uses equity and the pair's maintenance margin. The controller CF here only matters for **eligibility** (`assert_market_supported` requires `get_market_cf > 0`) and for **ordinary** users.
- Set the `rpCL-*` controller CF to **exactly 1%** (shared model section 5b). That still lets ordinary accounts borrow up to 1% of share value; accept it explicitly and include it in the aggregate exposure model. Tests: an ordinary account borrows at most 1% of share value; a margin-held position cannot be liquidated through the ordinary controller path. Strict zero ordinary borrowing would need the reviewed `is_market_supported` change (not v1).
- Leverage and maintenance margin come from the V3 pair config (`set_perps_pair_config`: `max_leverage`, `maintenance_margin_scaled`).

### 3d. Leverage and liquidation

- `margin-controller` V3 (perps entrypoints only) from the funding group.
- **Share execution route: decided (b), a pool-interface wrapper `cl-share-pool-facade`.** V3 does not route through a generic adapter call: it validates a direct pool for the position, then calls `AquariusPoolClient::estimate_swap` and `AquariusPoolClient::swap` on that pool directly; the swap adapter only supplies the binding check. A swap-adapter zap route alone would not redirect those calls. The facade is a new contract that **looks exactly like a two-token Aquarius pool to V3** and executes against the share vault. Verified from `margin-controller/src/perps.rs` and `storage.rs` on `leveraged-fix`:

  | V3 behaviour | What the facade must do |
  | --- | --- |
  | Interface used: only `estimate_swap(in_idx: u32, out_idx: u32, amount_in: u128) -> u128` and `swap(user: Address, in_idx: u32, out_idx: u32, amount_in: u128, amount_out_min: u128) -> u128` (`AquariusPoolContract` trait) | Implement exactly these two, same argument order and types |
  | `direct_pool_indices` requires the pair config's `pool_tokens` to have length 2 and maps tokens to indices 0/1 | Fixed token order, e.g. `[underlying, share]`, stored at init and exposed (`get_tokens`) for verification; the perps pair config uses the same order |
  | Validation: nonzero `pool_id`, and `swap_adapter.is_pool_binding_allowed(pool_id, pool)` (adapter-local `set_pool_allowed` + `set_pool_binding`) | Register the facade address with its own `pool_id` through the swap adapter admin; no other pool behaviour is checked |
  | `authorize_controller_pool_swap` pre-authorizes `facade.swap(controller, in_idx, out_idx, amount_in, out_min)` with a nested `token_in.transfer(controller → facade, amount_in)` | The facade's first action in `swap` is exactly `token_in.transfer(user, facade, amount_in)` (authorized by that nested entry). Any other auth shape fails. `user` must be the calling V3 controller; reject others (allow-list the controller) |
  | V3 measures output by the **balance delta of `token_out` at the controller** and panics below `amount_out_min`; the reported return value is ignored | The facade transfers the full output to `user` in the same call and enforces `amount_out_min` itself as well |
  | `estimate_direct_pool_swap` panics on a zero quote; `pool_quote_min_out` applies the pair's slippage to the quote | `estimate_swap` returns a **conservative, non-manipulable** quote: underlying → shares at the share vault's oracle NAV including the zap cost estimate; shares → underlying at oracle NAV minus the paired-leg swap estimate. Return 0 (V3 then refuses) when the share vault's NAV root is stale or the underlying pool is killed for the needed direction. Pauses are **directional**: a deposit pause returns 0 only for underlying → shares and keeps otherwise valid exit quotes (shares → underlying); a full pause returns 0 for both. Never quote from pool spot alone |
  | Liquidation quotes call `estimate_swap` from collateral to debt asset | Same conservative quote path; tests check it never exceeds what `swap` then delivers |

  Execution inside `swap`: underlying → share uses `share_vault.deposit_underlying(facade, amount_in, min_shares)` then transfers shares to `user`; share → underlying uses `share_vault.redeem(facade, shares, min_out)` then transfers underlying to `user`. Both check balance deltas. The facade's admin can pause it, rotate the allowed controller through a delayed upgrade like other Peridot contracts, and sweep **unencumbered donations** to a fixed treasury. It has no power over user funds or reward liabilities: between calls the facade holds neither, so a sweep can only move balances that no operation or ledger entry owns; test that a sweep during or around an operation cannot move operation funds.

  **Facade implementation requirements:**

  | Area | Required behavior |
  | --- | --- |
  | Authorization | Require the configured controller's **authorization** (`user.require_auth()` with `user` equal to the configured controller), not merely an equality check. Consume exactly the expected controller → facade `token_in.transfer` authorization. The facade's own deeper calls (share vault deposit/redeem, token transfers from the facade) are authorized separately by the facade (`authorize_as_current_contract` with exact entries) |
  | Donations and residuals | Measure **per-operation deltas**: the facade must end the call with the same balances it started with (no residual from **this** operation). Do **not** require absolute balances to be zero: dust sent to the facade must not be able to disable it. Pre-existing balances neither subsidize a swap nor get paid to the caller; the restricted admin sweep (unencumbered donations only) handles them |
  | Quotes | Conservative quotes are tested against execution **at the same pinned state**. Across transactions, price or liquidity changes can invalidate a quote; execution then must meet `amount_out_min` or revert. A conservative quote is **not** an output ceiling: transfer the full output produced by this operation, require output ≥ `amount_out_min`, and never subsidize it with pre-existing balances |
  | Pause behavior | Separate flags: a **deposit pause** blocks underlying → shares only; shares → underlying stays available for closes and liquidations. A full pause blocks both. The funded in-kind liquidation fallback (shared model section 10) works **independently of the facade**, so a facade quote of zero (swaps unavailable) never blocks it |
  | Reward calls | Several share movements in one transaction (open: facade → controller → position; liquidation: position → facade → burn), including claims that succeed but return zero. Temporary facade custody must never capture historical rewards or strand an entitlement: the facade is excluded from reward accrual, or settles to zero in the same call; tested both ways |
  | Test method | Exercise the **real** Soroban authorization tree (`require_auth`, `authorize_as_current_contract`). Do not use blanket auth mocks (`mock_all_auths`) for facade tests; they hide exactly the integration failures this wrapper must catch |

  **Reward ownership through the facade:** shares pass through the facade only within one call, so it never accrues rewards. The share vault's ownership-change rule (shared model section 8a) applies to both transfers (facade → controller, controller → facade); the facade path counts as a normal transfer when the upstream claim works, and as an authorized recovery path for close and liquidation when it does not. Test both.

  **First engineering checkpoint:** a **compiled** integration harness (real WASM for V3, swap adapter, facade, share vault, receipt markets, controller; real Aquarius pool WASM as in the existing exact-pool tests) proving underlying → shares → underlying through V3 with the real authorization tree, donated dust on the facade, reward-claim failures, and measured resources. Develop the **funded in-kind liquidation path** alongside it. Maintenance margin stays unset until the economic and liquidation evidence exists; that does not delay stage 1.

  **Coverage required:** open (underlying → shares), voluntary close and liquidation (shares → underlying), recovery of interrupted multi-step opens (`begin → execute → swap → activate`, expiry and cancel at each step), liquidation quote vs execution, killed pool, stale NAV, paused share vault, wrong caller, wrong auth shape, residual balances, footprint per step with `scripts/check_budget.sh` on mainnet state. Adversarial review of the facade before any binding on mainnet.
- Position = long the share token, debt = the underlying (USDC for a USDC-paired vault).
- **Leverage is an integer in V3 today** (`notional_value = margin_value.checked_mul(leverage)`, no divisor; passing 150 would mean 150x). **Launch at 2x** with a maintenance margin chosen from the backtest and liquidation-capacity tool. Fractional leverage is deferred; if it is ever built it needs scaled leverage in the entrypoint, fee maths, `max_leverage` validation in the same scale, rounding never in the user's favour, migration of existing pair configs and compatibility tests.
- **Position health after costs:** test that a 2x open, after opening fees, zap cost and slippage, starts above maintenance with a stated buffer, and that the first interest accrual or a 1-tick move cannot make it liquidatable.
- Raise leverage only after stressed liquidation rehearsals pass.
- Re-verify our three historical margin bugs are fixed on `leveraged-fix` (handoff 02 section 5).
- **Liquidation routes**, specified separately from the emergency exit:
  - **Normal:** seize shares, redeem single-asset through the zap (pool withdrawal plus swap of the paired leg), repay; may be flash-loan funded.
  - **Fallback when the pool's swap is killed** (prices still trustworthy): funded in-kind liquidation (shared model section 10). The **liquidator supplies the debt asset (USDC)**; repayment and seizure settle atomically; the liquidator receives the seized shares, or the redeemed PYUSD and USDC in kind, at the bonus computed from oracle prices, and bears the exit risk. **No flash loan** on this route, because the collateral cannot be sold inside the transaction. Prefer handing over underlying tokens over illiquid pTokens.
  - Operational requirement: a **funded liquidator** (Peridot's own with a USDC budget, plus partners if available), a capacity estimate for the fallback route, and aggregate caps set so that capacity covers the stressed scenario.
  - If the **price** is untrustworthy, neither route runs; repay only, recovery policy per shared model section 6.
  - Test all of these.
- Liquidation capacity per shared model section 4: our own liquidity leaves the pool first; correlated liquidations in sequence; stressed third-party liquidity.
- **Multi-transaction opens** (`begin → execute → swap → activate`): for each intermediate state, specify and test who holds the borrowed funds and the shares, the deadline, cancellation by the user and by anyone after the deadline, price revalidation at the next step, and who owes the debt if the flow is abandoned (shared model section 9). Measure each step with `scripts/check_budget.sh` on mainnet state.

### 3e. Lender yield

Borrowed funds come from the funding group's local cash (handoff 02's `min_cash_reserve_bps` applies). Lenders earn borrow interest; once the optional Upshift allocation is enabled, also Upshift yield on the deployed fraction. **This does not remove dependence on Blend**: Upshift's earnUSDC itself held about $10.8M in Blend at reading. Revisit the stable rate model when utilization is visible; the backtest's net LP return bounds what leveraged users can pay.

## 4. Keeper

Extend `bots/aquarius-keeper` (from `aquarius-keeper-v0.4.3`): add the share vault; NAV root refresh; recenter; range-cross `checkpoint_rewards` (always, even when AQUA is small); AQUA conversion as a separate, economics-gated step; receipt cache refresh. Same single-signer, sequence-safe, no-protocol-role rules. Margin liquidator separate with its own key.

## 5. Tests and gates

| Gate | Evidence |
| --- | --- |
| Pair comparison | The stage 3 gate in section 1, including the reduced-AQUA scenarios; operator confirms the leveraged pair |
| Share vault | Mock reproducing binary reward checkpointing, 20-position cap, exact-in remainder and **full fee payout on partial withdrawal**; every proof in shared model section 8; **Alice/Bob AQUA invariant with unclaimed upstream rewards**; index conservation across mint/burn/transfer; burn with claims killed records a ledger entry that the next claim settles; dust and TTL tests; emergency in-kind redemption with swaps and claims killed and oracle stale; checkpoint succeeds while conversion fails |
| Pricing | ShareNav fail-closed; depeg of the stable leg lowers collateral value; account-wide validity test (invalid share collateral blocks borrowing USDC from the funding market, transfer and `exit_market`; repay works) |
| Leverage | Open/close/liquidate at 2x through the facade (all rows of its coverage list); recovery of interrupted opens at every step; post-cost health above maintenance with buffer; ordinary accounts borrow at most 1% of share value; margin positions not liquidatable via the ordinary path; full receipt → share → underlying recovery flow with both reward calls failing; funded in-kind fallback with swap killed; abandoned multi-step open at every step; footprint per step |
| Mainnet | Share vault paused-deploy, tiny cap, 1x deposits; collateral listing; margin configs after stressed rehearsals |

## 6. Open operator decisions

1. Confirm PYUSD/USDC as the 1x pilot; leveraged pair after the stage 3 comparison.
2. Accept sUSD issuer and depeg risk if USDC/sUSD is chosen for leverage.
3. Maintenance margin for 2x. (Decided: share execution through the pool-interface facade; CF fixed at 1%; fractional leverage deferred.)
4. AQUA pass-through implementation for shares posted as collateral (forfeit is not used; on PYUSD/USDC AQUA is the whole income).
5. Funded liquidator and its USDC budget for the in-kind fallback.
6. When to enable the optional Upshift allocation in the funding group.
