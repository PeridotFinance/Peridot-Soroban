#!/usr/bin/env bash
set -euo pipefail

# Rebalance the current testnet Aquarius XLM/mock-USDT pool toward the oracle price.
#
# Default behavior is DRY_RUN=true. Set DRY_RUN=false to submit swaps.
# AUTO mode compares the pool quote with the oracle band and chooses the corrective
# direction. It handles both underpayment and overpayment of XLM.
#
# Common usage:
#   bash scripts/rebalance_aquarius_pool_testnet.sh
#   DRY_RUN=false REBALANCE_AMOUNT=500000000 MAX_STEPS=3 bash scripts/rebalance_aquarius_pool_testnet.sh
#
# Optional env:
#   IDENTITY                  local Stellar identity used to sign swaps, default: dev
#   USER_ADDRESS              public key for contract auth, default: identity public key
#   NETWORK                   default: testnet
#   PROBE_USDT_IN             USDT input used to test Long route health, default: 1800000
#   MAX_SLIPPAGE_SCALED       oracle slippage tolerance in 1e6 scale, default: 50000 (5%)
#   AUTO_AMOUNT               binary-search the needed amount, default: true
#   REBALANCE_AMOUNT          max amount for auto-search, or fixed amount when AUTO_AMOUNT=false
#                             default: 500000000 (50 XLM)
#   FEE_SAMPLE_AMOUNT         amount used to infer pool fee from estimate_swap, default: 50000000
#   SEARCH_BUFFER_SCALED      target buffer for model error in 1e6 scale, default: 5000 (0.5%)
#   REBALANCE_SLIPPAGE_SCALED min-out tolerance for the rebalance swap, default: 50000 (5%)
#   XLM_BALANCE_BUFFER        XLM retained by the signer for reserves/fees, default: 1000000000 (100 XLM)
#   TOKEN_BALANCE_BUFFER      input-token dust retained for non-XLM swaps, default: 10000000 (1 token)
#   MAX_STEPS                 max submitted rebalance swaps when DRY_RUN=false, default: 3
#   REBALANCE_DIRECTION       AUTO, XLM_TO_USDT, or USDT_TO_XLM, default: AUTO

IDENTITY=${IDENTITY:-dev}
NETWORK=${NETWORK:-testnet}

PERIDOTTROLLER=${PERIDOTTROLLER:-CDMXPWG55776NECXQMWNBXMEQXZUAWA2AJBCQS7SU7SA64XHMO3KB3O6}
SWAP_ID=${SWAP_ID:-CBSTR53W52JHCRXW4I4QAL7FJJIT2D7MVFTNC3VJRNOPTIKCCZKDTKDL}

XLM_TOKEN=${XLM_TOKEN:-CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC}
USDT_TOKEN=${USDT_TOKEN:-CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY}
AQUARIUS_POOL_ID=${AQUARIUS_POOL_ID:-9ac7a9cde23ac2ada11105eeaa42e43c2ea8332ca0aa8f41f58d7160274d718e}
AQUARIUS_POOL=${AQUARIUS_POOL:-CCMNSENXDBNJSY72BDIPH5CCXLLHBKZ4LXTRKDLKZN4UI2NJFQLWTLD6}

PROBE_USDT_IN=${PROBE_USDT_IN:-1800000}
MAX_SLIPPAGE_SCALED=${MAX_SLIPPAGE_SCALED:-50000}
AUTO_AMOUNT=${AUTO_AMOUNT:-true}
REBALANCE_AMOUNT=${REBALANCE_AMOUNT:-500000000}
FEE_SAMPLE_AMOUNT=${FEE_SAMPLE_AMOUNT:-50000000}
SEARCH_BUFFER_SCALED=${SEARCH_BUFFER_SCALED:-5000}
REBALANCE_SLIPPAGE_SCALED=${REBALANCE_SLIPPAGE_SCALED:-50000}
XLM_BALANCE_BUFFER=${XLM_BALANCE_BUFFER:-1000000000}
TOKEN_BALANCE_BUFFER=${TOKEN_BALANCE_BUFFER:-10000000}
MAX_STEPS=${MAX_STEPS:-3}
DRY_RUN=${DRY_RUN:-true}
REBALANCE_DIRECTION=${REBALANCE_DIRECTION:-AUTO}
STELLAR_WORKDIR=${STELLAR_WORKDIR:-/tmp}

