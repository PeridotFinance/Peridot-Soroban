#!/usr/bin/env bash
# Host-side monitor: still works when the keeper container has stopped.
set +x
set -euo pipefail
umask 077

STATE_DIR=${STATE_DIR:-/var/lib/peridot-stellar-margin-monitor}
KEEPER_CONTAINER=${KEEPER_CONTAINER:-peridot-stellar-margin-keeper-1}
DATABASE_CONTAINER=${DATABASE_CONTAINER:-peridot-stellar-margin-postgres-1}
HEARTBEAT_MAX_AGE=${HEARTBEAT_MAX_AGE:-180}
KEEPER_PUBLIC_KEY=${KEEPER_PUBLIC_KEY:-}
MIN_WALLET_XLM=${MIN_WALLET_XLM:-5}
TELEGRAM_TOKEN_FILE=${TELEGRAM_TOKEN_FILE:-/etc/peridot-stellar-margin/telegram-token}
TELEGRAM_CHAT_ID_FILE=${TELEGRAM_CHAT_ID_FILE:-/etc/peridot-stellar-margin/telegram-chat-id}
NOTIFY=${NOTIFY:-false}

[[ "$HEARTBEAT_MAX_AGE" =~ ^[1-9][0-9]*$ ]]
[[ "$MIN_WALLET_XLM" =~ ^[0-9]+([.][0-9]+)?$ ]]
[[ "$NOTIFY" == true || "$NOTIFY" == false ]]
install -d -m 0700 "$STATE_DIR"
exec 9>"$STATE_DIR/lock"
flock -n 9 || exit 0
now=$(date +%s)
issues=()
add() { issues+=("$1"); }

keeper=$(timeout 15 docker inspect --format '{{json .State}}' "$KEEPER_CONTAINER" 2>/dev/null) || keeper='{}'
database=$(timeout 15 docker inspect --format '{{json .State}}' "$DATABASE_CONTAINER" 2>/dev/null) || database='{}'
if ! jq -e '.Running == true' <<<"$keeper" >/dev/null; then
  add keeper_stopped
else
  logs=$(timeout 15 docker logs --since 10m --tail 1000 "$KEEPER_CONTAINER" 2>&1) || logs=''
  heartbeat=$(jq -Rrs '[split("\n")[] | fromjson? | select(.message == "keeper cycle complete") | .time] | last // empty' <<<"$logs")
  heartbeat_epoch=$(date -d "$heartbeat" +%s 2>/dev/null) || heartbeat_epoch=0
  if [[ -z "$heartbeat" ]] || (( heartbeat_epoch > now + 10 || now - heartbeat_epoch > HEARTBEAT_MAX_AGE )); then
    add heartbeat_stale
  fi
  if jq -Rrse '[split("\n")[] | fromjson? | select(.level == "error")] | length > 0' <<<"$logs" >/dev/null; then
    add recent_keeper_error
  fi
fi
if ! jq -e '.Running == true and .Health.Status == "healthy"' <<<"$database" >/dev/null; then
  add database_unhealthy
fi
disk_used=$(df -P "$STATE_DIR" | awk 'NR==2 {gsub(/%/, "", $5); print $5}')
[[ "$disk_used" =~ ^[0-9]+$ ]] || exit 1
(( disk_used < 85 )) || add disk_low

if [[ -n "$KEEPER_PUBLIC_KEY" ]]; then
  [[ "$KEEPER_PUBLIC_KEY" =~ ^G[A-Z2-7]{55}$ ]] || exit 1
  account=$(curl --fail --silent --max-time 15 "https://horizon.stellar.org/accounts/$KEEPER_PUBLIC_KEY" 2>/dev/null) || account='{}'
  if ! jq -e --arg key "$KEEPER_PUBLIC_KEY" '.account_id == $key and (.balances | type == "array")' <<<"$account" >/dev/null; then
    add fee_account_unavailable
  elif ! jq -e --argjson minimum "$MIN_WALLET_XLM" '[.balances[] | select(.asset_type == "native") | (.balance | tonumber)] | length == 1 and .[0] >= $minimum' <<<"$account" >/dev/null; then
    add wallet_xlm_low
  fi
fi

status=healthy
if (( ${#issues[@]} )); then
  status=$(IFS=,; printf '%s' "${issues[*]}")
fi
printf 'margin_monitor status=%s\n' "$status"
if [[ "$NOTIFY" != true ]]; then
  printf '%s\n' 'margin_monitor Telegram delivery disabled'
  exit 0
fi

previous=unknown
last_sent=0
if [[ -f "$STATE_DIR/last-sent" ]]; then
  read -r last_sent previous < "$STATE_DIR/last-sent"
  [[ "$last_sent" =~ ^[0-9]+$ ]] || exit 1
fi
if [[ "$status" == "$previous" ]] && { [[ "$status" == healthy ]] || (( now - last_sent < 3600 )); }; then
  exit 0
fi
# Messages contain only fixed health labels, never logs, key material or XDRs.
token=$(< "$TELEGRAM_TOKEN_FILE")
chat=$(< "$TELEGRAM_CHAT_ID_FILE")
[[ "$token" =~ ^[0-9]+:[A-Za-z0-9_-]+$ && "$chat" =~ ^-?[0-9]+$ ]] || exit 1
response=$(printf 'url = "https://api.telegram.org/bot%s/sendMessage"\n' "$token" |
  curl --config - --fail --silent --max-time 15 --request POST \
    --data-urlencode "chat_id=$chat" \
    --data-urlencode "text=Peridot Mainnet margin keeper: $status" 2>/dev/null) || {
  printf '%s\n' 'margin_monitor Telegram delivery failed' >&2
  exit 1
}
jq -e '.ok == true' <<<"$response" >/dev/null || exit 1
printf '%s %s\n' "$now" "$status" > "$STATE_DIR/last-sent.tmp"
mv "$STATE_DIR/last-sent.tmp" "$STATE_DIR/last-sent"
