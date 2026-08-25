#!/usr/bin/env bash
# Deploys two NEW, supply-only ReceiptVault markets over the same concentrated
# PYUSD/USDC Aquarius pool:
#
#   PYUSD ReceiptVault -> PYUSD-settled AquariusLpVault (pool token index 0)
#   USDC  ReceiptVault -> USDC-settled  AquariusLpVault (pool token index 1)
#
# A single AquariusLpVault cannot safely settle both assets: it has one
# immutable underlying token, one share ledger, and one ReceiptVault binding.
# The two strategy instances still use the same underlying concentrated pool.
#
#   ADMIN=G... IDENTITY=peridot-mainnet \
#     bash scripts/deploy_aquarius_pyusd_usdc_mainnet.sh
#
# Set PREFLIGHT_ONLY=true to perform every read-only pool/oracle check and stop
# before building or submitting a transaction.
#
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-public}
IDENTITY=${IDENTITY:?set IDENTITY to the deploying stellar CLI identity}
ADMIN=${ADMIN:?set ADMIN to the pinned init-admin public key}

# Verified on-chain on 2026-08-24. This is deliberately the concentrated pool,
# with token order [PYUSD, USDC].
POOL=${POOL:-CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX}
PYUSD=${PYUSD:-CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2}
USDC=${USDC:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}

# Reflector publishes USDC as Other("USDC") but does not currently publish
# PYUSD. For this temporary supply-only rollout both tokens deliberately use
# the USDC feed. The executable pool quote below remains a deployment gate,
# and each vault's runtime pool-divergence guard blocks new LP entry if the
# pair moves outside its bound.
UPSTREAM_ORACLE=${UPSTREAM_ORACLE:-CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN}
PEG_PROBE_AMOUNT=${PEG_PROBE_AMOUNT:-1000000000} # 100 PYUSD (7 decimals)
MIN_PEG_RATIO_BPS=${MIN_PEG_RATIO_BPS:-9500}

# Require an explicit governance choice. The recommended value is the separate
# LP-market Peridottroller, not the existing DeFindex market group.
CONTROLLER=${CONTROLLER:-}
JRM=${JRM:-CCI5LBBNYOASPQ62GIRY54PDEYWWURJB75HNRAFOU4LTOU3XBC73IB5I}
AQUA=${AQUA:-CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK}
# Reward conversion routes only; neither is the invested PYUSD/USDC pool.
AQUA_PYUSD_ROUTE=${AQUA_PYUSD_ROUTE:-CADFWSBBD6VMCL45DEPZ37X3JNXOZXIWEVJJTHMQH3UEB3JSQVJSPG2I}
AQUA_USDC_ROUTE=${AQUA_USDC_ROUTE:-CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O}

# Keep the initial combined strategy capacity at 25,000 units instead of
# silently doubling the old PYUSD-only 25,000-unit budget. Raise the two sides
# together only after measuring pool depth, reward dilution, and deposit mix.
PYUSD_SUPPLY_CAP=${PYUSD_SUPPLY_CAP:-125000000000} # 12,500 PYUSD
PYUSD_MAX_DEPLOY=${PYUSD_MAX_DEPLOY:-125000000000} # 12,500 PYUSD
USDC_SUPPLY_CAP=${USDC_SUPPLY_CAP:-125000000000}   # 12,500 USDC
USDC_MAX_DEPLOY=${USDC_MAX_DEPLOY:-125000000000}   # 12,500 USDC
SLIPPAGE_BPS=${SLIPPAGE_BPS:-100}                  # 1%
MAX_DIVERGENCE_BPS=${MAX_DIVERGENCE_BPS:-200}      # 2%
IDLE_BUFFER_BPS=${IDLE_BUFFER_BPS:-3000}           # 30%
HARVEST_COOLDOWN=${HARVEST_COOLDOWN:-3600}
PREFLIGHT_ONLY=${PREFLIGHT_ONLY:-false}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}

if [[ "$PREFLIGHT_ONLY" != "true" && "$PREFLIGHT_ONLY" != "false" ]]; then
  echo "ERROR: PREFLIGHT_ONLY must be true or false." >&2
  exit 2
fi
case "$PEG_PROBE_AMOUNT:$MIN_PEG_RATIO_BPS" in
  *[!0-9:]*|:*|*:)
    echo "ERROR: PEG_PROBE_AMOUNT and MIN_PEG_RATIO_BPS must be positive integers." >&2
    exit 2
    ;;
esac
if (( PEG_PROBE_AMOUNT == 0 || MIN_PEG_RATIO_BPS == 0 || MIN_PEG_RATIO_BPS > 10000 )); then
  echo "ERROR: peg probe must be positive and peg floor must be within 1..10000 bps." >&2
  exit 2
