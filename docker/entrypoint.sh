#!/bin/sh
# Applies pending sqlx migrations, then execs the container command.
#
# Without schema 20260828121000 (`stack_template_version.config_contract`, a
# column with no `#[sqlx(default)]`) Stacker cannot resolve a template at all,
# so this has to happen before boot. Fail closed: a container that cannot
# migrate must not start.
set -e

MIGRATIONS_DIR="${MIGRATIONS_DIR:-/app/migrations}"
CONFIGURATION_YAML="${CONFIGURATION_YAML:-/app/configuration.yaml}"

# sqlx-cli only speaks DATABASE_URL, while production configuration.yaml keeps
# the same connection details as five separate keys. The userinfo is
# percent-encoded: the server builds its own PgConnectOptions and tolerates raw
# credentials, but a password containing @ / : # in a URL silently points sqlx
# at the wrong host.
db_key() {
    awk -v key="$1" '
        /^[[:space:]]*#/ { next }
        /^database:[[:space:]]*$/ { in_db = 1; next }
        in_db && /^[^[:space:]]/ { exit }
        in_db {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            if (index(line, key ":") == 1) {
                value = substr(line, length(key) + 2)
                sub(/[[:space:]]+#.*$/, "", value)
                sub(/^[[:space:]]+/, "", value)
                sub(/[[:space:]]+$/, "", value)
                gsub(/^[\047"]|[\047"]$/, "", value)
                print value
                exit
            }
        }
    ' "$CONFIGURATION_YAML"
}

# Same key lookup, but the value is percent-encoded for use in the URL userinfo.
db_userinfo() {
    db_key "$1" | awk '
        BEGIN {
            for (i = 0; i < 256; i++) ord[sprintf("%c", i)] = i
        }
        {
            out = ""
            for (i = 1; i <= length($0); i++) {
                c = substr($0, i, 1)
                if (c ~ /^[A-Za-z0-9._~-]$/) out = out c
                else out = out sprintf("%%%02X", ord[c])
            }
            print out
        }
    '
}

if [ -z "${DATABASE_URL:-}" ]; then
    if [ ! -f "$CONFIGURATION_YAML" ]; then
        echo "entrypoint: DATABASE_URL is unset and $CONFIGURATION_YAML is missing" >&2
        exit 1
    fi

    host=$(db_key host)
    port=$(db_key port)
    username=$(db_userinfo username)
    password=$(db_userinfo password)
    name=$(db_key database_name)

    if [ -z "$host" ] || [ -z "$port" ] || [ -z "$username" ] || [ -z "$name" ]; then
        echo "entrypoint: no usable database block in $CONFIGURATION_YAML" >&2
        exit 1
    fi

    DATABASE_URL="postgres://${username}:${password}@${host}:${port}/${name}"
fi
export DATABASE_URL

echo "entrypoint: applying migrations from $MIGRATIONS_DIR"
sqlx migrate run --source "$MIGRATIONS_DIR"

if [ "$#" -eq 0 ]; then
    set -- /app/server
fi

exec "$@"
