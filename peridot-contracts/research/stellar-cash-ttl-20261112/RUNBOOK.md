# USDC full exit, TTL maintenance and split top-ups

## Approved order

1. Ledger withdraws its entire 90-USDC receipt position. Read back owner shares zero, 1 USDC backing and permanent shares remaining, exchange rate 1000000, debt zero. Record `node scripts/cash_operator_checks.cjs usdc-empty`.
2. Ledger redeposits exactly 90 USDC. Read back owner shares 900000000, backing 910000000 raw, unchanged permanent floor/rate. Record `node scripts/cash_operator_checks.cjs usdc-redeposited`.
3. Fund the Ledger wallet for the named additional top-ups, then execute the first half in each market. Run `node scripts/cash_operator_checks.cjs first-halves`.
4. The approved fee-only deployer extends TTLs. The second-half gate requires actual successful native extension transactions **after** all first halves and exact TTL readbacks. A quote, simulation, waiting job or no-op does not satisfy it.
5. Execute the second halves only after `node scripts/cash_operator_checks.cjs second-gate` passes.

The full USDC withdrawal simulation passed at ledger 65440675: 18,419,555 declared instructions, 45 keys, 7 writes, 34167 stroops minimum resource fee, source-account authorization. It has not been signed/applied by the agent. Redeposit is conditionally prepared; its live simulation must run after the actual full exit. The $90 minimum-position cycle is not claimed complete until both applied-state checkpoints exist.

## Current TTL and coverage

The complete per-key table is [TTL-INVENTORY.md](TTL-INVENTORY.md), also available as `inventory-latest.tsv` and machine-readable `inventory-latest.json`. The scan covered every transaction in contiguous ledger ranges beginning at fresh creation, inspecting only fresh-contract/dependency mutations for its registry. Segment boundary overlap can count a small number of transactions twice in scan statistics; the key registry and selected transaction hashes are deduplicated. No old contract was invoked.

Code and instances are separate entries. Instance-owned safeguards (CashBorrowingEnabled, MinCashReserveBps, permanent floor and token metadata) share the instance lifetime; they do not have separate TTLs. Every discovered persistent key, including receipt-owned and Ledger-owned token balances, is listed individually. The eight SAC entries are shared asset-instance dependencies and each fresh receipt's custody balance, not old Peridot dependencies.

The code's opportunistic core maintenance uses a 500000-ledger threshold and 1000000-ledger target. The receipt `bump_ttl` does not enumerate all user/controller keys; controller getters do not cover the entire controller either. Read-only simulations never persist those TTL bumps. Therefore the keeper uses native `ExtendFootprintTTLOp` on explicitly indexed keys, covering the full inventory rather than only broad getters.

## Installed schedule and authorization boundary

Installed LaunchAgent: `finance.peridot.fresh-cash-ttl-20261112`, at `/Users/joshua/Library/LaunchAgents/finance.peridot.fresh-cash-ttl-20261112.plist`. It runs `/opt/homebrew/bin/node scripts/cash_ttl_keeper.cjs scheduled` in this repository once per hour. Before the roundtrip/first-half gates exist it exits in a waiting state without signing. After activation it performs one complete discovery/TTL check per 24 hours; overdue/incomplete scans resume from their cursor. Only entries below the renewal threshold are extended.

The operator explicitly authorized use of `peridot-cash-fresh-20261111`, public address `GDBS26DYO2FMMLXGFPZ6JX3ZTHJBM2FVAAKD34J7K6ZWVKGLLANZTOYE`, for **TTL extensions only**. CLI signing uses the existing OS-stored identity; no private key is exported, printed or stored by the keeper. Ledger retains every admin/guardian role. The job validates one native TTL operation, pinned source/network/IDs/code hashes, an exact read-only footprint, no extra operation, no memo, bounded lifetime and fee. It cannot invoke a financial method, payment, upgrade, deployment or restoration. Signed envelopes stay in process memory and are not written to the repository.

Renew when fewer than **2419200 ledgers (~140 days)** remain; extend to **2592000 ledgers (~150 days)**. The captured network maximum is 3110400 ledgers (~180 days); no network or contract resource policy changed. Read-only footprint batches contain at most 30 keys, with each WASM in its own batch. Existing longer TTLs are not shortened.