ROUTE=${ROUTE:-"[[[\"$XLM_TOKEN\",\"$USDT_TOKEN\"],\"$AQUARIUS_POOL_ID\",\"$AQUARIUS_POOL\"]]"}

stellar_cli() {
  (cd "$STELLAR_WORKDIR" && stellar "$@")
}

USER_ADDRESS=${USER_ADDRESS:-$(stellar_cli keys public-key "$IDENTITY" | tail -n 1)}

require_python() {
  if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is required for JSON parsing and integer math." >&2
    exit 1
  fi
}

last_json_line() {
  python3 -c 'import sys; lines=[line.strip() for line in sys.stdin if line.strip()]; print(lines[-1] if lines else "")'
}

json_number() {
  python3 -c 'import json,sys; value=json.loads(sys.stdin.read().strip()); print(int(value))'
}

json_price_pair() {
  python3 -c '
import json, sys
value = json.loads(sys.stdin.read().strip())
if value is None:
    raise SystemExit("price unavailable")
print(f"{int(value[0])} {int(value[1])}")
'
}

int_ge() {
  python3 - "$1" "$2" <<'PY'
import sys
raise SystemExit(0 if int(sys.argv[1]) >= int(sys.argv[2]) else 1)
PY
}

min_int() {
  python3 - "$1" "$2" <<'PY'
import sys
print(min(int(sys.argv[1]), int(sys.argv[2])))
PY
}

subtract_floor_zero() {
  python3 - "$1" "$2" <<'PY'
import sys
print(max(0, int(sys.argv[1]) - int(sys.argv[2])))
PY
}

