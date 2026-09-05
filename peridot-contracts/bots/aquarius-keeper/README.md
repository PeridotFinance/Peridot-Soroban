# Peridot Aquarius Keeper

One sequence-safe worker maintains all three Mainnet Aquarius strategies:

1. refresh each strategy's cached NAV root every twenty minutes;
2. simulate `needs_rebalance` and, when enabled, recenter an edge-bound range;
3. claim, convert, and compound rewards every six hours;
4. refresh each ReceiptVault's boosted-underlying cache.

The worker services XLM, PYUSD, and USDC serially with one dedicated signer. Do
not scale it above one instance or run another live copy with the same key:
parallel Stellar transactions from one source account can collide on sequence
numbers.

Both network endpoints must use HTTPS. The worker refuses to start with a
plaintext RPC or Horizon URL.

## Signer

The dedicated public key is:

```text
GBLVLGECHVZAKOJ6XLLF4YRNNBTAZBM4O6ZMIL336BYIHKUZLACLXSE3
```

It has no protocol role and receives no harvest proceeds. Fund it with about 25
XLM for the initial multi-day pilot. The first Mainnet dry run measured roughly
0.013 XLM of resource fees per refresh-only cycle and 0.185 XLM for a cycle that
also harvests all three vaults. Twenty-minute refreshes plus four harvests per day
are therefore about 1.7 XLM/day before reward-dependent extra work and fee-market
variance. This cadence gives three refresh attempts inside the one-hour NAV safety
window while avoiding economically wasteful empty harvests for the initial small
positions. Reassess it as deposits and rewards grow. Keep its secret only in the
macOS secure store and DigitalOcean's encrypted `KEEPER_SECRET` runtime
variable. Never put it in Git, an image, logs, or a checked-in `.env` file.

## Local verification

```bash
npm ci
npm test
npm run check

DRY_RUN=true RUN_ONCE=true \
  KEEPER_PUBLIC_KEY=G... \
  npm start
```

With rebalancing disabled, dry-run mode simulates all nine maintenance calls
without signing or submitting. With `RUN_REBALANCE=true`, it additionally reads
`needs_rebalance` and simulates `rebalance` only for a target that needs it. A live
cycle requires `KEEPER_SECRET`; if `KEEPER_PUBLIC_KEY` is also configured, the
worker refuses to start unless both resolve to the same account.

## DigitalOcean App Platform

Use the repository spec at `.do/aquarius-keeper.yaml`. It deliberately starts
in `DRY_RUN=true` mode and has automatic deployments disabled. The intended
rollout is:

1. Create the app from `.do/aquarius-keeper.yaml`; the public repository is
   cloned directly from dedicated immutable release branch
   `aquarius-keeper-v0.2.0` without GitHub OAuth. Never move or replace that
   branch or its matching tag; publish a new reviewed ref for each release.
2. Keep `RUN_REBALANCE=false` until all six contract upgrades are complete and
   all three live positions have been migrated by the guarded rollout script.
3. Confirm one dry-run cycle completes for all three upgraded targets, then set
   `RUN_REBALANCE=true` and confirm the read-only checks remain clean.
4. Add `KEEPER_SECRET` as an encrypted runtime variable.
5. Change `DRY_RUN` and `HARVEST_ON_START` to `false`, then redeploy exactly one
   worker instance. This prevents a restart inside the contract cooldown from
   creating a false failure loop.
6. Verify confirmed transaction hashes for NAV refresh, any required rebalance,
   harvest, and market cache refresh; monitor the following cycles.

The component costs approximately $5/month at the pinned 512 MiB worker size,
excluding Stellar transaction fees. The App Platform alert fires above one restart
within five minutes. Also alert on failed cycles, low XLM,
route-floor failures, stale NAV, material idle AQUA, and pool kill switches.
