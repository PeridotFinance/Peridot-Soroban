#!/usr/bin/env bash
# Deploys the isolated, supply-only controller for Aquarius LP markets.
#
# Temporary oracle policy:
#   XLM, yXLM -> Reflector Other("XLM")
#   PYUSD, USDC -> Reflector Other("USDC")
#
# The aliases intentionally assume each pair is at par. They are suitable only
# for the initial CF=0, borrow-paused rollout, with the pair deployment scripts'
# executable-quote preflight and the vaults' runtime divergence guards enabled.
#
#   ADMIN=G... IDENTITY=peridot-mainnet CONFIRM_MAINNET=DEPLOY \
#     bash scripts/deploy_aquarius_lp_controller_mainnet.sh
#
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-public}
IDENTITY=${IDENTITY:?set IDENTITY to the deploying stellar CLI identity}
ADMIN=${ADMIN:?set ADMIN to the pinned init-admin public key}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}

REFLECTOR=${REFLECTOR:-CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN}
XLM=${XLM:-CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA}
YXLM=${YXLM:-CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC}
PYUSD=${PYUSD:-CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2}
USDC=${USDC:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}

invoke() {
  stellar contract invoke \
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" -- "${@:2}"
}

view() {
  stellar contract invoke \
    --id "$1" --source-account "$IDENTITY" --network "$NETWORK" --send no -- "${@:2}"
}

if [[ "$CONFIRM_MAINNET" != "DEPLOY" ]]; then
  echo "ERROR: set CONFIRM_MAINNET=DEPLOY to submit mainnet transactions." >&2
  exit 2
fi

EXPECTED_ADMIN=$(stellar keys public-key "$IDENTITY")
if [[ "$ADMIN" != "$EXPECTED_ADMIN" ]]; then
  echo "ERROR: ADMIN must equal the deploying identity's public key." >&2
  echo "admin=$ADMIN identity_public_key=$EXPECTED_ADMIN" >&2
  exit 2
fi

echo "==> Checking direct Reflector feeds"
XLM_PRICE=$(view "$REFLECTOR" lastprice --asset '{"Other":"XLM"}')
USDC_PRICE=$(view "$REFLECTOR" lastprice --asset '{"Other":"USDC"}')
if [[ "$XLM_PRICE" == "null" || "$USDC_PRICE" == "null" ]]; then
  echo "ERROR: Reflector must currently publish both XLM and USDC." >&2
  exit 1
fi

echo "==> Building controller with the mainnet admin guard"
INIT_ADMIN="$ADMIN" bash scripts/build_wasm.sh

echo "==> Deploying isolated Aquarius LP Peridottroller"
CONTROLLER_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/simple_peridottroller.optimized.wasm \
  --source-account "$IDENTITY" --network "$NETWORK")
echo "    controller = $CONTROLLER_ID"
invoke "$CONTROLLER_ID" initialize --admin "$ADMIN"
invoke "$CONTROLLER_ID" set_oracle --oracle "$REFLECTOR"

invoke "$CONTROLLER_ID" set_oracle_asset_symbol --token "$XLM" --symbol '"XLM"'
invoke "$CONTROLLER_ID" set_oracle_asset_symbol --token "$YXLM" --symbol '"XLM"'
invoke "$CONTROLLER_ID" set_oracle_asset_symbol --token "$PYUSD" --symbol '"USDC"'
invoke "$CONTROLLER_ID" set_oracle_asset_symbol --token "$USDC" --symbol '"USDC"'

ACTUAL_ADMIN=$(view "$CONTROLLER_ID" get_admin)
ACTUAL_ORACLE=$(view "$CONTROLLER_ID" get_oracle)
XLM_CONTROLLER_PRICE=$(view "$CONTROLLER_ID" get_price_usd --token "$XLM")
YXLM_CONTROLLER_PRICE=$(view "$CONTROLLER_ID" get_price_usd --token "$YXLM")
PYUSD_CONTROLLER_PRICE=$(view "$CONTROLLER_ID" get_price_usd --token "$PYUSD")
USDC_CONTROLLER_PRICE=$(view "$CONTROLLER_ID" get_price_usd --token "$USDC")

if [[ "$ACTUAL_ADMIN" != "\"$ADMIN\"" || \
      "$ACTUAL_ORACLE" != "\"$REFLECTOR\"" || \
      "$XLM_CONTROLLER_PRICE" == "null" || "$YXLM_CONTROLLER_PRICE" == "null" || \
      "$PYUSD_CONTROLLER_PRICE" == "null" || "$USDC_CONTROLLER_PRICE" == "null" || \
      "$XLM_CONTROLLER_PRICE" != "$YXLM_CONTROLLER_PRICE" || \
      "$PYUSD_CONTROLLER_PRICE" != "$USDC_CONTROLLER_PRICE" ]]; then
  echo "ERROR: isolated controller oracle verification failed." >&2
  exit 1
fi

cat <<SUMMARY

────────────────────────────────────────────────────────────────
  Isolated Aquarius LP controller deployed
────────────────────────────────────────────────────────────────
  Controller       $CONTROLLER_ID
  Admin            $ADMIN
  Oracle           $REFLECTOR
  XLM + yXLM       Other("XLM")
  PYUSD + USDC     Other("USDC")

  Deploy the three supply-only markets with:
    CONTROLLER=$CONTROLLER_ID ADMIN=$ADMIN IDENTITY=$IDENTITY \
      CONFIRM_MAINNET=DEPLOY bash scripts/deploy_aquarius_xlm_yxlm_mainnet.sh
    CONTROLLER=$CONTROLLER_ID ADMIN=$ADMIN IDENTITY=$IDENTITY \
      CONFIRM_MAINNET=DEPLOY bash scripts/deploy_aquarius_pyusd_usdc_mainnet.sh
────────────────────────────────────────────────────────────────
SUMMARY
