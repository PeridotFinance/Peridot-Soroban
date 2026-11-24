# Frontend handoff: leverage loop ("Open up to 2.5x short XLM / 1.7x long XLM") on Peridot cash markets

**Date:** 24 Nov 2026 · **Network:** Stellar Mainnet · **Status:** contracts are live and unchanged; the loop was tested
end to end on Mainnet today (long 1.70x and short 2.00x, opened and closed, 47 transactions, all SUCCESS).
**Nothing on-chain is new.** This is a frontend-only feature built from existing calls.

## 1. What to build

A guided stepper on top of the existing lending UI:

- User picks **side** (Long XLM / Short XLM), **leverage** and an **equity amount**.
- The app runs a sequence of single transactions (one wallet signature each):
  deposit → borrow → swap on Aquarius → deposit → … until the target leverage is reached.
- A matching **Close** flow unwinds: withdraw → swap → repay → … → repay interest dust → final withdraw.
- After every step, show health factor, leverage and liquidation price.

Soroban allows one contract call per transaction, so this is **not atomic**: ~10 signatures to open, ~11 to close.
Show it as "Step 4 of 10" with **Continue** / **Stop** buttons. Stopping at any step leaves a valid, healthy position.

## 2. Contracts (Mainnet)

| What | Address |
|---|---|
| Controller | `CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK` |
| USDC receipt (market) | `CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F` |
| XLM receipt (market) | `CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS` |
| USDC token (SAC) | `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75` |
| XLM token (SAC) | `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA` |
| Aquarius router | `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK` |

