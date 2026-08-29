# Peridot Aquarius Keeper

One sequence-safe worker maintains all three Mainnet Aquarius strategies:

1. refresh each strategy's cached NAV root every five minutes;
2. claim, convert, and compound rewards once per hour;
3. refresh each ReceiptVault's boosted-underlying cache.

The worker services XLM, PYUSD, and USDC serially with one dedicated signer. Do
not scale it above one instance or run another live copy with the same key:
parallel Stellar transactions from one source account can collide on sequence
numbers.

## Signer

The dedicated public key is:

```text
GBLVLGECHVZAKOJ6XLLF4YRNNBTAZBM4O6ZMIL336BYIHKUZLACLXSE3
```

It has no protocol role and receives no harvest proceeds. Fund it with about 25
XLM for the initial multi-day pilot. The first Mainnet dry run measured roughly
0.013 XLM of resource fees per refresh-only cycle and 0.185 XLM for a cycle that
also harvests all three vaults; five-minute refreshes plus hourly harvests are
therefore about 8 XLM/day before reward-dependent extra work and fee-market
variance. Keep its secret only in the
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

Dry-run mode simulates all nine calls without signing or submitting. A live
cycle requires `KEEPER_SECRET`; if `KEEPER_PUBLIC_KEY` is also configured, the
worker refuses to start unless both resolve to the same account.

## DigitalOcean App Platform

Use the repository spec at `.do/aquarius-keeper.yaml`. It deliberately starts
in `DRY_RUN=true` mode and has automatic deployments disabled. The intended
rollout is:

1. Create the app from `.do/aquarius-keeper.yaml`; the public repository is
   cloned directly from the `leveraged-fix` branch without GitHub OAuth.
2. Confirm one dry-run cycle completes for all three targets.
3. Add `KEEPER_SECRET` as an encrypted runtime variable.
4. Change `DRY_RUN` to `false` and redeploy exactly one worker instance.
5. Verify confirmed transaction hashes for NAV refresh, harvest, and market
   cache refresh; monitor the following hourly cycles.

The component costs approximately $5/month at the pinned 512 MiB worker size,
excluding Stellar transaction fees. Alert on repeated failed cycles, low XLM,
route-floor failures, stale NAV, material idle AQUA, and pool kill switches.
