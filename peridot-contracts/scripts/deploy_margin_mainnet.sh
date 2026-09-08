#!/usr/bin/env bash
# Deploy and configure an inert Mainnet MarginController V3 + SwapAdapter.
#
# Default mode is read-only. This script never wires the ReceiptVaults or
# authorizes margin liquidation in SimplePeridottroller; activation is handled
# separately by activate_margin_mainnet.sh after governance and keeper checks.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

MODE=${MODE:-preflight} # preflight | deploy | verify
NETWORK=${NETWORK:-mainnet-gateway}
IDENTITY=${IDENTITY:-peridot-mainnet}
INCLUSION_FEE=${INCLUSION_FEE:-100000}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}
SIMULATE_UPLOAD_FEES=${SIMULATE_UPLOAD_FEES:-true}
VERIFY_INERT=${VERIFY_INERT:-true}

ADMIN=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
GOVERNANCE_ADMIN=GDM5XP4ACJPXPYP5G2UYGB6WC3G2MUBCYI2ZUCYWVB5CL2JWGCFWNWA3
PERIDOTTROLLER=CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX
AQUARIUS_ROUTER=CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK

XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
XLM_VAULT=CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE
USDC_VAULT=CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN

# Aquarius concentrated XLM/USDC pool, token order [XLM, USDC].
POOL_ID=24f9c991c44acf33fff5f44031c40385d235dc212d7379e824ba3db1c35371f3
POOL=CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV

# Deterministic IDs derived from ADMIN + these salts on Mainnet.
SWAP_SALT=f2475cffc59a0d076059b03909d2f0694639f2beaf6ffae8e76deeaea189040b
MARGIN_SALT=fbfc4b4a29d70c4416ac543c132d2adad288937367c5b364ee5d2263af586ba9
SWAP_ID=CCEMGF23WSRRR3C343LQ7SVQWGCNEQ2OA7I5VD7FY7FD4HJAYNQFV7BW
MARGIN_ID=CC2MVMWNUSEBJ5ITIKTKTLL3URUPYPGRQB6SWX2HAKEKKXDMYF4EYP5C

SWAP_WASM=${SWAP_WASM:-target/wasm32v1-none/release/swap_adapter.optimized.wasm}
MARGIN_WASM=${MARGIN_WASM:-target/wasm32v1-none/release/margin_controller.wasm}
SWAP_WASM_HASH=1bc1266e03cd017700eb6c41afbc0daf04a85f11cf7fcf104c49a3b7eac11062
MARGIN_WASM_HASH=1667c93ce21ef31520c4804f077cf0f7b2507323c63ba65df76a2d721984e656
AUDITED_COMMIT=230101e6f79bdfd1459f7854e64b81324aeab08a

# Pilot policy. V3 fee charging is intentionally not configured because the
# current V3 paths do not consume OpenFeeBps/CloseFeeBps.
MAX_LEVERAGE=3
MAINTENANCE_MARGIN_SCALED=50000       # 5%
LIQUIDATION_INCENTIVE_SCALED=10000    # 1%
MAX_OPEN_DEVIATION_SCALED=50000       # 5%
OPEN_SLIPPAGE_SCALED=20000            # 2%
CLOSE_SLIPPAGE_SCALED=20000           # 2%
LIQUIDATION_SLIPPAGE_SCALED=50000     # 5%
MAX_CLOSE_DEVIATION_SCALED=150000     # 15%
MAX_LIQ_DEVIATION_SCALED=250000       # 25%

MAX_MARGIN_UPLOAD_FEE_STROOPS=${MAX_MARGIN_UPLOAD_FEE_STROOPS:-1100000000}
MAX_SWAP_UPLOAD_FEE_STROOPS=${MAX_SWAP_UPLOAD_FEE_STROOPS:-220000000}
MIN_DEPLOYER_XLM=${MIN_DEPLOYER_XLM:-140}

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

