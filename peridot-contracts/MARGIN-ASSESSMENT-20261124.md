# Margin on the live cash markets: assessment, 24 Nov 2026

Branch `assess/margin-20261124` (worktree `.wt-margin-assess`, based on `main` fe8d562). Nothing was signed, sent, uploaded or deployed. Mainnet was used read-only: `getLedgerEntries` and `simulateTransaction` only.

**Scope change (owner, mid-task):** the margin controller must work custody-only. The receipts' MarginController hook stays unset, cash-market lenders must never call into margin, and no receipt or controller upgrade is to be planned.

## Verdict

**No-go for an unsupervised public "one-click margin" tomorrow.** A conditional go is possible for a supervised, team-run pilot of about $200, at the earliest Wed 25 Nov around 15:00 CET. That needs gates G1–G4 below to pass first.

| Question | Answer |
|---|---|
| Does the `main` margin-controller work against the live ABI? | **No.** It needs the hook on every open path, and its `begin_margin_withdraw` call has the wrong arity. |
| Is the hook required? | Yes for `main`'s code. **No for the custody-only rewrite on this branch.** |
| Does the rewrite need a receipt or controller upgrade? | **No.** It touches only the new margin contract. |
| Can lenders be affected by margin calls? | No. While `MarginController` is unset, no lender path calls margin (see §3). |

## 1. Live bytes: source, verified by hash

**Live hashes.** A read-only Mainnet read on 2026-11-24 at 10:30 UTC (`node read-state.cjs`) returned:
- controller `CBMRS7LK…JRK`: `f41e5260…e2d8`
- receipts USDC, XLM, PYUSD and EURC: `5630a701…e452`
- `get_margin_controller = None` on all four receipts
- `MarginLiquidationControllers = null`
- `MaxUserMarkets` unset, so the default `DEFAULT_MAX_USER_MARKETS = 2` applies (`.wt-cash-upgrade/.../simple-peridottroller/src/constants.rs:22`)

**Source of those bytes.** `research/cash-borrowing-upgrade-20261118/build-hashes-upgrade.json` gives the patched source: worktree `.wt-cash-upgrade` (branch `wip/cash-borrow-upgrade-checks`, base `4c03928`, uncommitted, diff = `upgrade.patch`), built at the canonical OpenZeppelin path.
- The recorded sha256 values `f41e5260…` (70,507 B) and `5630a701…` (89,808 B) match the files in `artifacts/` (re-hashed today).
- Interfaces below come from `stellar contract info interface --wasm` on those exact artifacts.

## 2. Q1: `main`'s margin-controller against the live ABI

Line numbers refer to `main:peridot-contracts/contracts/margin-controller/src/contract.rs`; `live` refers to `.wt-cash-upgrade/peridot-contracts/contracts/receipt-vault/src/contract.rs`.

| Call made by `main` margin | Live export | Works? |
|---|---|---|
| receipt `deposit`, `withdraw`, `borrow`, `repay(user, u128)`, `get_exchange_rate`, `get_ptoken_balance`, `get_user_borrow_balance`, `get_underlying_token` | Same signatures | Yes, as a plain account |
| receipt `transfer(from, to, i128)` | `to` is now a `MuxedAddress` | Yes; an Address value is accepted |
| receipt `get_margin_controller` (`assert_margin_lock_configured`, :1311, called at :290/:304/:306/:446-448/:638/:651 on every open) | Exists, returns `None` live | **No.** Every open panics "margin lock not configured" unless the hook is set |
| receipt `begin_margin_withdraw(controller, user)` (:1408, via `try_invoke`) | Live takes 4 args `(margin_controller, user, recipient, max_ptokens)` and needs controller auth (live :2746-2775) | **No.** Arity mismatch: `try_invoke` swallows it, then withdraw or transfer hits `enforce_margin_lock` |
| receipt `init_margin_borrow_state`, `borrow_for_margin`, `repay_for_margin` (V2, :508-509, :907, :1131) | Same signatures, but all call `require_margin_controller_auth` (live :1108-1116, :4103, :4149, :4407) | **No** without the hook |
| controller `liquidate_for_margin` (V1, :1032) | Same 8-arg signature | Only if the controller admin sets `set_margin_liquidation_ctrl` (currently null) |
| controller `get_price_usd`, `cache_price`, `enter_market`, `account_liquidity`, `get_market_cf` | Same | Yes |
| adapter `swap_chained(user = controller)` (close_v2 :893) | Adapter ABI on `main` | **No.** There is no `authorize_as_current_contract`, so the router's `require_auth` for the controller is non-root and fails |
| `transfer_spot_to_margin` (:131) | `receipt.transfer(user → margin)` | Hook set: the receipt calls `locked_ptokens_in_market` back into margin while margin is mid-call, which is contract re-entry and a host error. Hook unset: it works, but only as a ledger entry |

