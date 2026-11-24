# Frontend: v0 leverage loop (Peridot cash markets, Mainnet)

The reference implementation is `research/cash-borrowing-activation-20261117/tools/loop-core.cjs`. It is pure, so it can be ported to the frontend unchanged. The I/O is in `loop.cjs`.

Contracts:
- Controller: `CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK`
- USDC receipt: `CDW3GDNK…HY3F`
- XLM receipt: `CBHMG4CN…4WMS`
- Aquarius router: `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK`

## Principles

1. **One wallet signature per step.** Soroban allows one contract call per transaction. A loop is 10 transactions to open and 10–13 to close. Show it as a stepper ("Step 4 of 10") with Continue / Stop buttons.
2. **Not atomic: keep HF ≥ 1.25 after every step**, including right after a borrow, before the swap and redeposit. The live liquidator can act between two transactions.
3. **Re-read live state before building each step.** Read pTokens × exchange rate, `get_user_borrow_balance`, wallet balance, `get_price_usd`, `get_market_cf`, then simulate. Never reuse amounts precomputed at the start; swap outputs differ from estimates.
4. **Leverage selector bounds.** max = HF_min / (HF_min − CF).
   - Long XLM (CF 55 %): offer at most **1.7x**. 2x would end at HF 1.10.
   - **Short XLM** (USDC collateral, CF 75 %): up to **2.5x** (ends at HF 1.25; liquidation if XLM rises ~25 %).
     Rounds grow steeply near the cap because 2.5x sits exactly on the HF floor (projected with `projectOpen`, 0.3 % swap cost):
     2.0x = 3 borrow rounds (~10 tx) · 2.3x = 4 · 2.4x = 6 (~19 tx) · 2.45x = 7 · **2.5x = 15 (~46 tx)**.
     Show the projected round/signature count next to the leverage selector; suggest 2.4x as the practical maximum.
   - Disable values above the bound and explain why.
5. **One position at a time.** Refuse to borrow asset B while B is also supplied, or while the other asset is borrowed.

## Formulas (CF and prices from the controller)

- `collateralUsd = Σ ptokens·rate/1e6 · price`
- `weighted = Σ collateralUsd·CF`
- `debtUsd = Σ debt·price`
- `HF = weighted / debtUsd` (liquidatable when HF < 1, which equals controller shortfall > 0)
- `equity = collateralUsd − debtUsd`
- `leverage = collateralUsd / equity`
- Next borrow, in USD: `min((L−1)·equity − debtUsd, weighted/HF_min − debtUsd)`
- Liquidation XLM price:
  - long: `debtUsd / (CF_XLM · XLM collateral)`
  - short: `CF_USDC · USDC collateralUsd / XLM debt`
- Cross-check against `controller.account_liquidity(user)`, which returns `(liquidity, shortfall)`.

## Open: long XLM (short is the mirror: deposit USDC, borrow XLM, swap XLM→USDC, deposit USDC)

| # | Transaction | Show afterwards |
|---|---|---|
| 0 | (once) `controller.enter_market(user, XLM receipt)` and `(user, USDC receipt)`, only if `get_user_markets` lacks them | "Markets enabled as collateral" |
| 1 | `XLM receipt.deposit(user, equity)` (keep ≥ 3–8 XLM in the wallet for reserves and fees) | collateral $, HF ∞, leverage 1.00x |
| 2 | `USDC receipt.borrow(user, b)`, with b from the formula above | debt $, **HF (≥ 1.25)**, "proceeds in wallet, not yet invested" |
| 3 | `router.swap(user, [XLM,USDC] sorted, USDC, XLM, pool_index, b, out_min)` | received XLM vs quote, price impact, deviation from oracle |
| 4 | `XLM receipt.deposit(user, received)` | collateral, HF, **leverage now / target**, liquidation price |
| 5+ | Repeat 2–4 until leverage is within 0.1 % of target (3 rounds for 1.7x long and 2x short; 6 for 2.4x short, 15 for 2.5x short) | final summary: leverage, HF, liquidation price, borrow APR (USDC 8 % / XLM 6 % static) |

## Swap (Aquarius)

- **Discover pools on-chain:** `router.get_pools(sorted [tokenA, tokenB])` returns a map from pool_index to pool. Do not use the HTTP `/pools/` API; it hides live pools.
- **Quote:** `router.estimate_swap(tokens, in, out, pool_index, amount)` for every pool, **with the exact amount**. Pick the largest output.
- **Oracle guard:** `dev = 1 − (out·price_out)/(in·price_in)`. Refuse if |dev| > 1 %, and show "Pool price differs from the oracle".
- **Slippage:** `out_min = quote × (1 − slippage)`. Default 1 %, user-adjustable from 0.1 % to 3 %. Re-quote right before signing.
- **Call:** `router.swap(user, tokens, token_in, token_out, pool_index, in_amount, out_min)`. Simulate it.
  - The auth tree holds router.swap → pool.swap → token_in.transfer(user, …, in_amount).
  - Show the wallet exactly this. Reject any other authorized call.

## Close (unwind)

1. `withdraw(user, ptokens)` for the collateral. Size it to cover the debt + 2.5 %, but never below HF 1.25. Show HF after.
2. Swap exactly the withdrawn amount to the debt asset, with out_min from the slippage setting. Do not touch the user's other balance.
3. `repay(user, min(walletDebtAsset, get_user_borrow_balance))`.
4. Repeat 1–3. Expect 3 rounds, because the first withdrawals are HF-limited.
5. **Repay dust.**
   - `repay(user, amount)` transfers at most the pre-accrual debt, so a little interest remains.
   - Loop `repay(user, get_user_borrow_balance(user))` until it reads **0**. Usually 1–2 extra transactions.
   - Never send more than that balance: an overpay makes the signed transfer amount depend on accrual timing.
   - Do **not** use `repay_max`: it fails if interest accrues between simulation and execution.
6. `withdraw(user, all pTokens)` only when the debt is 0 in **every** market. With dust debt, withdraw fails `InvalidAction`, so disable the button until the debt is 0.

## What to show at all times

- Collateral $, debt $, equity $, HF (colours: green ≥ 1.5, amber 1.25–1.5, red < 1.25), leverage, liquidation XLM price.
- Wallet balances, with a reserve warning when XLM falls below 3.
- A per-step projected HF ("after this step: 1.25") computed before signing. Block the step when the projection is below the floor.
- Network fee from simulation (about 0.005–0.007 XLM per step on 24 Nov) and the remaining number of steps.
- On a failed or expired transaction: re-read state and resume from the computed next step. The flow is stateless and idempotent, so never blindly resend old XDR.
