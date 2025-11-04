# Peridot Liquidation Bot

Headless service that monitors Peridot lending markets and liquidates
undercollateralized positions on Soroban.

## Prerequisites

- Node.js 18+
- Soroban RPC access (defaults to public testnet)
- Liquidator account funded with the underlying assets it may need to repay

## Configuration

Create a `.env` alongside this README or export the variables before running:

```
NETWORK_PASSPHRASE="Test SDF Future Network ; October 2022"
RPC_URL="https://soroban-testnet.stellar.org"
PERIDOTTROLLER_ID="CAWEZM3CRRMBUAGYMCCFHXI6ZKCLVMQTVE4LPXQCH7MM3ZU2PMQTKUXM"
LIQUIDATOR_SECRET="SB..."

# Optional overrides
POLL_INTERVAL_MS=5000
BORROWER_REFRESH_MS=15000
MIN_SHORTFALL=0
EVENT_BACKLOG=50
MARKETS_JSON='[{"symbol":"XLM","vaultId":"CCBRKJ5ZZZB6A7GSAPVPDWFEOJXZZ43F65RL6NJGJX7AQJ2JS64DGU7G","decimals":7},{"symbol":"USDC","vaultId":"CDNSMCOHX4NJTIYEILEVEBAS5LKPJRDH6CPLWJ4SQ2YUB4LVNQWPXG3L","decimals":6}]'
```

`MARKETS_JSON` should list every vault the bot may repay or seize from. When
omitted, the defaults in `src/config.ts` (XLM & USDC) are used.

## Install & Run

```
npm install
npm run build
npm start
```

During development you can use hot reload:

```
npm run dev
```

## How It Works

1. Streams controller and vault events to discover active borrowers.
2. Periodically evaluates each borrower’s health via `account_liquidity`.
3. When `shortfall` exceeds `MIN_SHORTFALL`, determines the repay market,
   repay amount (capped by `preview_repay_cap`), and optimal collateral market
   using `preview_seize_ptokens`.
4. Submits a signed `liquidate` transaction and waits for confirmation.

The liquidator account must hold enough underlying assets for each repay market
(e.g. Soroban USDC or wrapped XLM). Failed attempts are logged and retried on
the next evaluation cycle.

## Notes

- The bot is stateless across restarts; it rebuilds borrower lists from recent
  events (`EVENT_BACKLOG` ledgers back).
- Market pause flags, stale oracles, and zero balances are respected by the
  on-chain contracts; the bot surfaces the resulting errors.
- Extend `src/liquidationBot.ts` if you need custom heuristics (e.g. prioritise
  specific markets or integrate alerting).