calc_oracle_min_xlm_out() {
  python3 - "$PROBE_USDT_IN" "$1" "$2" "$3" "$4" "$MAX_SLIPPAGE_SCALED" <<'PY'
import sys

usdt_in = int(sys.argv[1])
usdt_price_num = int(sys.argv[2])
usdt_price_den = int(sys.argv[3])
xlm_price_num = int(sys.argv[4])
xlm_price_den = int(sys.argv[5])
slippage_scaled = int(sys.argv[6])

precision = 1_000_000
numerator = usdt_in * usdt_price_num * xlm_price_den * (precision - slippage_scaled)
denominator = usdt_price_den * xlm_price_num * precision
print((numerator + denominator - 1) // denominator)
PY
}

calc_oracle_expected_xlm_out() {
  python3 - "$PROBE_USDT_IN" "$1" "$2" "$3" "$4" <<'PY'
import sys

usdt_in = int(sys.argv[1])
usdt_price_num = int(sys.argv[2])
usdt_price_den = int(sys.argv[3])
xlm_price_num = int(sys.argv[4])
xlm_price_den = int(sys.argv[5])

numerator = usdt_in * usdt_price_num * xlm_price_den
denominator = usdt_price_den * xlm_price_num
print((numerator + denominator - 1) // denominator)
PY
}

calc_oracle_max_xlm_out() {
  python3 - "$1" "$MAX_SLIPPAGE_SCALED" <<'PY'
import sys
expected = int(sys.argv[1])
slippage_scaled = int(sys.argv[2])
print((expected * (1_000_000 + slippage_scaled)) // 1_000_000)
PY
}

calc_min_out() {
  python3 - "$1" "$REBALANCE_SLIPPAGE_SCALED" <<'PY'
import sys
estimated = int(sys.argv[1])
slippage_scaled = int(sys.argv[2])
print((estimated * (1_000_000 - slippage_scaled)) // 1_000_000)
PY
}

calc_recommended_rebalance() {
  python3 - "$1" "$2" "$3" "$4" "$5" "$6" "$7" "$8" "$9" "${10}" <<'PY'
import sys

reserve_xlm = int(sys.argv[1])
reserve_usdt = int(sys.argv[2])
probe_usdt = int(sys.argv[3])
current_probe_out = int(sys.argv[4])
target_probe_out = int(sys.argv[5])
fee_sample_amount = int(sys.argv[6])
fee_sample_out = int(sys.argv[7])
direction = sys.argv[8]
max_amount = int(sys.argv[9])
buffer_scaled = int(sys.argv[10])

FEE_SCALE = 10**12
PRECISION = 1_000_000

def ceil_div(a, b):
    return (a + b - 1) // b

def fee_from_quote(amount_in, reserve_in, reserve_out, quoted_out):
    if amount_in <= 0 or reserve_in <= 0 or reserve_out <= 0:
        return FEE_SCALE
    if quoted_out <= 0 or quoted_out >= reserve_out:
        return FEE_SCALE
    numerator = quoted_out * reserve_in * FEE_SCALE
    denominator = (reserve_out - quoted_out) * amount_in
    fee = numerator // denominator
    if fee <= 0:
        return FEE_SCALE
    return min(fee, FEE_SCALE)

def cp_out(amount_in, reserve_in, reserve_out, fee_scaled):
    if amount_in <= 0 or reserve_in <= 0 or reserve_out <= 0:
        return 0
    amount_eff = amount_in * fee_scaled
    return (amount_eff * reserve_out) // (reserve_in * FEE_SCALE + amount_eff)

probe_fee = fee_from_quote(probe_usdt, reserve_usdt, reserve_xlm, current_probe_out)
if direction == "XLM_TO_USDT":
    rebalance_fee = fee_from_quote(fee_sample_amount, reserve_xlm, reserve_usdt, fee_sample_out)
elif direction == "USDT_TO_XLM":
    rebalance_fee = fee_from_quote(fee_sample_amount, reserve_usdt, reserve_xlm, fee_sample_out)
else:
    raise SystemExit(f"unsupported direction {direction}")

if direction == "XLM_TO_USDT":
    search_target = ceil_div(target_probe_out * (PRECISION + buffer_scaled), PRECISION)
else:
    search_target = (target_probe_out * (PRECISION - buffer_scaled)) // PRECISION

def post_probe_out(rebalance_amount):
    if direction == "XLM_TO_USDT":
        usdt_out = cp_out(rebalance_amount, reserve_xlm, reserve_usdt, rebalance_fee)
        xlm_after = reserve_xlm + rebalance_amount
        usdt_after = max(1, reserve_usdt - usdt_out)
    else:
        xlm_out = cp_out(rebalance_amount, reserve_usdt, reserve_xlm, rebalance_fee)
        usdt_after = reserve_usdt + rebalance_amount
        xlm_after = max(1, reserve_xlm - xlm_out)
    return cp_out(probe_usdt, usdt_after, xlm_after, probe_fee)

max_post_out = post_probe_out(max_amount)
target_unreachable = (
    max_post_out < search_target
    if direction == "XLM_TO_USDT"
    else max_post_out > search_target
)
if target_unreachable:
    print(f"ERR {max_post_out} {search_target} {probe_fee} {rebalance_fee}")
    raise SystemExit(0)

lo = 0
hi = max_amount
while lo + 1 < hi:
    mid = (lo + hi) // 2
    reached = (
        post_probe_out(mid) >= search_target
        if direction == "XLM_TO_USDT"
        else post_probe_out(mid) <= search_target
    )
    if reached:
        hi = mid
    else:
        lo = mid

print(f"OK {hi} {post_probe_out(hi)} {search_target} {probe_fee} {rebalance_fee}")
PY
}

invoke_json() {
  local raw
  if ! raw=$(stellar_cli contract invoke "$@"); then
    return 1
  fi
  printf '%s\n' "$raw" | last_json_line
}

get_price_pair() {
  local token="$1"
  invoke_json \
    --id "$PERIDOTTROLLER" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --send no \
    -- \
    get_price_usd \
    --token "$token" | json_price_pair
}

estimate_pool_swap() {
  local in_idx="$1"
  local out_idx="$2"
  local amount="$3"
  invoke_json \
    --id "$SWAP_ID" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --send no \
    -- \
    estimate_pool_swap \
    --pool "$AQUARIUS_POOL" \
    --in_idx "$in_idx" \
    --out_idx "$out_idx" \
    --amount_in "$amount" | json_number
}

token_balance() {
  local token="$1"
  local owner="$2"
  local raw

  if raw=$(invoke_json \
    --id "$token" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --send no \
    -- \
    balance \
    --id "$owner" 2>/dev/null); then
    printf '%s\n' "$raw" | json_number
    return
  fi

  raw=$(invoke_json \
    --id "$token" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --send no \
    -- \
    balance \
    --who "$owner" 2>/dev/null)
  printf '%s\n' "$raw" | json_number
}

submit_rebalance_swap() {
  local token_in="$1"
  local amount="$2"
  local min_out="$3"

  stellar_cli contract invoke \
    --id "$SWAP_ID" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    -- \
    swap_chained \
    --user "$USER_ADDRESS" \
    --swaps_chain "$ROUTE" \
    --token_in "$token_in" \
    --amount "$amount" \
    --amount_with_slippage "$min_out"
}

print_rebalance_command() {
  local token_in="$1"
  local amount="$2"
  local min_out="$3"

  cat <<EOF
DRY_RUN=false REBALANCE_AMOUNT=$amount AUTO_AMOUNT=false bash scripts/rebalance_aquarius_pool_testnet.sh

Equivalent direct invoke:
stellar contract invoke --id "$SWAP_ID" --source-account "$IDENTITY" --network "$NETWORK" -- \\
  swap_chained \\
  --user "$USER_ADDRESS" \\
  --swaps_chain '$ROUTE' \\
  --token_in "$token_in" \\
  --amount "$amount" \\
  --amount_with_slippage "$min_out"
EOF
}

require_python

read -r xlm_price_num xlm_price_den < <(get_price_pair "$XLM_TOKEN")
read -r usdt_price_num usdt_price_den < <(get_price_pair "$USDT_TOKEN")

target_xlm_out=$(calc_oracle_min_xlm_out "$usdt_price_num" "$usdt_price_den" "$xlm_price_num" "$xlm_price_den")
expected_xlm_out=$(calc_oracle_expected_xlm_out "$usdt_price_num" "$usdt_price_den" "$xlm_price_num" "$xlm_price_den")
max_xlm_out=$(calc_oracle_max_xlm_out "$expected_xlm_out")

prepare_rebalance() {
  local requested_direction="$1"
  local direction
  local required_direction
  local direction_target
  local pool_xlm_balance
  local pool_usdt_balance
  local current_xlm_out
  local fee_sample_estimate
  local input_balance
  local balance_buffer
  local available_rebalance_amount
  local search_max_amount
  local recommendation
  local recommendation_status
  local model_post_out
  local model_target
  local probe_fee
  local rebalance_fee

  current_xlm_out=$(estimate_pool_swap 1 0 "$PROBE_USDT_IN")

  echo "Pool: $AQUARIUS_POOL"
  echo "Route: USDT -> XLM probe amount $PROBE_USDT_IN"
  echo "Oracle expected XLM out: $expected_xlm_out"
  echo "Accepted target band: $target_xlm_out .. $max_xlm_out"
  echo "Current pool XLM out: $current_xlm_out"

  if int_ge "$current_xlm_out" "$target_xlm_out" && int_ge "$max_xlm_out" "$current_xlm_out"; then
    echo "No rebalance needed for this probe size."
    exit 0
  fi

  if int_ge "$target_xlm_out" "$current_xlm_out"; then
    required_direction="XLM_TO_USDT"
    direction_target="$target_xlm_out"
  else
    required_direction="USDT_TO_XLM"
    direction_target="$max_xlm_out"
  fi

  case "$requested_direction" in
    AUTO)
      direction="$required_direction"
      ;;
    XLM_TO_USDT|USDT_TO_XLM)
      direction="$requested_direction"
      if [[ "$direction" != "$required_direction" ]]; then
        echo "REBALANCE_DIRECTION=$direction would move the pool farther outside the oracle band; required direction is $required_direction." >&2
        exit 1
      fi
      ;;
    *)
      echo "Unsupported REBALANCE_DIRECTION=$requested_direction. Use AUTO, XLM_TO_USDT, or USDT_TO_XLM." >&2
      exit 1
      ;;
  esac
  rebalance_direction="$direction"

  case "$direction" in
    XLM_TO_USDT)
      rebalance_token_in="$XLM_TOKEN"
      balance_buffer="$XLM_BALANCE_BUFFER"
      fee_sample_estimate=$(estimate_pool_swap 0 1 "$FEE_SAMPLE_AMOUNT")
      ;;
    USDT_TO_XLM)
      rebalance_token_in="$USDT_TOKEN"
      balance_buffer="$TOKEN_BALANCE_BUFFER"
      fee_sample_estimate=$(estimate_pool_swap 1 0 "$FEE_SAMPLE_AMOUNT")
      ;;
  esac

  input_balance=$(token_balance "$rebalance_token_in" "$USER_ADDRESS")
  available_rebalance_amount=$(subtract_floor_zero "$input_balance" "$balance_buffer")
  search_max_amount=$(min_int "$REBALANCE_AMOUNT" "$available_rebalance_amount")
  echo "Signer input-token balance: $input_balance"
  echo "Maximum funded rebalance amount: $search_max_amount"
  if [[ "$search_max_amount" == "0" ]]; then
    echo "Signer has no spendable input-token balance after the safety buffer." >&2
    exit 1
  fi

  if [[ "$AUTO_AMOUNT" == "false" || "$AUTO_AMOUNT" == "0" || "$AUTO_AMOUNT" == "no" ]]; then
    rebalance_amount="$REBALANCE_AMOUNT"
    if int_ge "$rebalance_amount" "$((available_rebalance_amount + 1))"; then
      echo "REBALANCE_AMOUNT=$rebalance_amount exceeds the funded maximum $available_rebalance_amount." >&2
      exit 1
    fi
    model_post_out="n/a"
    model_target="n/a"
    probe_fee="n/a"
    rebalance_fee="n/a"
  else
    pool_xlm_balance=$(token_balance "$XLM_TOKEN" "$AQUARIUS_POOL")
    pool_usdt_balance=$(token_balance "$USDT_TOKEN" "$AQUARIUS_POOL")
    recommendation=$(calc_recommended_rebalance \
      "$pool_xlm_balance" \
      "$pool_usdt_balance" \
      "$PROBE_USDT_IN" \
      "$current_xlm_out" \
      "$direction_target" \
      "$FEE_SAMPLE_AMOUNT" \
      "$fee_sample_estimate" \
      "$direction" \
      "$search_max_amount" \
      "$SEARCH_BUFFER_SCALED")

    read -r recommendation_status rebalance_amount model_post_out model_target probe_fee rebalance_fee <<<"$recommendation"
    if [[ "$recommendation_status" != "OK" ]]; then
      echo "Auto-search cannot reach target within funded maximum $search_max_amount." >&2
      echo "Model post-probe XLM out at max: $rebalance_amount" >&2
      echo "Buffered model target: $model_post_out" >&2
      echo "Increase REBALANCE_AMOUNT or reduce SEARCH_BUFFER_SCALED." >&2
      exit 1
    fi

    echo "Pool XLM reserve: $pool_xlm_balance"
    echo "Pool USDT reserve: $pool_usdt_balance"
    echo "Auto-search buffered model target: $model_target"
    echo "Auto-search model post-probe XLM out: $model_post_out"
  fi

  case "$direction" in
    XLM_TO_USDT)
      rebalance_estimate=$(estimate_pool_swap 0 1 "$rebalance_amount")
      ;;
    USDT_TO_XLM)
      rebalance_estimate=$(estimate_pool_swap 1 0 "$rebalance_amount")
      ;;
  esac
  rebalance_min_out=$(calc_min_out "$rebalance_estimate")

  echo "Rebalance direction: $direction"
  echo "Rebalance amount: $rebalance_amount"
  echo "Estimated rebalance output: $rebalance_estimate"
  echo "Rebalance min output: $rebalance_min_out"
}