Live parameters (read them from the controller, don't hard-code): collateral factor USDC 75 %, XLM 55 %;
borrow APR USDC 8 %, XLM 6 %; oracle Reflector via `controller.get_price_usd`.

## 3. Hard rules (safety)

1. **Health factor ≥ 1.25 after every single step**, including right after a borrow (before swap/redeposit).
   The live liquidator can act between two transactions. Liquidation is at HF < 1.0.
2. **Leverage limits** (max = HF_min / (HF_min − CF)):
   - **Long XLM** (XLM collateral, CF 55 %): offer at most **1.7x**. 2x long would end at HF 1.10 — disable it and say why.
   - **Short XLM** (USDC collateral, CF 75 %): up to **2.5x** (ends at HF 1.25; liquidation if XLM rises ~25 %).
     Rounds grow steeply near the cap because 2.5x sits exactly on the HF floor (projected with `projectOpen`, 0.3 % swap cost):
     2.0x = 3 borrow rounds (~10 tx) · 2.3x = 4 · 2.4x = 6 (~19 tx) · 2.45x = 7 · **2.5x = 15 (~46 tx)**.
     Show the projected round/signature count next to the leverage selector; suggest 2.4x as the practical maximum.
3. **Re-read live state before building each step.** Never reuse amounts computed at the start; swap outputs differ.
4. **One position at a time.** Refuse to borrow an asset the user also supplies, or a second asset.
5. **Swap guard:** refuse if the pool price differs from the oracle by more than 1 %; default slippage 1 %
   (user-adjustable 0.1–3 %); re-quote right before signing.
6. **Never blindly resend old XDR.** On a failed/expired transaction: re-read state and resume from the computed next step.
   The flow is stateless — the next step is always derivable from on-chain state.
7. Keep **≥ 3 XLM** in the wallet for reserves and fees; warn below that.

## 4. Formulas (CF and prices from the controller)

- `collateralUsd = Σ pTokens · exchangeRate / 1e6 · price`
- `weighted = Σ collateralUsd · CF`
- `debtUsd = Σ debt · price`
- `HF = weighted / debtUsd`  ·  `equity = collateralUsd − debtUsd`  ·  `leverage = collateralUsd / equity`
- **Next borrow (USD):** `min((L − 1) · equity − debtUsd,  weighted / 1.25 − debtUsd)`
- **Liquidation XLM price:** long `debtUsd / (CF_XLM · XLM collateral)` · short `CF_USDC · USDC collateralUsd / XLM debt`
- Cross-check against `controller.account_liquidity(user)` → `(liquidity, shortfall)`; shortfall must be 0.

**Reference implementation:** `research/cash-borrowing-activation-20261117/tools/loop-core.cjs` (137 lines, pure JS, no I/O)
— `health`, `nextBorrow`, `nextWithdraw`, `swapGuard`, `bestPool`, `projectOpen`, `projectClose`, `maxSafeLeverage`.
Port it unchanged. `loop.cjs` next to it shows the chain reads, simulation and transaction building.
Branch: `feat/cash-leverage-loop-v0` in the Peridot-Soroban repo.

## 5. Open flow (long XLM; short is the mirror: deposit USDC, borrow XLM, swap XLM→USDC, deposit USDC)

| # | Transaction | Show afterwards |
|---|---|---|
| 0 | (once) `controller.enter_market(user, XLM receipt)` and `(user, USDC receipt)` if `get_user_markets` lacks them | "Markets enabled as collateral" |
| 1 | `XLM receipt.deposit(user, equity)` | collateral $, HF ∞, leverage 1.00x |
| 2 | `USDC receipt.borrow(user, b)` with `b` from the formula | debt $, **HF (≥ 1.25)**, "proceeds in wallet, not yet invested" |
| 3 | `router.swap(...)` USDC → XLM for exactly `b` | received vs quote, deviation from oracle |
| 4 | `XLM receipt.deposit(user, received)` | collateral, HF, leverage now / target, liquidation price |
| 5+ | Repeat 2–4 until leverage is within 0.1 % of target (3 rounds for 1.7x long and 2x short; see the round table in section 3 for higher short targets) | summary: leverage, HF, liquidation price, APR |

## 6. Swap (Aquarius)

- **Pools:** `router.get_pools(sorted [tokenA, tokenB])` → map pool_index → pool. **Do not use the HTTP `/pools/` API**; it hides live pools.
- **Quote:** `router.estimate_swap(tokens, in, out, pool_index, amount)` for every pool with the exact amount; pick the best output.
- **Oracle guard:** `dev = 1 − (out·price_out)/(in·price_in)`; refuse if |dev| > 1 %.
- **Call:** `router.swap(user, tokens, token_in, token_out, pool_index, in_amount, out_min)`, `out_min = quote · (1 − slippage)`. Simulate first.
- **Auth:** the tree is router.swap → pool.swap → token_in.transfer(user, …, in_amount). Show the wallet exactly this; reject any other authorized call.
- On 24 Nov the best pool alternated between two pools; deviations were −4 to +25 bps.

## 7. Close flow

1. `withdraw(user, pTokens)` of the collateral, sized to cover the debt + 2.5 %, but never below HF 1.25.
2. Swap **exactly the withdrawn amount** to the debt asset. Don't touch the user's other balance.
3. `repay(user, min(walletDebtAsset, get_user_borrow_balance))`.
4. Repeat 1–3 (expect 3 rounds; the first withdrawals are HF-limited).
5. **Interest dust:** `repay` covers the pre-accrual debt, so a little interest remains. Loop
   `repay(user, get_user_borrow_balance(user))` until it reads **0** (usually 1 extra tx).
   Never send more than that balance; **do not use `repay_max`** (fails if interest accrues between simulation and execution).
6. `withdraw(user, all pTokens)` **only when debt is 0 in every market** (otherwise it fails `InvalidAction`) — disable the button until then.

## 8. UI: always visible

- Collateral $, debt $, equity $, **HF** (green ≥ 1.5, amber 1.25–1.5, red < 1.25), leverage, liquidation XLM price.
- Wallet balances; XLM reserve warning below 3.
- **Projected HF for the next step** before signing; block the step if it would go below 1.25.
- Network fee from simulation (~0.005–0.008 XLM per step; an account's very first borrow ~0.19 XLM for storage rent) and steps remaining.

## 9. Test results to match (Mainnet, 24 Nov, ~$10 equity)

| | Long XLM | Short XLM |
|---|---|---|
| Leverage reached | 1.70x | 2.00x |
| HF at open | 1.335 | 1.500 |
| Liquidation price | XLM ≤ $0.0952 | XLM ≥ $0.1899 |
| Transactions | 10 open + 11 close | 10 open + 11 close |
| Round-trip cost | ~1 % (swaps + price move) | ~0.1 % |

Example transactions (stellar.expert/explorer/public/tx/…):
- short open, first borrow: `750d0b0de20ea9eb65713cf61e9b998b47075af6b064775a4876d09cd476fb3c`
- short close, swap: `c2cb9c2165e98c3d957bc43fda01fb064adbba9d3727ad796148fba0c8dfaad8`
- short close, final withdraw: `43854741722dcef7b20413757d6c41b2ac06d608f49aa2f15cd539c0d6d321b0`

The full hash list is in `MAINNET-RESULTS.md` next to this file.

## 10. Out of scope / don't

- No new contracts, no margin controller, no receipt `MarginController` hook (it must stay unset).
- No atomic/one-signature loop (not possible with the current contracts).
- Don't offer long > 1.7x or short > 2.5x; don't loop EURC/PYUSD (PYUSD borrowing is off).
- Test on Mainnet with ≤ $10 first; borrow caps on the markets are small (USDC 900, XLM 4,500 total).
