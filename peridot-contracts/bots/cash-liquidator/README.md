# Peridot cash-market liquidator

This is a single-process keeper for the fresh cash markets, using controller `CBMRS7LK…7RK` and markets USDC, XLM and EURC. PYUSD is
supply-only. The CL staged receipt is in `neverTouch` and is never chosen.

Each cycle (every `POLL_INTERVAL_MS`):

1. **Discovery.** `getEvents` returns `borrow_event` events from the borrowable markets. The cursor and the borrower set
   persist in `STATE_FILE`. On first start it scans `EVENTS_LOOKBACK_LEDGERS` back. If the cursor falls outside RPC
   retention (about 7 days), it logs an `alert`. Then seed the missing borrowers with `BORROWERS_SEED`.
2. **Health.** `account_liquidity(borrower)` is called for each known borrower. Only accounts in shortfall go further.
3. **Plan** (`src/plan.mjs`, pure and unit-tested).
   - Repay market: the largest priced debt for which we hold inventory.
   - Collateral market: the largest entered, priced, CF>0 market other than the repay market. The controller rejects repay == collateral, so same-market loops are skipped with an alert.
   - Repay = min(close factor × debt, inventory, collateral / incentive).
4. **Simulate** `liquidate`. The returned seize must cover repay × (1 + `MIN_PROFIT_BPS`). It refuses above `CPU_STOP` or `MAX_FEE_STROOPS`.
5. **Sign and send** (only when `DRY_RUN=false`). The intent hash is written to `JOURNAL_FILE` before sending. It polls until the transaction succeeds or fails.
6. **Recycle.** If `WITHDRAW_SEIZED=true`, it withdraws the seized pTokens to underlying. If cash is lent out, it retries next cycle.
7. **Depeg watcher.** It compares external USDC with $1 and external EURC with fiat EUR, and logs `alert` lines only. The response is the
   Ledger kill switch (`research/cash-borrowing-activation-20261117/CEREMONY.md` §E).

At startup the keeper refuses to run if the RPC network or the controller or market WASM hashes differ from `config/mainnet.json`.

## Signer

- **Mainnet:** `LIQUIDATOR_SECRET`, an encrypted DigitalOcean run-time secret set by the operator. It is checked against
  `LIQUIDATOR_PUBLIC_KEY`. No key exists in the repo, and the agent never creates or handles one.
- **Testnet rehearsal only:** `SIGNER_CLI_ALIAS`. The Stellar CLI signs, and the keeper process never sees the secret. It is rejected on Mainnet.

## Funding (operator; PROPOSED, see PLAN.md §7)

| Asset | Minimum inventory | Why |
|---|---|---|
| USDC | 500 | close factor 50% × 1,000 USDC borrow cap |
| XLM | 2,500 plus 20 for fees | 50% × 5,000 XLM cap; fee floor alert at 20 XLM |
| EURC | 250 | 50% × 500 EURC cap |

The account needs USDC and EURC trustlines. The total is about $1.2k at current prices.

## Run

```bash
npm ci && npm test
LIQUIDATOR_PUBLIC_KEY=G... DRY_RUN=true RUN_ONCE=true npm start      # keyless smoke cycle
```

Deploy with `deploy/digitalocean-app.yaml`: one worker, auto-deploy off, starting with `DRY_RUN=true`. App Platform has no persistent
disk, so after a restart the keeper relies on the 24 h event lookback plus `BORROWERS_SEED`. A Droplet with a
volume, like `bots/margin-liquidator/deploy/droplet`, avoids that gap.
