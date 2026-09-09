# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Build all contracts (from peridot-contracts/)
bash scripts/build_wasm.sh

# Build a single contract (from peridot-contracts/)
stellar contract build --package receipt-vault

# IMPORTANT: Always use `stellar contract build`, never `cargo build` for wasm output.
# Wasm artifacts: target/wasm32v1-none/release/*.wasm

# Run all tests
cargo test

# Run a single contract's tests
cargo test -p receipt-vault

# Run a specific test
cargo test -p receipt-vault -- test_deposit_receives_ptokens

# Lint and format
cargo clippy
cargo fmt
```

## Architecture

This is the **Peridot DeFi Lending Protocol** — a Compound-style lending system on Soroban (Stellar smart contracts).

### Contract Dependency Graph

```
SimplePeridottroller (risk manager)
  ├── ReceiptVault (one per market, holds underlying tokens)
  │     └── JumpRateModel (dynamic interest rates)
  ├── PeridotToken (reward token, minted by peridottroller)
  └── Oracle (Reflector, external)

MarginController (leveraged trading, optional)
  └── SwapAdapter (Aquarius DEX wrapper)
```

### Core Contracts

- **`receipt-vault`**: Per-market vault. Handles deposit/withdraw (mints/burns pTokens), borrow/repay with interest accrual, flash loans, supply/borrow caps. Delegates risk checks to peridottroller.
- **`simple-peridottroller`**: Cross-market risk manager. Oracle pricing, collateral factors, account liquidity checks, liquidation coordination, pause controls, reward distribution.
- **`jump-rate-model`**: Utilization-based interest rate with kink mechanic. Called by vaults during `update_interest()`.
- **`peridot-token`**: Reward token with max supply cap. Admin (peridottroller) mints on reward claims.

### Supporting Contracts

- **`margin-controller`** / **`swap-adapter`**: Optional leveraged margin trading via Aquarius DEX. Legacy Margin V1 exports are disabled; new work must target Margin V2 entrypoints only.
- **`aquarius-lp-vault`**: ReceiptVault-gated boosted vault backed by an Aquarius concentrated-liquidity position, as an alternative to a DeFindex vault. Implements the same boosted-vault ABI, so it attaches through the unmodified `set_boosted_vault`; the strategy must then be bound back to that market with `set_receipt_vault`. **See `contracts/aquarius-lp-vault/CLAUDE.md` for the full handoff** — design decisions, reward compounding, the Aquarius auth-tree gotcha, transaction-footprint limits, and open review findings.
- **`mocks/mock-token`**, **`mocks/mock-lending-vault`**: Test-only mocks.

### Aquarius LP Rollout Invariants

- All three +/-40 migrations completed on Mainnet on 2026-09-09, after the
  individual proposal timelocks matured. Strategies now run the reviewed
  `00a1e9097339cbd1ee194a7a7f938d8d72918a7c95f89520c23c5ea2d4b8162e` WASM;
  ReceiptVault binaries are unchanged. Actual positions: XLM `[-40,40]`, PYUSD
  and USDC `[-60,20]`, each 80 ticks total. Policies are half40/margin20,
  cooldown3600/divergence100, enabled. Independent post-migration pool snapshots
  matched strategy liquidity: 60426561125 / 12081792690 / 12081852089 respectively.
  All pToken supplies, strategy shares and tracked deposits were preserved.
  Deposits/redemptions are open, CF=0, borrows=0, borrow breakers renewed.
  Full-holder withdrawal simulations passed for all three after migration; no
  deposits or withdrawals were submitted. All 36 rollout transactions succeeded,
  costing 0.5377548 XLM total; deployer retained 48.5861082 XLM. No repeated upload.
  Keeper v0.4.0 remains on its existing single-worker deployment: XLM checks hourly,
  stablecoin checks and NAV/cache maintenance every twenty minutes. Reward floors,
  conversion routes and harvest threshold remain unchanged; no APY is guaranteed.
  First post-migration keeper cycle completed at 09:50:44 UTC with zero failures,
  six maintenance refreshes and one XLM harvest. Decoded harvest events confirm
  0.3470593 AQUA converted into 0.0006317 XLM, without a harvest_skipped event.
  EURC and the unrelated Margin V3 rollout were not modified. Complete upgrade and
  rebalance hashes are in Agents.md. The proposal-only records below are historical.
- PYUSD/USDC +/-40 extension approved on 2026-09-07: reuse the exact uploaded
  `00a1e909...` candidate below for BOTH existing stablecoin settlement strategies.
  `scripts/rollout_aquarius_stable40_mainnet.sh` requires LABEL=PYUSD or LABEL=USDC,
  defaults to read-only, verifies uploaded bytes without another upload, and pins
  each strategy/market/pool/settlement binding. Proposed policy is half40/margin20,
  cooldown3600/divergence100; existing twenty-minute stablecoin checks stay unchanged.
  Each strategy needs its own 24-hour proposal. Submission requires
  CONFIRM_MAINNET=PROPOSE_STABLE40; execution requires MODE=execute and
  CONFIRM_MAINNET=MIGRATE_STABLE40, both with PREFLIGHT_ONLY=false. Proposal resource
  plus inclusion fees are capped at 0.11 XLM each. Do not rerun the XLM proposal.
  No contract/keeper code changes; existing candidate scans and WASM tests apply.
  Both proposals are now confirmed: PYUSD tx `9d057b4a...`, safe execution
  2026-09-08 19:55:24 CEST; USDC tx `ddaf2861...`, safe execution 19:55:35 CEST.
  Combined proposal fees 0.0847930 XLM; no repeated WASM upload. Both were executed
  on 2026-09-09 as recorded above. See Agents.md for complete hashes and ETAs.
  EURC/USDC is assessment-only: real FX exposure, live concentrated pool spacing 60
  means minimum half-width 120, separate EURC/USDC oracle prices are available,
  but the checked EURC sale quote is 1.586% below oracle fair value. Investigate
  before launch; do not relax guards or repurpose the existing EURC market.
- XLM-only +/-40-tick pilot candidate (2026-09-07): aligned width changes now
  trigger the existing guarded rebalance even away from the old range's edge.
  Cooldown, oracle/quote checks, balance-delta checks and atomic rollback remain
  unchanged; no storage or ABI migration is added. Proposed policy is half-width
  40, margin 20, cooldown 3600, divergence 100 bps. The later approved extension
  applies the same policy to PYUSD/USDC, as recorded above.
  Keeper v0.4.0 adds `XLM_REBALANCE_INTERVAL_MS=3600000`; stable range checks and
  all NAV/cache refreshes remain on twenty-minute cycles. Harvest threshold and
  reward floors are untouched. This is not a guaranteed yield increase.
  `scripts/rollout_aquarius_xlm40_mainnet.sh` defaults to a read-only proposal
  preflight. It pins the XLM strategy, production admin and optimized target
  `00a1e9097339cbd1ee194a7a7f938d8d72918a7c95f89520c23c5ea2d4b8162e`
  (60,556 bytes, `aquarius_lp_vault.xlm40.optimized.wasm`). It refuses another
  pending target and does not reset a matching timelock. `MODE=execute` enforces
  maturity plus 30 seconds, pauses only XLM, simulates the exact live rebalance,
  checks the actual 80-tick position and accounting, then restores availability.
  Failure after pausing leaves XLM paused for inspection. Full-range Almanax scan
  `96041ede-a26e-4005-a87c-64dedb658f4b` (`4054dbd..f3d8b3e`) returned zero findings.
  XLM proposal `f222d9f4b44364db131c956e362077eaa4a728d2d5446df34cc7be5645e5173d`
  is confirmed; safe execution is 2026-09-08 19:07:08 CEST (ETA + 30 seconds).
  Execution completed on 2026-09-09 as recorded above. CLI contract-data
  output is CSV containing JSON columns, so the executor parses the ETA column
  strictly. Final full-range rescan `c7aa7b9b-ae99-4c71-813b-d9474665ed6b`
  (`4054dbd..a23fc82`) also returned zero findings. Keeper v0.4.0 immutable ref
  is pinned at `f3d8b3e`; live deployment `2a44ce76-f82e-454b-9cd9-13a1eb3bdecd`
  is ACTIVE with one worker. First live cycle confirmed six refreshes and three
  negative range checks with zero failures. Upload/proposal charged 66.8843791 XLM
  total; the inclusion-fee cap does not cap Soroban resource/rent fees. Deployer
  balance afterward was 35.6168927 XLM. See `Agents.md` for the transaction trail.
  That proposal-only checkpoint is superseded by the verified live migration above.
- Keeper v0.3.0 (`b5a4aea`, immutable branch/tag `aquarius-keeper-v0.3.0`) adds a
  harvest-only threshold: simulate the exact harvest and sign only when expected
  settlement funds reach `HARVEST_MIN_UNDERLYING_RAW=10000` (0.001 XLM/PYUSD/USDC).
  Include existing settlement cash, converted rewards and pool fees; exclude raw AQUA
  and failed nested calls. Below-threshold results defer to the next twenty-minute
  cycle without failing/restarting the worker. Successful harvests retain the six-hour
  interval; NAV/cache refreshes and range checks continue independently. This is a dust
  deployment gate, not a profitability guarantee or atomic on-chain minimum.
  Conversion skips are logged, and the on-chain harvest cooldown is checked before
  simulation. Missing simulation evidence fails closed. No reward floors were changed.
  All 17 keeper tests pass; Almanax full-range scan
  `506550e9-bf92-4897-91b5-f152ebb7d712` (`3895b7a..b5a4aea`) returned zero findings.
  Mainnet simulation at ledger 64317866 deferred all three harvests with expected
  settlement balances 4267/5329/5320 raw units and completed with zero failed steps.
  DigitalOcean dry-run deployment `640f70ff-db3c-427b-8ce9-3dc765ecf656` passed all
  three gates and all maintenance simulations. Live deployment
  `00ef5021-83da-40c1-b5bb-eb75adf0581a` is ACTIVE on the exact release commit,
  with one worker, DRY_RUN=false, HARVEST_ON_START=false and RUN_REBALANCE=true.
  Its first live cycle at 2026-09-07 15:07 UTC confirmed all six refresh transactions
  (ledgers 64317888..64317893), no rebalances and zero failures. No harvest was due
  during startup; the first scheduled threshold check is six hours after startup.
- Users enter only through ReceiptVault; AquariusLpVault is permanently bound to one
  matching market and rejects direct deposits. Its internal strategy shares are
  non-transferable and may grow only for that bound ReceiptVault; users transfer the
  ReceiptVault pTokens instead.
- XLM/yXLM uses an XLM-settled strategy. PYUSD and USDC use separate settlement
  strategies and ReceiptVaults that share the same concentrated PYUSD/USDC pool.
- `harvest()` converts the configurable primary reward (AQUA at launch) and gauge
  rewards into each market's underlying, then redeploys. Reward token and route are
  independently admin-configurable. Each reward also requires a governance-set,
  1e7-scaled minimum raw-underlying/raw-reward rate; a missing or breached floor leaves
  the reward idle instead of trusting the route pool's own quote. Empty or failed
  permissionless harvests do not consume the cooldown; it starts only after value is
  actually claimed, converted, or deployed.
- Root token-transfer authorizations required by the deployed Aquarius ABI are guarded
  by post-call input balance-delta caps, and every swap also verifies that its actual
  output-token balance increase meets the independently computed minimum. ReceiptVault
  persists and bounds the boosted strategy's output-vector length at binding, so a
  multi-asset strategy quote outage cannot change the authorized redemption shape.
  If that append-only key is absent on an upgraded/archived market, a failed live quote
  triggers a zero-share shape probe and persists the recovered count before redemption;
  the admin setter remains the fallback when the strategy cannot answer even that probe.
  ReceiptVault always marks live NAV losses down;
  fixed-cash redemption sizing first rejects quotes below 90% of independent accounting.
  If that exact, cash-bounded exit fails and the strategy supplied a lower positive live
  quote, ReceiptVault retries once with enough shares for the same nonzero cash minimum.
  This keeps withdrawals live after a genuine large loss without letting a dust quote
  force an otherwise healthy strategy into a full unwind.
- Use an isolated LP-market Peridottroller, CF=0, and borrow paused. Deployment scripts
  require the controller address explicitly. Reuse the appropriate existing JRM while
  the markets remain supply-only.
- The temporary supply-only rollout points the isolated controller and all strategies
  directly at Reflector. Oracle symbol aliases map XLM and yXLM to `Other("XLM")`, and
  PYUSD and USDC to `Other("USDC")`. This assumes both pairs remain at par; keep CF=0,
  borrowing paused, the executable-quote deployment gates, and runtime pool-divergence
  guards. PriceRouter remains the preferred depeg-aware follow-up before collateral or
  borrowing is enabled, and it returns no pegged-asset price whenever the executable
  observation pool is unavailable rather than assuming even the configured floor.
  Extreme pool/oracle values that exceed their final numeric range are treated the same
  way: PriceRouter returns `None`, while AquariusLpVault retains only its bounded
  last-good NAV root. AquariusLpVault uses exact Soroban `U256` products for mul-div
  pricing and share accounting, so a representable result never becomes a saturation
  sentinel merely because the intermediate product exceeds `u128`.
- Past the strategy NAV stale bound, public boosted quotes fail soft. ReceiptVault
  redeems from its cached/accounting estimate with a nonzero cash minimum; only that
  protected exit may use the last NAV ratio, and the Aquarius quote must still satisfy
  the configured divergence guard. This preserves supplier exits during an oracle outage
  without making the stale value eligible for fresh deposits or unguarded swaps.
- Concentrated positions use a keeper-refreshed snapshot of their actual token0/token1
  composition, valued only at the independent oracle ratio. The keeper recenters only
  near a configured edge and only after a tighter two-sided pool/oracle check. A
  rebalance withdraws the old position, derives the exact token ratio from Aquarius'
  quote for the new spacing-aligned range, requires that ratio to match the independent
  live-tick/range geometry within five percentage points, performs one guarded
  excess-leg swap, and
  atomically fails unless a new position is minted with at least 95% of pair value
  redeployed. All three now target half-width 40 ticks (roughly 0.8% total price
  width), with a one-hour recenter cooldown. Range centers follow the pool tick grid.
- The supply-only Mainnet rollout completed on 2026-08-27 after the clean Almanax
  scan and live pool/oracle/route preflights. A live deposit/partial-withdrawal smoke
  test passed for all three markets on 2026-08-28. Isolated controller
  `CCZKDMAP…ENGC` owns XLM market `CBRJTPI…ZECZ`, PYUSD market `CBNVNCP…MLMA`,
  and USDC market `CBIOHQF…AZP7`; their strategies are `CB3WLG4…H6RW`,
  `CANCOWO…5EKY`, and `CAQZ7XP…KGIN`. The deployer retains 24 XLM, 4.8 PYUSD,
  and 4.8 USDC pTokens backed by live concentrated positions. The 2026-09-04 upgrade
  installed the final-holder ReceiptVault build and first concentrated strategy build,
  but its exact live XLM rebalance simulation exposed a range-ratio defect before any
  position mutation. The reviewed recovery migrated all three positions on 2026-09-06:
  XLM ticks `[-200,200]`; both stablecoin strategies `[-120,80]`. CF remains 0;
  the controller's defense-in-depth borrow
  breakers expire automatically after 72 hours and must not be mistaken for the durable
  supply-only control. The single-worker DigitalOcean keeper is live for NAV, reward
  conversion, and cache
  refresh; range maintenance is now enabled. Do not enable borrowing or
  collateral until cross-market footprint, depeg-aware PriceRouter, and boosted-
  valuation staleness work is redesigned and re-audited. See `Agents.md` for complete
  IDs, hashes, transactions, and next steps.
- The concentrated-range and final-holder release is implemented in `f2fcdca` with
  balance-delta deposit hardening in `4bfe469`, both pushed on `leveraged-fix`.
  Almanax full-range scan `4f97a6f1-24d9-4887-9a69-d2c3d4995761` found one High
  that `4bfe469` fixes; exact fix scan `3a290a92-65af-4fbf-8951-d2d428d8c598`
  completed with zero findings. After one balance-rejected upload created no state, the
  funded 2026-09-03 retry uploaded both candidate artifacts and staged all six exact
  24-hour upgrades. All six upgrades executed on 2026-09-04. The guarded executor then
  stopped on a no-send XLM rebalance: an aligned narrow range at the live tick needed an
  approximately 0.8972:1 XLM/yXLM ratio, while the deployed code assumed equal value and
  would have left 5.138% idle, just above its hard 5% cap. The simulation rolled back;
  all three positions and liquidities initially remained unchanged full-range. Deposits/redemptions
  were reopened, borrowing remains paused, and CF/borrows remain zero. The range-aware
  strategy-only fix is pinned in guarded recovery scripts. Final Almanax scan
  `e09ce891-d72f-4504-b97a-a381b6997ae7` over `4c2e48d..96ee13b` completed with zero
  findings. After funding, the reviewed strategy was uploaded and all three recovery
  proposals confirmed on 2026-09-05. All three upgrades and rebalances completed after
  maturity on 2026-09-06, with 36/36 successful transactions costing 0.6531179 XLM.
  Accounting invariants and all three full-holder withdrawal simulations passed;
  deposits/redemptions are open. Do not repeat the completed proposals or migration.
  Keeper v0.2 deployment `908b8045-5ffb-41fe-abbf-8ded8c87b6f8` is active from exact
  clean-scanned commit `4bfe469` with one live-signing worker and
  `RUN_REBALANCE=true`; its first six refresh transactions and all three range checks
  completed with zero failures. Harvest remains scheduled, with no startup harvest.
- The separate existing XLM/USDC/EURC markets have the clean-scanned ReceiptVault
  borrow-footprint fix staged under their 24-hour upgrade timelocks. All three target
  hash `5f35bc16…04e1` and mature on 2026-08-29 between 11:04:44 and 11:04:54 CEST.
  They still run the old code until the required pause/execute/verify/unpause sequence.
  `scripts/execute_receipt_vault_borrow_fix_mainnet.sh` defaults to a read-only,
  pinned-state preflight and gates mutation until 11:05:24 CEST, including a 30-second
  safety margin. It is resumable only with explicit acknowledgement of an all-paused
  prior attempt, leaves markets paused on incomplete verification, restores the exact
  original policy only after all checks pass, and never submits the final borrow.
- Full implementation, operational details, verified addresses, tests, and remaining
  gates are in `contracts/aquarius-lp-vault/CLAUDE.md` and `Agents.md`.
- The production keeper is `bots/aquarius-keeper`: one process services XLM, PYUSD,
  and USDC serially with a dedicated, fee-only signer to avoid Stellar sequence
  collisions. `.do/aquarius-keeper.yaml` defines a single 512 MiB DigitalOcean worker,
  automatic deployments disabled, starting in dry-run mode. Never commit its secret or
  scale multiple live instances with the same key.

### Key Patterns

- **Fixed-point math**: `SCALE_1E6 = 1_000_000` for rates/percentages (e.g., `600_000` = 60%). Borrow index uses `1e18` scaling.
- **`#![no_std]`**: All contracts. No standard library, no randomness, fully deterministic.
- **Auth**: Admin functions use `admin.require_auth()`, user actions use `user.require_auth()`. Liquidation hooks (`repay_on_behalf`, `seize`) only callable when vault is wired to peridottroller.
- **Lazy interest accrual**: Interest updates happen on user actions (deposit/withdraw/borrow/repay), not on a schedule.
- **Re-entry protection**: Cross-contract aggregation uses exclusion parameters to skip the calling vault.
- **Oracle staleness**: Price stale if `price.timestamp + k*resolution < now` (k=2 default). Missing prices treat collateral as 0 USD.
- **Events**: Single-tuple topics: `(Symbol("event_name"),)`.
- **Checked arithmetic**: Use `.checked_add()`, `.checked_mul()` etc. to prevent overflow. `overflow-checks = true` in release profile.
- **Cross-contract safety (FIND-039)**: Use `try_invoke_contract()` instead of `invoke_contract()` for all external contract calls to prevent account lockout from TTL-expired or malicious markets. Apply conservative fallbacks: collateral failures → $0, debt failures → skip market, token/price failures → skip market. Critical for `sum_positions_usd`, `exit_market`, and liquidation flows.
- **Core lending footprint**: ReceiptVault `get_account_snapshot()` must use its
  narrow, key-specific TTL/read path. Calling the broad `ensure_initialized()`
  from this controller hot loop makes the XLM/USDC/EURC three-market borrow
  exceed Soroban's 100-entry footprint (132 entries versus 81 with the narrow
  path). Keep `bump_ttl()` as the separate permissionless global keepalive.

### Storage

Contracts use `env.storage().persistent()` and `.instance()` for key-value state. Key enums are defined at the top of each contract's `lib.rs`.

## Workspace

Soroban SDK version: **25.0.0** (workspace dependency in root `Cargo.toml`).

OpenZeppelin Stellar contracts are a git submodule at `../../openzeppelin-stellar-contracts`. If builds fail with missing deps, run: `git submodule update --init --recursive`

## Deployment

Deploy scripts are in `scripts/`. The main flow:

```bash
export IDENTITY=dev
bash scripts/build_wasm.sh
bash scripts/deploy_testnet.sh        # deploys full protocol
bash scripts/verify_testnet.sh        # checks state
bash scripts/teardown_testnet.sh      # pauses everything
```

Contract invocations require `--` before function args:
```bash
stellar contract invoke --id <id> --source-account dev --network testnet -- deposit --user <addr> --amount 1000000
```

## Troubleshooting

- **"reference-types not enabled"**: Wrong build target. Use `stellar contract build`, not `cargo build`.
- **Missing OpenZeppelin deps**: Run `git submodule update --init --recursive`.
- **Test snapshots changed**: Test snapshots live in `contracts/*/test_snapshots/`. These are auto-generated; commit updated snapshots after intentional contract changes.
