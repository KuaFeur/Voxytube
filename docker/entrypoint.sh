#!/usr/bin/env bash
set -Eeuo pipefail

DATA_DIR="${VOXYTUBE_DATA_DIR:-/home/container/data}"
PGDATA="${POSTGRES_DATA_DIR:-$DATA_DIR/postgres}"
PG_RUN_DIR="$PGDATA/run"
PG_BIN="${PG_BIN:-$(find /usr/lib/postgresql -type f -name initdb -printf '%h\n' 2>/dev/null | sort -V | tail -n1)}"
DB_NAME="${POSTGRES_DB:-voxytube}"
DB_USER="${POSTGRES_USER:-voxytube}"
DB_PASSWORD_FILE="$DATA_DIR/.postgres_password"

# Pelican/Wings commonly runs game containers as an unprivileged UID.
# Use the postgres system account only when the entrypoint is actually root;
# otherwise PostgreSQL runs as the container UID that owns the mounted volume.
run_pg() {
  if [[ "$(id -u)" -eq 0 ]]; then gosu postgres "$@"; else "$@"; fi
}

if [[ -z "$PG_BIN" || ! -x "$PG_BIN/initdb" ]]; then
  echo "[VoxyTube] PostgreSQL binaries not found." >&2
  exit 1
fi
if [[ ! "$DB_NAME" =~ ^[A-Za-z_][A-Za-z0-9_]{0,62}$ || ! "$DB_USER" =~ ^[A-Za-z_][A-Za-z0-9_]{0,62}$ ]]; then
  echo "[VoxyTube] POSTGRES_DB and POSTGRES_USER must be simple SQL identifiers." >&2
  exit 1
fi

mkdir -p "$DATA_DIR" "$PGDATA"
chmod 0700 "$PGDATA"
if [[ "$(id -u)" -eq 0 ]]; then
  chown -R postgres:postgres "$PGDATA"
elif [[ ! -w "$PGDATA" ]]; then
  echo "[VoxyTube] Data volume is not writable by container UID $(id -u). Check Pelican volume permissions." >&2
  exit 1
fi

if [[ -n "${POSTGRES_PASSWORD:-}" ]]; then
  DB_PASSWORD="$POSTGRES_PASSWORD"
elif [[ -s "$DB_PASSWORD_FILE" ]]; then
  DB_PASSWORD="$(cat "$DB_PASSWORD_FILE")"
else
  umask 077
  DB_PASSWORD="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  printf '%s' "$DB_PASSWORD" > "$DB_PASSWORD_FILE"
fi
if [[ ! "$DB_PASSWORD" =~ ^[A-Za-z0-9._~-]{16,128}$ ]]; then
  echo "[VoxyTube] POSTGRES_PASSWORD must be 16-128 URL-safe characters." >&2
  exit 1
fi

if [[ ! -s "$PGDATA/PG_VERSION" ]]; then
  echo "[VoxyTube] Initializing embedded PostgreSQL..."
  run_pg "$PG_BIN/initdb" -D "$PGDATA" --username=postgres --encoding=UTF8 --auth-local=trust --auth-host=scram-sha-256
else
  if [[ "$(id -u)" -eq 0 ]]; then chown postgres:postgres "$PGDATA"; fi
fi

# initdb requires PGDATA to be empty; create the socket directory only afterwards.
run_pg mkdir -p "$PG_RUN_DIR"
run_pg chmod 0700 "$PG_RUN_DIR"

export PGUSER=postgres
export PGHOST=127.0.0.1
echo "[VoxyTube] Starting embedded PostgreSQL..."
run_pg "$PG_BIN/pg_ctl" -D "$PGDATA" \
  -o "-c listen_addresses=127.0.0.1 -c unix_socket_directories=$PG_RUN_DIR -c port=5432 -c max_connections=${POSTGRES_MAX_CONNECTIONS:-30}" \
  -w start

cleanup() {
  local status=${1:-0}
  trap - TERM INT
  if [[ -n "${APP_PID:-}" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill -TERM "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" || true
  fi
  echo "[VoxyTube] Stopping embedded PostgreSQL..."
  run_pg "$PG_BIN/pg_ctl" -D "$PGDATA" -m fast -w stop || true
  exit "$status"
}
trap 'cleanup 0' TERM INT

if run_pg "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname = '$DB_USER'" | grep -q '^1$'; then
  run_pg "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -c "ALTER ROLE \"$DB_USER\" WITH LOGIN PASSWORD '$DB_PASSWORD'"
else
  run_pg "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -c "CREATE ROLE \"$DB_USER\" WITH LOGIN PASSWORD '$DB_PASSWORD'"
fi

if ! run_pg "$PG_BIN/psql" -v ON_ERROR_STOP=1 --dbname=postgres -tAc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q '^1$'; then
  run_pg "$PG_BIN/createdb" --owner="$DB_USER" "$DB_NAME"
fi

if [[ -n "${POSTGRES_RESTORE_FILE:-}" && -f "$POSTGRES_RESTORE_FILE" ]]; then
  RESTORE_MARKER="$DATA_DIR/.restore_done.$(basename "$POSTGRES_RESTORE_FILE")"
  if [[ ! -f "$RESTORE_MARKER" ]]; then
    echo "[VoxyTube] Restoring PostgreSQL dump from $POSTGRES_RESTORE_FILE..."
    run_pg "$PG_BIN/pg_restore" --clean --if-exists --no-owner --role="$DB_USER" --dbname="$DB_NAME" "$POSTGRES_RESTORE_FILE"
    touch "$RESTORE_MARKER"
    echo "[VoxyTube] PostgreSQL restore completed."
  fi
fi

export DATABASE_URL="postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:5432/$DB_NAME"
export PORT="${PORT:-8080}"
export DATABASE_MAX_CONNECTIONS="${DATABASE_MAX_CONNECTIONS:-10}"
export RUST_LOG="${RUST_LOG:-voxytube=info,tower_http=info}"

echo "[VoxyTube] Starting Rust service on port $PORT..."
if [[ "$(id -u)" -eq 0 ]]; then
  gosu voxytube /usr/local/bin/voxytube &
else
  /usr/local/bin/voxytube &
fi
APP_PID=$!
set +e
wait "$APP_PID"
APP_STATUS=$?
set -e
cleanup "$APP_STATUS"