unquote() {
  local value=$1
  value=${value#\"}
  value=${value%\"}
  printf '%s\n' "$value"
}

expect() {
  local label=$1 actual expected=$3
  actual=$(unquote "$2")
  [[ "$actual" == "$expected" ]] || fail "$label mismatch: expected=$expected actual=$actual"
}

view() {
  local id=$1
  shift
  stellar contract invoke --no-cache --id "$id" --source-account "$IDENTITY" \
    --network "$NETWORK" --send no -- "$@"
}

invoke() {
  local id=$1
  shift
  stellar contract invoke --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --id "$id" --source-account "$IDENTITY" --network "$NETWORK" -- "$@"
}

contract_hash() {
  local attempt hash
  for attempt in 1 2 3 4 5; do
    if hash=$(stellar contract info hash --no-cache --id "$1" --network "$NETWORK" 2>/dev/null); then
      printf '%s\n' "$hash"
      return 0
    fi
    sleep 2
  done
  return 1
}

artifact_hash() {
  stellar contract info hash --wasm "$1"
}

oracle_expected_out() {
  node -e '
    const [amount, inValue, inScale, outValue, outScale] = process.argv.slice(1).map(BigInt);
    console.log((amount * inValue * outScale / inScale / outValue).toString());
  ' "$@"
}

deviation_scaled() {
  node -e '
    const [actual, expected] = process.argv.slice(1).map(BigInt);
    if (expected === 0n) process.exit(2);
    const delta = actual > expected ? actual - expected : expected - actual;
    console.log((delta * 1000000n / expected).toString());
  ' "$@"
}

simulate_upload_fee() {
  local wasm=$1 raw_xdr prepared_xdr decoded fee
  raw_xdr=$(stellar contract upload --quiet --build-only --wasm "$wasm" \
    --source-account "$IDENTITY" --network "$NETWORK")
  prepared_xdr=$(stellar tx simulate --quiet --source-account "$IDENTITY" \
    --network "$NETWORK" "$raw_xdr")
  decoded=$(stellar tx decode "$prepared_xdr")
  fee=$(printf '%s' "$decoded" | jq -er '.tx.tx.fee')
  [[ "$fee" =~ ^[0-9]+$ ]] || fail "could not parse upload fee for $wasm"
  printf '%s\n' "$fee"
}

check_upload_fee() {
  local label=$1 wasm=$2 cap=$3 fee
  fee=$(simulate_upload_fee "$wasm")
  (( fee <= cap )) || fail "$label upload fee $fee stroops exceeds cap $cap"
  printf '    %s upload fee: %s stroops (%s XLM)\n' \
    "$label" "$fee" "$(awk -v fee="$fee" 'BEGIN { printf "%.7f", fee / 10000000 }')"
}

check_price() {
  local label=$1 token=$2 result
  result=$(view "$PERIDOTTROLLER" get_price_usd --token "$token")
  printf '%s' "$result" | jq -e \
    'length == 2 and (.[0] | tonumber) > 0 and (.[1] | tonumber) > 0' >/dev/null || \
    fail "$label oracle price unavailable: $result"
  printf '%s\n' "$result"
}

check_pool_quote() {
  local label=$1 in_idx=$2 out_idx=$3 amount=$4 in_price=$5 out_price=$6
  local quote expected deviation in_value in_scale out_value out_scale
  quote=$(unquote "$(view "$POOL" estimate_swap --in_idx "$in_idx" \
    --out_idx "$out_idx" --in_amount "$amount")")
  in_value=$(printf '%s' "$in_price" | jq -r '.[0]')
  in_scale=$(printf '%s' "$in_price" | jq -r '.[1]')
  out_value=$(printf '%s' "$out_price" | jq -r '.[0]')
  out_scale=$(printf '%s' "$out_price" | jq -r '.[1]')
  expected=$(oracle_expected_out "$amount" "$in_value" "$in_scale" "$out_value" "$out_scale")
  deviation=$(deviation_scaled "$quote" "$expected")
  (( deviation <= MAX_OPEN_DEVIATION_SCALED )) || \
    fail "$label pool/oracle deviation $deviation exceeds $MAX_OPEN_DEVIATION_SCALED"
  echo "    $label quote=$quote oracle=$expected deviation_scaled=$deviation"
}

check_deployer_balance() {
  local account_json balance
  account_json=$(curl -fsSL "https://horizon.stellar.org/accounts/$ADMIN")
  balance=$(printf '%s' "$account_json" | jq -er \
    '.balances[] | select(.asset_type == "native") | .balance')
  awk -v balance="$balance" -v minimum="$MIN_DEPLOYER_XLM" \
    'BEGIN { exit !(balance + 0 >= minimum + 0) }' || \
    fail "deployer has $balance XLM; fund it to at least $MIN_DEPLOYER_XLM XLM"
  echo "    deployer balance: $balance XLM"
}

verify_candidate_id() {
  local label=$1 salt=$2 expected=$3 actual
  actual=$(stellar contract id wasm --salt "$salt" --source-account "$IDENTITY" \
    --network "$NETWORK")
  expect "$label deterministic ID" "$actual" "$expected"
}

verify_dependencies() {
  local xlm_price usdc_price pool_tokens
  echo "==> Verifying source, artifacts, identities, and deterministic IDs"
  expect "identity" "$(stellar keys public-key "$IDENTITY")" "$ADMIN"
  git diff --quiet "$AUDITED_COMMIT" -- contracts/margin-controller contracts/swap-adapter || \
    fail "margin/swap sources differ from audited commit $AUDITED_COMMIT"
  [[ -f "$SWAP_WASM" ]] || fail "missing SwapAdapter artifact: $SWAP_WASM"
  [[ -f "$MARGIN_WASM" ]] || fail "missing MarginController artifact: $MARGIN_WASM"
  expect "SwapAdapter artifact" "$(artifact_hash "$SWAP_WASM")" "$SWAP_WASM_HASH"
  expect "MarginController artifact" "$(artifact_hash "$MARGIN_WASM")" "$MARGIN_WASM_HASH"
  LC_ALL=C grep -a -q "$ADMIN" "$SWAP_WASM" || fail "SwapAdapter init-admin guard mismatch"
  LC_ALL=C grep -a -q "$ADMIN" "$MARGIN_WASM" || fail "MarginController init-admin guard mismatch"
  verify_candidate_id "SwapAdapter" "$SWAP_SALT" "$SWAP_ID"
  verify_candidate_id "MarginController" "$MARGIN_SALT" "$MARGIN_ID"

  echo "==> Verifying live Mainnet lending and oracle dependencies"
  expect "Peridottroller admin" "$(view "$PERIDOTTROLLER" get_admin)" "$ADMIN"
  expect "XLM vault admin" "$(view "$XLM_VAULT" get_admin)" "$ADMIN"
  expect "USDC vault admin" "$(view "$USDC_VAULT" get_admin)" "$ADMIN"
  expect "XLM vault underlying" "$(view "$XLM_VAULT" get_underlying_token)" "$XLM"
  expect "USDC vault underlying" "$(view "$USDC_VAULT" get_underlying_token)" "$USDC"
  expect "XLM market collateral factor" \
    "$(view "$PERIDOTTROLLER" get_market_cf --market "$XLM_VAULT")" 700000
  expect "USDC market collateral factor" \
    "$(view "$PERIDOTTROLLER" get_market_cf --market "$USDC_VAULT")" 900000
  expect "XLM borrow pause" "$(view "$PERIDOTTROLLER" is_borrow_paused --market "$XLM_VAULT")" false
  expect "USDC borrow pause" "$(view "$PERIDOTTROLLER" is_borrow_paused --market "$USDC_VAULT")" false
  xlm_price=$(check_price XLM "$XLM")
  usdc_price=$(check_price USDC "$USDC")

  echo "==> Verifying Aquarius XLM/USDC execution"
  pool_tokens=$(view "$POOL" get_tokens)
  [[ "$pool_tokens" == "[\"$XLM\",\"$USDC\"]" ]] || \
    fail "unexpected pool token order: $pool_tokens"
  expect "pool swap kill switch" "$(view "$POOL" get_is_killed_swap)" false
  check_pool_quote "1000 USDC -> XLM" 1 0 10000000000 "$usdc_price" "$xlm_price"
  check_pool_quote "5320 XLM -> USDC" 0 1 53200000000 "$xlm_price" "$usdc_price"
}

verify_deployed() {
  local swap_hash margin_hash margin_admin swap_admin config execution exit_config
  echo "==> Verifying deployed inert margin stack"
  swap_hash=$(contract_hash "$SWAP_ID") || fail "SwapAdapter is not deployed at $SWAP_ID"
  margin_hash=$(contract_hash "$MARGIN_ID") || fail "MarginController is not deployed at $MARGIN_ID"
  expect "deployed SwapAdapter hash" "$swap_hash" "$SWAP_WASM_HASH"
  expect "deployed MarginController hash" "$margin_hash" "$MARGIN_WASM_HASH"
  swap_admin=$(unquote "$(view "$SWAP_ID" get_admin)")
  margin_admin=$(unquote "$(view "$MARGIN_ID" get_admin)")
  [[ "$swap_admin" == "$ADMIN" || "$swap_admin" == "$GOVERNANCE_ADMIN" ]] || \
    fail "unexpected SwapAdapter admin: $swap_admin"
  [[ "$margin_admin" == "$ADMIN" || "$margin_admin" == "$GOVERNANCE_ADMIN" ]] || \
    fail "unexpected MarginController admin: $margin_admin"
  expect "adapter controller allowlist" \
    "$(view "$SWAP_ID" is_pool_allowed --pool "$MARGIN_ID")" true
  expect "adapter pool allowlist" "$(view "$SWAP_ID" is_pool_allowed --pool "$POOL")" true
  expect "adapter pool binding" \
    "$(view "$SWAP_ID" is_pool_binding_allowed --pool_id "$POOL_ID" --pool "$POOL")" true

  for side in Long Short; do
    config=$(view "$MARGIN_ID" get_perps_pair_config \
      --margin_asset "$USDC" --base_asset "$XLM" --side "$side")
    printf '%s' "$config" | jq -e \
      --arg lev "$MAX_LEVERAGE" --arg mm "$MAINTENANCE_MARGIN_SCALED" \
      --arg li "$LIQUIDATION_INCENTIVE_SCALED" \
      '.max_leverage == $lev and .maintenance_margin_scaled == $mm and .liquidation_incentive_scaled == $li' \
      >/dev/null || fail "$side risk config mismatch: $config"
    execution=$(view "$MARGIN_ID" get_perps_pair_execution_config \
      --margin_asset "$USDC" --base_asset "$XLM" --side "$side")
    printf '%s' "$execution" | jq -e \
      --arg open_dev "$MAX_OPEN_DEVIATION_SCALED" --arg open_slip "$OPEN_SLIPPAGE_SCALED" \
      --arg close_slip "$CLOSE_SLIPPAGE_SCALED" --arg liq_slip "$LIQUIDATION_SLIPPAGE_SCALED" \
      '.max_open_deviation_scaled == $open_dev and .open_slippage_scaled == $open_slip and .close_slippage_scaled == $close_slip and .liquidation_slippage_scaled == $liq_slip' \
      >/dev/null || fail "$side execution config mismatch: $execution"
    exit_config=$(view "$MARGIN_ID" get_perps_pair_exit_config \
      --margin_asset "$USDC" --base_asset "$XLM" --side "$side")
    printf '%s' "$exit_config" | jq -e \
      --arg close_dev "$MAX_CLOSE_DEVIATION_SCALED" --arg liq_dev "$MAX_LIQ_DEVIATION_SCALED" \
      '.max_close_deviation_scaled == $close_dev and .max_liq_deviation_scaled == $liq_dev' \
      >/dev/null || fail "$side exit config mismatch: $exit_config"
  done

  if [[ "$VERIFY_INERT" == true ]]; then
    # A configured but unwired stack cannot take custody or open positions.
    expect "XLM vault remains inert" "$(view "$XLM_VAULT" get_margin_controller)" null
    expect "USDC vault remains inert" "$(view "$USDC_VAULT" get_margin_controller)" null
    echo "    deployed stack is configured and still inert"
  else
    for binding in \
      "$(unquote "$(view "$XLM_VAULT" get_margin_controller)")" \
      "$(unquote "$(view "$USDC_VAULT" get_margin_controller)")"
    do
      [[ "$binding" == null || "$binding" == "$MARGIN_ID" ]] || \
        fail "vault is wired to an unexpected margin controller: $binding"
    done
    echo "    deployed stack and current vault bindings are valid"
  fi
}

deploy_contract_if_missing() {
  local label=$1 id=$2 salt=$3 hash=$4 actual deployed
  if actual=$(contract_hash "$id"); then
    expect "$label existing hash" "$actual" "$hash"
    echo "    $label already deployed: $id"
    return
  fi
  deployed=$(stellar contract deploy --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --wasm-hash "$hash" --salt "$salt" --source-account "$IDENTITY" --network "$NETWORK")
  expect "$label deployed ID" "$deployed" "$id"
  echo "    $label deployed: $id"
}

initialize_adapter_if_needed() {
  local current
  if current=$(view "$SWAP_ID" get_admin 2>/dev/null); then
    expect "SwapAdapter admin" "$current" "$ADMIN"
    return
  fi
  invoke "$SWAP_ID" initialize --admin "$ADMIN" --router "$AQUARIUS_ROUTER"
  expect "SwapAdapter admin" "$(view "$SWAP_ID" get_admin)" "$ADMIN"
}

initialize_margin_if_needed() {
  local current
  if current=$(view "$MARGIN_ID" get_admin 2>/dev/null); then
    expect "MarginController admin" "$current" "$ADMIN"
    return
  fi
  invoke "$MARGIN_ID" initialize --admin "$ADMIN" --peridottroller "$PERIDOTTROLLER" \
    --swap_adapter "$SWAP_ID" --max_leverage "$MAX_LEVERAGE"
  expect "MarginController admin" "$(view "$MARGIN_ID" get_admin)" "$ADMIN"
}

configure_pair() {
  local side=$1 execution_config exit_config
  execution_config=$(printf \
    '{"max_open_deviation_scaled":"%s","open_slippage_scaled":"%s","close_slippage_scaled":"%s","liquidation_slippage_scaled":"%s"}' \
    "$MAX_OPEN_DEVIATION_SCALED" "$OPEN_SLIPPAGE_SCALED" \
    "$CLOSE_SLIPPAGE_SCALED" "$LIQUIDATION_SLIPPAGE_SCALED")
  exit_config=$(printf \
    '{"max_close_deviation_scaled":"%s","max_liq_deviation_scaled":"%s"}' \
    "$MAX_CLOSE_DEVIATION_SCALED" "$MAX_LIQ_DEVIATION_SCALED")
  invoke "$MARGIN_ID" set_perps_pair_config --admin "$ADMIN" --margin_asset "$USDC" \
    --base_asset "$XLM" --side "$side" --max_leverage "$MAX_LEVERAGE" \
    --maintenance_margin_scaled "$MAINTENANCE_MARGIN_SCALED" \
    --liquidation_incentive_scaled "$LIQUIDATION_INCENTIVE_SCALED"
  invoke "$MARGIN_ID" set_perps_pair_execution_config --admin "$ADMIN" \
    --margin_asset "$USDC" --base_asset "$XLM" --side "$side" --config "$execution_config"
  invoke "$MARGIN_ID" set_perps_pair_exit_config --admin "$ADMIN" \
    --margin_asset "$USDC" --base_asset "$XLM" --side "$side" --config "$exit_config"
}

case "$MODE" in
  preflight|deploy|verify) ;;
  *) fail "MODE must be preflight, deploy, or verify" ;;
