#!/usr/bin/env bash
# Deploys a boosted market backed by an Aquarius concentrated-liquidity vault.
#
# This creates a NEW market. It does not touch any existing deployed contract:
# the audited ReceiptVault wasm is reused as-is, and the vault is attached
# through the existing `set_boosted_vault` entrypoint.
#
#   ADMIN=G... IDENTITY=peridot-mainnet bash scripts/deploy_aquarius_lp_market_mainnet.sh
#
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-public}
IDENTITY=${IDENTITY:?set IDENTITY to the deploying stellar CLI identity}
ADMIN=${ADMIN:?set ADMIN to the pinned init-admin public key}

# ── Mainnet wiring ───────────────────────────────────────────────────────────
# USDC/EURC concentrated pool, fee 0.3%. Verified on-chain: pool_type
# "concentrated", tokens [USDC, EURC], AQUA emissions live.
POOL=${POOL:-CDTSE6RLRI7ZO25JSER6E4SQR4PHJJNONEGS5HDJ3Y6LAKECRZKYN5CA}
# Index of the settlement token in the pool's sorted token vector.
# USDC (CCW67TSZ...) sorts before EURC (CDTKPWPL...), so USDC is index 0.
UNDERLYING_INDEX=${UNDERLYING_INDEX:-0}
UNDERLYING=${UNDERLYING:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}
ORACLE=${ORACLE:-CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN}
CONTROLLER=${CONTROLLER:?set CONTROLLER explicitly (prefer the isolated LP-market Peridottroller)}
JRM=${JRM:-CCI5LBBNYOASPQ62GIRY54PDEYWWURJB75HNRAFOU4LTOU3XBC73IB5I}
AQUA=${AQUA:-CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK}
# Aquarius pool used to sell harvested AQUA for the underlying.
# AQUA/USDC constant-product 0.3%. Chosen on-chain over the two alternatives:
# the concentrated 0.3% pool holds ~$90k and the 1% pool is dead ($119). This
# one holds ~$520k with ~88M AQUA of daily volume, so a weekly harvest moves it
# negligibly. AQUA has no Reflector feed, so permissionless reward swaps use a
# governance floor initialized from a live route quote below.
AQUA_ROUTE=${AQUA_ROUTE:-CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O}
REWARD_PROBE_AMOUNT=${REWARD_PROBE_AMOUNT:-10000000000} # 1,000 AQUA
REWARD_RATE_FLOOR_BPS=${REWARD_RATE_FLOOR_BPS:-9500}
AQUA_MIN_RATE_SCALED=${AQUA_MIN_RATE_SCALED:-}

# ── Risk parameters ─────────────────────────────────────────────────────────
# Realised APR scales with pool_tvl / (pool_tvl + deployed). The USDC/EURC pool
# held ~$151k when this was written, so an uncapped vault would dilute its own
# yield to nothing. Default cap is deliberately small; raise it only alongside
# pool TVL.
# Two separate ceilings, both agreed at $100k for the initial launch:
#  - SUPPLY_CAP  caps what the MARKET will accept from suppliers at all.
#  - MAX_DEPLOY  caps what the VAULT pushes into the Aquarius pool. Anything
#    above it stays as idle cash in the vault and earns nothing, so this is the
#    yield knob; realised APR scales with pool_tvl / (pool_tvl + deployed).
# At $100k against the pool's ~$151k, the vault would own ~40% of it — see the
# capacity note in contracts/aquarius-lp-vault/CLAUDE.md before raising these.
SUPPLY_CAP=${SUPPLY_CAP:-1000000000000}  # 100,000 USDC (7 decimals)
MAX_DEPLOY=${MAX_DEPLOY:-1000000000000}  # 100,000 USDC (7 decimals)
SLIPPAGE_BPS=${SLIPPAGE_BPS:-100}        # 1%
# Refuses the entry swap when the pool is mispriced against the oracle. The
# slippage guard cannot catch this — it is derived from the pool's own quote.
MAX_DIVERGENCE_BPS=${MAX_DIVERGENCE_BPS:-200}  # 2%
IDLE_BUFFER_BPS=${IDLE_BUFFER_BPS:-3000} # keep 30% of market deposits liquid
HARVEST_COOLDOWN=${HARVEST_COOLDOWN:-3600}