fi

invoke() {
  stellar contract invoke \
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" -- "${@:2}"
}

view() {
  stellar contract invoke \
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" --send no -- "${@:2}"
}

verify_route() {
  local route=$1
  local settlement=$2
  local label=$3
  local route_tokens
  route_tokens=$(view "$route" get_tokens)
  if [[ "$route_tokens" != *"$settlement"* || "$route_tokens" != *"$AQUA"* ]]; then
    echo "ERROR: $label must contain AQUA and its settlement asset; got $route_tokens" >&2
    exit 1
  fi
}

echo "==> Verifying concentrated PYUSD/USDC pool and both AQUA routes"
POOL_TYPE=$(view "$POOL" pool_type)
POOL_TOKENS=$(view "$POOL" get_tokens)
EXPECTED_POOL_TOKENS="[\"$PYUSD\",\"$USDC\"]"
if [[ "$POOL_TYPE" != '"concentrated"' || "$POOL_TOKENS" != "$EXPECTED_POOL_TOKENS" ]]; then
  echo "ERROR: unexpected PYUSD/USDC pool type or token order." >&2
  echo "type=$POOL_TYPE tokens=$POOL_TOKENS" >&2
  exit 1
fi
verify_route "$AQUA_PYUSD_ROUTE" "$PYUSD" AQUA_PYUSD_ROUTE
verify_route "$AQUA_USDC_ROUTE" "$USDC" AQUA_USDC_ROUTE

USDC_PRICE=$(view "$UPSTREAM_ORACLE" lastprice --asset '{"Other":"USDC"}')
if [[ "$USDC_PRICE" == "null" ]]; then
  echo 'ERROR: upstream oracle has no Other("USDC") price.' >&2
  exit 1
fi

PEG_QUOTE=$(view "$POOL" estimate_swap \
  --in_idx 0 --out_idx 1 --in_amount "$PEG_PROBE_AMOUNT")
PEG_QUOTE_RAW=${PEG_QUOTE//\"/}
case "$PEG_QUOTE_RAW" in
  ''|*[!0-9]*)
    echo "ERROR: invalid PYUSD/USDC probe quote: $PEG_QUOTE" >&2
    exit 1
    ;;
esac
if (( PEG_QUOTE_RAW * 10000 < PEG_PROBE_AMOUNT * MIN_PEG_RATIO_BPS )); then
  echo "ERROR: PYUSD/USDC executable quote is below the configured peg floor." >&2
  echo "probe_in=$PEG_PROBE_AMOUNT quote_out=$PEG_QUOTE_RAW floor_bps=$MIN_PEG_RATIO_BPS" >&2
  exit 1
fi

if [[ "$PREFLIGHT_ONLY" == "true" ]]; then
  echo "Preflight passed: concentrated pool, oracle, peg quote, and both AQUA routes are valid."
  exit 0
fi

: "${CONTROLLER:?set CONTROLLER explicitly (prefer the isolated LP-market Peridottroller)}"
if [[ "$CONFIRM_MAINNET" != "DEPLOY" ]]; then
  echo "ERROR: set CONFIRM_MAINNET=DEPLOY to submit mainnet transactions." >&2
  exit 2
fi
EXPECTED_ADMIN=$(stellar keys public-key "$IDENTITY")
if [[ "$ADMIN" != "$EXPECTED_ADMIN" ]]; then
  echo "ERROR: ADMIN must equal the deploying identity's public key." >&2
  exit 2
fi
CONTROLLER_ADMIN=$(view "$CONTROLLER" get_admin)
CONTROLLER_ORACLE=$(view "$CONTROLLER" get_oracle)
CONTROLLER_PYUSD_PRICE=$(view "$CONTROLLER" get_price_usd --token "$PYUSD")
CONTROLLER_USDC_PRICE=$(view "$CONTROLLER" get_price_usd --token "$USDC")
if [[ "$CONTROLLER_ADMIN" != "\"$ADMIN\"" || \
      "$CONTROLLER_ORACLE" != "\"$UPSTREAM_ORACLE\"" || \
      "$CONTROLLER_PYUSD_PRICE" == "null" || \
      "$CONTROLLER_PYUSD_PRICE" != "$CONTROLLER_USDC_PRICE" ]]; then
  echo "ERROR: controller admin/oracle/PYUSD-USDC alias verification failed." >&2
  exit 1
fi

echo "==> Building deployable WASMs"
INIT_ADMIN="$ADMIN" bash scripts/build_wasm.sh

