# Peridot cash-market liquidator

This is a single-process keeper for the fresh cash markets, using controller `CBMRS7LK…7RK` and markets USDC, XLM and EURC. PYUSD is
supply-only. The CL staged receipt is in `neverTouch` and is never chosen.

Each cycle (every `POLL_INTERVAL_MS`):

1. **Discovery.** `getEvents` returns `borrow_event` events from the borrowable markets. The cursor and the borrower set
   persist in `STATE_FILE`. On first start it scans `EVENTS_LOOKBACK_LEDGERS` back. If the cursor falls outside RPC
   retention (about 7 days), it logs an `alert`. Then seed the missing borrowers with `BORROWERS_SEED`.
2. **Price refresh.** It calls the permissionless `cache_price(token)` for USDC, XLM and EURC on every new Reflector round, that is, whenever the feed timestamp is newer than the controller's `PriceCache`.
   - Fee limits: its own bid (`REFRESH_INCLUSION_FEE_STROOPS`, 1,000), a per-transaction cap (`REFRESH_MAX_FEE_STROOPS`, 0.01 XLM), 30 s validity, and a persisted per-UTC-day budget (`REFRESH_DAILY_BUDGET_STROOPS`, 3 XLM) charged at the actual `feeCharged`.
   - It idles while the controller oracle is unset. A failure never blocks the liquidation pass.
3. **Health.** `account_liquidity(borrower)` is called for each known borrower. Only accounts in shortfall go further.
4. **Plan** (`src/plan.mjs`, pure and unit-tested).
   - Repay market: the largest priced debt for which we hold inventory.
   - Collateral market: the largest entered, priced, CF>0 market other than the repay market. The controller rejects repay == collateral, so same-market loops are skipped with an alert.
   - Repay = min(close factor × debt, inventory, collateral / incentive). XLM inventory excludes the 20 XLM fee floor and the 2 XLM account reserve.
5. **Simulate** `liquidate`. The returned seize must cover repay × (1 + `MIN_PROFIT_BPS`). It refuses above `CPU_STOP` or `MAX_FEE_STROOPS`.
6. **Sign and send** (only when `DRY_RUN=false`). The intent hash is written to `JOURNAL_FILE` before sending. It polls until the transaction succeeds or fails.
7. **Recycle.** If `WITHDRAW_SEIZED=true`, it withdraws the seized pTokens to underlying. If cash is lent out, it retries next cycle.
8. **Depeg log.** It compares external USDC with $1 and external EURC with fiat EUR, and logs `alert` lines only. Automatic pausing is
   the separate pause watcher below, which runs with a different key.

At startup the keeper refuses to run in any of these cases:
- the RPC network differs from `config/mainnet.json`;
- the controller or market WASM hashes differ from it;
- its key is the admin or the on-chain `PauseGuardian`.

## Margin pass (pooled margin v2, same process, same signer)

`src/margin.mjs` (I/O) and `src/margin-plan.mjs` (pure, unit-tested) add a pass after the cash pass in every cycle. A second process
with the same key would collide on sequence numbers, so it runs here, strictly after the cash pass. Contract:
`contracts/margin-controller` build `b1cb1cb2…766f`; spec `research/pooled-margin-20261124/` (SPEC §6, INTERFACE "WP6 changes").

- **Off by default.** It runs only when `margin.enabled` is true and `margin.contract` is set. Set these in `config/mainnet.json`, or in an
  operator overlay file named by `MARGIN_CONFIG_FILE`. The overlay can never change the release-pinned `margin.wasmAllowed` or the controller.
- **Own dry-run flag.** `MARGIN_DRY_RUN` defaults to true, and the effective value is `DRY_RUN || MARGIN_DRY_RUN`. A margin dry stage therefore
  never switches off live cash liquidations.
- **Pins, checked every cycle.**
  - The margin WASM must be in `wasmAllowed`.
  - The instance `Ctrl`, `Usdc`, `Xlm` and `Market(USDC/XLM)` must equal the pinned controller and markets.
  - The keeper key must not be the margin `Admin` or `Guardian`.
  - On a mismatch the margin pass does nothing and alerts; the cash pass is unaffected.
