# Peridot Frontend Integration Guide

This guide describes the current audited Peridot lending deployment and the contract calls a frontend should use. It is written for a Compound-style supply/borrow UI on Stellar Soroban.

## 1. Production Deployment

Mainnet launch is **core lending only**. The live markets are XLM, USDC, and EURC. Margin trading, Aquarius swap adapter flows, and smart-account UX are not part of the current mainnet launch surface.

### 1.1. Mainnet Contract IDs

```ts
export const PERIDOT_MAINNET = {
  networkPassphrase: "Public Global Stellar Network ; September 2015",
  controllerId: "CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX",
  oracleId: "CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN",
  periTokenId: "CDNJSOJKURHQUDBO7OHK7Z64R2CNMIAWXENHM24ALK7Y3H56EU6PUOKR",
  markets: [
    {
      symbol: "XLM",
      vaultId: "CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE",
      underlying: "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
      decimals: 7,
      collateralFactor: 700000n,
      rateModel: "CCPJFBH5WSNZVMCUQCBM4X5334L6ZL3W4Q33XJAK45RCDHJ2JGJ5AP6A",
    },
    {
      symbol: "USDC",
      vaultId: "CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN",
      underlying: "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
      decimals: 7,
      collateralFactor: 900000n,
      rateModel: "CCI5LBBNYOASPQ62GIRY54PDEYWWURJB75HNRAFOU4LTOU3XBC73IB5I",
    },
    {
      symbol: "EURC",
      vaultId: "CD3WN3PLW63HFZXE56OTRLMBV46WG54TFPGRL4RDQ43HQTTWVB4RPO3G",
      underlying: "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
      decimals: 7,
      collateralFactor: 900000n,
      rateModel: "CCI5LBBNYOASPQ62GIRY54PDEYWWURJB75HNRAFOU4LTOU3XBC73IB5I",
    },
  ],
};
```

Use vault IDs for market actions. Use underlying token IDs for wallet balances, oracle price queries, and user-facing asset labels.

### 1.2. Mainnet RPC

```ts
const rpcUrl = "https://YOUR_MAINNET_RPC_PROVIDER";
const networkPassphrase = "Public Global Stellar Network ; September 2015";
```

For testnet development only:

```ts
const rpcUrl = "https://soroban-testnet.stellar.org";
const networkPassphrase = "Test SDF Network ; September 2015";
```

## 2. Contract Roles

- **SimplePeridottroller**: market registry, oracle pricing, collateral factors, account liquidity, pause checks, liquidation coordination, rewards accounting.
- **ReceiptVault**: one vault per asset. Handles pToken mint/burn, `deposit`, `withdraw`, `borrow`, `repay`, pToken `transfer`, caps, reserves, flash loans, and interest accrual.
- **JumpRateModel**: utilization-based borrow/supply rates used by vaults during `update_interest`.
- **PeridotToken (`P`)**: reward token. Rewards are deployed but disabled at launch because reward speeds are zero.

## 3. Generated Clients

Generate TypeScript bindings from the same WASM artifacts used for deployment:

```bash
stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/receipt_vault.wasm \
  --out-dir web/src/contracts/receiptVault

stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/simple_peridottroller.wasm \
  --out-dir web/src/contracts/controller

stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/peridot_token.wasm \
  --out-dir web/src/contracts/peridotToken
```

Generated clients are recommended because they keep argument names and Soroban XDR encoding aligned with the contracts.

## 4. Read-Only Data

### Controller Reads

- `get_admin()`
- `get_oracle()`
- `get_price_usd(token)` -> `Option<(price, scale)>`
- `get_market_cf(market)` -> scaled by `1e6`
- `account_liquidity(user)` -> `(liquidityUsd, shortfallUsd)`
- `get_user_markets(user)` -> entered markets
- `preview_borrow_max(user, market)` -> max additional borrow in underlying base units
- `preview_repay_cap(borrower, repay_market)` -> max repay amount after close-factor rules
- `portfolio(user)` -> `(rows, totals)` where rows are `(market, ptoken_balance, debt, collateral_usd, borrow_usd)`
- `get_accrued(user)` -> accrued PERI rewards

### Vault Reads

- `get_underlying_token()`
- `get_exchange_rate()` -> underlying per pToken, scaled `1e6`
- `get_ptoken_balance(user)`
- `get_user_borrow_balance(user)`
- `get_user_collateral_value(user)`
- `get_available_liquidity()`
- `get_total_deposited()`
- `get_total_ptokens()`
- `get_total_borrowed()`
- `get_total_reserves()`
- `get_total_admin_fees()`

### Pause Reads

For each market vault, call the controller:

- `is_deposit_paused(market)`
- `is_borrow_paused(market)`
- `is_redeem_paused(market)`
- `is_liquidation_paused(market)`

Disable the relevant UI action if the pause read returns true.

## 5. User Flows

All amounts are integer base units. XLM, USDC, and EURC SACs are 7-decimal assets on Stellar mainnet.

### 5.1. Deposit / Supply

Call the vault for the chosen market:

```ts
await vault.deposit({
  user: userAddress,
  amount: 10_0000000n, // 10.0000000 units for 7-decimal assets
});
```

`deposit` calls `user.require_auth()`, transfers underlying from the user to the vault, mints pTokens, accrues interest, and checks deposit pause state through the controller.

### 5.2. Enter Market

After supplying collateral, the user must enter the market for it to count toward cross-market collateral:

```ts
await controller.enter_market({
  user: userAddress,
  market: vaultId,
});
```