The first full quote covers 134 eligible keys in 7 native transactions; five entries already exceed the threshold. Minimum aggregate fee including nominal inclusion is **54.6164716 XLM**, including shared code rent counted once. A 20% resource-fee allowance gives **65.5397521 XLM**. Fixed operational ceilings are 40 XLM per transaction and 80 XLM per run, with at least 10 XLM retained in the fee-only account. These are gas-spending guards, not changes to financial resource limits. The first provisional 30-XLM batch ceiling rejected the receipt-code rent quote; that failure is preserved. No extension has yet been signed or submitted.

Each batch is re-simulated with a fresh sequence before signing. Unknown submissions retain a hash marker and halt until reconciled; failed/unknown transactions are never blindly repeated. A partial run can preserve completed extensions while remaining work stays gated. Actual Core success, post-transaction TTLs and fees are recorded before `first-extension-confirmed.json` is created. The second-half helper also verifies that file's hashes against public RPC and binds it to the exact top-up plan.

This is a **local Mac user-session schedule**, not an always-on hosted keeper. It requires the Mac awake/logged in, network access and OS keychain access. Background signing access remains unproven until the first authorized extension runs; failure leaves the second-half gate closed. History older than RPC retention or unexplained missing keys stops maintenance for reconciliation rather than guessing. No paid infrastructure/subscription was added. Public discovery on a busy network is substantial; the first full scan processed roughly a million transaction records. Logs, cursors and registry are durable locally; monitor failure/backlog and the fee-account balance.

Commands:

```sh
# Read-only inventory refresh:
node scripts/cash_ttl_inventory.cjs
# Read-only fee quote:
node scripts/cash_ttl_keeper.cjs quote
# After the first halves: approved fee-only signer; native TTL operations only:
node scripts/cash_ttl_keeper.cjs run
# Read-only second-half gate:
node scripts/cash_operator_checks.cjs second-gate
```

Eleven local signing-boundary tests pass. The installed waiting path ran successfully with no signing or submission. Current status is `keeper-status.json`; public transaction receipts are recorded only after a real run. `keeper.stdout.log` / `keeper.stderr.log` and `keeper-error.json` are the local operational diagnostics.

A full `launchctl print` diagnostic unexpectedly included an inherited Almanax credential. Its value was not persisted in repository files and was not used. It must be rotated by the operator. The job now clears that unrelated environment variable; future status inspection emits only an explicit allowlist of safe status fields. Never print inherited environments.

## Frozen top-up amounts

Quotes fetched 2026-11-12 13:19:52 UTC: XLM/USD 0.143417; EURC/USD 1.1080586256352148, from the public Coinbase spot endpoints recorded in `topup-plan.json`. USDC and PYUSD are exact token amounts. XLM/EURC values are approximate USD equivalents, not executable swap quotes. Quantities round down to seven decimals, then split without losing a raw unit. Do not silently reprice a plan after its first half; the second gate checks its hash.

| Market | Additional total | First half | Second half |
|---|---:|---:|---:|
| USDC | 5900 | 2950 | 2950 |
| XLM | 20220.7548616 | 10110.3774308 | 10110.3774308 |
| PYUSD | 1900 | 950 | 950 |
| EURC | 1714.7107166 | 857.3553583 | 857.3553583 |

No top-up has been submitted or claimed to pass live simulation. Supply sufficient assets to the Ledger wallet first; all deposits simulate immediately before Ledger signing. Never fund a market by direct token transfer. There is no repeat permanent seed and no borrowing/strategy activation.

```sh
source scripts/cash_topup_commands.sh
cash_topup USDC 1 &&
cash_topup XLM 1 &&
cash_topup PYUSD 1 &&
cash_topup EURC 1 &&
node scripts/cash_operator_checks.cjs first-halves

# Only after confirmed TTL application; each call enforces the second-half gate:
cash_topup USDC 2 &&
cash_topup XLM 2 &&
cash_topup PYUSD 2 &&
cash_topup EURC 2
```

These use the existing `ledger_call`, `read_fresh`, `expect_read` and `check_admin` terminal helpers. Every deposit has before/after accounting checks and requires Ledger approval. Stop on a failed readback or unknown submission; reconcile before resuming individual remaining markets. DefiLlama preparation follows the approved top-up sequence and points only at fresh addresses; no PR is opened here.
