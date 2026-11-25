# Frontend handoff (single file): leverage loop ("Open up to 2.5x short XLM / 1.7x long XLM") on Peridot cash markets

**Date:** 24 Nov 2026, **updated 25 Nov 2026** · **Network:** Stellar Mainnet · **Status:** contracts are live and
unchanged; the loop was tested end to end on Mainnet (long 1.70x and short 2.00x, opened and closed, 47 transactions,
all SUCCESS). **Nothing on-chain is new for this feature.** It is built from existing calls.

**This one file is everything you need.** Appendix A: protocol-30 / JS SDK note. Appendix B: the reference
implementation `loop-core.cjs` (full source). Appendix C: every Mainnet test transaction hash.

## 0. Update 25 Nov: read first

1. **Protocol 30 (live on Mainnet since 23 Nov).** The app's `@stellar/stellar-sdk` **17.4.0 is fine**: it decodes the
   new ledger header (checked against Mainnet with the app's own `node_modules`). Don't downgrade it. If production
   was built from a commit older than `390cf93` (25 Oct, the 13.3.0 -> 17.4.0 switch), redeploy from `main`.
   Details and the raw-JSON fallback: Appendix A.
2. **Urgent before the SDF test: the group-1 keeper** (`keeper/liquidator/margin` in `peridot-markets`, unit
   `peridot-keeper-group1`) locks SDK **16.3.1** and calls `getLatestLedger()` every cycle (`stellar.mjs:61`, from
   `keeper.mjs:49`/`:89` and `lending.mjs:160`). On protocol 30 that throws
   `XDR Read Error: unknown StellarValueType member for value 3`, so `refresh_boosted_underlying` / `update_interest`
   stop and borrows in the app can fail on stale state. Check its journal for that error; fix = bump to **16.4.1**
   (drop-in for 16.x; don't jump to 17.x without rework) or use the raw `getLatestLedger` in Appendix A.
3. **Addresses:** accept only `G...` user accounts (muxed `M...`/`W...` are rejected by every lending call). Use the
   ledger `sequence`, never `closeTime`, to detect a new ledger (two ledgers can now share a `closeTime`).
4. **Swap guard (section 6) in fast markets.** Measured 25 Nov: the Aquarius pool and the Reflector oracle usually agree
   within ~0.1 % (median 8.7 bps, p99 63 bps), but right after a fast XLM move the oracle (5-min rounds) can lag the
   pool by ~1 % for a few minutes. When the 1 % guard refuses a step, show "price moving, retry in a few minutes" and
   re-quote after the next oracle round; never widen the guard automatically. The direct Aquarius concentrated pool is
   the cheapest route at every size we tested (other Aquarius pools 2-5 bps worse; Soroswap/Phoenix ~1.3 % at $15).
5. **Reference code** is in Appendix B (also on branch `docs/leverage-loop-frontend` in the Peridot-Soroban repo,
   `research/cash-borrowing-activation-20261117/tools/loop-core.cjs`).
6. **Coming, not live yet: pooled margin up to 5x** (one-signature open/close, backstop pool). Don't build against it
   yet; a separate handoff follows when it opens (default leverage in the UI will be 3x).

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

**Reference implementation:** `loop-core.cjs`, full source in **Appendix B** (137 lines, pure JS, no I/O)
— `health`, `nextBorrow`, `nextWithdraw`, `swapGuard`, `bestPool`, `projectOpen`, `projectClose`, `maxSafeLeverage`.
Port it unchanged. The chain reads, simulation and transaction building are what sections 5–7 describe:
before each step read pTokens × exchange rate, `get_user_borrow_balance`, wallet balance, `controller.get_price_usd`,
`controller.get_market_cf`, build the one call, simulate, show the result, then ask for the signature.

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
   Never send more than that balance (an overpay makes the signed transfer amount depend on accrual timing);
   **do not use `repay_max`** (fails if interest accrues between simulation and execution).
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

The full hash list is in Appendix C.

## 10. Out of scope / don't

- No new contracts, no margin controller, no receipt `MarginController` hook (it must stay unset).
- No atomic/one-signature loop (not possible with the current contracts).
- Don't offer long > 1.7x or short > 2.5x; don't loop EURC/PYUSD (PYUSD borrowing is off).
- Test on Mainnet with ≤ $10 first; borrow caps on the markets are small (USDC 900, XLM 4,500 total).

---

## Appendix A. JS SDK and protocol 30 (24 Nov, verified 25 Nov)

Stellar Mainnet and Testnet run **protocol 30** since 23 Nov. Its new ledger-header format breaks
`@stellar/stellar-sdk` **16.3.x and older** on every call that decodes a ledger header:
`rpc.getLatestLedger()`, `rpc.getLedgers()`, `getAssetBalance()` for G-addresses, `AssembledTransaction.signAuthEntries()`
without an explicit expiration, and any `LedgerHeader` / `LedgerCloseMeta` `fromXDR` (incl. Horizon `header_xdr`).
Error: `XDR Read Error: unknown StellarValueType member for value 3`.
SDK 13.x is worse: `getTransaction(s)` / `pollTransaction` already fail since protocol 23.

### What to use

| You are on | Do |
|---|---|
| **17.4.0** (the live `peridot-markets` app today) | **Nothing.** Verified: `getLatestLedger` header decodes. Don't downgrade. |
| 17.0–17.2.x | Upgrade to **17.4.0** (17.3.0 is the first fixed 17.x). |
| 16.x | Upgrade to **16.4.1** — drop-in, same XDR API (16.4.0 is the fix; 16.4.1 adds an axios security bump). |
| 13.x–15.x (e.g. the legacy `peridot-dashboard`) | Move to **16.4.1** or 17.4.0; test `getTransaction` / `pollTransaction` paths. |

Don't mix: 17.x rebuilt `xdr` (plain objects instead of `.switch()` / `.header()` accessors, `Uint8Array` instead of
`Buffer`, Node ≥ 22.12, auth v2 default). Code written against 16.x `xdr.*` accessors needs changes on 17.

### Workaround if you can't upgrade yet

Read the latest ledger as raw JSON-RPC (no header decode):

```js
async function latestLedger(rpcUrl) {
  const r = await (await fetch(rpcUrl, {method: 'POST', headers: {'content-type': 'application/json'},
    body: JSON.stringify({jsonrpc: '2.0', id: 1, method: 'getLatestLedger'})})).json();
  if (r.error) throw new Error(JSON.stringify(r.error));
  return {sequence: r.result.sequence, closeTime: Number(r.result.closeTime), protocolVersion: r.result.protocolVersion};
}
```

`simulateTransaction`, `assembleTransaction` / `prepareTransaction`, `sendTransaction`, `getTransaction`,
`getEvents`, `getLedgerEntries`, `getAccount`, `getNetwork`, `getFeeStats` work on 16.3.x.

### Two protocol-30 details for the UI

- **Muxed addresses (CAP-84):** accept only **G…** addresses as the user account. Reject `M…` / `W…` with a clear
  message — every lending call (deposit, withdraw, borrow, repay, enter_market) takes a plain address and rejects
  muxed ones. SDK 16.4.x `Address` / `scValToNative` also throw on `W…`.
- **Timestamps (CAP-88):** two consecutive ledgers can now have the same whole-second `closeTime`. Don't use
  `closeTime` to detect "a new ledger" or as a unique key — use the ledger `sequence`.

### Update 25 Nov: verified status

- **App (`peridot-markets`): OK.** `app/package-lock.json` locks 17.4.0 on `main` and every remote branch; the wallet kit
  (`@creit.tech/stellar-wallets-kit` 2.7.1) dedupes to the same 17.4.0; `@stellar/freighter-api` 6.x carries no SDK.
  A read-only Mainnet probe with the app's own install decodes the protocol-30 header (`baseReserve` 5000000,
  `baseFee` 100, as `trustline.ts:82` needs). Header call sites `rpc.ts:193,238`, `control-client.ts:77`,
  `trustline.ts:72/82` are fine on 17.4.0. User addresses are already G-only; `closeTime` is only a 120 s staleness bound.
