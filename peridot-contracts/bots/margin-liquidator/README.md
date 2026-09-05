# Peridot Margin Liquidation Keeper

This service indexes MarginController position lifecycle events and executes the
budget-safe Perps V3 liquidation flow:

```text
begin_liquidation_v3
swap_liquidation_v3
finish_liquidation_v3
```

It also resumes abandoned staged liquidations after the takeover timeout,
recovers expired swapped pending opens, and restores expired pre-swap closes.

## Contract Requirements

The controller must expose:

- `get_position_counter`
- `preview_liquidation_v3`
- `position_created` and `position_removed` contract events

The keeper bootstraps all existing IDs from `1..=get_position_counter`, then
uses events to maintain its active set. Persist `STATE_FILE`; deleting it causes
a safe but RPC-heavy full rescan.

## Testnet Setup

Use a dedicated Stellar account. It needs XLM for transaction fees but does not
need the full position debt: V3 liquidation swaps controller-held collateral to
repay the debt vault.

```bash
cp .env.example .env
npm ci
npm test
set -a; source .env; set +a
npm start
```

Start with `DRY_RUN=true`. Dry-run mode simulates `begin_liquidation_v3` but does
not advance to swap/finish because no state is committed.
Set `RUN_ONCE=true` for a bootstrap/simulation smoke test that exits after one
polling cycle.

`LIQUIDATION_SLIPPAGE_BPS` is applied to the live Aquarius pool estimate. The
keeper refuses to swap when the pool estimate is below the contract's oracle
minimum and always passes the greater of the oracle floor and slippage-adjusted
pool quote.

## Docker

```bash
docker build -t peridot-margin-liquidator .
docker run -d \
  --name peridot-margin-liquidator \
  --restart unless-stopped \
  --env-file .env \
  -v peridot-liquidator-data:/app/data \
  peridot-margin-liquidator
```

## systemd

Install Node.js 20 or newer, create an unprivileged `peridot` user, put secrets
in `/etc/peridot/margin-liquidator.env` with mode `0600`, and install
`margin-liquidator.service.example` as
`/etc/systemd/system/peridot-margin-liquidator.service`.

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now peridot-margin-liquidator
sudo journalctl -u peridot-margin-liquidator -f
```

## Operations

- Run one active process per signing key to avoid account-sequence collisions.
- A standby keeper should use a different funded key; it can take over after
  the on-chain timeout.
- Use a private authenticated RPC provider for mainnet.
- Keep `HORIZON_URL` configured as an account-sequence fallback. Contract reads,
  simulations, events, and submissions still use `RPC_URL`.
- Store the secret in a server secret manager, never in Git or a Docker image.
- Alert on `position processing failed`, `pool quote ... below oracle floor`,
  low keeper XLM, and repeated transaction timeouts.
- RPC event history is bounded. The persistent cursor plus bootstrap counter
  makes recovery independent of retaining the full event history.
