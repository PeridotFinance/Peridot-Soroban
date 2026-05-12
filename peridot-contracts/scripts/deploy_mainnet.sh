#!/usr/bin/env bash
set -euo pipefail

# Mainnet deployment — Peridot core lending protocol
# Markets: XLM (volatile), USDC (stable), EURC (stable)
# No margin controller. No mock tokens. No $P rewards at launch.
#
# Prerequisites:
#   1. Build WASMs with INIT_ADMIN baked in:
#        ADMIN=$(stellar keys public-key peridot-mainnet)
#        SIMPLE_PERIDOTTROLLER_INIT_ADMIN=$ADMIN \
#        JUMP_RATE_MODEL_INIT_ADMIN=$ADMIN \
#        PERIDOT_TOKEN_INIT_ADMIN=$ADMIN \
#        bash scripts/build_wasm.sh
#   2. Fund peridot-mainnet with ~100 XLM.

IDENTITY=${IDENTITY:-peridot-mainnet}
NETWORK="--network mainnet"
ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
INCLUSION_FEE=${INCLUSION_FEE:-10000}

# ── Mainnet asset SAC addresses ───────────────────────────────────────────────
XLM_TOKEN=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC_TOKEN=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
EURC_TOKEN=CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV

# ── Reflector oracle (mainnet) ────────────────────────────────────────────────
ORACLE_ID=CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN

# ── Interest rate models ──────────────────────────────────────────────────────
# JRM Volatile (XLM): base=1%, mult=18%, jump=400%, kink=80%
# Borrow APR: 1% @ 0% util → 15.4% @ kink → 95.4% @ 100%
JRM_VOL_BASE=10000       # 1%
JRM_VOL_MULT=180000      # 18%
JRM_VOL_JUMP=4000000     # 400%
JRM_VOL_KINK=800000      # 80%

# JRM Stable (USDC + EURC): base=0%, mult=8%, jump=200%, kink=90%
# Borrow APR: 0% @ 0% util → 7.2% @ kink → 27.2% @ 100%
JRM_STB_BASE=0           # 0%
JRM_STB_MULT=80000       # 8%
JRM_STB_JUMP=2000000     # 200%
JRM_STB_KINK=900000      # 90%

# ── Protocol parameters ───────────────────────────────────────────────────────
CF_XLM=700000            # 70% collateral factor
CF_USDC=900000           # 90%
CF_EURC=900000           # 90%
RESERVE_FACTOR=100000    # 10% of interest to reserves
FLASH_FEE=2500           # 0.25% flash loan fee
# Close factor (50%) and liquidation incentive (8%) use contract defaults.

# ── Seed amounts (initialize exchange rates, prevent first-depositor attack) ──
SEED_XLM=1000000000    # 100 XLM  (7 decimals)
SEED_USDC=100000000    # 10 USDC  (7 decimals)
SEED_EURC=100000000    # 10 EURC  (7 decimals)

# ── $P token — no rewards or treasury at launch ───────────────────────────────
# Reward speeds stay 0. The admin holds the $P mint authority and funds the
# controller treasury only when rewards are activated.
PERI_MAX_SUPPLY=100000000000000   # 100M tokens (6 decimals)

# ── WASM paths ────────────────────────────────────────────────────────────────
WASM_CONTROLLER="$ROOT_DIR/target/wasm32v1-none/release/simple_peridottroller.optimized.wasm"
WASM_VAULT="$ROOT_DIR/target/wasm32v1-none/release/receipt_vault.optimized.wasm"
WASM_JRM="$ROOT_DIR/target/wasm32v1-none/release/jump_rate_model.optimized.wasm"
WASM_PERI="$ROOT_DIR/target/wasm32v1-none/release/peridot_token.optimized.wasm"

for f in "$WASM_CONTROLLER" "$WASM_VAULT" "$WASM_JRM" "$WASM_PERI"; do
  [[ -f "$f" ]] || { echo "ERROR: $f not found. Run build_wasm.sh first with INIT_ADMIN set." >&2; exit 1; }
done

ADMIN=$(stellar keys public-key "$IDENTITY")

echo "================================================"
echo " Peridot Protocol — Mainnet Deployment"
echo "================================================"
echo " Identity   : $IDENTITY"
echo " Admin      : $ADMIN"
echo " Oracle     : $ORACLE_ID"
echo " XLM token  : $XLM_TOKEN"
echo " USDC token : $USDC_TOKEN"
echo " EURC token : $EURC_TOKEN"
echo ""
echo " JRM Volatile  base=$JRM_VOL_BASE mult=$JRM_VOL_MULT jump=$JRM_VOL_JUMP kink=$JRM_VOL_KINK"
echo " JRM Stable    base=$JRM_STB_BASE mult=$JRM_STB_MULT jump=$JRM_STB_JUMP kink=$JRM_STB_KINK"
echo " CF XLM=$CF_XLM  USDC=$CF_USDC  EURC=$CF_EURC"
echo " Reserve factor=$RESERVE_FACTOR  Flash fee=$FLASH_FEE"
echo " Seed amounts  XLM=$SEED_XLM  USDC=$SEED_USDC  EURC=$SEED_EURC"
echo " Inclusion fee=$INCLUSION_FEE stroops"
echo " \$P rewards: DISABLED at launch"
echo ""
read -rp "Deploy to MAINNET? Type 'yes' to proceed: " CONFIRM
[[ "$CONFIRM" == "yes" ]] || { echo "Aborted."; exit 1; }
echo ""