- **Group-1 keeper (`keeper/liquidator/margin`): broken by code.** It locks 16.3.1 on main, demo-merge,
  keeper-all-groups and keeper-inclusion-fee, and calls `getLatestLedger()` every cycle (`stellar.mjs:61`). Reproduced
  with its own install. Fix: 16.4.1, or the raw `getLatestLedger` above. Check its journal first.
- **Legacy `peridot-dashboard`** is on 13.3.0 (also breaks `getTransaction`/`pollTransaction`); no deployment found.

---

## Appendix B. Reference implementation: `loop-core.cjs` (full source)

Pure functions, BigInt raw 7-decimal amounts, prices as `{p, s}` from `controller.get_price_usd`. `CAPS` are the
test tool's own small per-transaction caps (a ~$10 test); in the app, size from the user's input and the market's caps.

```js
'use strict';
// Pure sizing / health / slippage / sequencing logic for loop.cjs (v0 leverage loop). No I/O; unit-tested offline.
// Units: every token amount is a raw 7-decimal integer (BigInt). A price is {p, s}: USD per raw unit = p / s, as returned
// by controller.get_price_usd(token) -> (price, scale). USD values are raw 7-decimal USD (BigInt). CF is 1e6-scaled.
//
// Health factor (HF) = sum(collateral_usd * CF) / debt_usd. The controller reports shortfall > 0 (liquidatable) exactly
// when HF < 1. A loop is NOT atomic (the live liquidator can act between any two transactions), so every single step,
// including a borrow that is not yet redeposited, must keep HF >= hfMin.
const SCALE_CF = 1_000_000n;
const SIDES = Object.freeze({
  long: {coll: 'XLM', debt: 'USDC', label: 'long XLM (XLM collateral, USDC debt)'},
  short: {coll: 'USDC', debt: 'XLM', label: 'short XLM (USDC collateral, XLM debt)'},
});
// The loop tool's own caps (raw). Independent of team.cjs caps; deliberately small: this is a ~$10 test, not a product.
const CAPS = Object.freeze({
  deposit: {XLM: 1_500_000_000n, USDC: 250_000_000n},   // 150 XLM / 25 USDC per transaction
  borrow: {XLM: 900_000_000n, USDC: 120_000_000n},      // 90 XLM / 12 USDC per transaction
  swapIn: {XLM: 1_500_000_000n, USDC: 250_000_000n},
});
const usd = (raw, px) => BigInt(raw) * BigInt(px.p) / BigInt(px.s);
const fromUsd = (v, px) => BigInt(v) * BigInt(px.s) / BigInt(px.p);
const pct = (bps) => Number(bps) / 100;

// pos = {XLM: {coll, debt, cf, px}, USDC: {...}} (coll = underlying raw, i.e. pTokens * exchangeRate / 1e6)
function health(pos) {
  let collUsd = 0n, weightedUsd = 0n, debtUsd = 0n;
  for (const m of Object.values(pos)) {
    const c = usd(m.coll, m.px);
    collUsd += c; weightedUsd += c * BigInt(m.cf) / SCALE_CF; debtUsd += usd(m.debt, m.px);
  }
  const equityUsd = collUsd - debtUsd;
  return {collUsd, weightedUsd, debtUsd, equityUsd, hf: debtUsd === 0n ? Infinity : Number(weightedUsd * 1_000_000n / debtUsd) / 1e6,
    leverage: equityUsd > 0n ? Number(collUsd * 1_000_000n / equityUsd) / 1e6 : Infinity,
    liquidityUsd: weightedUsd > debtUsd ? weightedUsd - debtUsd : 0n, shortfallUsd: debtUsd > weightedUsd ? debtUsd - weightedUsd : 0n};
}
const hfStr = h => h === Infinity ? 'inf' : h.toFixed(3);
// Extra USD debt that keeps HF >= hfMin: weighted / hfMin - debt (floored at 0). hfMin as Number, converted to 1e6.
function maxExtraDebtUsd(pos, hfMin) {
  const h = health(pos), k = BigInt(Math.round(hfMin * 1e6));
  const cap = h.weightedUsd * 1_000_000n / k;
  return cap > h.debtUsd ? cap - h.debtUsd : 0n;
}
// Asymptotic leverage limit when every step must keep HF >= hfMin: L* = hfMin / (hfMin - CF).
function maxSafeLeverage(cf, hfMin) { const c = Number(cf) / 1e6; return hfMin > c ? hfMin / (hfMin - c) : Infinity; }
const clone = pos => Object.fromEntries(Object.entries(pos).map(([k, v]) => [k, {...v}]));
function apply(pos, ev) {
  const p = clone(pos), m = p[ev.market];
  if (ev.action === 'deposit') m.coll = BigInt(m.coll) + BigInt(ev.amount);
  else if (ev.action === 'borrow') m.debt = BigInt(m.debt) + BigInt(ev.amount);
  else if (ev.action === 'repay') m.debt = BigInt(m.debt) > BigInt(ev.amount) ? BigInt(m.debt) - BigInt(ev.amount) : 0n;
  else if (ev.action === 'withdraw') m.coll = BigInt(m.coll) > BigInt(ev.amount) ? BigInt(m.coll) - BigInt(ev.amount) : 0n;
  else throw Error('apply: ' + ev.action);
  return p;
}
// Borrow amount for the next round: the remaining gap to target leverage, clamped so HF stays >= hfMin right after
// the borrow (before the swapped proceeds are redeposited). Target debt = (L - 1) * equity.
function nextBorrow(pos, side, lev, hfMin) {
  const d = SIDES[side].debt, h = health(pos), px = pos[d].px;
  const targetDebtUsd = h.equityUsd * BigInt(Math.round((lev - 1) * 1e6)) / 1_000_000n;
  const gapUsd = targetDebtUsd > h.debtUsd ? targetDebtUsd - h.debtUsd : 0n;
  const safeUsd = maxExtraDebtUsd(pos, hfMin);
  const limited = gapUsd > safeUsd;
  let amount = fromUsd(limited ? safeUsd : gapUsd, px);
  const capped = amount > CAPS.borrow[d]; if (capped) amount = CAPS.borrow[d];
  return {amount, market: d, limitedByHf: limited, capped, targetDebtUsd, gapUsd};
}
// Swap guard: quote vs the controller's oracle. ratio = USD out / USD in at oracle prices (1.0 = oracle-fair, before
// fees). Refuse when |1 - ratio| > maxDevBps. outMin = quote * (1 - slippageBps).
function swapGuard({amountIn, quoteOut, pxIn, pxOut, slippageBps = 100, maxDevBps = 100}) {
  const vin = usd(amountIn, pxIn), vout = usd(quoteOut, pxOut);
  if (vin <= 0n || BigInt(quoteOut) <= 0n) throw Error('swap: zero amount or zero quote');
  const devBps = Number((vin - vout) * 10000n / vin);   // positive = worse than oracle
  if (Math.abs(devBps) > maxDevBps) throw Error(`swap refused: pool quote deviates ${devBps} bps from the oracle (max ${maxDevBps})`);
  if (slippageBps < 1 || slippageBps > 300) throw Error('slippage-bps must be 1..300');
  const outMin = BigInt(quoteOut) * BigInt(10000 - slippageBps) / 10000n;
  return {outMin, devBps, oracleOut: fromUsd(vin, pxOut)};
}
// Pick the pool with the largest exact-amount quote (quotes = [{index, pool, out}]).
function bestPool(quotes) {
  const ok = quotes.filter(q => BigInt(q.out) > 0n);
  if (!ok.length) throw Error('no pool quotes a positive output');
  return ok.reduce((a, b) => BigInt(b.out) > BigInt(a.out) ? b : a);
}
// Close: pTokens-equivalent underlying to withdraw from the collateral market next. Covers the remaining debt plus
// buffer (swap fees/slippage + dust), but never more than keeps HF >= hfMin. 0 debt -> everything.
function nextWithdraw(pos, side, hfMin, bufferBps = 250) {
  const {coll: c, debt: d} = SIDES[side], h = health(pos), m = pos[c];
  if (h.debtUsd === 0n) return {amount: BigInt(m.coll), all: true};
  const need = fromUsd(usd(pos[d].debt, pos[d].px) * BigInt(10000 + bufferBps) / 10000n, m.px);
  // Safe: CF_c * (coll_c - w) * px + otherWeighted >= hfMin * debt  ->  w <= coll_c - (hfMin*debt - otherWeighted)/(CF*px)
  const k = BigInt(Math.round(hfMin * 1e6)), reqWeighted = h.debtUsd * k / 1_000_000n;
  const otherWeighted = h.weightedUsd - usd(m.coll, m.px) * BigInt(m.cf) / SCALE_CF;
  const ceil = (a, b) => (a + b - 1n) / b;
  const reqLocalUsd = reqWeighted > otherWeighted ? ceil((reqWeighted - otherWeighted + 1n) * SCALE_CF, BigInt(m.cf)) : 0n;
  const reqLocal = ceil(reqLocalUsd * BigInt(m.px.s), BigInt(m.px.p)) + 100n;   // round against the user (+100 raw margin)
  const safe = BigInt(m.coll) > reqLocal ? BigInt(m.coll) - reqLocal : 0n;
  const amount = need < safe ? need : safe;
  return {amount, all: false, limitedByHf: safe < need, need, safe};
}
// Offline projection of the whole open/close sequence (used by `plan` and the tests). feeBps = swap loss vs oracle.
function projectOpen({side, lev, hfMin = 1.25, equity, pos0, feeBps = 30, maxRounds = 8}) {
  const {coll: c, debt: d} = SIDES[side];
  let pos = clone(pos0); const steps = [];
  const push = (ev, note) => { const before = health(pos); pos = apply(pos, ev); const after = health(pos);
    steps.push({...ev, hfBefore: before.hf, hfAfter: after.hf, levAfter: after.leverage, note}); };
  push({action: 'deposit', market: c, amount: BigInt(equity)}, 'equity');
  for (let r = 1; r <= maxRounds; r++) {
    const b = nextBorrow(pos, side, lev, hfMin);
    if (b.amount <= 0n || b.gapUsd * 1000n < health(pos).equityUsd) break;     // within 0.1% of equity: done
    push({action: 'borrow', market: d, amount: b.amount}, `round ${r}${b.limitedByHf ? ' (HF-limited)' : ''}`);
    const out = fromUsd(usd(b.amount, pos[d].px) * BigInt(10000 - feeBps) / 10000n, pos[c].px);
    steps.push({action: 'swap', from: d, to: c, amount: b.amount, estOut: out, hfBefore: health(pos).hf, hfAfter: health(pos).hf, levAfter: health(pos).leverage});
    push({action: 'deposit', market: c, amount: out}, `round ${r}`);
  }
  if (steps.filter(s => s.action === 'borrow').length === maxRounds) throw Error('did not converge');
  return {steps, final: health(pos), pos, borrows: steps.filter(s => s.action === 'borrow').length};
}
function projectClose({side, hfMin = 1.25, pos0, feeBps = 30, maxRounds = 10}) {
  const {coll: c, debt: d} = SIDES[side];
  let pos = clone(pos0); const steps = [];
  const push = (ev, note) => { const before = health(pos); pos = apply(pos, ev); const after = health(pos);
    steps.push({...ev, hfBefore: before.hf, hfAfter: after.hf, note}); };
  for (let r = 1; r <= maxRounds && BigInt(pos[d].debt) > 0n; r++) {
    const w = nextWithdraw(pos, side, hfMin);
    if (w.amount <= 0n) throw Error(`close stuck: HF ${hfStr(health(pos).hf)} leaves no safe withdrawal; repay from wallet first`);
    push({action: 'withdraw', market: c, amount: w.amount}, `round ${r}${w.limitedByHf ? ' (HF-limited)' : ''}`);
    const out = fromUsd(usd(w.amount, pos[c].px) * BigInt(10000 - feeBps) / 10000n, pos[d].px);
    steps.push({action: 'swap', from: c, to: d, amount: w.amount, estOut: out, hfBefore: health(pos).hf, hfAfter: health(pos).hf});
    const rep = out < BigInt(pos[d].debt) ? out : BigInt(pos[d].debt);
    push({action: 'repay', market: d, amount: rep}, `round ${r}`);
  }
  if (BigInt(pos[d].debt) > 0n) throw Error('close did not converge');
  push({action: 'withdraw', market: c, amount: BigInt(pos[c].coll)}, 'remainder (debt 0)');
  return {steps, final: health(pos), pos};
}
module.exports = {SIDES, CAPS, SCALE_CF, usd, fromUsd, pct, health, hfStr, maxExtraDebtUsd, maxSafeLeverage, apply, nextBorrow, swapGuard,
  bestPool, nextWithdraw, projectOpen, projectClose};
```