- **Cycle.** It runs once per ledger sequence, every `POLL_INTERVAL_MS` (≤ 30 s, measured from the cycle start). It also wakes early on a new
  Reflector round, checked every `MARGIN_ROUND_POLL_MS`, 5 s by default. Each cycle does the following, in order:
  1. **Discovery.** It bootstraps from `get_position_counter` + `get_position(id)`, keeping `open == true`. It then follows the
     `margin_open/close/liquidate` events (raw JSON `getEvents`, id = `data[0]`) and `bs_request/cancel/withdraw` for backers.
  2. **Liquidations.** It reads `get_position_ltv`/`debt`/`funding` for every open position, then handles every one with LTV ≥ its own
     `liq_ltv`, worst first:
     - simulate `liquidate(keeper, id, max_pay)`, where `max_pay` = worst case (long: D + 1 % + funding; short: D) + 0.5 %, capped at inventory;
     - profit check: the seize from the simulated `margin_liquidate` event, the pay from the keeper's net SAC transfers (the contract-math model
       is the fallback), minus the transaction fee; the profit must be > 0 and ≥ `margin.minProfitBps`;
     - journal the intent, then send.
     No allowance is needed: `liquidate` uses `transfer` under the source-account auth.
  3. **Settle.** `settle_backstop_debt(keeper, asset, settle_max_ptokens(asset), 0)` for USDC and XLM, repeated while
     `get_bs_debt_shares > 0` (at most `maxSettleIters`). `PooledShortfall` #35 alerts (recapitalise) and stops; a send with no progress stops.
  4. **Expired backstop requests.** `backstop_clear_expired(keeper, backer)` for tracked backers with `now > expires_at`.
  5. **Reconcile.** It simulates `reconcile` and sends only when it returns `true`.
  6. **Alerts**, rate-limited per key (`realertSecs`):
     - a position liquidatable for > 10 min;
     - `pooled_health` hf < `hf_target` or hf_down < 1.10;
     - `margin_impaired` (state or event);
     - opens refused (#25) while `BsDebtShares > 0`;
     - USDC or XLM inventory below the largest long or short debt payment;
     - position views failing (price #11).
- **CAP-88.** Cycles, event checkpoints and "liquidatable since" observations are keyed on ledger **sequence**. The 10-min timer uses the
  keeper clock, never a ledger-timestamp delta. A repeated timestamp neither stalls nor double-counts.
- **Cash exclusion.** `neverLiquidateBorrowers`, plus the margin contract whenever it is configured (enabled or not). The cash pass alerts on
  their shortfall and never plans, simulates or signs a liquidation of them.
- **Caps.** `MARGIN_CPU_STOP` (289 M) and `MARGIN_MAX_FEE_STROOPS` (3 XLM). Margin flows run to 198 M instructions on the fork, above the cash `CPU_STOP`.
- **SDK.** `@stellar/stellar-sdk` stays at 16.3.1. Latest ledger, transaction status and margin events are raw JSON-RPC, so no protocol-30
  header or meta is ever decoded by the SDK.
- **Alert check:** `node src/margin-alert-check.mjs [names]`. It never signs: dry run is forced, and sign/submit throw. With margin enabled it
  runs one real read-only margin cycle. It then sends a synthetic snapshot through the same alert path, with every line tagged
  `alertTest:true`, and exits non-zero if any requested alert did not fire.
- **Rollout:** `research/pooled-margin-20261124/WP7-KEEPER-ROLLOUT.md`.

## Pause watcher (separate process, separate key: `npm run start:pause`)

`src/pause-main.mjs` reads the external feed (USDC, XLM, EURC), the fiat EUR feed and the controller's USDC price band every
`POLL_INTERVAL_MS`. Policy (2026-11-21): **a stale or missing price never pauses; it is an alert only** (the controller already
returns no price after k × resolution, so borrows that need it revert). It pauses only on the following, each evaluated on that asset's own fresh
observation (age ≤ `STALE_AFTER_SECONDS`, 900):
- a USDC price outside the controller band: on-chain `get_price_band(USDC)` (PriceBand, 1e6 scale) when set, else the pinned
  `pauseWatcher.usdcBand1e6` [980000, 1020000]; inclusive, the same exact test as the contract's `price_in_band`;
- an EURC depeg: EURC more than ±`EURC_VS_EUR_BPS` (100) from the fresh fiat EUR reference.

Alert only: stale/missing prices, and a USDC deviation over ±`USDC_DEPEG_BPS` (50) from $1 that is still inside the band. PYUSD
is not monitored: the pinned feed has no PYUSD price and the market is not borrowable, so it carries no collateral value.

A failed read of our own RPC is a warning, not staleness. After `PAUSE_CONFIRMATIONS` (2) consecutive pause-triggering observations (each a new observation of the triggering asset), it calls
`pause_borrow_g(pauseKey, market, true)` on USDC, XLM and EURC, skipping markets already paused. It then latches until `PAUSE_CLEAR_CYCLES` (20) clean cycles have passed,
so a Ledger unpause is not overridden and the 72 h pause is not extended.

Safeguards:
- **Restriction.** `assertPauseTx` refuses any transaction except one `controller.pause_borrow_g(pauseKey, borrowable receipt, true)` with no nested authorization. PYUSD and the CL receipt are never targets.
- **Dry run by default.** In dry run it simulates only and never signs.
- **Live-mode checks.** Live mode refuses to start unless the on-chain `PauseGuardian` equals `PAUSE_PUBLIC_KEY`. It also refuses if the key equals the admin or `LIQUIDATOR_PUBLIC_KEY`, or if any liquidator credential variable is present.
- **Unpausing.** Only the Ledger unpauses (`ceremony.cjs unpause-borrow <M>`).

## Signer

- **Mainnet (Droplet, recommended): `LIQUIDATOR_KEY_FILE`.**
  - It is a 0600 file owned by the service user, created ON the host by `node src/keygen.mjs <abs path>`. That command prints only the public key and never overwrites.
  - The file is loaded at the first signature. It is refused if it is a symlink, has group or other permission bits, has a foreign owner or does not match `LIQUIDATOR_PUBLIC_KEY`.
  - The pause watcher uses `PAUSE_KEY_FILE` in the same way.
  - Bootstrap: `research/cash-borrowing-ops-20261118/droplet/`.
- **Legacy (App Platform):** `LIQUIDATOR_SECRET`, an encrypted DigitalOcean run-time secret. It is checked against `LIQUIDATOR_PUBLIC_KEY`. No key exists in the repo.
- **Testnet rehearsal only:** `SIGNER_CLI_ALIAS`. The Stellar CLI signs, and the keeper process never sees the secret. It is rejected on Mainnet.

## Funding (operator; PROPOSED, see `research/cash-borrowing-ops-20261118/funding.md`)

The rule is borrow cap × 50% close factor × 1.10 ≤ inventory. The caps are 900 USDC, 4,500 XLM and 450 EURC.

| Asset | Inventory | When |
|---|---|---|
| XLM | 25 (reserves + fees) | T0: before the dry run, then `node src/trustlines.mjs` (dry run, then `CONFIRM_TRUSTLINES=yes`) for USDC and EURC |
| USDC | 500 | T1: after the soak, before `DRY_RUN=false` |
| EURC | 250 | T1 |
| XLM | 2,600 (2,500 inventory + 100 fees) | T1 |

The pause key gets 10 XLM. The total is about $1,148. The keeper alerts (in live mode) when net inventory falls below `minInventoryRaw` in `config/mainnet.json`.

## Run

```bash
npm ci && npm test
LIQUIDATOR_PUBLIC_KEY=G... DRY_RUN=true RUN_ONCE=true npm start      # keyless smoke cycle
```

Deploy on a Droplet with a volume: `research/cash-borrowing-ops-20261118/droplet/bootstrap-liquidator.sh` (systemd, `DRY_RUN=true`,
key created on the host). The pause watcher has its own `bootstrap-pause-watcher.sh`, preferably on its own host.
`deploy/digitalocean-app.yaml` is superseded: App Platform has no persistent disk, so after a restart the keeper would rely on the 24 h event lookback plus `BORROWERS_SEED`.
