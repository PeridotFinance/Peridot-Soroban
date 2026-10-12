#!/usr/bin/env bash
set -euo pipefail

# Only run by the official PostgreSQL entrypoint on a new, empty data volume.
export KEEPER_DB_PASSWORD
KEEPER_DB_PASSWORD="$(< /run/secrets/keeper-db-password)"
psql -v ON_ERROR_STOP=1 --username postgres --dbname margin_keeper <<'SQL'
\getenv keeper_password KEEPER_DB_PASSWORD
CREATE ROLE keeper LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'keeper_password';
ALTER DATABASE margin_keeper OWNER TO keeper;
REVOKE ALL ON DATABASE margin_keeper FROM PUBLIC;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
GRANT USAGE, CREATE ON SCHEMA public TO keeper;
SQL
unset KEEPER_DB_PASSWORD
