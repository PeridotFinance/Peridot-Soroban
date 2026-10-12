#!/usr/bin/env bash
# Prepare, simulate, and add the treasury account's first signature to one
# MarginController/SwapAdapter accept_admin transaction. It never submits.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

TARGET=${TARGET:?set TARGET=margin or TARGET=swap}
NETWORK=${NETWORK:-mainnet-gateway}
TREASURY_IDENTITY=${TREASURY_IDENTITY:-treasury}
TREASURY_ADDRESS=GDM5XP4ACJPXPYP5G2UYGB6WC3G2MUBCYI2ZUCYWVB5CL2JWGCFWNWA3
EXPECTED_CURRENT_ADMIN=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL

case "$TARGET" in
  margin) CONTRACT_ID=CC2MVMWNUSEBJ5ITIKTKTLL3URUPYPGRQB6SWX2HAKEKKXDMYF4EYP5C ;;
  swap) CONTRACT_ID=CCEMGF23WSRRR3C343LQ7SVQWGCNEQ2OA7I5VD7FY7FD4HJAYNQFV7BW ;;
  *) echo "ERROR: TARGET must be margin or swap" >&2; exit 1 ;;
esac

actual_treasury=$(stellar keys public-key "$TREASURY_IDENTITY")
[[ "$actual_treasury" == "$TREASURY_ADDRESS" ]] || {
  echo "ERROR: $TREASURY_IDENTITY resolves to $actual_treasury, expected $TREASURY_ADDRESS" >&2
  exit 1
}

current_admin=$(stellar contract invoke --no-cache --id "$CONTRACT_ID" \
  --source-account "$TREASURY_IDENTITY" --network "$NETWORK" --send no -- get_admin)
current_admin=${current_admin#\"}
current_admin=${current_admin%\"}
[[ "$current_admin" == "$EXPECTED_CURRENT_ADMIN" ]] || {
  echo "ERROR: unexpected current admin: $current_admin" >&2
  exit 1
}

RAW_XDR=$(stellar contract invoke --quiet --build-only --no-cache --id "$CONTRACT_ID" \
  --source-account "$TREASURY_ADDRESS" --network "$NETWORK" -- accept_admin)
PREPARED_XDR=$(stellar tx simulate --quiet --source-account "$TREASURY_ADDRESS" \
  --network "$NETWORK" "$RAW_XDR")
FIRST_SIGNED_XDR=$(stellar tx sign --network "$NETWORK" \
  --sign-with-key "$TREASURY_IDENTITY" "$PREPARED_XDR")
EXPECTED_TX_HASH=$(stellar tx hash --network "$NETWORK" "$FIRST_SIGNED_XDR")

cat <<SUMMARY
Prepared $TARGET accept_admin transaction. Nothing was submitted.

Contract: $CONTRACT_ID
Source/admin: $TREASURY_ADDRESS
Expected hash: $EXPECTED_TX_HASH

Send the hash through a separate trusted channel. Send this command block to
one treasury co-signer; they must replace YOUR_IDENTITY with their local key:

FIRST_SIGNED_XDR='$FIRST_SIGNED_XDR'
EXPECTED_TX_HASH='$EXPECTED_TX_HASH'
ACTUAL_TX_HASH=\$(stellar tx hash --network $NETWORK "\$FIRST_SIGNED_XDR")
if [ "\$ACTUAL_TX_HASH" != "\$EXPECTED_TX_HASH" ]; then
  echo "Transaction hash mismatch; refusing to sign." >&2
  exit 1
fi
FULLY_SIGNED_XDR=\$(stellar tx sign --network $NETWORK --sign-with-key YOUR_IDENTITY "\$FIRST_SIGNED_XDR")
stellar tx send --network $NETWORK "\$FULLY_SIGNED_XDR"
SUMMARY
