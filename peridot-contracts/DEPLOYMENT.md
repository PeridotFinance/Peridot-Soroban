## Peridot Soroban Contracts Deployment Guide

This guide covers end-to-end deployment of the Peridot lending protocol contracts to local sandbox and Stellar testnet, including build, configuration, verification, teardown/reset, and common troubleshooting.

### Prerequisites

- Rust toolchain installed.
- Stellar CLI installed and on PATH (`stellar` command). If needed, install via your package manager or cargo.
- For init-gated contracts, set expected admin env vars before build/deploy:
  - `PERIDOT_TOKEN_INIT_ADMIN`
  - `SWAP_ADAPTER_INIT_ADMIN`
  - `JUMP_RATE_MODEL_INIT_ADMIN`
  - These must match the admin address you will pass to `initialize`.
- Testnet network configured in the CLI:
  ```bash
  stellar config network add testnet \
    --rpc-url https://soroban-testnet.stellar.org \
    --network-passphrase "Test SDF Future Network ; October 2022"
  ```
- A funded testnet account (identity) for deployments. You can generate and fund in one step:
  ```bash
  stellar keys generate --global dev --network testnet --fund
  ```

### Build Artifacts (for Testnet)

Testnet requires the v1 Wasm target (wasm32v1-none). Build each contract with the Stellar CLI from its contract directory:

```bash
# ReceiptVault
cd contracts/receipt-vault && stellar contract build && cd -
# SimplePeridottroller
cd contracts/simple-peridottroller && stellar contract build && cd -
# JumpRateModel
cd contracts/jump-rate-model && stellar contract build && cd -
# PeridotToken
cd contracts/peridot-token && stellar contract build && cd -
```

Artifacts are emitted per contract to `contracts/<name>/target/wasm32v1-none/release/`:

- `receipt_vault.wasm`
- `simple_peridottroller.wasm`
- `jump_rate_model.wasm`
- `peridot_token.wasm`

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

### Deploy to Testnet (step-by-step)

Below is the canonical, CLI-aligned flow (matching the Stellar docs). Replace placeholders where noted.

1. Set identity and capture admin address

```bash
IDENTITY=dev
ADMIN=$(stellar keys address "$IDENTITY" --network testnet)
echo "Admin: $ADMIN"
```

2. Deploy ReceiptVault (market)

```bash
RV_WASM=contracts/receipt-vault/target/wasm32v1-none/release/receipt_vault.wasm
RV_ID=$(stellar contract deploy \
  --wasm "$RV_WASM" \
  --source-account "$IDENTITY" \
  --network testnet \
  --alias peridot_vault)
echo "ReceiptVault: $RV_ID"
```

3. Initialize ReceiptVault with an underlying token

```bash
TOKEN_A=<asset_contract_id_on_testnet>

stellar contract invoke \
  --id peridot_vault \
  --source-account "$IDENTITY" \
  --network testnet \
  -- \
  initialize \
  --token "$TOKEN_A" \
  --supply_yearly_rate_scaled 0 \
  --borrow_yearly_rate_scaled 0 \
  --admin "$ADMIN"
```

4. Deploy and configure JumpRateModel (dynamic APR)

```bash
JRM_WASM=contracts/jump-rate-model/target/wasm32v1-none/release/jump_rate_model.wasm
JRM_ID=$(stellar contract deploy \
  --wasm "$JRM_WASM" \
  --source-account "$IDENTITY" \
  --network testnet \
  --alias peridot_jrm)

stellar contract invoke --id "$JRM_ID" --source-account "$IDENTITY" --network testnet -- \
  initialize --base 20000 --multiplier 180000 --jump 4000000 --kink 800000 --admin "$ADMIN"

stellar contract invoke --id peridot_vault --source-account "$IDENTITY" --network testnet -- \
  set_interest_model --model "$JRM_ID"
```

5. Deploy controller and wire market

```bash
CTRL_WASM=contracts/simple-peridottroller/target/wasm32v1-none/release/simple_peridottroller.wasm
CTRL_ID=$(stellar contract deploy \
  --wasm "$CTRL_WASM" \
  --source-account "$IDENTITY" \
  --network testnet \
  --alias peridot_ctrl)

stellar contract invoke --id "$CTRL_ID" --source-account "$IDENTITY" --network testnet -- \
  initialize --admin "$ADMIN"

# Wire vault → controller and add market
stellar contract invoke --id peridot_vault --source-account "$IDENTITY" --network testnet -- \
  set_peridottroller --peridottroller "$CTRL_ID"

stellar contract invoke --id "$CTRL_ID" --source-account "$IDENTITY" --network testnet -- \
  add_market --market peridot_vault

# Set collateral factor (example: 60%)
stellar contract invoke --id "$CTRL_ID" --source-account "$IDENTITY" --network testnet -- \
  set_market_cf --market peridot_vault --cf_scaled 600000
```