rebalance_direction="$REBALANCE_DIRECTION"
prepare_rebalance "$rebalance_direction"

if [[ "$DRY_RUN" != "false" && "$DRY_RUN" != "0" && "$DRY_RUN" != "no" ]]; then
  echo
  echo "Dry run only. To submit the searched amount, run:"
  print_rebalance_command "$rebalance_token_in" "$rebalance_amount" "$rebalance_min_out"
  echo
  echo "Or let the script search and submit:"
  echo "DRY_RUN=false bash scripts/rebalance_aquarius_pool_testnet.sh"
  exit 0
fi

for ((step = 1; step <= MAX_STEPS; step++)); do
  echo
  echo "Submitting rebalance step $step/$MAX_STEPS..."
  submit_rebalance_swap "$rebalance_token_in" "$rebalance_amount" "$rebalance_min_out"

  current_xlm_out=$(estimate_pool_swap 1 0 "$PROBE_USDT_IN")
  echo "Post-swap pool XLM out: $current_xlm_out"

  if int_ge "$current_xlm_out" "$target_xlm_out" && int_ge "$max_xlm_out" "$current_xlm_out"; then
    echo "Rebalance target reached."
    exit 0
  fi

  prepare_rebalance "$REBALANCE_DIRECTION"
done

echo "Max steps reached. Re-run with a larger REBALANCE_AMOUNT or MAX_STEPS if the route is still outside the oracle band." >&2
exit 1