---

## Appendix C. Mainnet test results (24 Nov 2026)

Team account T1 `GA7X3VGKFRLQXZCJUOAXDBGM7NDEW7XGB47YNIJENKMIPUUD5F7U6OOJ` (Ledger HD 1). Live cash controller
CBMRS7…JRK (f41e5260), receipts 5630a701, receipt MarginController hook unset. Aquarius swaps with 1 % slippage bound.
Every transaction: SUCCESS. Explorer: `https://stellar.expert/explorer/public/tx/<hash>`.

### Summary

| Position | Opened at | HF at open | Liquidation | Close | Round-trip cost |
|---|---|---|---|---|---|
| Long XLM (XLM collateral, USDC debt) | 1.70x | 1.335 | XLM falls to $0.0952 | debt 0 | equity $6.60 → $6.53 (~1 %, swaps + XLM price move) |
| Short XLM (USDC collateral, XLM debt) | 2.00x | 1.500 | XLM rises to $0.1899 | debt 0 | ~$16.54 → ~$16.52 total wallet value (~0.1 %) |

Health factor never went below 1.25 on any step (non-atomic loop rule).
Final T1 state: 0 collateral, 0 debt, wallet 60.0036 XLM + 8.9239 USDC.

### Setup

| Step | Hash |
|---|---|
| l01 fund T1 (admin) | 08020e686ec5a0f47518e647fcc082e57fe26d77b9bc6c36b27305da49a27473 |
| l02 T1 USDC trustline | 2fe95af3b101dcd3bf3d92393b405ce45b76b12484293728f1be4ec8f7648ae7 |