inv() { stellar contract invoke --source-account "$IDENTITY" $NETWORK --inclusion-fee "$INCLUSION_FEE" "$@"; }

upload_wasm() {
  local label=$1
  local wasm=$2
  echo "Uploading $label WASM..." >&2
  local hash
  hash=$(stellar contract upload \
    --wasm "$wasm" \
    --source-account "$IDENTITY" \
    $NETWORK \
    --inclusion-fee "$INCLUSION_FEE")
  echo "        $label hash: $hash" >&2
  printf '%s' "$hash"
}

deploy_hash() {
  local label=$1
  local hash=$2
  echo "$label" >&2
  stellar contract deploy \
    --wasm-hash "$hash" \
    --source-account "$IDENTITY" \
    $NETWORK \
    --inclusion-fee "$INCLUSION_FEE"
}

# Upload WASMs separately from instance deployment. This makes mainnet deploys
# resumable after RPC submission timeouts and avoids re-installing identical code.
echo "[0/14] Uploading WASM artifacts..."
CTRL_HASH=$(upload_wasm "SimplePeridottroller" "$WASM_CONTROLLER")
JRM_HASH=$(upload_wasm "JumpRateModel" "$WASM_JRM")
PERI_HASH=$(upload_wasm "PeridotToken" "$WASM_PERI")
VAULT_HASH=$(upload_wasm "ReceiptVault" "$WASM_VAULT")

# ── SimplePeridottroller ──────────────────────────────────────────────────────
echo "[1/14] Deploying SimplePeridottroller..."
CTRL_ID=$(deploy_hash "Deploying SimplePeridottroller instance..." "$CTRL_HASH")
echo "        $CTRL_ID"
inv --id "$CTRL_ID" -- initialize --admin "$ADMIN"

# ── JumpRateModel: Volatile (XLM) ─────────────────────────────────────────────
echo "[2/14] Deploying JumpRateModel (Volatile — XLM)..."
JRM_VOL_ID=$(deploy_hash "Deploying JumpRateModel instance (volatile)..." "$JRM_HASH")
echo "        $JRM_VOL_ID"
inv --id "$JRM_VOL_ID" -- initialize \
  --base "$JRM_VOL_BASE" --multiplier "$JRM_VOL_MULT" \
  --jump "$JRM_VOL_JUMP" --kink "$JRM_VOL_KINK" --admin "$ADMIN"

# ── JumpRateModel: Stable (USDC + EURC) ──────────────────────────────────────
echo "[3/14] Deploying JumpRateModel (Stable — USDC/EURC)..."
JRM_STB_ID=$(deploy_hash "Deploying JumpRateModel instance (stable)..." "$JRM_HASH")
echo "        $JRM_STB_ID"
inv --id "$JRM_STB_ID" -- initialize \
  --base "$JRM_STB_BASE" --multiplier "$JRM_STB_MULT" \
  --jump "$JRM_STB_JUMP" --kink "$JRM_STB_KINK" --admin "$ADMIN"

# ── PeridotToken ($P) ─────────────────────────────────────────────────────────
echo "[4/14] Deploying PeridotToken (\$P)..."
PERI_ID=$(deploy_hash "Deploying PeridotToken instance..." "$PERI_HASH")
echo "        $PERI_ID"
inv --id "$PERI_ID" -- initialize \
  --name "Peridot" --symbol "P" --decimals 6 --admin "$ADMIN" --max_supply "$PERI_MAX_SUPPLY"

# Register $P with the controller. Speeds stay at 0 — no rewards distributed
# until the admin explicitly sets speeds AND funds the controller treasury.
inv --id "$CTRL_ID" -- set_peridot_token --token "$PERI_ID"

# ── ReceiptVault markets ──────────────────────────────────────────────────────
echo "[5/14] Deploying ReceiptVault (XLM)..."
VA_ID=$(deploy_hash "Deploying ReceiptVault instance (XLM)..." "$VAULT_HASH")
echo "        $VA_ID"

echo "[6/14] Deploying ReceiptVault (USDC)..."
VB_ID=$(deploy_hash "Deploying ReceiptVault instance (USDC)..." "$VAULT_HASH")
echo "        $VB_ID"

echo "[7/14] Deploying ReceiptVault (EURC)..."
VC_ID=$(deploy_hash "Deploying ReceiptVault instance (EURC)..." "$VAULT_HASH")
echo "        $VC_ID"

