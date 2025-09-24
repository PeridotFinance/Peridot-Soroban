## Peridot Soroban Contracts Deployment Guide

This guide covers end-to-end deployment of the Peridot lending protocol contracts to local sandbox and Stellar testnet, including build, configuration, verification, teardown/reset, and common troubleshooting.

### Prerequisites

- Rust toolchain and wasm target:
  - `rustup target add wasm32-unknown-unknown`
- Soroban CLI installed and on PATH.
- For testnet:
  - A funded testnet account and soroban identity configured (e.g., `soroban config identity generate myadmin`).

### Build Artifacts

From `receipt-vault` root directory:

```bash
bash scripts/build_wasm.sh
```

Artifacts are emitted to `target/wasm32-unknown-unknown/release/`:

- `simple-peridottroller.wasm`
- `receipt-vault.wasm`
- `jump-rate-model.wasm`
- `peridot-token.wasm`

### Deploy to Sandbox

Requirements:

- Local sandbox running: `soroban rpc serve` in another terminal.

Steps:

```bash
bash scripts/build_wasm.sh
bash scripts/deploy_sandbox.sh
```

What the script does:

- Deploys and initializes `SimplePeridottroller` with `alice` as admin.
- Deploys and initializes `JumpRateModel` (2% base, 18% multiplier, 400% jump, 80% kink).
- Deploys `Peridot Token` and sets controller as token admin; controller is configured with token via `set_peridot_token`.
- Deploys two `ReceiptVault` instances; initializes with placeholder underlying token addresses (derived from local dev keys `bob`, `carol`).
- Wires vaults to controller (`set_peridottroller`), adds markets on controller.
- Sets per-market CF (collateral factor) and reward speeds.
- Configures flash-loan premium (`set_flash_loan_fee` at 2%) on both vaults so fees route to reserves.

Script output prints the deployed contract IDs for controller (`CTRL_ID`), jump model, vaults (`VA_ID`, `VB_ID`), and PERI token.

### Deploy to Testnet

Configure identity and token addresses, then run:

```bash
soroban config identity generate myadmin   # if not already created
export IDENTITY=myadmin
export TOKEN_A=<asset_contract_address_A_on_testnet>
export TOKEN_B=<asset_contract_address_B_on_testnet>
bash scripts/build_wasm.sh
bash scripts/deploy_testnet.sh
```

What the script does:

- Uses `IDENTITY` to derive `ADMIN` address.
- Deploys and initializes `SimplePeridottroller` with `ADMIN`.
- Deploys `JumpRateModel` and configures base/mult/jump/kink.
- Deploys `Peridot Token` (symbol `P`, 6 decimals) and points controller to it (`set_peridot_token`).
- Deploys two `ReceiptVault` markets and initializes with `TOKEN_A` and `TOKEN_B` (placeholders must be real contract addresses).
- Wires vaults to controller, adds markets, sets CF (e.g., 1_000_000 on `VB_ID`) and reward speeds on `VA_ID`.
- Configures flash-loan premium on each vault (`set_flash_loan_fee`); default uses `FLASH_FEE` env var (2% if unset).

Environment variables:

- `IDENTITY`: soroban identity name (default `myadmin`).
- `TOKEN_A`, `TOKEN_B`: underlying asset contract IDs for the two vaults.
- `FLASH_FEE`: flash-loan premium scaled by 1e6 (defaults to `20000`, i.e., 2%).

The script prints IDs: `CTRL_ID`, `VA_ID`, `VB_ID`, `JRM_ID`, `PERI_ID`.

### Verify Deployment (testnet)

Use the provided verification script to read controller and vault configuration:

```bash
export CTRL_ID=<controller_id>
export VA_ID=<vault_a_id>
export VB_ID=<vault_b_id>
bash scripts/verify_testnet.sh
```

This prints:

- Controller admin and oracle (if set).
- Per-market collateral factor and pause statuses (deposit/borrow/redeem/liquidation).
- Vault admin, exchange rate, total deposited, pTokens, total borrowed, reserves, admin fees.

### Teardown / Reset (testnet)

To safely disable markets and zero reward speeds:

```bash
export CTRL_ID=<controller_id>
export VA_ID=<vault_a_id>
export VB_ID=<vault_b_id>
bash scripts/teardown_testnet.sh
```

Actions performed:

- `set_pause_*` (deposit, borrow, redeem, liquidation) to `true` on both markets.
- Set `set_supply_speed` and `set_borrow_speed` to `0` for both markets.
- Reset collateral factor to `500_000` (50%).

### Optional Configuration

- Set oracle on controller:

```bash
soroban contract invoke --network testnet --id "$CTRL_ID" -- set_oracle --oracle <reflector_oracle_id>
soroban contract invoke --network testnet --id "$CTRL_ID" -- set_oracle_max_age_multiplier --k 3
```

- Point vault to JumpRateModel (instead of static rates):

```bash
soroban contract invoke --network testnet --id "$VA_ID" -- set_interest_model --model "$JRM_ID"
soroban contract invoke --network testnet --id "$VB_ID" -- set_interest_model --model "$JRM_ID"
```

- Reserve routing and liquidation fee:

```bash
soroban contract invoke --network testnet --id "$CTRL_ID" -- set_reserve_recipient --recipient <address_or_contract>
soroban contract invoke --network testnet --id "$CTRL_ID" -- set_liquidation_fee --fee_scaled 50000   # 5%
soroban contract invoke --network testnet --id "$VA_ID" -- set_flash_loan_fee --fee_scaled 20000       # 2%
```

### Operational Tips

- Always build artifacts before deploying: `bash scripts/build_wasm.sh`.
- Keep a log of printed IDs from deploy scripts; export them in your shell for subsequent commands.
- Use the verify script after any change to confirm expected configuration.
- For upgrades: both `ReceiptVault` and `SimplePeridottroller` expose `upgrade_wasm` (admin-only). Upload the new code and pass the 32-byte wasm hash.

### Troubleshooting

- Missing or wrong token addresses on testnet: the vault `initialize` will succeed but you won’t be able to move assets. Ensure `TOKEN_A` and `TOKEN_B` are correct.
- Paused actions: if actions revert, check pause flags via `verify_testnet.sh`.
- Collateral factor not set: controller defaults to 50% if unset; set explicitly with `set_market_cf`.
- Re-entry errors during hypothetical checks: ensure you are not calling cross-market flows from within a market callback.
- Identity issues on testnet: verify identity and address with `soroban keys address <identity> --network testnet`.
