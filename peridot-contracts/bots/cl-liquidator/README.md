# cl-liquidator (Testnet prototype)

Staged liquidation keeper for the CL PYUSD collateral group
(`research/cl-collateral-group-20261118/DESIGN.md` §2-5, `D3-CONTINUITY.md`;
receipt ABI `prototypes/cl-collateral-group/receipt-v2/staged_v2.rs`).

## Stages (one transaction each)

| Stage | Call | Auth | When |
|---|---|---|---|
| L1a | receipt `checkpoint_nav()` | none | NAV missing or > 45 s old (while someone owes); while blocked, every 15 s and always first in the send order (D3 heartbeat, 20 s target incl. latency); immediately when a chain exists and the pool is healthy (clears it) |
| L1b | receipt `checkpoint_fallback()` | none | NAV blocked, chain `last - since >= 600` and `now - last <= 60`, and a borrower is short at the simulated fallback rate |
| L2 | controller `cache_price(USDC)` | none | before an L3 when the cache is invalid at submission (required) or expires within 60 s after (preferred, once per oracle round). The cache keeps the ORACLE timestamp, so a late round cannot be extended: L3 then waits with an alert |
| APPROVE | USDC `approve(liquidator, usdc_market, repayCapRaw, seq+100000)` | liquidator | allowance below the planned repay; approves the stable cap (required for `--live`) so a drifting repay does not re-trigger it; sent before L2/L1 so it does not eat the 60 s checkpoint window |
| L3 | controller `liquidate(borrower, usdc_market, receipt, repay, liquidator)` | liquidator | shortfall > 0 at a checkpoint (NAV, else fallback) that is still valid at `now + submitMarginSec` |
| L4 | vault `checkpoint_rewards()` | none | a seizure is queued and the last claim is older than `60 - submitMarginSec` s |
| L5 | receipt `sync_seized(id)` | none | every pending id, ascending, within 60 s of L4 (else a reward boundary opens; allowed, alerted when L4 fails) |

Repay = min(debt x close factor, `repayCapRaw`, USDC inventory, collateral value / incentive). One L3 per
cycle (largest shortfall). Oracle stale (`checkpoint_nav` simulation reverts #2105): **repay-only**, no L1/L3,
alert; L4/L5 still run. A borrower with a queued seizure, or an L3 journal intent without a result, gets no L3.

All times are ledger close times (`getLatestLedger.closeTime`). Shortfall is computed off-chain from
`get_liquidation_account_snapshot` (units), the borrower's USDC debt and the market CF (collateral and debt are
both USDC, so prices cancel): `account_liquidity` is indeterminate whenever the NAV checkpoint is stale, so it is
not used. The `liquidate` simulation is the final gate.

## Commands

```bash
cp config/testnet.example.json config/testnet.local.json   # fill in contract ids / addresses (git-ignored)
node src/main.mjs --config config/testnet.local.json --once            # dry run: plan + simulations, sends nothing
node src/main.mjs --config config/testnet.local.json --watch           # dry-run loop
node src/main.mjs --config config/testnet.local.json --watch --live    # TESTNET ONLY: sends via the Stellar CLI
node src/main.mjs --obs observation.json                               # offline: plan a recorded observation
node --test
```

The SDK (`@stellar/stellar-sdk` 16.3.1) is not installed here: set `STELLAR_SDK_DIR` to a directory whose
`node_modules` holds it, or rely on the sibling `bots/cash-liquidator` install. Only `chain.mjs` loads it; the
planner, journal, watcher and tests are SDK-free and offline.

The planner returns an ordered stage list; the bot sends the head, re-observes and re-plans (simulations do not
chain, so in dry run a stage behind an unsent one is reported as "planned, not simulable"). `--once --live` sends
at most 8 stages. `--watch` re-plans every `watchIntervalSec` (<= 20) seconds, 1 s after a send.

## Safety rules

- Never Mainnet: `--live` requires the exact Testnet passphrase, checked at start and on every call (unit-tested).
  The RPC's passphrase must match the config.
- No key material in this process. Live sends run `stellar contract invoke --source-account <alias> --send yes`
  (no shell, `STELLAR_ACCOUNT` dropped); the CLI holds the key. The alias must not look like a strkey or a seed
  phrase; config keys named like secrets are refused. The bot never calls `stellar keys`.
- Journal: `<dataDir>/journal.jsonl`, append-only; an intent is written before each send and a result after.
  An unresolved L3 intent blocks that borrower until 600 s pass with `next_pending` unchanged (no seizure landed);
  if the queue moved it stays blocked for operator review. A CLI timeout/signal leaves the intent unresolved (the
  transaction may still land); only a non-zero CLI exit is journalled as a failure. The child process gets no
  `STELLAR_*`/`SOROBAN_*` overrides except `STELLAR_CONFIG_HOME`. `state.json` keeps the pending-scan low-water mark.
- D3: only a SENT observation extends or clears a chain. A dry-run watcher does not keep the chain alive and
  does not clear it; the live keeper must run at <= 20 s while a chain is open.
- Depeg watcher (D2, `watcher.mjs`): alert when the feed is > 100 bps from par or from the optional external
  reference (disabled by default), or older than 600 s. It prints the recommended guardian calls (receipt
  `stage_pause_deposits`, controller `pause_borrow_g(usdc_market)`); it never sends them.
