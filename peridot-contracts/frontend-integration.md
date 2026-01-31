# Peridot Frontend Integration Guide

This document explains how the frontend can interact with the Peridot Soroban contracts in a Compound-style UI. It assumes you are comfortable with TypeScript/React (or Astro) and have the Stellar CLI installed.

## 1. High-Level Architecture

- **Controller (`SimplePeridottroller`)** – central risk engine. Contract ID: `CCBAEMMG4STILW6SYTNCIVG44OF4TQDDCYPU7GS3ZOEKLTC75ONTLCI2`
- **ReceiptVault (XLM market)** – contract ID: `CCHBN5RRP7KH4O7ICSIQTSYFFZBYFEBCF35UOQBGDI7GZZKKWXWVLLPX`
- **ReceiptVault (USDT market)** – contract ID: `CBP2U7FVTQ2EIAQ474CTYN74KCEU6YLCCGH6KRY2RAMQEDSKREKSAGSO`
- **Jump Rate Model** – contract ID: `CCIDO7HBNBPUKFWEI3PRA6O6QU2JXUKVIZAERCZWBNGGK7LO7MFBKKOA`
- **Peridot Reward Token (`P`)** – contract ID: `CBCA56UIBQA3WT2JUIIG2BHW325CMLNAC7CKL33T37GHN25RCGR6SXPB`
- **Mock USDT (open mint)** – contract ID: `CDBWTU527WNACRCET2NF6RZFQ3WAPJOQM3OQ5VLUNHJRDQ6ICVO2JTJP`
- **Reflector Oracle** – contract ID: `CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63`
- **Swap Adapter (Aquarius router wrapper)** – contract ID: `CAGLARN3MUMRGCRNKXZ3SH7NVCZ3P3CDGHL2FQEEXIC4MPAGTQTACY6S`
- **Margin Controller (true margin trading)** – contract ID: `CAZQWGJDKG2JQYV66VV3ONBDLYAE77YVKSBUNWUY7MV6WVLLHT4URFX7`
- **Soroswap Router (AMM)** – `CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD`
- **Soroswap Factory** – `CDP3HMUH6SMS3S7NPGNDJLULCOXXEPSHY4JKUKMBNQMATHDHWXRRJTBY`
- **Soroswap Aggregator** – `CC74XDT7UVLUZCELKBIYXFYIX6A6LGPWURJVUXGRPQO745RWX7WEURMA`

Your frontend will mainly call the controller and the vault contracts. The controller handles account liquidity checks, oracle pricing, and incentives; each vault exposes ERC20-like `deposit`, `withdraw`, `borrow`, `repay`, and `transfer` entrypoints.

## 2. Generated Clients (Recommended)

Use `stellar contract bindings` to generate TypeScript clients for each contract:

```bash
# From the repo root after building WASMs
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

Each generated package exports a class with strongly typed methods. Import them into your frontend and pass the contract ID plus RPC details.

## 3. RPC Configuration

Use the public testnet RPC endpoint:

```ts
const rpcUrl = "https://soroban-testnet.stellar.org";
const networkPassphrase = "Test SDF Future Network ; October 2022";
```

When using Freighter, you can derive the user’s public key and sign transactions locally. The generated clients provide helpers to build and submit transactions; otherwise you can roll your own using `@stellar/stellar-sdk`.

## 4. Core Interaction Flows

### 4.1. Read Market Data (no signature)

- Controller

  - `get_price_usd(token: Address)` → current USD price scaled by `10^decimals`
  - `account_liquidity(account: Address)` → [liquidity, shortfall] in USD
  - `get_market_cf(market: Address)` → collateral factor (scaled 1e6)

- Vault
  - `get_exchange_rate()` → underlying per pToken (scaled 1e6)
  - `get_ptoken_balance(account: Address)`
  - `get_user_borrow_balance(account: Address)`
  - `get_available_liquidity()` → currently borrowable assets

Use these to build supply/borrow tables, account dashboards, and health meters.

### 4.2. Supply / Mint pTokens

1. User approves vault to withdraw underlying
2. Call `deposit(user: Address, amount: u128)` on the vault

Example (TypeScript using generated client):

```ts
const vault = new ReceiptVaultClient({
  contractId: "CCBRKJ5ZZZ...",
  networkPassphrase,
  rpcUrl,
});

