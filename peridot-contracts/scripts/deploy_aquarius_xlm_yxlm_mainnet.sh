#!/usr/bin/env bash
# Deploys a NEW, supply-only XLM ReceiptVault whose idle liquidity earns the
# XLM/yXLM Aquarius LP yield. It deliberately does not replace the existing
# XLM market's DeFindex vault.
#
#   ADMIN=G... IDENTITY=peridot-mainnet \
#     bash scripts/deploy_aquarius_xlm_yxlm_mainnet.sh
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

# Verified on-chain on 2026-08-24. The concentrated pool token order is
# [native XLM, yXLM], so XLM is the settlement asset at index 0.
POOL=${POOL:-CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F}
XLM=${XLM:-CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA}
YXLM=${YXLM:-CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC}
UNDERLYING_INDEX=${UNDERLYING_INDEX:-0}

# Reflector publishes XLM as Other("XLM"). For this temporary supply-only
# rollout both XLM and yXLM deliberately use that same feed. The executable
# pool quote below remains a deployment gate, and the vault's runtime pool-
# divergence guard blocks new LP entry when XLM/yXLM moves outside its bound.
UPSTREAM_ORACLE=${UPSTREAM_ORACLE:-CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN}
PEG_PROBE_AMOUNT=${PEG_PROBE_AMOUNT:-1000000000} # 100 yXLM (7 decimals)
MIN_PEG_RATIO_BPS=${MIN_PEG_RATIO_BPS:-9000}

# Require an explicit governance choice. The recommended value is the separate
# LP-market Peridottroller, not the existing DeFindex market group.
CONTROLLER=${CONTROLLER:-}
JRM=${JRM:-CCPJFBH5WSNZVMCUQCBM4X5334L6ZL3W4Q33XJAK45RCDHJ2JGJ5AP6A}
AQUA=${AQUA:-CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK}
# Aquarius' documented XLM/AQUA pool. The vault only accepts a reward route
# containing both the reward token and this market's underlying token.
AQUA_ROUTE=${AQUA_ROUTE:-CCY2PXGMKNQHO7WNYXEWX76L2C5BH3JUW3RCATGUYKY7QQTRILBZIFWV}
REWARD_PROBE_AMOUNT=${REWARD_PROBE_AMOUNT:-10000000000} # 1,000 AQUA
REWARD_RATE_FLOOR_BPS=${REWARD_RATE_FLOOR_BPS:-9500}    # 95% of live route quote
AQUA_MIN_RATE_SCALED=${AQUA_MIN_RATE_SCALED:-}

SUPPLY_CAP=${SUPPLY_CAP:-1000000000000}  # 100,000 XLM
MAX_DEPLOY=${MAX_DEPLOY:-1000000000000}  # 100,000 XLM
SLIPPAGE_BPS=${SLIPPAGE_BPS:-100}        # 1%
MAX_DIVERGENCE_BPS=${MAX_DIVERGENCE_BPS:-200} # 2%
IDLE_BUFFER_BPS=${IDLE_BUFFER_BPS:-3000} # 30%
HARVEST_COOLDOWN=${HARVEST_COOLDOWN:-3600}
PREFLIGHT_ONLY=${PREFLIGHT_ONLY:-false}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}

if [[ "$UNDERLYING_INDEX" != "0" ]]; then
  echo "ERROR: this dedicated XLM market requires UNDERLYING_INDEX=0." >&2
  exit 2
fi
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
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" -- "${@:2}"
}

view() {
  stellar contract invoke \
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" --send no -- "${@:2}"
}

echo "==> Verifying XLM/yXLM and AQUA/XLM pools"
POOL_TYPE=$(view "$POOL" pool_type)
POOL_TOKENS=$(view "$POOL" get_tokens)
EXPECTED_POOL_TOKENS="[\"$XLM\",\"$YXLM\"]"
if [[ "$POOL_TYPE" != '"concentrated"' || "$POOL_TOKENS" != "$EXPECTED_POOL_TOKENS" ]]; then
  echo "ERROR: unexpected XLM/yXLM pool type or token order." >&2
  echo "type=$POOL_TYPE tokens=$POOL_TOKENS" >&2
  exit 1
fi
ROUTE_TOKENS=$(view "$AQUA_ROUTE" get_tokens)
if [[ "$ROUTE_TOKENS" == "[\"$AQUA\",\"$XLM\"]" ]]; then
  REWARD_IN_IDX=0
  REWARD_OUT_IDX=1