invoke() { stellar contract invoke --id "$1" --source-account "$IDENTITY" --network "$NETWORK" -- "${@:2}"; }
view() { stellar contract invoke --id "$1" --source-account "$IDENTITY" --network "$NETWORK" --send no -- "${@:2}"; }

case "$REWARD_PROBE_AMOUNT:$REWARD_RATE_FLOOR_BPS" in
  *[!0-9:]*|:*|*:)
    echo "ERROR: reward probe and floor bps must be positive integers." >&2
    exit 2
    ;;
esac
if (( REWARD_PROBE_AMOUNT == 0 || REWARD_RATE_FLOOR_BPS == 0 || \
      REWARD_RATE_FLOOR_BPS > 10000 )); then
  echo "ERROR: reward probe must be positive and floor must be within 1..10000 bps." >&2
  exit 2
fi

echo "==> Building wasm"
INIT_ADMIN="$ADMIN" bash scripts/build_wasm.sh

echo "==> Sanity-checking the target pool"
POOL_TYPE=$(stellar contract invoke --id "$POOL" --source-account "$IDENTITY" --network "$NETWORK" --send=no -- pool_type)
echo "    pool_type = $POOL_TYPE"
if [[ "$POOL_TYPE" != '"concentrated"' ]]; then
  echo "ERROR: $POOL is not a concentrated pool; the full-range NAV formula does not apply." >&2
  exit 1
fi
stellar contract invoke --id "$POOL" --source-account "$IDENTITY" --network "$NETWORK" --send=no -- get_tokens
stellar contract invoke --id "$POOL" --source-account "$IDENTITY" --network "$NETWORK" --send=no -- get_reserves

if [[ -n "$AQUA_ROUTE" ]]; then
  ROUTE_TOKENS=$(view "$AQUA_ROUTE" get_tokens)
  if [[ "$ROUTE_TOKENS" == "[\"$AQUA\",\"$UNDERLYING\"]" ]]; then
    REWARD_IN_IDX=0
    REWARD_OUT_IDX=1
  elif [[ "$ROUTE_TOKENS" == "[\"$UNDERLYING\",\"$AQUA\"]" ]]; then
    REWARD_IN_IDX=1
    REWARD_OUT_IDX=0
  else
    echo "ERROR: AQUA_ROUTE must contain AQUA and the underlying; got $ROUTE_TOKENS" >&2
    exit 1
  fi
  REWARD_QUOTE=$(view "$AQUA_ROUTE" estimate_swap \
    --in_idx "$REWARD_IN_IDX" --out_idx "$REWARD_OUT_IDX" \
    --in_amount "$REWARD_PROBE_AMOUNT")
  REWARD_QUOTE_RAW=${REWARD_QUOTE//\"/}
  case "$REWARD_QUOTE_RAW" in
    ''|*[!0-9]*)
      echo "ERROR: invalid AQUA reward quote: $REWARD_QUOTE" >&2
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
    echo "ERROR: AQUA reward quote and configured rate floor must be non-zero." >&2
    exit 1
  fi
fi

echo "==> Deploying AquariusLpVault"
VAULT_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/aquarius_lp_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    vault = $VAULT_ID"

invoke "$VAULT_ID" initialize \
  --admin "$ADMIN" --pool "$POOL" \
  --underlying_index "$UNDERLYING_INDEX" --oracle "$ORACLE"