6. Deploy and configure Peridot reward token (optional but recommended)

```bash
PERI_WASM=contracts/peridot-token/target/wasm32v1-none/release/peridot_token.wasm
PERI_ID=$(stellar contract deploy \
  --wasm "$PERI_WASM" \
  --source-account "$IDENTITY" \
  --network testnet \
  --alias peridot_peri)

stellar contract invoke --id "$PERI_ID" --source-account "$IDENTITY" --network testnet -- \
  initialize --name Peridot --symbol P --decimals 6 --admin "$CTRL_ID"

stellar contract invoke --id "$CTRL_ID" --source-account "$IDENTITY" --network testnet -- \
  set_peridot_token --token "$PERI_ID"
```

7. (Optional) Add a second market and/or set reward speeds on controller

```bash
# Second vault for TOKEN_B
RV2_WASM=$RV_WASM
RV2_ID=$(stellar contract deploy \
  --wasm "$RV2_WASM" \
  --source-account "$IDENTITY" \
  --network testnet \
  --alias peridot_vault_b)

TOKEN_B=<asset_contract_id_on_testnet>
stellar contract invoke --id "$RV2_ID" --source-account "$IDENTITY" --network testnet -- \
  initialize --token "$TOKEN_B" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"

stellar contract invoke --id "$RV2_ID" --source-account "$IDENTITY" --network testnet -- \
  set_peridottroller --peridottroller "$CTRL_ID"

stellar contract invoke --id "$CTRL_ID" --source-account "$IDENTITY" --network testnet -- \
  add_market --market "$RV2_ID"

# Example reward speeds
stellar contract invoke --id "$CTRL_ID" --source-account "$IDENTITY" --network testnet -- \
  set_supply_speed --market peridot_vault --speed_per_sec 5
stellar contract invoke --id "$CTRL_ID" --source-account "$IDENTITY" --network testnet -- \
  set_borrow_speed --market peridot_vault --speed_per_sec 3
```

Notes:

- All `invoke` commands require the `--` separator before the function and its arguments.
- If your identity is not funded, the CLI will fail with "Account not found". Re-run the `stellar keys generate ... --fund` step or fund via friendbot.

### Verify Deployment (testnet)

Use the provided verification script to read controller and vault configuration:

```bash
export CTRL_ID=<controller_id>
export VA_ID=<vault_a_id>
export VB_ID=<vault_b_id>
bash scripts/verify_testnet.sh   # uses stellar CLI under the hood
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
stellar contract invoke --network testnet --id "$CTRL_ID" -- \
  set_oracle --oracle <reflector_oracle_id>
stellar contract invoke --network testnet --id "$CTRL_ID" -- \
  set_oracle_max_age_multiplier --k 3
```

- Point vault to JumpRateModel (instead of static rates):

```bash
stellar contract invoke --network testnet --id "$VA_ID" -- \
  set_interest_model --model "$JRM_ID"
stellar contract invoke --network testnet --id "$VB_ID" -- \
  set_interest_model --model "$JRM_ID"
```

- Reserve routing and liquidation fee:

```bash
stellar contract invoke --network testnet --id "$CTRL_ID" -- \
  set_reserve_recipient --recipient <address_or_contract>
stellar contract invoke --network testnet --id "$CTRL_ID" -- \
  set_liquidation_fee --fee_scaled 50000   # 5%
stellar contract invoke --network testnet --id "$VA_ID" -- \
  set_flash_loan_fee --fee_scaled 20000     # 2%
```

### Operational Tips

- Always build artifacts before deploying: `bash scripts/build_wasm.sh`.
- Keep a log of printed IDs from deploy scripts; export them in your shell for subsequent commands.
- Use the verify script after any change to confirm expected configuration.
- For upgrades: both `ReceiptVault` and `SimplePeridottroller` expose `upgrade_wasm` (admin-only). Upload the new code and pass the 32-byte wasm hash.
- For margin trading: `MarginController`, `SwapAdapter`, `PeridotToken`, and `JumpRateModel` also expose `upgrade_wasm` (admin-only).
- Use deploy scripts that initialize immediately after deploy to avoid front-run initialization.