esac
[[ "$SIMULATE_UPLOAD_FEES" == true || "$SIMULATE_UPLOAD_FEES" == false ]] || \
  fail "SIMULATE_UPLOAD_FEES must be true or false"
[[ "$VERIFY_INERT" == true || "$VERIFY_INERT" == false ]] || \
  fail "VERIFY_INERT must be true or false"

for command in stellar git grep jq node curl awk; do
  need "$command"
done

if [[ "$MODE" == verify ]]; then
  verify_dependencies
  verify_deployed
  exit 0
fi

verify_dependencies

if [[ "$SIMULATE_UPLOAD_FEES" == true ]]; then
  echo "==> Simulating upload fees without signing or submitting"
  check_upload_fee MarginController "$MARGIN_WASM" "$MAX_MARGIN_UPLOAD_FEE_STROOPS"
  check_upload_fee SwapAdapter "$SWAP_WASM" "$MAX_SWAP_UPLOAD_FEE_STROOPS"
fi
check_deployer_balance

if [[ "$MODE" == preflight ]]; then
  echo "==> Read-only preflight passed; no Mainnet transaction was submitted"
  echo "    To deploy inertly: MODE=deploy CONFIRM_MAINNET=DEPLOY_MARGIN_INERT bash $0"
  exit 0
