#!/usr/bin/env bash
set -euo pipefail
umask 077

if [[ "$(uname -s)" != Linux || "$EUID" != 0 ]]; then
  printf '%s\n' 'Run as root on the NEW Linux keeper droplet, not on your Mac.' >&2
  exit 1
fi
command -v openssl >/dev/null
command -v docker >/dev/null
docker compose version >/dev/null
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CONFIG=/etc/peridot-stellar-margin

# A partial preparation requires inspection too. Never regenerate DB credentials.
if [[ -e "$CONFIG" ]]; then
  printf '%s\n' 'Configuration already exists; refusing to overwrite it.' >&2
  exit 1
fi
if docker volume inspect peridot-stellar-margin-journal >/dev/null 2>&1; then
  printf '%s\n' 'Journal volume exists; restore its original configuration, do not reseed.' >&2
  exit 1
fi
docker info >/dev/null
install -d -m 0700 "$CONFIG"
openssl rand -hex 32 > "$CONFIG/postgres-password"
openssl rand -hex 32 > "$CONFIG/keeper-db-password"
openssl req -x509 -newkey rsa:3072 -sha256 -nodes -days 365 \
  -subj '/CN=postgres' -addext 'subjectAltName=DNS:postgres' \
  -addext 'basicConstraints=critical,CA:TRUE' \
  -keyout "$CONFIG/server.key" -out "$CONFIG/server.crt" 2>/dev/null
# postgres:16-bookworm uses UID/GID 999. Confirm that before provisioning.
chown root:999 "$CONFIG/server.key" "$CONFIG/keeper-db-password" "$CONFIG/postgres-password"
chmod 0640 "$CONFIG/server.key" "$CONFIG/keeper-db-password" "$CONFIG/postgres-password"
chmod 0644 "$CONFIG/server.crt"
install -m 0600 "$HERE/keeper.env.example" "$CONFIG/keeper.env"
printf '\nDATABASE_URL=postgresql://keeper:%s@postgres:5432/margin_keeper\n' \
  "$(< "$CONFIG/keeper-db-password")" >> "$CONFIG/keeper.env"
printf '%s\n' 'Dry-run configuration prepared. No services started or Stellar keys generated.'