### Init Admin Expectations

Some contracts enforce an expected admin at initialization time. This prevents third‑party initialization if a contract is deployed but not initialized immediately.

Set these env vars **before building/deploying** so `initialize` accepts your admin:

```bash
export PERIDOT_TOKEN_INIT_ADMIN="$ADMIN"
export SWAP_ADAPTER_INIT_ADMIN="$ADMIN"
export JUMP_RATE_MODEL_INIT_ADMIN="$ADMIN"
export SMART_ACCOUNT_FACTORY_INIT_ADMIN="$ADMIN"
export SMART_ACCOUNT_FACTORY_ID="<FACTORY_CONTRACT_ID>"
```

If these are not set, contracts default to the hardcoded dev admin value.

### TTL Maintenance (State Archival)

Soroban expires persistent storage entries if they are not extended. We bump TTLs inside core entrypoints, but **admin should still ensure periodic traffic** so state does not expire.

Recommended practice:
- Schedule a small “keepalive” job (daily or weekly) that calls:
  - `swap-adapter.bump_ttl`
  - any frequent read/write on `margin-controller` (e.g., `get_user_positions`) to bump its critical keys
  - `peridot-token.name` or `symbol` (bumps its critical keys)
  - `jump-rate-model.get_borrow_rate` (bumps its critical keys)
  - `smart-account-factory.get_account` (bumps its critical keys)
- For high-availability, run this keepalive even when user activity is low.

If critical keys expire, the contract can become unusable or, in worst cases, allow re‑initialization. TTL bumping prevents that.

### Smart Accounts (Basic) Deployment

Build:
- `contracts/smart-account-basic`
- `contracts/smart-account-factory`

Deploy and initialize (example flow):
1) Deploy the factory WASM and call `initialize(admin)`.
2) Upload the Basic account WASM and call `set_wasm_hash(AccountType::Basic, wasm_hash)` on the factory.
3) Call `create_account(config, salt)` where:
   - `owner` = user address
   - `signer` = ed25519 public key (BytesN<32>)
   - `peridottroller` = controller ID
   - `margin_controller` = margin controller ID

The returned account address is used as the `user`/`borrower` in vault and margin calls. It intercepts `require_auth` and verifies signatures.

### Boosted Markets (DeFindex)

To enable a boosted market, set the DeFindex vault address on the target ReceiptVault:

```bash
VAULT=<RECEIPT_VAULT_ID>
DEFINDEX_VAULT=<DEFINDEX_VAULT_ID>
ADMIN=<ADMIN_ADDRESS>

stellar contract invoke --id "$VAULT" --source-account dev --network testnet -- \
  set_boosted_vault --admin "$ADMIN" --boosted_vault "$DEFINDEX_VAULT"
```

Quick testnet helpers:
- Deploy factory + set wasm hash:
  - `bash scripts/deploy_smart_accounts_testnet.sh`
- Create a Basic smart account (auto-generates signer + salt):
  - `PERIDOTTROLLER_ID=<CTRL_ID> MARGIN_CONTROLLER_ID=<MARGIN_ID> bash scripts/create_smart_account_testnet.sh`
  - Output includes signer public key and salt used.

Example: borrow via Smart Account address (testnet)
```bash
SMART_ACCOUNT="<SMART_ACCOUNT_ADDRESS>"
VAULT="<RECEIPT_VAULT_ID>"

stellar contract invoke --id "$VAULT" --source-account dev --network testnet -- \
  borrow --user "$SMART_ACCOUNT" --amount 1000000
```

### Troubleshooting

- "HostError: reference-types not enabled": you built the wrong target. Rebuild with `stellar contract build` to produce `wasm32v1-none` artifacts and redeploy.
- "Account not found": your identity isn’t funded. Run `stellar keys generate --global <name> --network testnet --fund` or fund via friendbot.
- Missing or wrong token addresses: vault `initialize` may succeed but transfers will fail. Ensure underlying token contract IDs are valid on testnet.
- Paused actions: if actions revert, check pause flags via `verify_testnet.sh`.
- Collateral factor not set: controller defaults to 50% (500_000). Set explicitly with `set_market_cf`.
- Re-entry issues during hypothetical checks: avoid calling cross-market flows from within a market callback.
