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
  redeployed. XLM/yXLM targets roughly +/-2%; both PYUSD- and USDC-settled strategies
  target roughly +/-1% in their shared pool, with a six-hour recenter cooldown.
- The supply-only Mainnet rollout completed on 2026-08-27 after the clean Almanax
  scan and live pool/oracle/route preflights. A live deposit/partial-withdrawal smoke
  test passed for all three markets on 2026-08-28. Isolated controller
  `CCZKDMAP…ENGC` owns XLM market `CBRJTPI…ZECZ`, PYUSD market `CBNVNCP…MLMA`,
  and USDC market `CBIOHQF…AZP7`; their strategies are `CB3WLG4…H6RW`,
  `CANCOWO…5EKY`, and `CAQZ7XP…KGIN`. The deployer retains 24 XLM, 4.8 PYUSD,
  and 4.8 USDC pTokens backed by live full-range positions. The 2026-09-04 upgrade
  installed the final-holder ReceiptVault build and first concentrated strategy build,
  but its exact live XLM rebalance simulation exposed a range-ratio defect before any
  position mutation. CF remains 0; the controller's defense-in-depth borrow
  breakers expire automatically after 72 hours and must not be mistaken for the durable
  supply-only control. The single-worker DigitalOcean keeper is live for NAV, reward
  conversion, and cache
  refresh; range maintenance remains disabled until migration completes. Do not enable borrowing or
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
  all three positions and liquidities remain unchanged full-range. Deposits/redemptions
  were reopened, borrowing remains paused, and CF/borrows remain zero. The range-aware
  strategy-only fix is pinned in guarded recovery scripts. Final Almanax scan
  `e09ce891-d72f-4504-b97a-a381b6997ae7` over `4c2e48d..96ee13b` completed with zero
  findings. After funding, the reviewed strategy was uploaded and all three recovery
  proposals confirmed on 2026-09-05. Their pending hashes and deadlines were read back;
  the executor's earliest permitted start is 2026-09-06 08:09:21 CEST, including its
  30-second margin. Upload and proposals cost 66.9493182 XLM. Execute and verify all
  three concentrated positions before enabling range maintenance; do not re-propose.
  Keeper v0.2 deployment `dbbea016-b18a-415f-a023-f1b12581e545` is active from exact
  clean-scanned commit `4bfe469` with one live-signing worker and
  `RUN_REBALANCE=false`; its first six refresh-only transactions all succeeded.
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
