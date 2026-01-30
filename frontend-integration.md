# Peridot Frontend Integration Guide

This document explains how the frontend can interact with the Peridot Soroban contracts in a Compound-style UI. It assumes you are comfortable with TypeScript/React (or Astro) and have the Stellar CLI installed.

## 1. High-Level Architecture

- **Controller (`SimplePeridottroller`)** – central risk engine. Contract ID: `CDKBJC5E44FEZVVETYU2IZZLUVKN2BUH4XOMEMKTYKM4SBSRT5ZR34V3`
- **ReceiptVault (XLM market)** – contract ID: `CCPQYPFNAGQPQTMPAEBGNPNSQJ4FAJYPX6WLYBKE5SO5ZONXANCUEYE7`
- **ReceiptVault (USDT market)** – contract ID: `CDM37TMZO2QQQP6CIMU7E6OIBR6IQMM46P5PCSQ5D7AX6GMEFQX7NTKL`
- **Jump Rate Model** – contract ID: `CD43R6PGESRAKAUYHRDNMSS7PHU6TT26D3WAAHYEKMDGHB5FALTHMFEI`
- **Peridot Reward Token (`P`)** – contract ID: `CBTHWQJX2766UIH4J6TGRU3XVRVDMOX33RWT5IP36HB3D5RGD7XBPSR5`
- **Mock USDT (open mint)** – contract ID: `CBX3DOZH4HUR3EJS6LAKHXN6RARXKMUT33OUMVVSUW5HCXEIECD4WT75`
- **Reflector Oracle** – contract ID: `CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63`
- **Margin Manager (leveraged trading)** – contract ID: pending deploy via `scripts/deploy_margin_testnet.sh`

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

## 8. Leveraged Margin (Optional)

Margin trading uses `margin-manager` with multi-op transactions (invoke + classic path payment). The manager pulls prices from Reflector and coordinates peridottroller-backed vault borrows.

Core calls:
- `deposit_collateral(user, asset, amount)`
- `open_position(user, side, size, leverage)`
- `finalize_open(user, pending_id)` after swap output is transferred to the manager
- `close_position(user, position_id)` + `finalize_close` after repayment funds are transferred
- `get_health_factor(user)`

When wiring the frontend, store:
- `marginManagerId` (from `scripts/deploy_margin_testnet.sh` output)
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