echo "==> Deploying PYUSD-settled AquariusLpVault (pool index 0)"
PYUSD_VAULT_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/aquarius_lp_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    PYUSD vault = $PYUSD_VAULT_ID"
invoke "$PYUSD_VAULT_ID" initialize \
  --admin "$ADMIN" --pool "$POOL" --underlying_index 0 --oracle "$UPSTREAM_ORACLE"
invoke "$PYUSD_VAULT_ID" set_oracle_symbol \
  --admin_addr "$ADMIN" --token "$PYUSD" --symbol '"USDC"'
invoke "$PYUSD_VAULT_ID" set_oracle_symbol \
  --admin_addr "$ADMIN" --token "$USDC" --symbol '"USDC"'
invoke "$PYUSD_VAULT_ID" set_max_deploy \
  --admin_addr "$ADMIN" --max_deploy "$PYUSD_MAX_DEPLOY"
invoke "$PYUSD_VAULT_ID" set_slippage_bps --admin_addr "$ADMIN" --bps "$SLIPPAGE_BPS"
invoke "$PYUSD_VAULT_ID" set_harvest_cooldown \
  --admin_addr "$ADMIN" --seconds "$HARVEST_COOLDOWN"
invoke "$PYUSD_VAULT_ID" set_max_pool_divergence_bps \
  --admin_addr "$ADMIN" --bps "$MAX_DIVERGENCE_BPS"
invoke "$PYUSD_VAULT_ID" set_primary_reward_token \
  --admin_addr "$ADMIN" --reward_token "$AQUA"
invoke "$PYUSD_VAULT_ID" set_reward_route \
  --admin_addr "$ADMIN" --reward_token "$AQUA" --route "$AQUA_PYUSD_ROUTE"
PYUSD_NAV_ROOT=$(invoke "$PYUSD_VAULT_ID" refresh_nav_root)
if [[ "${PYUSD_NAV_ROOT//\"/}" == "0" ]]; then
  echo "ERROR: PYUSD/USDC Reflector aliases produced a zero NAV root." >&2
  exit 1
fi

echo "==> Deploying USDC-settled AquariusLpVault (pool index 1)"
USDC_VAULT_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/aquarius_lp_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    USDC vault = $USDC_VAULT_ID"
invoke "$USDC_VAULT_ID" initialize \
  --admin "$ADMIN" --pool "$POOL" --underlying_index 1 --oracle "$UPSTREAM_ORACLE"
invoke "$USDC_VAULT_ID" set_oracle_symbol \
  --admin_addr "$ADMIN" --token "$PYUSD" --symbol '"USDC"'
invoke "$USDC_VAULT_ID" set_oracle_symbol \
  --admin_addr "$ADMIN" --token "$USDC" --symbol '"USDC"'
invoke "$USDC_VAULT_ID" set_max_deploy \
  --admin_addr "$ADMIN" --max_deploy "$USDC_MAX_DEPLOY"
invoke "$USDC_VAULT_ID" set_slippage_bps --admin_addr "$ADMIN" --bps "$SLIPPAGE_BPS"
invoke "$USDC_VAULT_ID" set_harvest_cooldown \
  --admin_addr "$ADMIN" --seconds "$HARVEST_COOLDOWN"
invoke "$USDC_VAULT_ID" set_max_pool_divergence_bps \
  --admin_addr "$ADMIN" --bps "$MAX_DIVERGENCE_BPS"
invoke "$USDC_VAULT_ID" set_primary_reward_token \
  --admin_addr "$ADMIN" --reward_token "$AQUA"
invoke "$USDC_VAULT_ID" set_reward_route \
  --admin_addr "$ADMIN" --reward_token "$AQUA" --route "$AQUA_USDC_ROUTE"
USDC_NAV_ROOT=$(invoke "$USDC_VAULT_ID" refresh_nav_root)
if [[ "${USDC_NAV_ROOT//\"/}" == "0" ]]; then
  echo "ERROR: PYUSD/USDC Reflector aliases produced a zero NAV root." >&2
  exit 1
fi

echo "==> Deploying and binding the PYUSD ReceiptVault"
PYUSD_MARKET_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/receipt_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    PYUSD market = $PYUSD_MARKET_ID"
invoke "$PYUSD_MARKET_ID" initialize \
  --token_address "$PYUSD" --supply_yearly_rate_scaled 0 \
  --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
invoke "$PYUSD_MARKET_ID" set_interest_model --model "$JRM"
invoke "$PYUSD_MARKET_ID" set_idle_cash_buffer_bps \
  --admin "$ADMIN" --idle_cash_buffer_bps "$IDLE_BUFFER_BPS"