Two more facts about the hook design:
- **`borrow_for_margin` has no health check** (live :4137-4260; it checks only pause, liquidity, borrow cap and cash reserve). Margin debt there is position-namespaced and invisible to `account_liquidity`, so lender solvency would rest entirely on margin code.
- **The V3 controller on `4c03928` was written for this ABI** (4-arg `begin_margin_withdraw`, direct pool swaps). It is still hook-based, so it is out of scope.

## 3. Q2/Q3: hook semantics on the live receipt

These facts apply only if someone sets the hook; the custody-only design never does.

**Setter.** `set_margin_controller(admin, Option<Address>)` (live :2703-2738) is admin-only (`GC4T5…`) and **immediate, with no timelock**. Setting the hook probes `locked_ptokens_in_market` on the target. Passing `None` unsets it immediately, and this is the kill switch.

**Once the hook is set, every lender pays a margin call:**
- `withdraw` (live :1915-1917) and `transfer`/`transfer_from` (live :2337-2342) call `enforce_margin_lock` (live :1155-1177).
- That function uses `call_contract_or_panic` to invoke `locked_ptokens_in_market` on margin for **every lender**. It is a plain invoke, not `try_invoke`.

**Ways that call can fail other than through a bug:**
- The margin instance, its code or a position entry is archived. Since protocol 23 these restore automatically, but the transaction must carry and pay the restore.
- Resource growth: `locked_ptokens_in_market` loops over all of the user's positions.
- A panic, or a margin upgrade that removes or changes the export.

On any such failure, **lender withdraws and transfers revert** until the hook is unset. Unsetting is immediate but needs an admin Ledger signature.

**If the hook is unset while V2 margin debt exists,** `repay_for_margin` and `borrow_for_margin` panic "margin controller not set". Margin debt can then no longer be repaid or liquidated through margin, which strands lender funds.

**The custody-only design removes the coupling entirely:**
- With `MarginController` unset, `enforce_margin_lock` is a no-op (live :1164: `if let Some(...)`).
- The bypass path (`consume_margin_withdraw_bypass`, live :1118-1144) only reads a key that never gets written.
- Lenders never touch margin storage or code.

## 4. Q4: fixes, implemented on this branch

**This is a rewrite, not the existing contract.** `main`'s margin-controller and its tests are deleted. Also dropped: `transfer_spot_to_margin`/margin balances, the swap-adapter routes, multi-step V1 leverage and V2. The new crate has no unit tests yet; it is covered only by the fork test.

The rewrite is in `contracts/margin-controller/src/contract.rs`, and the crate is now standalone. **No receipt or controller change is needed: no upgrade and no timelock.** The swap-adapter is no longer needed; margin swaps directly on the Aquarius pool.

**The margin contract is a normal account.** It uses only the plain entrypoints: `deposit`, `enter_market`, `borrow`, `repay`, `withdraw`, `transfer`, `get_*` and `get_price_usd`/`get_market_cf`.
- The live controller health-checks the pooled margin account on every borrow, withdraw and transfer.
- Removed: every hook call (`get_margin_controller` assertion, `begin_margin_withdraw`, `*_for_margin`, `liquidate_for_margin`, `locked_ptokens_in_market`).
- `deposit` and `repay` pull the underlying from the margin contract in a nested call. Each is preceded by a **root** `authorize_as_current_contract` entry for exactly `token.transfer(self, receipt, x)`.
- For repay, `x = min(amount, get_user_borrow_balance(self))`, matching the receipt's pre-accrual cap (live :4280-4285, :4318-4320).

**Swap.** It calls Aquarius pool `swap(self, in, out, amount, min)` directly, with the input-token transfer authorized at the root. This matches the deployed pool: `swap` does not `require_auth`.
- `min_out` is the larger of the user's floor and the oracle minimum (oracle price × (1 − max slippage, default 1%)).
- The received amount is checked by balance delta.

**Position ledger.**
- Each position holds `held_ptokens` and `debt_shares`. Debt is `shares × pooled_debt / total_shares`, rounded up, and the last shareholder owes the full balance.
- Long XLM is held XLM against USDC debt; short XLM is held USDC against XLM debt.
- The 2-entered-market cap means exactly one pair (XLM, USDC) is possible on the pooled account.