elif [[ "$ROUTE_TOKENS" == "[\"$XLM\",\"$AQUA\"]" ]]; then
  REWARD_IN_IDX=1
  REWARD_OUT_IDX=0
else
  echo "ERROR: AQUA_ROUTE must contain AQUA and XLM; got $ROUTE_TOKENS" >&2
  exit 1
fi
REWARD_QUOTE=$(view "$AQUA_ROUTE" estimate_swap \
  --in_idx "$REWARD_IN_IDX" --out_idx "$REWARD_OUT_IDX" \
  --in_amount "$REWARD_PROBE_AMOUNT")
REWARD_QUOTE_RAW=${REWARD_QUOTE//\"/}
case "$REWARD_QUOTE_RAW" in
  ''|*[!0-9]*)
    echo "ERROR: invalid AQUA/XLM reward quote: $REWARD_QUOTE" >&2
    exit 1
    ;;
esac
QUOTED_REWARD_RATE_SCALED=$(( REWARD_QUOTE_RAW * 10000000 / REWARD_PROBE_AMOUNT ))
if [[ -z "$AQUA_MIN_RATE_SCALED" ]]; then
  AQUA_MIN_RATE_SCALED=$(( QUOTED_REWARD_RATE_SCALED * REWARD_RATE_FLOOR_BPS / 10000 ))
fi
case "$AQUA_MIN_RATE_SCALED" in
  ''|*[!0-9]*)
    echo "ERROR: AQUA_MIN_RATE_SCALED must be a positive integer." >&2
    exit 1
    ;;
esac
if (( REWARD_QUOTE_RAW == 0 || AQUA_MIN_RATE_SCALED == 0 )); then
  echo "ERROR: AQUA/XLM reward quote and configured rate floor must be non-zero." >&2
  exit 1
fi

XLM_UPSTREAM_PRICE=$(view "$UPSTREAM_ORACLE" lastprice --asset '{"Other":"XLM"}')
if [[ "$XLM_UPSTREAM_PRICE" == "null" ]]; then
  echo 'ERROR: upstream oracle has no Other("XLM") price.' >&2
  exit 1
fi

PEG_QUOTE=$(view "$POOL" estimate_swap \
  --in_idx 1 --out_idx 0 --in_amount "$PEG_PROBE_AMOUNT")
PEG_QUOTE_RAW=${PEG_QUOTE//\"/}
case "$PEG_QUOTE_RAW" in
  ''|*[!0-9]*)
    echo "ERROR: invalid yXLM/XLM probe quote: $PEG_QUOTE" >&2
    exit 1
    ;;
esac
if (( PEG_QUOTE_RAW * 10000 < PEG_PROBE_AMOUNT * MIN_PEG_RATIO_BPS )); then
  echo "ERROR: yXLM/XLM executable quote is below the configured peg floor." >&2
  echo "probe_in=$PEG_PROBE_AMOUNT quote_out=$PEG_QUOTE_RAW floor_bps=$MIN_PEG_RATIO_BPS" >&2
  exit 1
fi

if [[ "$PREFLIGHT_ONLY" == "true" ]]; then
  echo "Preflight passed: concentrated XLM/yXLM pool, oracle, peg quote, and AQUA route are valid."
  echo "AQUA/XLM minimum rate (1e7 scale): $AQUA_MIN_RATE_SCALED"
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

echo "==> Deploying AquariusLpVault"
VAULT_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/aquarius_lp_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    vault = $VAULT_ID"
invoke "$VAULT_ID" initialize \
  --admin "$ADMIN" --pool "$POOL" \
  --underlying_index "$UNDERLYING_INDEX" --oracle "$UPSTREAM_ORACLE"
invoke "$VAULT_ID" set_oracle_symbol \
  --admin_addr "$ADMIN" --token "$XLM" --symbol '"XLM"'
invoke "$VAULT_ID" set_oracle_symbol \
  --admin_addr "$ADMIN" --token "$YXLM" --symbol '"XLM"'
invoke "$VAULT_ID" set_max_deploy --admin_addr "$ADMIN" --max_deploy "$MAX_DEPLOY"
invoke "$VAULT_ID" set_slippage_bps --admin_addr "$ADMIN" --bps "$SLIPPAGE_BPS"
invoke "$VAULT_ID" set_harvest_cooldown --admin_addr "$ADMIN" --seconds "$HARVEST_COOLDOWN"
invoke "$VAULT_ID" set_max_pool_divergence_bps \
  --admin_addr "$ADMIN" --bps "$MAX_DIVERGENCE_BPS"
