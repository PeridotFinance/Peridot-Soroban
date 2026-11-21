#!/usr/bin/env bash
# usage: px.sh show | swap <in_idx 0=PYUSD|1=USDC> <amount_raw> | oracle <price_1e14>   (clg-admin, TESTNET)
source "$(dirname "$0")/lib.sh"
case $1 in
  show) echo "slot0=$(view "$POOL" get_slot0) oracle_pyusd=$(view "$ORACLE" lastprice --asset "{\"Stellar\":\"$PYUSD\"}")";;
  swap) inv "pool swap in=$2 amount=$3" "$POOL" clg-admin swap --user "$ADM" --in_idx "$2" --out_idx $((1-$2)) --in_amount "$3" --out_min 0; echo "slot0=$(view "$POOL" get_slot0)";;
  oracle) inv "oracle PYUSD=$2" "$ORACLE" clg-admin set_price --asset "{\"Stellar\":\"$PYUSD\"}" --price "$2";;
esac
