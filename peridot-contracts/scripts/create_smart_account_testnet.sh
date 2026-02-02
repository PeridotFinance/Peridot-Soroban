#!/usr/bin/env bash
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
IDENTITY="${IDENTITY:-dev}"
FACTORY_ID="${FACTORY_ID:-peridot_smart_account_factory}"
OWNER="${OWNER:-$(stellar keys address "$IDENTITY")}"
PERIDOTTROLLER_ID="${PERIDOTTROLLER_ID:-}"
MARGIN_CONTROLLER_ID="${MARGIN_CONTROLLER_ID:-}"

if [[ -z "$PERIDOTTROLLER_ID" || -z "$MARGIN_CONTROLLER_ID" ]]; then
  echo "Set PERIDOTTROLLER_ID and MARGIN_CONTROLLER_ID env vars first."
  exit 1
fi

SIGNER_HEX="${SIGNER_HEX:-}"
SALT_HEX="${SALT_HEX:-}"

if [[ -z "$SIGNER_HEX" ]]; then
  if ! command -v openssl >/dev/null 2>&1; then
    echo "openssl not found; set SIGNER_HEX (0x + 64 hex chars) manually."
    exit 1
  fi
  TMP_DIR="$(mktemp -d)"
  KEY_PATH="$TMP_DIR/signer_ed25519.pem"
  openssl genpkey -algorithm Ed25519 -out "$KEY_PATH" >/dev/null 2>&1
  PUB_HEX="$(openssl pkey -in "$KEY_PATH" -pubout -outform DER 2>/dev/null | tail -c 32 | xxd -p -c 32)"
  SIGNER_HEX="${PUB_HEX}"
  echo "Generated signer key at: $KEY_PATH"
  echo "Signer pubkey (BytesN<32>): $SIGNER_HEX"
fi

if [[ -z "$SALT_HEX" ]]; then
  if ! command -v openssl >/dev/null 2>&1; then
    echo "openssl not found; set SALT_HEX (0x + 64 hex chars) manually."
    exit 1
  fi
  SALT_HEX="$(openssl rand -hex 32)"
fi

SIGNER_HEX="${SIGNER_HEX#0x}"
SALT_HEX="${SALT_HEX#0x}"

CONFIG_JSON=$(cat <<EOF
{"account_type":"Basic","owner":"$OWNER","signer":"$SIGNER_HEX","peridottroller":"$PERIDOTTROLLER_ID","margin_controller":"$MARGIN_CONTROLLER_ID"}
EOF
)

echo "Creating smart account..."
stellar contract invoke --id "$FACTORY_ID" --source-account "$IDENTITY" --network "$NETWORK" -- \
  create_account --config "$CONFIG_JSON" --salt "$SALT_HEX"

echo "Done."
echo "Owner: $OWNER"
echo "Signer pubkey: $SIGNER_HEX"
echo "Salt: $SALT_HEX"