await vault.deposit(
  { user: userAddress, amount: BigInt(1_000_000) },
  { signer: freighterSigner }
);
```

### 4.3. Redeem / Withdraw

Call `withdraw(user, ptoken_amount)` on the same vault. The method handles collateral checks via the controller (if the user has borrow positions).

### 4.4. Borrow

Call `borrow(user, amount)` on the vault. The controller’s `hypothetical_liquidity` is invoked internally to ensure the resulting position is safe. Display the user’s max borrow by reading `preview_borrow_max`.

### 4.5. Repay

Call `repay(user, amount)` with the underlying asset amount. For “max repay,” pass a high number or call `preview_repay_cap` on the controller first.

### 4.6. Rewards

To show accrued rewards and allow claiming:

```ts
const controller = new SimplePeridottrollerClient({
  contractId: "CAWEZ...",
  networkPassphrase,
  rpcUrl,
});

const accrued = await controller.get_accrued({ user: userAddress });
await controller.claim_self({ user: userAddress }, { signer: freighterSigner });
```

`claim_self` mints `P` tokens to the user via the controller→token hook.

## 5. Supported Assets

Store the known asset addresses in your config file:

```ts
export const markets = [
  {
    symbol: "XLM",
    vaultId: "CCPQYPFNAGQPQTMPAEBGNPNSQJ4FAJYPX6WLYBKE5SO5ZONXANCUEYE7",
    underlying: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
    decimals: 7,
  },
  {
    symbol: "USDT",
    vaultId: "CDM37TMZO2QQQP6CIMU7E6OIBR6IQMM46P5PCSQ5D7AX6GMEFQX7NTKL",
    underlying: "CBX3DOZH4HUR3EJS6LAKHXN6RARXKMUT33OUMVVSUW5HCXEIECD4WT75",
    decimals: 6,
  },
];
```

Use these IDs for price queries and UI labels.

## 6. Health Factor Calculation

To display the Compound-style health factor:

1. Fetch collateral USD and borrow USD via `account_liquidity`
2. Compute `healthFactor = collateralUSD / borrowUSD`
3. Highlight positions where `borrowUSD > collateralUSD` (shortfall > 0).

For per-market views, call `portfolio(user)` on the controller to get `[market, ptoken_balance, borrow_balance, collateral_usd, borrow_usd]` for each entered market.

## 7. Oracle Considerations

The controller calls the Reflector oracle synchronously. If `get_price_usd` returns `None`, the UI should warn that the oracle is stale or missing. Poll once per page load and cache responses for a few seconds; the oracle updates roughly every 5 minutes.

## 8. Leveraged Margin (True Borrow/Swap)

Margin trading uses real vault borrows + AMM swaps (Soroswap) coordinated by `margin-controller` and `swap-adapter`.

Core calls:
- `open_position(user, collateral_asset, base_asset, collateral_amount, leverage, side, path, amount_out_min, deadline)`
- `open_position_no_swap(user, collateral_asset, debt_asset, collateral_amount, borrow_amount, leverage, side)`
- `close_position(user, position_id, path, amount_out_min, deadline)`
- `liquidate_position(liquidator, position_id)` (liquidation uses peridottroller liquidation + vaults)

Key notes for frontend engineers:
- **`path`** is the Soroswap route vector of token addresses (e.g., `[USDC, XLM]`).
- **`amount_out_min`** enforces user slippage. Compute `amount_out_min = quoted_out * (1 - slippage_bps/10_000)`.
- **`deadline`** is a unix timestamp cutoff for swap execution (e.g., now + 5 minutes).
- **`side`** is `Long` or `Short`.

Recommended UX flow for open/close:
1. Fetch best route off-chain (Soroswap API) and extract `path`.
2. Compute `amount_out_min`.
3. Call `open_position`/`close_position` with `path`, `amount_out_min`, `deadline`.

Budget‑safe open flow (recommended for testnet limits):
1. User swaps USDC→XLM directly via Soroswap router (outside MarginController).
2. Call `open_position_no_swap` to deposit XLM collateral and borrow USDC (no router call).

CLI example (two‑step, USDC → XLM → open):
```bash
# Step 1: swap USDC -> XLM via Soroswap router
deadline=$(($(date +%s)+600))
stellar contract invoke --id "CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD" \
  --source-account dev --network testnet -- \
  swap_exact_tokens_for_tokens \
  --amount_in 10000000 --amount_out_min 1 \
  --path '["USDC_CONTRACT","XLM_CONTRACT"]' \
  --to "USER_ADDRESS" --deadline "$deadline"