invoke "$PYUSD_MARKET_ID" set_supply_cap --cap "$PYUSD_SUPPLY_CAP"
invoke "$PYUSD_MARKET_ID" set_boosted_vault \
  --admin "$ADMIN" --boosted_vault "$PYUSD_VAULT_ID"
invoke "$PYUSD_VAULT_ID" set_receipt_vault \
  --admin_addr "$ADMIN" --receipt_vault "$PYUSD_MARKET_ID"

echo "==> Deploying and binding the USDC ReceiptVault"
USDC_MARKET_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/receipt_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    USDC market = $USDC_MARKET_ID"
invoke "$USDC_MARKET_ID" initialize \
  --token_address "$USDC" --supply_yearly_rate_scaled 0 \
  --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
invoke "$USDC_MARKET_ID" set_interest_model --model "$JRM"
invoke "$USDC_MARKET_ID" set_idle_cash_buffer_bps \
  --admin "$ADMIN" --idle_cash_buffer_bps "$IDLE_BUFFER_BPS"
invoke "$USDC_MARKET_ID" set_supply_cap --cap "$USDC_SUPPLY_CAP"
invoke "$USDC_MARKET_ID" set_boosted_vault \
  --admin "$ADMIN" --boosted_vault "$USDC_VAULT_ID"
invoke "$USDC_VAULT_ID" set_receipt_vault \
  --admin_addr "$ADMIN" --receipt_vault "$USDC_MARKET_ID"

echo "==> Listing both markets with zero collateral value and borrowing paused"
for market_id in "$PYUSD_MARKET_ID" "$USDC_MARKET_ID"; do
  invoke "$CONTROLLER" add_market --market "$market_id"
  # New markets default to CF=0. The setter rejects values below 1%, so leave it.
  invoke "$CONTROLLER" set_pause_borrow --market "$market_id" --paused true
  invoke "$market_id" set_peridottroller --peridottroller "$CONTROLLER"
done

PYUSD_CF=$(view "$CONTROLLER" get_market_cf --market "$PYUSD_MARKET_ID")
USDC_CF=$(view "$CONTROLLER" get_market_cf --market "$USDC_MARKET_ID")
PYUSD_BORROW_PAUSED=$(view "$CONTROLLER" is_borrow_paused --market "$PYUSD_MARKET_ID")
USDC_BORROW_PAUSED=$(view "$CONTROLLER" is_borrow_paused --market "$USDC_MARKET_ID")
PYUSD_BOUND_MARKET=$(view "$PYUSD_VAULT_ID" get_receipt_vault)
USDC_BOUND_MARKET=$(view "$USDC_VAULT_ID" get_receipt_vault)
if [[ "$PYUSD_CF" != "0" || "$USDC_CF" != "0" || \
      "$PYUSD_BORROW_PAUSED" != "true" || "$USDC_BORROW_PAUSED" != "true" || \
      "$PYUSD_BOUND_MARKET" != "\"$PYUSD_MARKET_ID\"" || \
      "$USDC_BOUND_MARKET" != "\"$USDC_MARKET_ID\"" ]]; then
  echo "ERROR: post-deployment supply-only or ReceiptVault binding check failed." >&2
  exit 1
fi

cat <<SUMMARY

────────────────────────────────────────────────────────────────
  PYUSD + USDC concentrated yield markets deployed (supply-only)
────────────────────────────────────────────────────────────────
  Shared pool          $POOL
  Oracle               $UPSTREAM_ORACLE (PYUSD + USDC -> Other("USDC"))

  PYUSD Aquarius vault $PYUSD_VAULT_ID
  PYUSD ReceiptVault   $PYUSD_MARKET_ID
  PYUSD AQUA route     $AQUA_PYUSD_ROUTE

  USDC Aquarius vault  $USDC_VAULT_ID
  USDC ReceiptVault    $USDC_MARKET_ID
  USDC AQUA route      $AQUA_USDC_ROUTE

  Controller           $CONTROLLER
  Collateral factors   PYUSD=$PYUSD_CF USDC=$USDC_CF
  Borrow paused        PYUSD=$PYUSD_BORROW_PAUSED USDC=$USDC_BORROW_PAUSED

  Start one keeper per settlement vault:
    VAULT_ID=$PYUSD_VAULT_ID MARKET_ID=$PYUSD_MARKET_ID IDENTITY=$IDENTITY NETWORK=$NETWORK \
      bash scripts/run_aquarius_vault_keeper.sh
    VAULT_ID=$USDC_VAULT_ID MARKET_ID=$USDC_MARKET_ID IDENTITY=$IDENTITY NETWORK=$NETWORK \
      bash scripts/run_aquarius_vault_keeper.sh
────────────────────────────────────────────────────────────────
SUMMARY