**Entrypoints:**
- `open(user, held, debt, collateral, borrow, min_out)`: transfer in, deposit, borrow, swap, deposit. It checks the LTV before the trade (at the oracle minimum output) and after it (actual fill).
- `close(user, id, min_out)`: withdraws only enough collateral to repay at the slippage bound, swaps, repays, then sends the surplus debt asset and the remaining pTokens to the owner.
- `close_with_repay(user, id)`: the owner pays the debt from their wallet, with no swap. It works at any LTV.
- `liquidate(liquidator, id)`: permissionless once `ltv ≥ liq_ltv`. The liquidator repays the full debt and receives pTokens worth debt × bonus (default 5%); the owner gets the remainder.

**Risk parameters, checked against the live CF.** `set_risk` enforces `open_ltv < liq_ltv < controller CF(held market)`. Live CFs are XLM 0.55 and USDC 0.75.
- Suggested settings are XLM-held `open 0.30 / liq 0.45` and USDC-held `open 0.40 / liq 0.62`.
- The resulting maximum leverage is about 1.43× for an XLM long and about 0.67× short exposure.
- These limits keep a single borrow step and a single-withdraw close inside the controller's health check without loops. Closes above about 0.35 XLM-LTV must use `close_with_repay`.

**Caps and kill switch (both in margin, both immediate):**
- `set_caps(debt, per_position, global)` caps the borrow per open and the pooled debt including interest.
- **Required fix before deploy:** there is no collateral cap. `open` accepts any collateral, so custody in margin is not bounded to about $200; only debt is.
- Contract creation leaves opens paused. `set_opens_paused` can be called by the admin or the guardian (guardian may only pause). Close and liquidate are never paused.
- Margin's own upgrade is timelocked to 24h.
- Initialisation runs in `__constructor`, so it is atomic with the create and cannot be front-run.

**Lender-side backstops already live:**
- per-market `BorrowCap` (USDC 900, XLM 4,500)
- 70% cash reserve
- immediate borrow pause by the pause guardian `GBSK3…`

## 5. Q5: resource measurements (Mainnet, read-only)

The raw numbers are in `.scratch/margin-assess-20261124/MARGIN-ASSESSMENT-20261124.sim.json` (git-ignored evidence, sha256 `f1486228…ad97`), produced by `scripts/margin_assess_sim.cjs` at ledger 65646170.

| Item | Result |
|---|---|
| `margin_controller.wasm` (`stellar contract build`), first build `4dc93a0f…e06c` (22,086 B, used for the upload quote) | Final build: `80a082ec…6ea8`, 23,062 B |
| Upload simulation | 19.67M instructions, 1 entry, write 22,188 B, **min resource fee 34.88 XLM** |
| Create simulation | `Error(Storage, MissingValue)`, because the code isn't uploaded. It cannot be quoted before the upload; expect less than 1 XLM (one instance entry plus constructor writes). |
| Aquarius CL pool `CBBMQB…BUCV` `swap`, 100 XLM→USDC | 14.63M instructions, 10 footprint entries, 0.0027 XLM |
| Same pool, 15 USDC→XLM | 10.29M instructions, 10 entries, 0.0025 XLM |

**Measured end-to-end on a Mainnet fork.** Results are in `.scratch/margin-assess-20261124/MARGIN-ASSESSMENT-20261124.fork.json` (git-ignored, sha256 `da31d07f…8961`); the test is `harness/margin-fork/tests/margin.rs`.
- **Fork:** fetched 2026-11-24 at ledger 65646193, with live controller `f41e5260` and live receipts `5630a701`. Final margin WASM is `80a082ec…6ea8` (23,062 B), which includes both swap-auth shapes.
- **Host:** protocol-29. Mainnet reports protocol 30, so the snapshot version was clamped to 29.
- **Auth:** user and liquidator calls ran under explicit, non-`mock_all` auth trees. The contract's own root transfer authorizations were therefore really exercised.
- **Pool:** a native mock, because the real pool's storage isn't in the fork. It pulls the input with `transfer`, so it exercises the root-transfer auth shape only.
- **Real pool auth shape:** a Mainnet simulation of the live CL pool's `swap` records **`pool.swap(user, …)` → nested `token.transfer(user, pool, amount)`**. So this pool DOES `require_auth` on `swap`, unlike the memory note. Margin now authorizes both shapes, root transfer and swap-with-nested-transfer, as V3 does. The swap shape is still unverified against the real pool code.

| Flow | Instructions | Memory | Footprint keys | Writes | Mainnet fit (400M / 400 / 200) | Internal 100-key gate |
|---|---|---|---|---|---|---|
| `open` long XLM (300 XLM + 15 USDC borrow) | 96.5M | 9.9 MB | 100 | 30 | yes | at the limit |
| `close` | 102.0M | 11.7 MB | 102 | 31 | yes | **exceeds** |
| `liquidate` after XLM −50% | 86.5M | 10.3 MB | 97 | 25 | yes | yes |

