#!/usr/bin/env bash
# Prepare unsigned receipt keepalive calls only. No signing or submission.
# The revised bump_ttl renews instance safeguards and persistent global state.
set -euo pipefail
: "${RPC_URL:?explicit RPC required}"
: "${SOURCE_PUBLIC:?public fee-paying source required}"
: "${CASH_MARKET_MANIFEST:?reviewed fresh market manifest required}"
: "${OUT_DIR:?new output directory required}"
[[ "$SOURCE_PUBLIC" =~ ^G[A-Z2-7]{55}$ ]] || exit 2
[[ ! -e "$OUT_DIR" ]] || { echo 'Refusing to overwrite previous envelopes' >&2; exit 2; }
# Only explicitly reviewed four-market manifests; retired registry IDs forbidden.
markets=$(python3 - "$CASH_MARKET_MANIFEST" <<'PY'
import json,re,sys
from pathlib import Path
m=json.loads(Path(sys.argv[1]).read_text())
assert m['status']=='VERIFIED_FRESH_DEPLOYMENT'
assert set(m['markets'])=={'USDC','XLM','PYUSD','EURC'}
legacy=Path('addresses.md').read_text()
ids=[]
for name,row in m['markets'].items():
 a=row['receipt'];assert re.fullmatch(r'C[A-Z2-7]{55}',a)
 assert a not in legacy,'Address already in historical registry; review required'
 assert row['borrowing_enabled'] is False and row['strategy'] is None
 ids.append(a)
assert len(set(ids))==4
print('\n'.join(ids))
PY
)
mkdir -m 700 "$OUT_DIR"
while IFS= read -r market; do
  stellar contract invoke --id "$market" --source-account "$SOURCE_PUBLIC" \
    --rpc-url "$RPC_URL" --network-passphrase 'Public Global Stellar Network ; September 2015' \
    --build-only -- bump_ttl > "$OUT_DIR/$market.unsigned.xdr"
done <<< "$markets"
echo 'Unsigned only: simulate, inspect restoration preamble/resources, then operator review. No transaction sent.'
