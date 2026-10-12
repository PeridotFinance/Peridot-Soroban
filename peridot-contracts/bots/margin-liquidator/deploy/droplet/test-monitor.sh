#!/usr/bin/env bash
set -euo pipefail
[[ "$(uname -s)" == Linux ]] || { printf '%s\n' 'Requires Linux tools (GNU date, flock)'; exit 77; }
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
mkdir "$ROOT/bin"
export REAL_DATE
REAL_DATE=$(command -v date)
export STATE_DIR="$ROOT/state" MOCK_SEND_LOG="$ROOT/sent" MOCK_ARGS="$ROOT/args"
export TELEGRAM_TOKEN_FILE="$ROOT/token" TELEGRAM_CHAT_ID_FILE="$ROOT/chat"
printf '%s\n' '123:TEST_ONLY_NOT_A_REAL_TOKEN' > "$TELEGRAM_TOKEN_FILE"
printf '%s\n' '1234' > "$TELEGRAM_CHAT_ID_FILE"
cat > "$ROOT/bin/docker" <<'SH'
#!/usr/bin/env bash
if [[ "$1" == logs ]]; then
  age='now'
  [[ "${SCENARIO:-}" != stale ]] || age='10 minutes ago'
  printf '{"message":"keeper cycle complete","time":"%s"}\n' "$("$REAL_DATE" -u -d "$age" +%Y-%m-%dT%H:%M:%SZ)"
elif [[ "${SCENARIO:-}" == stopped ]]; then
  printf '%s\n' '{"Running":false}'
else
  printf '%s\n' '{"Running":true,"Health":{"Status":"healthy"}}'
fi
SH
cat > "$ROOT/bin/curl" <<'SH'
#!/usr/bin/env bash
if [[ "$*" == *horizon.stellar.org* ]]; then
  printf '{"account_id":"%s","balances":[{"asset_type":"native","balance":"0.1000000"}]}\n' "$KEEPER_PUBLIC_KEY"
  exit 0
fi
cat >/dev/null
printf '%s\n' "$*" >> "$MOCK_ARGS"
printf 'sent\n' >> "$MOCK_SEND_LOG"
[[ "${SCENARIO:-}" != delivery_failed ]] || exit 22
printf '%s\n' '{"ok":true}'
SH
cat > "$ROOT/bin/df" <<'SH'
#!/usr/bin/env bash
printf 'Filesystem 1024-blocks Used Available Capacity Mounted\n/dev/test 100 10 90 10%% /\n'
SH
chmod 0700 "$ROOT/bin/"*
export PATH="$ROOT/bin:$PATH"
export NOTIFY=false KEEPER_PUBLIC_KEY=
run() { bash "$HERE/monitor.sh"; }
output=$(run)
grep -q 'status=healthy' <<<"$output"
printf '%s\n' 'PASS healthy with notifications disabled'
output=$(SCENARIO=stopped run)
grep -q 'status=keeper_stopped,database_unhealthy' <<<"$output"
printf '%s\n' 'PASS detects stopped containers independently'
output=$(SCENARIO=stale run)
grep -q 'status=heartbeat_stale' <<<"$output"
printf '%s\n' 'PASS detects stale heartbeat'
export KEEPER_PUBLIC_KEY=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
output=$(run)
grep -q 'status=wallet_xlm_low' <<<"$output"
printf '%s\n' 'PASS low wallet balance detection using mocked Horizon'
export KEEPER_PUBLIC_KEY= NOTIFY=true
run >/dev/null
run >/dev/null
[[ "$(wc -l < "$MOCK_SEND_LOG")" -eq 1 ]]
! grep -q 'TEST_ONLY' "$MOCK_ARGS"
printf '%s\n' 'PASS healthy notification deduplicates; token not in curl arguments'
SCENARIO=stopped run >/dev/null
SCENARIO=stopped run >/dev/null
[[ "$(wc -l < "$MOCK_SEND_LOG")" -eq 2 ]]
printf '%s\n' 'PASS failure transition and cooldown'
if SCENARIO=delivery_failed run >/dev/null 2>&1; then
  printf '%s\n' 'Unexpected successful notification' >&2
  exit 1
fi
grep -q 'keeper_stopped,database_unhealthy' "$STATE_DIR/last-sent"
printf '%s\n' 'PASS failed delivery does not mark alert delivered'
run >/dev/null
grep -q 'healthy' "$STATE_DIR/last-sent"
printf '%s\n' 'PASS recovery notification'