- **Real-pool adjustment.** The real pool swap costs 10.3–14.6M instructions and 10 entries, against roughly 2 for the mock. Expect about 110–115M instructions and about 108 keys per flow. That still fits Mainnet comfortably, with 3.5× instruction headroom.

**Checks that passed:**
- Opens are paused at create.
- The receipt hook stays `None` throughout.
- Open LTV is 0.282.
- Close repaid everything: pooled USDC debt is 0, the user got 1.36 USDC surplus plus 298.66 XLM of pTokens.
- After XLM −50% (LTV 0.564 ≥ 0.45), liquidation succeeded: the liquidator received 247.6 XLM of pTokens, the owner got the remainder, and pooled debt is 0.

**Bug found and fixed by the fork run.** The receipt's `get_user_borrow_balance` view lagged the index that its next state-changing call applies. The position debt was read as 150,000,000 but the receipt then held 150,035,811, which left residual debt and made the final pToken transfer fail its health check. Margin now calls the receipt's public `update_interest()` before every debt read. **Live receipt finding, affecting every cash borrower, not just margin.** A fresh 15 USDC borrow was charged 35,811 raw (0.0036 USDC) of immediate interest. That equals 8% APR on 150,000,000 raw over the market's idle time: fork close time 1795517197 minus USDC `LastUpdateTime` 1795423081 is 94,116 s, about 26.1 h, which predicts about 35,800. So a new borrower appears to pay interest for the period before their borrow. The effect is small and proportional to how long the market sat idle, but it needs a receipt-side review. Margin works around it by accruing before reading debt, and does not fix it.

## 6. Q6: ceremony, caps and timing

**Gates before any Mainnet step:**
- **G1 (partly done):** open, close and liquidate pass on the live-bytes Mainnet fork with strict auth (§5). Still open:
  - `close_with_repay`, short XLM, and long and short positions open at the same time on the pooled shares (the fork test had one long position at a time)
  - the real Aquarius pool: add its contract keys to the fork. A post-deploy simulation of `open` cannot stand in, because `open` fails on `OpensPaused` before the swap.
  - a protocol-30 host (Mainnet is now p30; `net-limits.cjs` at ledger 65646323 shows unchanged limits of 400M instructions, 200 reads and 200 writes)
- **G2:** native tests in the crate. None exist yet on this branch; the old `test.rs` targeted the hook ABI and was removed.
- **G3:** Almanax scan of the branch, with zero High or Medium findings.
- **G4:** owner sign-off on the parameters, and a liquidation keeper running, because internal liquidation must come before the controller's own liquidation of the pooled account.

**Ceremony (admin `GC4T5…` on Ledger, HD path 0; each step simulate → review → sign):**
1. Upload `margin_controller.wasm`: rebuild from the reviewed commit, verify the sha256 (current build `80a082ec…6ea8`), about 35 XLM.
2. Create the contract with constructor `(admin, controller CBMRS7…JRK, guardian)`. Opens start paused.
3. Call `set_market` for XLM (`CBHMG4…WMS`) and for USDC (`CDW3GD…Y3F`).
4. Call `enter_markets([XLM, USDC])`.
5. Call `set_pool(CBBMQB…BUCV)` and verify the tokens are `[XLM, USDC]`.
6. Call `set_risk` for XLM (300000 / 450000) and USDC (400000 / 620000), then `set_limits(10000, 1050000)`.
7. Call `set_caps`: USDC per position 50e7, global 200e7; XLM per position about 390e7, global about 1550e7 (about $200 at $0.129).
8. Run the read-back verification script.
9. `set_opens_paused(false)`.
10. Canary: open, close, then `close_with_repay` with $5, by a team wallet.
11. Start the keeper.

No receipt or controller transaction is needed, and nothing waits on a timelock.

**Earliest time:** Wed 25 Nov around 15:00 CET, provided G1–G4 close tonight or tomorrow morning. If G1 or G3 slip, Thu 26 Nov.

**Residual risks:**
- The pooled account can be liquidated by the controller's own liquidators if internal liquidations lag. Positions would then be left short of the pTokens recorded in the ledger, and admin recovery would be needed.
- Bad debt is bounded by the margin caps and absorbed by the operator.
- Oracle staleness blocks opens and closes.

**Next step: keeper take-profit and stop orders.** The user would pre-sign nothing on-chain; a keeper watches prices and calls `close` for the user. This needs a delegated close (for example `close_for(owner, id, min_out, keeper)` with a per-position trigger stored on-chain), which is a follow-up release.