`enter_market` requires user auth. If the user only wants to lend without borrowing, entering is optional. If they want collateral value, entering is required.

### 5.3. Withdraw / Redeem

```ts
await vault.withdraw({
  user: userAddress,
  ptoken_amount: pTokenAmount,
});
```

The vault checks pToken balance, market pause state, margin locks if configured, and account liquidity before burning pTokens and returning underlying.

### 5.4. Borrow

```ts
await vault.borrow({
  user: userAddress,
  amount: borrowAmount,
});
```

Before showing the borrow button, query:

```ts
const maxBorrow = await controller.preview_borrow_max({
  user: userAddress,
  market: vaultId,
});
```

Borrow will fail if the user has not entered enough collateral markets, if the target market is paused, if prices are stale/missing, if the borrow cap is reached, or if vault liquidity is insufficient.

### 5.5. Repay

```ts
await vault.repay({
  user: userAddress,
  amount: repayAmount,
});
```

For max repay UX, either pass the displayed borrow balance plus a small buffer or read the current debt with `get_user_borrow_balance`. For liquidation-specific repay caps use `preview_repay_cap`.

### 5.6. Exit Market

```ts
await controller.exit_market({
  user: userAddress,
  market: vaultId,
});
```

Exit only succeeds if removing the market does not create a shortfall.

### 5.7. Rewards

Rewards are disabled at mainnet launch. UI can still show accrued rewards:

```ts
const accrued = await controller.get_accrued({ user: userAddress });
```

When rewards are enabled later, users claim with:

```ts
await controller.claim({ user: userAddress });
```

`claim` requires user auth. `claim_self` exists as a convenience wrapper and also requires user auth. `claim_all` loops over `claim`, so it still requires auth for every user in the batch.

## 6. Health and Risk UI

Use `account_liquidity(user)` for the primary account health signal:

```ts
const [liquidityUsd, shortfallUsd] = await controller.account_liquidity({ user });
```

Recommended display:

- If `shortfallUsd > 0`, account is liquidatable.
- If `liquidityUsd > 0`, account has remaining borrow capacity.
- Health factor can be shown as `collateralUsd / borrowUsd` using `portfolio(user)` totals.

`portfolio(user)` returns collateral already discounted by market collateral factors. Do not apply CF a second time in the frontend.

## 7. Oracle Handling

`get_price_usd(token)` returns `None` when the Reflector oracle price is unavailable, stale, or invalid and no valid fallback exists. UI behavior:

- Show an oracle warning for the affected asset.
- Disable new borrows involving missing-price collateral/debt.
- Avoid displaying stale cached prices as current values.
- Poll prices on page load and refresh every few seconds; Reflector cadence is not a frontend guarantee.

## 8. Liquidation UI / Bots

Core liquidation path:

```ts
await controller.liquidate({
  liquidator,
  borrower,
  repay_market: debtVault,
  collateral_market: collateralVault,
  repay_amount,
});
```

Useful helper:

```ts
await controller.repay_on_behalf_for_liquidator({
  borrower,
  repay_market: debtVault,
  repay_amount,
  liquidator,
});
```

Notes:

- `repay_amount` is in the debt market underlying base units.
- The liquidator must hold enough debt underlying.
- Liquidation is blocked if the controller reports no shortfall or if the liquidation pause is active.

## 9. Smart Accounts

The repository still contains Basic Smart Account contracts and factory contracts, but the current mainnet lending deployment does not require them. Standard wallet-auth flows are enough because vault/controller user functions call `user.require_auth()`.

If you integrate smart accounts later:

- BasicSmartAccount verifies ed25519 signatures.
- WebAuthn/passkeys produce P-256 signatures and cannot directly satisfy the current BasicSmartAccount verifier.
- Recommended UX is passkey-protected ed25519 key storage: use passkey to unlock/decrypt an ed25519 signer, then sign Soroban auth payloads with ed25519.
- Treat smart accounts as a separate integration track from the current core lending launch.

## 10. Margin / Swap Adapter Status

MarginController and SwapAdapter are present in the repository, but they are not wired in `scripts/deploy_mainnet.sh` and should not be exposed in the production UI for this launch.

If/when margin is enabled later, update this guide with the deployed mainnet IDs and use the audited margin-specific flows at that time. Do not reuse old testnet IDs or old Aquarius route examples for mainnet.

## 11. Frontend Checklist

- [ ] Configure a production mainnet RPC provider.
- [ ] Use `Public Global Stellar Network ; September 2015` as the mainnet network passphrase.
- [ ] Generate TypeScript bindings from current WASMs.
- [ ] Copy `PERIDOT_MAINNET` into frontend config.
- [ ] Implement wallet connection and Soroban transaction signing.
- [ ] Build reusable services for `deposit`, `withdraw`, `enter_market`, `borrow`, `repay`, `exit_market`, `claim`.
- [ ] Render market APY, exchange rate, pToken balance, borrow balance, and liquidity.
- [ ] Disable UI actions based on pause states and oracle availability.
- [ ] Use `preview_borrow_max` before borrow and `account_liquidity` / `portfolio` for health UI.
- [ ] Keep margin/swap UI disabled until mainnet margin contracts are explicitly deployed and verified.

## 12. Verification Command

After deployment, verify the live mainnet config with:

```bash
CTRL_ID=CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX \
VA_ID=CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE \
VB_ID=CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN \
VC_ID=CD3WN3PLW63HFZXE56OTRLMBV46WG54TFPGRL4RDQ43HQTTWVB4RPO3G \
IDENTITY=peridot-mainnet \
bash scripts/verify_mainnet.sh
```