### Long open (1.70x)

| Step | Hash |
|---|---|
| l10 deposit XLM | 6a1cf1c814627a66edfcca96a2212a01e470af4df02539ce49db06b249720ac8 |
| l11 borrow USDC | 69fab4d20bb022276c7d5abeb401fdd0b6301439728156872760adb0fc3c9b54 |
| l12 swap USDC→XLM | d0b1869ed5d2c6eb735a3c802b80c54dc05e02ceb2f6b8efdd55bad4361947f8 |
| l13 deposit XLM | 9823cb74b15e160df3d8d46f0e6c048e6544e41a24cd8578f1994043b2016698 |
| l14 borrow USDC | 79d05e4dd2b9b65ed96b145878c715e3705ee3109495201a2455df79fcdd93f8 |
| l15 swap | 93e48f09a163467cb381c2509ba90fd4bf6aee48eac3fe15a534cc5089cd7513 |
| l16 deposit | e1e5c8ac7e450036e8ac85c237a15e6256856e969d3231ad18aaede0d49980d7 |
| l17 borrow | d1e01b4a2d13a1c65a94fc2f1cca79973da8c7c098ba33d933ea4b07e7191b02 |
| l18 swap | d2fb1bfbba5c900e097f79f902b6872b8677b700f78ba732be9aeb6918c4d273 |
| l19 deposit | 7b021b330c0a31b539009ebc48b63e2556d0fe0771223e350898b66b1ec3ffc2 |

