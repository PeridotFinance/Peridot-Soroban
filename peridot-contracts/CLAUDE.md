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
  by post-call input balance-delta caps. ReceiptVault always marks live NAV losses down;
  only fixed-cash redemption sizing rejects quotes below 90% of independent accounting.
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
  Extreme pool/oracle values that overflow fixed-point intermediates are treated the
  same way: PriceRouter returns `None`, while AquariusLpVault retains only its bounded
  last-good NAV root.
- Past the strategy NAV stale bound, public boosted quotes fail soft. ReceiptVault
  redeems from its cached/accounting estimate with a nonzero cash minimum; only that
  protected exit may use the last NAV ratio, and the Aquarius quote must still satisfy
  the configured divergence guard. This preserves supplier exits during an oracle outage
  without making the stale value eligible for fresh deposits or unguarded swaps.
- Before mainnet: complete final review and Almanax scan, build with the production admin
  guard, and repeat live read-only pool/oracle/route preflights. These exact pools are not
  available on Testnet, so launch empty with conservative caps and verify configuration
  before accepting supplier capital. Do not enable borrowing until cross-market
  transaction-footprint and boosted-valuation staleness findings are redesigned.
- Full implementation, operational details, verified addresses, tests, and remaining
  gates are in `contracts/aquarius-lp-vault/CLAUDE.md` and `Agents.md`.

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