invoke "$VAULT_ID" set_primary_reward_token \
  --admin_addr "$ADMIN" --reward_token "$AQUA"
invoke "$VAULT_ID" set_reward_route \
  --admin_addr "$ADMIN" --reward_token "$AQUA" --route "$AQUA_ROUTE"
invoke "$VAULT_ID" set_reward_min_rate \
  --admin_addr "$ADMIN" --reward_token "$AQUA" \
  --min_rate_scaled "$AQUA_MIN_RATE_SCALED"
NAV_ROOT=$(invoke "$VAULT_ID" refresh_nav_root)
if [[ "${NAV_ROOT//\"/}" == "0" ]]; then
  echo "ERROR: direct Reflector aliases produced a zero NAV root." >&2
  exit 1
fi

echo "==> Deploying a new supply-only XLM ReceiptVault"
MARKET_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/receipt_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    market = $MARKET_ID"
invoke "$MARKET_ID" initialize \
  --token_address "$XLM" \
  --supply_yearly_rate_scaled 0 \
  --borrow_yearly_rate_scaled 0 \
  --admin "$ADMIN"
invoke "$MARKET_ID" set_interest_model --model "$JRM"
invoke "$MARKET_ID" set_idle_cash_buffer_bps \
  --admin "$ADMIN" --idle_cash_buffer_bps "$IDLE_BUFFER_BPS"
invoke "$MARKET_ID" set_supply_cap --cap "$SUPPLY_CAP"
# Attach before the controller so boosted-vault ownership can be bound when the
# market is wired below.
invoke "$MARKET_ID" set_boosted_vault --admin "$ADMIN" --boosted_vault "$VAULT_ID"
BOOSTED_ASSET_COUNT=$(view "$MARKET_ID" get_boosted_asset_count)
if [[ "$BOOSTED_ASSET_COUNT" != "1" ]]; then
  echo "ERROR: ReceiptVault persisted boosted asset count $BOOSTED_ASSET_COUNT; expected 1." >&2
  exit 1
fi
# Close the loop after ReceiptVault points at the strategy. Until this succeeds,
# AquariusLpVault rejects every deposit, including calls from arbitrary users.
invoke "$VAULT_ID" set_receipt_vault --admin_addr "$ADMIN" --receipt_vault "$MARKET_ID"

echo "==> Listing with zero collateral value and borrowing paused"
invoke "$CONTROLLER" add_market --market "$MARKET_ID"
# A newly listed market defaults to CF=0. Do not call set_market_cf: that setter
# deliberately rejects values below 1%.
invoke "$CONTROLLER" set_pause_borrow --market "$MARKET_ID" --paused true
invoke "$MARKET_ID" set_peridottroller --peridottroller "$CONTROLLER"

CF=$(view "$CONTROLLER" get_market_cf --market "$MARKET_ID")
BORROW_PAUSED=$(view "$CONTROLLER" is_borrow_paused --market "$MARKET_ID")
CONTROLLER_XLM_PRICE=$(view "$CONTROLLER" get_price_usd --token "$XLM")
if [[ "$CF" != "0" || "$BORROW_PAUSED" != "true" || \
      "$CONTROLLER_XLM_PRICE" == "null" ]]; then
  echo "ERROR: supply-only/oracle verification failed: cf=$CF borrow_paused=$BORROW_PAUSED xlm_price=$CONTROLLER_XLM_PRICE" >&2
  exit 1
fi

cat <<SUMMARY

────────────────────────────────────────────────────────────────
  XLM/yXLM yield market deployed (supply-only)
────────────────────────────────────────────────────────────────
  Oracle          $UPSTREAM_ORACLE (XLM + yXLM -> Other("XLM"))
  Aquarius vault $VAULT_ID
  XLM market      $MARKET_ID
  Pool            $POOL
  AQUA rate floor $AQUA_MIN_RATE_SCALED (1e7 raw XLM/raw AQUA)
  Collateral CF   $CF
  Borrow paused   $BORROW_PAUSED

  Start the keeper:
    VAULT_ID=$VAULT_ID MARKET_ID=$MARKET_ID IDENTITY=$IDENTITY NETWORK=$NETWORK \
      bash scripts/run_aquarius_vault_keeper.sh
────────────────────────────────────────────────────────────────
SUMMARY