### Long close

| Step | Hash |
|---|---|
| l20 withdraw XLM | 3ca9c74d042ff8c1c84a7ed71098666aaf84ce4957024980c8653b7a5ac483a5 |
| l21 swap XLM→USDC | c3be0b61efcea02240d9eb51435ee660dc54f55b32943b420385eb35f02b89d0 |
| l22 repay USDC | 9cc114f98e277e8896fa583513a5ca686d1caec300b037c5f7f28a38717eec51 |
| l23 withdraw | 9d955f8019d0790a6e003d6abf03089afc589d901a29de06bd6f756379b2389d |
| l24 swap | 308645f1c08aa099125dfad5db6e0de811860633f339210a10989c514e64a1da |
| l25 repay | a1dce2cbe9645cc6ac0528e2f98b715763f1a253625d788a28cf9fd05ccde6d2 |
| l26 withdraw | 754f8f9db2fc512b5b45096a434e5a3304c464929c0c49878e23960af62262bd |
| l27 swap | ec4a532614c8f5d12f853c443c5b22dc6c3808e89df8712647b1c622fc9fb7d6 |
| l28 repay | 612d328577e34359e8ed08a16c37f629bdf3de7bdc2bdd4b03ae03c19244dc62 |
| l29 repay interest dust | 3094cb7a496541217cd6866c55de3c413988ac4fe37d8ed93478d01aa4b7a2d6 |
| l30 withdraw all XLM | 34552ff2295ffaa622d8df11595794cac7de9b17621368fc95897b208518ec95 |