# Step 2: open position without swap
stellar contract invoke --id "MARGIN_CONTROLLER_ID" --source-account dev --network testnet -- \
  open_position_no_swap \
  --user "USER_ADDRESS" \
  --collateral_asset "XLM_CONTRACT" \
  --debt_asset "USDC_CONTRACT" \
  --collateral_amount 15123603 \
  --borrow_amount 2000000 \
  --leverage 2 --side Long
```

Notes:
- Set `amount_out_min` based on your slippage tolerance.
- Use the swap output as `collateral_amount`.

Liquidation helper (peridottroller):
- `repay_on_behalf_for_liquidator(borrower, repay_market, repay_amount, liquidator)`
  - Use this for liquidation bots that want to repay a borrow without seizing collateral.
  - The peridottroller authorizes the vault call via contract auth, so no vault allowlist is required.

Example CLI (repay-on-behalf helper):
```bash
stellar contract invoke \
  --id "$PERIDOTTROLLER" \
  --source-account "$LIQUIDATOR" \
  $NETWORK -- \
  repay_on_behalf_for_liquidator \
  --borrower "$BORROWER" \
  --repay_market "$REPAY_VAULT" \
  --repay_amount 40000000 \
  --liquidator "$LIQUIDATOR"
```
Notes:
- `repay_amount` is in underlying token base units (e.g., 6 decimals for USDT).
- The liquidator must hold sufficient underlying balance in the repay market token.

When wiring the frontend, store:
- `marginControllerId` (from `scripts/deploy_margin_controller_testnet.sh` output)
- `swapAdapterId`
- `aquariusRouterId`
- `peridottrollerId`
- `oracleId` (Reflector)
- vault IDs and underlying token IDs (as above)

## 9. Testing

- Use the scripts (`build_wasm.sh`, `deploy_testnet.sh`, `verify_testnet.sh`) to keep the contracts in sync with the frontend environment.
- For local development, you can target the sandbox (`deploy_sandbox.sh`) and point the frontend RPC to `http://localhost:8000`.
- Consider adding mock data layers for unit testing UI components without hitting the network.

## 10. Checklist

- [ ] Configure RPC + network passphrase
- [ ] Generate TypeScript bindings for controller and vaults
- [ ] Copy contract IDs and asset metadata into the frontend config
- [ ] Implement wallet connection (Freighter recommended)
- [ ] Create reusable hooks/services for `deposit`, `withdraw`, `borrow`, `repay`, `claim`
- [ ] Render health factor and liquidity data via controller reads
- [ ] Handle oracle stale/missing states gracefully
- [ ] If using margin, add multi-op transaction builder for swaps
- [ ] Test flows on testnet before mainnet deployment

With these pieces, you can recreate a Compound-like experience on Stellar Soroban, showing supply/borrow balances, USD valuations, and liquidation status in real time.
