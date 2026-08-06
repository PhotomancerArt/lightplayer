#!/usr/bin/env bash
# Container entrypoint: restore the database if this machine has none, then
# run the server *under* litestream so every write is replicated as it lands.
#
# The order matters. A fresh machine (new volume, restored volume, or a
# rebuilt one after a fly platform move) has an empty /data; opening the
# server there would create a blank database and then happily replicate the
# blankness over the good backup. Restoring first makes the disaster-recovery
# path the boot path, which is the only way it stays tested.
set -euo pipefail

CONFIG="${LITESTREAM_CONFIG:-/etc/litestream.yml}"

# The server builds this path itself: `LP_CLOUD_DATA_DIR` + "cloud.sqlite"
# (lp-cloud-server/src/app_state.rs). Keep the two in step — litestream
# replicating a file the server never writes is a silent, total backup
# failure.
DB="${LP_CLOUD_DB_PATH:-${LP_CLOUD_DATA_DIR:-/data}/cloud.sqlite}"

mkdir -p "$(dirname "$DB")"

# Both guards, deliberately:
#   -if-db-not-exists   this machine already has data — never overwrite it
#   -if-replica-exists  first boot ever — there is nothing to restore yet
# With both, this line is a no-op in the two normal cases and does the real
# work only in the one that matters (empty disk, populated bucket).
echo "[entrypoint] litestream restore -> ${DB}"
litestream restore -if-db-not-exists -if-replica-exists -config "$CONFIG" "$DB"

# `-exec` makes litestream the supervisor: it forwards SIGTERM to the server
# (which drains — see main.rs's shutdown_signal) and performs a final sync
# before exiting, so a deploy does not lose the last few seconds of writes.
echo "[entrypoint] litestream replicate -exec lp-cloud-server"
exec litestream replicate -config "$CONFIG" -exec "lp-cloud-server"