### Short open (2.00x)

| Step | Hash |
|---|---|
| l40 pay T1 9 USDC (admin) | cf5f75e79ac8f2bd386566068aedf58fe7037cd80e2314304e39ef71848e713e |
| l41 deposit USDC | 1263f0c68eb73c983fc664b1616cf839c6eee22da2daa41c60006ca808d56afc |
| l42 borrow 42.83 XLM | 750d0b0de20ea9eb65713cf61e9b998b47075af6b064775a4876d09cd476fb3c |
| l43 swap XLM→USDC | 57392c4d9417be07ea34312919277c50e242d15a083af4ed3f96b849ea0e6741 |
| l44 deposit USDC | 1d732d45b11c541c5a511a13fe8dc0d0f0f4b98644bb5fa018c7632ffec1d449 |
| l45 borrow 25.71 XLM | 4dde82ad3a142f546bc405a513f8c293c9e2a72be957209ca435a03c021b1de4 |
| l46 swap | 367c7366889d4bf4ed06c23ac342010111790ac65458ce71a7bd2671f68cc644 |
| l47 deposit | e45a305330d37ae20d05952e0673831a938f8bb52e901bba54a7856e0e201205 |
| l48 borrow 2.79 XLM | c391b387af7e8f7903ea18c0435877e8db60e9de667f8a99aba9544508b74fd9 |
| l49 swap | b5a4e02d88fb9a31c6fdd3470fdfa972f65ca36e82c9cde3da77698812085d47 |
| l50 deposit | e37f4dbf5bb986b40519d5507bf6c9df86d182b8c0489fb94ad388e44f5a70cd |

