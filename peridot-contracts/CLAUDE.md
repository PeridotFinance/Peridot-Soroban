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

- **`margin-controller`** / **`swap-adapter`**: Leveraged margin trading via Aquarius DEX.
- **`mocks/mock-token`**, **`mocks/mock-lending-vault`**: Test-only mocks.

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
