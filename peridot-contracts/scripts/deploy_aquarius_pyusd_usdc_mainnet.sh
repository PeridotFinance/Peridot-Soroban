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
REWARD_PROBE_AMOUNT=${REWARD_PROBE_AMOUNT:-10000000000} # 1,000 AQUA
REWARD_RATE_FLOOR_BPS=${REWARD_RATE_FLOOR_BPS:-9500}    # 95% of live route quote
PYUSD_AQUA_MIN_RATE_SCALED=${PYUSD_AQUA_MIN_RATE_SCALED:-}
USDC_AQUA_MIN_RATE_SCALED=${USDC_AQUA_MIN_RATE_SCALED:-}

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
INCLUSION_FEE=${INCLUSION_FEE:-100000} # max 0.01 XLM per submitted transaction

if [[ "$PREFLIGHT_ONLY" != "true" && "$PREFLIGHT_ONLY" != "false" ]]; then
  echo "ERROR: PREFLIGHT_ONLY must be true or false." >&2
  exit 2
fi
case "$PEG_PROBE_AMOUNT:$MIN_PEG_RATIO_BPS:$REWARD_PROBE_AMOUNT:$REWARD_RATE_FLOOR_BPS" in
  *[!0-9:]*|:*|*:)
    echo "ERROR: PEG_PROBE_AMOUNT and MIN_PEG_RATIO_BPS must be positive integers." >&2
    exit 2
    ;;
esac
if (( PEG_PROBE_AMOUNT == 0 || MIN_PEG_RATIO_BPS == 0 || MIN_PEG_RATIO_BPS > 10000 || \
      REWARD_PROBE_AMOUNT == 0 || REWARD_RATE_FLOOR_BPS == 0 || \
      REWARD_RATE_FLOOR_BPS > 10000 )); then
  echo "ERROR: peg probe must be positive and peg floor must be within 1..10000 bps." >&2
  exit 2
fi

invoke() {
  stellar contract invoke \
    --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" -- "${@:2}"
}

view() {
  stellar contract invoke \
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" --send no -- "${@:2}"
}

reward_min_rate() {
  local route=$1
  local settlement=$2
  local label=$3
  local configured=$4
  local route_tokens
  local in_idx
  local out_idx
  local quote
  local quote_raw
  local quoted_rate_scaled
  route_tokens=$(view "$route" get_tokens)
  if [[ "$route_tokens" == "[\"$AQUA\",\"$settlement\"]" ]]; then
    in_idx=0
    out_idx=1
  elif [[ "$route_tokens" == "[\"$settlement\",\"$AQUA\"]" ]]; then
    in_idx=1
    out_idx=0
  else
    echo "ERROR: $label must contain AQUA and its settlement asset; got $route_tokens" >&2
    exit 1
  fi
  quote=$(view "$route" estimate_swap \
    --in_idx "$in_idx" --out_idx "$out_idx" --in_amount "$REWARD_PROBE_AMOUNT")
  quote_raw=${quote//\"/}
  case "$quote_raw" in
    ''|*[!0-9]*)
      echo "ERROR: invalid $label reward quote: $quote" >&2
      exit 1
      ;;
  esac
  quoted_rate_scaled=$(( quote_raw * 10000000 / REWARD_PROBE_AMOUNT ))
  if [[ -z "$configured" ]]; then
    configured=$(( quoted_rate_scaled * REWARD_RATE_FLOOR_BPS / 10000 ))
  fi
  case "$configured" in
    ''|*[!0-9]*)
      echo "ERROR: $label minimum rate must be a positive integer." >&2
      exit 1
      ;;
  esac
  if (( quote_raw == 0 || configured == 0 )); then
    echo "ERROR: $label reward quote and configured rate floor must be non-zero." >&2
    exit 1
  fi
  echo "$configured"
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
PYUSD_AQUA_MIN_RATE_SCALED=$(reward_min_rate \
  "$AQUA_PYUSD_ROUTE" "$PYUSD" AQUA_PYUSD_ROUTE "$PYUSD_AQUA_MIN_RATE_SCALED")
USDC_AQUA_MIN_RATE_SCALED=$(reward_min_rate \
  "$AQUA_USDC_ROUTE" "$USDC" AQUA_USDC_ROUTE "$USDC_AQUA_MIN_RATE_SCALED")

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
  echo "AQUA/PYUSD minimum rate (1e7 scale): $PYUSD_AQUA_MIN_RATE_SCALED"
  echo "AQUA/USDC minimum rate (1e7 scale): $USDC_AQUA_MIN_RATE_SCALED"
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
if [[ "$CONTROLLER_ADMIN" != "\"$ADMIN\"" || \
      "$CONTROLLER_ORACLE" != "\"$UPSTREAM_ORACLE\"" ]]; then
  echo "ERROR: controller admin/oracle verification failed." >&2
  exit 1
fi

echo "==> Building deployable WASMs"
INIT_ADMIN="$ADMIN" bash scripts/build_wasm.sh