# ── Initialize vaults ─────────────────────────────────────────────────────────
echo "[8/14] Initializing vaults..."
inv --id "$VA_ID" -- initialize \
  --token_address "$XLM_TOKEN" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
inv --id "$VB_ID" -- initialize \
  --token_address "$USDC_TOKEN" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
inv --id "$VC_ID" -- initialize \
  --token_address "$EURC_TOKEN" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"

# ── Configure vaults ──────────────────────────────────────────────────────────
echo "[9/14] Configuring vaults (flash fee, reserve factor, interest models)..."
inv --id "$VA_ID" -- set_flash_loan_fee --fee_scaled "$FLASH_FEE"
inv --id "$VB_ID" -- set_flash_loan_fee --fee_scaled "$FLASH_FEE"
inv --id "$VC_ID" -- set_flash_loan_fee --fee_scaled "$FLASH_FEE"

inv --id "$VA_ID" -- set_reserve_factor --reserve_factor_scaled "$RESERVE_FACTOR"
inv --id "$VB_ID" -- set_reserve_factor --reserve_factor_scaled "$RESERVE_FACTOR"
inv --id "$VC_ID" -- set_reserve_factor --reserve_factor_scaled "$RESERVE_FACTOR"

# Volatile JRM for XLM; stable JRM shared by USDC and EURC
inv --id "$VA_ID" -- set_interest_model --model "$JRM_VOL_ID"
inv --id "$VB_ID" -- set_interest_model --model "$JRM_STB_ID"
inv --id "$VC_ID" -- set_interest_model --model "$JRM_STB_ID"

# ── Wire controller ↔ markets ─────────────────────────────────────────────────
# add_market MUST come before set_peridottroller: the vault's set_peridottroller
# smoke-tests the controller's accrue_user_market which requires the market to
# already be registered.
echo "[10/14] Registering markets in controller..."
inv --id "$CTRL_ID" -- add_market --market "$VA_ID"
inv --id "$CTRL_ID" -- add_market --market "$VB_ID"
inv --id "$CTRL_ID" -- add_market --market "$VC_ID"

echo "[11/14] Wiring vaults to controller..."
inv --id "$VA_ID" -- set_peridottroller --peridottroller "$CTRL_ID"
inv --id "$VB_ID" -- set_peridottroller --peridottroller "$CTRL_ID"
inv --id "$VC_ID" -- set_peridottroller --peridottroller "$CTRL_ID"

# ── Collateral factors ────────────────────────────────────────────────────────
echo "[12/14] Setting collateral factors..."
inv --id "$CTRL_ID" -- set_market_cf --market "$VA_ID" --cf_scaled "$CF_XLM"
inv --id "$CTRL_ID" -- set_market_cf --market "$VB_ID" --cf_scaled "$CF_USDC"
inv --id "$CTRL_ID" -- set_market_cf --market "$VC_ID" --cf_scaled "$CF_EURC"

# ── Oracle ────────────────────────────────────────────────────────────────────
echo "[13/14] Setting Reflector oracle..."
inv --id "$CTRL_ID" -- set_oracle --oracle "$ORACLE_ID"

# ── Seed markets ─────────────────────────────────────────────────────────────
# Deposits a small amount into each vault to initialise the pToken exchange rate.
# The deployer account must hold at least SEED_XLM, SEED_USDC, and SEED_EURC.
echo "[14/14] Seeding markets..."
inv --id "$VA_ID" -- deposit --user "$ADMIN" --amount "$SEED_XLM"
inv --id "$VB_ID" -- deposit --user "$ADMIN" --amount "$SEED_USDC"
inv --id "$VC_ID" -- deposit --user "$ADMIN" --amount "$SEED_EURC"

# ── Deployment summary ────────────────────────────────────────────────────────
echo ""
echo "================================================"
echo " SAVE THESE — production contract addresses"
echo "================================================"
echo " Controller  : $CTRL_ID"
echo " JRM Volatile: $JRM_VOL_ID  (XLM)"
echo " JRM Stable  : $JRM_STB_ID  (USDC + EURC)"
echo " PERI (\$P)   : $PERI_ID"
echo " Vault XLM   : $VA_ID"
echo " Vault USDC  : $VB_ID"
echo " Vault EURC  : $VC_ID"
echo " Oracle      : $ORACLE_ID"
echo ""
echo "Next steps:"
echo "  1. Verify: CTRL_ID=$CTRL_ID VA_ID=$VA_ID VB_ID=$VB_ID bash scripts/verify_testnet.sh"
echo "  2. When ready to activate \$P rewards:"
echo "     a. Mint treasury to controller:"
echo "        stellar contract invoke --id $PERI_ID $NETWORK -- mint --to $CTRL_ID --amount <N>"
echo "     b. Set reward speeds:"
echo "        stellar contract invoke --id $CTRL_ID $NETWORK -- set_supply_speed --market <vault> --speed_per_sec <N>"