fi

[[ "$CONFIRM_MAINNET" == DEPLOY_MARGIN_INERT ]] || \
  fail "set CONFIRM_MAINNET=DEPLOY_MARGIN_INERT for the inert Mainnet deployment"

echo "==> Uploading pinned contract artifacts"
uploaded=$(stellar contract upload --no-cache --inclusion-fee "$INCLUSION_FEE" \
  --wasm "$SWAP_WASM" --source-account "$IDENTITY" --network "$NETWORK")
expect "uploaded SwapAdapter" "$uploaded" "$SWAP_WASM_HASH"
uploaded=$(stellar contract upload --no-cache --inclusion-fee "$INCLUSION_FEE" \
  --wasm "$MARGIN_WASM" --source-account "$IDENTITY" --network "$NETWORK")
expect "uploaded MarginController" "$uploaded" "$MARGIN_WASM_HASH"

echo "==> Deploying and initializing the inert contracts"
deploy_contract_if_missing SwapAdapter "$SWAP_ID" "$SWAP_SALT" "$SWAP_WASM_HASH"
initialize_adapter_if_needed
deploy_contract_if_missing MarginController "$MARGIN_ID" "$MARGIN_SALT" "$MARGIN_WASM_HASH"

# MarginController.initialize validates this caller permission on the adapter.
invoke "$SWAP_ID" set_pool_allowed --admin "$ADMIN" --pool "$MARGIN_ID" --allowed true
initialize_margin_if_needed

