#!/usr/bin/env bash
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
IDENTITY="${IDENTITY:-dev}"
FACTORY_ID="${FACTORY_ID:-peridot_smart_account_factory}"
OWNER="${OWNER:-$(stellar keys address "$IDENTITY")}"

echo "Factory: $FACTORY_ID"
echo "Owner: $OWNER"

stellar contract invoke --id "$FACTORY_ID" --source-account "$IDENTITY" --network "$NETWORK" -- \
  get_account --user "$OWNER"