invoke "$VAULT_ID" set_max_deploy --admin_addr "$ADMIN" --max_deploy "$MAX_DEPLOY"
invoke "$VAULT_ID" set_slippage_bps --admin_addr "$ADMIN" --bps "$SLIPPAGE_BPS"
invoke "$VAULT_ID" set_harvest_cooldown --admin_addr "$ADMIN" --seconds "$HARVEST_COOLDOWN"
invoke "$VAULT_ID" set_max_pool_divergence_bps --admin_addr "$ADMIN" --bps "$MAX_DIVERGENCE_BPS"
invoke "$VAULT_ID" set_primary_reward_token --admin_addr "$ADMIN" --reward_token "$AQUA"
if [[ -n "$AQUA_ROUTE" ]]; then
  invoke "$VAULT_ID" set_reward_route \
    --admin_addr "$ADMIN" --reward_token "$AQUA" --route "$AQUA_ROUTE"
  invoke "$VAULT_ID" set_reward_min_rate \
    --admin_addr "$ADMIN" --reward_token "$AQUA" \
    --min_rate_scaled "$AQUA_MIN_RATE_SCALED"
else
  echo "    WARNING: AQUA_ROUTE unset — harvested AQUA will accumulate unsold."
fi

echo "==> Deploying the market (ReceiptVault, audited wasm, unmodified)"
MARKET_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/receipt_vault.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    market = $MARKET_ID"

invoke "$MARKET_ID" initialize \
  --token_address "$UNDERLYING" \
  --supply_yearly_rate_scaled 0 \
  --borrow_yearly_rate_scaled 0 \
  --admin "$ADMIN"

invoke "$MARKET_ID" set_interest_model --model "$JRM"
invoke "$MARKET_ID" set_idle_cash_buffer_bps --admin "$ADMIN" --idle_cash_buffer_bps "$IDLE_BUFFER_BPS"
invoke "$MARKET_ID" set_supply_cap --cap "$SUPPLY_CAP"
# Attach before setting the controller. Once a controller is wired,
# set_boosted_vault calls its ownership registry, which rejects a market that
# has not yet been listed. Listing and collateral policy remain an explicit
# governance step below.
invoke "$MARKET_ID" set_boosted_vault --admin "$ADMIN" --boosted_vault "$VAULT_ID"
BOOSTED_ASSET_COUNT=$(view "$MARKET_ID" get_boosted_asset_count)
if [[ "$BOOSTED_ASSET_COUNT" != "1" ]]; then
  echo "ERROR: ReceiptVault persisted boosted asset count $BOOSTED_ASSET_COUNT; expected 1." >&2
  exit 1
fi
invoke "$VAULT_ID" set_receipt_vault --admin_addr "$ADMIN" --receipt_vault "$MARKET_ID"

cat <<SUMMARY

────────────────────────────────────────────────────────────────
  Aquarius LP boosted market deployed
────────────────────────────────────────────────────────────────
  Vault           $VAULT_ID
  Market          $MARKET_ID
  Pool            $POOL
  Underlying      $UNDERLYING (index $UNDERLYING_INDEX)
  Supply cap      $SUPPLY_CAP (market accepts no more than this)
  Max deploy      $MAX_DEPLOY (vault pushes no more than this into the pool)
  Idle buffer     ${IDLE_BUFFER_BPS} bps

  Still to do:
    1. Choose the market policy. For a supply-only launch, keep collateral
       factor at 0 and pause borrowing. Otherwise complete the controller
       footprint review documented in README.md's transaction-footprint section.
    2. For the recommended supply-only policy, list, pause, and wire in this order:
         stellar contract invoke --id $CONTROLLER --source-account $IDENTITY --network $NETWORK -- add_market --market $MARKET_ID
         stellar contract invoke --id $CONTROLLER --source-account $IDENTITY --network $NETWORK -- set_pause_borrow --market $MARKET_ID --paused true
         stellar contract invoke --id $MARKET_ID --source-account $IDENTITY --network $NETWORK -- set_peridottroller --peridottroller $CONTROLLER
       A new market's collateral factor defaults to 0. Only set a non-zero CF
       and unpause borrowing after the cross-market footprint is redesigned.
    3. Set AQUA_ROUTE and a reviewed set_reward_min_rate floor if they were skipped.
    4. Run scripts/run_aquarius_vault_keeper.sh with VAULT_ID and MARKET_ID.
────────────────────────────────────────────────────────────────
SUMMARY
