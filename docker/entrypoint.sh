#!/usr/bin/env bash
set -Eeuo pipefail

DATA_DIR="${VOXYTUBE_DATA_DIR:-/home/container/data}"
PGDATA="${POSTGRES_DATA_DIR:-$DATA_DIR/postgres}"
PG_BIN="${PG_BIN:-$(find /usr/lib/postgresql -type f -name initdb -printf '%h\\n' 2>/dev/null | sort -V | tail -n1)}"
DB_NAME="${POSTGRES_DB:-voxytube}"
DB_USER="${POSTGRES_USER:-voxytube}"
DB_PASSWORD_FILE="$DATA_DIR/.postgres_password"

if [[ -z "$PG_BIN" || ! -x "$PG_BIN/initdb" ]]; then
  echo "[VoxyTube] PostgreSQL binaries not found." >&2
  exit 1
fi
if [[ ! "$DB_NAME" =~ ^[A-Za-z_][A-Za-z0-9_]{0,62}$ || ! "$DB_USER" =~ ^[A-Za-z_][A-Za-z0-9_]{0,62}$ ]]; then
  echo "[VoxyTube] POSTGRES_DB and POSTGRES_USER must be simple SQL identifiers." >&2
  exit 1
fi

mkdir -p "$DATA_DIR" "$PGDATA"
chown -R postgres:postgres "$PGDATA"
chmod 0700 "$PGDATA"

if [[ -n "${POSTGRES_PASSWORD:-}" ]]; then
  DB_PASSWORD="$POSTGRES_PASSWORD"
elif [[ -s "$DB_PASSWORD_FILE" ]]; then
  DB_PASSWORD="$(cat "$DB_PASSWORD_FILE")"
else
  umask 077
  DB_PASSWORD="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \\n')"
  printf '%s' "$DB_PASSWORD" > "$DB_PASSWORD_FILE"
fi
if [[ ! "$DB_PASSWORD" =~ ^[A-Za-z0-9._~-]{16,128}$ ]]; then
  echo "[VoxyTube] POSTGRES_PASSWORD must be 16-128 URL-safe characters." >&2
  exit 1
fi

if [[ ! -s "$PGDATA/PG_VERSION" ]]; then
  echo "[VoxyTube] Initializing embedded PostgreSQL..."
  gosu postgres "$PG_BIN/initdb" -D "$PGDATA" --username=postgres --auth-local=peer --auth-host=scram-sha-256
fi

echo "[VoxyTube] Starting embedded PostgreSQL..."
gosu postgres "$PG_BIN/pg_ctl" -D "$PGDATA" -o "-c listen_addresses=127.0.0.1 -c port=5432 -c max_connections=${POSTGRES_MAX_CONNECTIONS:-30}" -w start

cleanup() {
  local status=${1:-0}
  trap - TERM INT
  if [[ -n "${APP_PID:-}" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill -TERM "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" || true
  fi
  echo "[VoxyTube] Stopping embedded PostgreSQL..."
  gosu postgres "$PG_BIN/pg_ctl" -D "$PGDATA" -m fast -w stop || true
  exit "$status"
}
trap 'cleanup 0' TERM INT

if gosu postgres "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname = '$DB_USER'" | grep -q '^1$'; then
  gosu postgres "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -c "ALTER ROLE \"$DB_USER\" WITH LOGIN PASSWORD '$DB_PASSWORD'"
else
  gosu postgres "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -c "CREATE ROLE \"$DB_USER\" WITH LOGIN PASSWORD '$DB_PASSWORD'"
fi

if ! gosu postgres "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -tAc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q '^1$'; then
  gosu postgres "$PG_BIN/createdb" --owner="$DB_USER" "$DB_NAME"
fi

export DATABASE_URL="postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:5432/$DB_NAME"
export PORT="${PORT:-8080}"
export DATABASE_MAX_CONNECTIONS="${DATABASE_MAX_CONNECTIONS:-10}"
export RUST_LOG="${RUST_LOG:-voxytube=info,tower_http=info}"

echo "[VoxyTube] Starting Rust service on port $PORT..."
gosu voxytube /usr/local/bin/voxytube &
APP_PID=$!
set +e
wait "$APP_PID"
APP_STATUS=$?
set -e
cleanup "$APP_STATUS"