echo "==> Configuring markets, pool route, and explicit V3 pilot policy"
invoke "$MARGIN_ID" set_market --admin "$ADMIN" --asset "$USDC" --vault "$USDC_VAULT"
invoke "$MARGIN_ID" set_market --admin "$ADMIN" --asset "$XLM" --vault "$XLM_VAULT"
invoke "$SWAP_ID" set_pool_allowed --admin "$ADMIN" --pool "$POOL" --allowed true
invoke "$SWAP_ID" set_pool_binding --admin "$ADMIN" --pool_id "$POOL_ID" \
  --pool "$POOL" --allowed true
invoke "$MARGIN_ID" set_params --admin "$ADMIN" --max_leverage "$MAX_LEVERAGE"
invoke "$MARGIN_ID" set_max_slippage_scaled --admin "$ADMIN" --max_slippage_scaled 50000
invoke "$MARGIN_ID" set_open_fee_bps --admin "$ADMIN" --fee_bps 0
invoke "$MARGIN_ID" set_close_fee_bps --admin "$ADMIN" --fee_bps 0
configure_pair Long
configure_pair Short

echo "==> Proposing two-step admin transfers to the 2-of-3 treasury"
invoke "$MARGIN_ID" set_admin --admin "$ADMIN" --new_admin "$GOVERNANCE_ADMIN"
invoke "$SWAP_ID" set_admin --admin "$ADMIN" --new_admin "$GOVERNANCE_ADMIN"

verify_deployed

cat <<SUMMARY

Mainnet margin contracts are deployed, configured, and inert.
  MarginController: $MARGIN_ID
  SwapAdapter:      $SWAP_ID
  Admin now:        $ADMIN
  Pending admin:    $GOVERNANCE_ADMIN (2-of-3)
  Pool:             $POOL
  Max leverage:     ${MAX_LEVERAGE}x

No ReceiptVault was wired and no Peridottroller liquidation permission was
granted. Accept both admin transfers with the treasury multisig, deploy and
dry-run the Mainnet liquidation keeper, then use activate_margin_mainnet.sh.
SUMMARY