echo "==> Deploying PYUSD-settled AquariusLpVault (pool index 0)"
PYUSD_VAULT_ID=$(stellar contract deploy \
  --no-cache --inclusion-fee "$INCLUSION_FEE" \
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
  --admin_addr "$ADMIN" --reward_token "\"$AQUA\""
invoke "$PYUSD_VAULT_ID" set_reward_route \
  --admin_addr "$ADMIN" --reward_token "$AQUA" --route "\"$AQUA_PYUSD_ROUTE\""
invoke "$PYUSD_VAULT_ID" set_reward_min_rate \
  --admin_addr "$ADMIN" --reward_token "$AQUA" \
  --min_rate_scaled "$PYUSD_AQUA_MIN_RATE_SCALED"
PYUSD_NAV_ROOT=$(invoke "$PYUSD_VAULT_ID" refresh_nav_root)
if [[ "${PYUSD_NAV_ROOT//\"/}" == "0" ]]; then
  echo "ERROR: PYUSD/USDC Reflector aliases produced a zero NAV root." >&2
  exit 1
fi

echo "==> Deploying USDC-settled AquariusLpVault (pool index 1)"
USDC_VAULT_ID=$(stellar contract deploy \
  --no-cache --inclusion-fee "$INCLUSION_FEE" \
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
  --admin_addr "$ADMIN" --reward_token "\"$AQUA\""
invoke "$USDC_VAULT_ID" set_reward_route \
  --admin_addr "$ADMIN" --reward_token "$AQUA" --route "\"$AQUA_USDC_ROUTE\""
invoke "$USDC_VAULT_ID" set_reward_min_rate \
  --admin_addr "$ADMIN" --reward_token "$AQUA" \
  --min_rate_scaled "$USDC_AQUA_MIN_RATE_SCALED"
USDC_NAV_ROOT=$(invoke "$USDC_VAULT_ID" refresh_nav_root)
if [[ "${USDC_NAV_ROOT//\"/}" == "0" ]]; then
  echo "ERROR: PYUSD/USDC Reflector aliases produced a zero NAV root." >&2
  exit 1
fi

echo "==> Deploying and binding the PYUSD ReceiptVault"
PYUSD_MARKET_ID=$(stellar contract deploy \
  --no-cache --inclusion-fee "$INCLUSION_FEE" \
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
PYUSD_BOOSTED_ASSET_COUNT=$(view "$PYUSD_MARKET_ID" get_boosted_asset_count)
if [[ "$PYUSD_BOOSTED_ASSET_COUNT" != "1" ]]; then
  echo "ERROR: PYUSD market persisted boosted asset count $PYUSD_BOOSTED_ASSET_COUNT; expected 1." >&2
  exit 1
fi
invoke "$PYUSD_VAULT_ID" set_receipt_vault \
  --admin_addr "$ADMIN" --receipt_vault "$PYUSD_MARKET_ID"

echo "==> Deploying and binding the USDC ReceiptVault"
USDC_MARKET_ID=$(stellar contract deploy \
  --no-cache --inclusion-fee "$INCLUSION_FEE" \
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
USDC_BOOSTED_ASSET_COUNT=$(view "$USDC_MARKET_ID" get_boosted_asset_count)
if [[ "$USDC_BOOSTED_ASSET_COUNT" != "1" ]]; then
  echo "ERROR: USDC market persisted boosted asset count $USDC_BOOSTED_ASSET_COUNT; expected 1." >&2
  exit 1
fi
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
PYUSD_CF_RAW=${PYUSD_CF//\"/}
USDC_CF_RAW=${USDC_CF//\"/}
PYUSD_BORROW_PAUSED=$(view "$CONTROLLER" is_borrow_paused --market "$PYUSD_MARKET_ID")
USDC_BORROW_PAUSED=$(view "$CONTROLLER" is_borrow_paused --market "$USDC_MARKET_ID")
PYUSD_BOUND_MARKET=$(view "$PYUSD_VAULT_ID" get_receipt_vault)
USDC_BOUND_MARKET=$(view "$USDC_VAULT_ID" get_receipt_vault)
CONTROLLER_PYUSD_PRICE=$(view "$CONTROLLER" get_price_usd --token "$PYUSD")
CONTROLLER_USDC_PRICE=$(view "$CONTROLLER" get_price_usd --token "$USDC")
if [[ "$PYUSD_CF_RAW" != "0" || "$USDC_CF_RAW" != "0" || \
      "$PYUSD_BORROW_PAUSED" != "true" || "$USDC_BORROW_PAUSED" != "true" || \
      "$PYUSD_BOUND_MARKET" != "\"$PYUSD_MARKET_ID\"" || \
      "$USDC_BOUND_MARKET" != "\"$USDC_MARKET_ID\"" || \
      "$CONTROLLER_PYUSD_PRICE" == "null" || \
      "$CONTROLLER_PYUSD_PRICE" != "$CONTROLLER_USDC_PRICE" ]]; then
  echo "ERROR: post-deployment supply-only or ReceiptVault binding check failed." >&2
  echo "pyusd_price=$CONTROLLER_PYUSD_PRICE usdc_price=$CONTROLLER_USDC_PRICE" >&2
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
  PYUSD AQUA floor     $PYUSD_AQUA_MIN_RATE_SCALED (1e7 raw PYUSD/raw AQUA)

  USDC Aquarius vault  $USDC_VAULT_ID
  USDC ReceiptVault    $USDC_MARKET_ID
  USDC AQUA route      $AQUA_USDC_ROUTE
  USDC AQUA floor      $USDC_AQUA_MIN_RATE_SCALED (1e7 raw USDC/raw AQUA)

  Controller           $CONTROLLER
  Collateral factors   PYUSD=$PYUSD_CF_RAW USDC=$USDC_CF_RAW
  Borrow paused        PYUSD=$PYUSD_BORROW_PAUSED USDC=$USDC_BORROW_PAUSED

  Start one keeper per settlement vault:
    VAULT_ID=$PYUSD_VAULT_ID MARKET_ID=$PYUSD_MARKET_ID IDENTITY=$IDENTITY NETWORK=$NETWORK \
      bash scripts/run_aquarius_vault_keeper.sh
    VAULT_ID=$USDC_VAULT_ID MARKET_ID=$USDC_MARKET_ID IDENTITY=$IDENTITY NETWORK=$NETWORK \
      bash scripts/run_aquarius_vault_keeper.sh
────────────────────────────────────────────────────────────────
SUMMARY