### Short close

| Step | Hash |
|---|---|
| l60 withdraw USDC | c9df16ec2612fede71c4d35d07955573b90479c52a01ecb8eeab69a7a58ce6fd |
| l61 swap USDC→XLM (−4 bps vs oracle) | c2cb9c2165e98c3d957bc43fda01fb064adbba9d3727ad796148fba0c8dfaad8 |
| l62 repay XLM | d7ca4eec8733a6f55639d2f8ab65e16dd3d67335afda8ff7dd10baebbdc330e9 |
| l63 withdraw | b284b05a76335fe0fb0f79d57c79ac37cf1f3b9e04c8cf0463cb01ca13f000e3 |
| l64 swap (19 bps) | 201395e5edf593c670ba043ced0b2b60ef55019d9bbb3bd148f1b75939d0694e |
| l65 repay | 8d9bd9cd57433ebafc5f3eb6aad103b0a0f5a77cf2ad62c936fdf6952bcc79fe |
| l66 withdraw | 9ad8d71672759e43929b5ef2d22fbb8b65f6381dbc483eb8b9cfe729f0e2a1a3 |
| l67 swap (25 bps) | 2c191788fe5cdf3dbabfaaa61607f32be86cae594934369d84ef91bd1da65a1d |
| l68 repay | b8c1ff36c75996418298d0002470e4b32d0ca6dddc14915aec49139247e0def6 |
| l69 repay interest dust | 7e91c73a11275d50748250a7520fcdec489aa7d34fdc246f882827ac345c0577 |
| l70 withdraw all USDC | 43854741722dcef7b20413757d6c41b2ac06d608f49aa2f15cd539c0d6d321b0 |
