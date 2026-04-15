#!/usr/bin/env bash
# Historical: one-time migration from file-level / filter-based annotations to
#   // @unimplemented(wasm.q32)  and  // @unsupported(rv32c.q32)
# The migration was run via a temporary `migrate-annotations` binary (removed after use).
# This script remains as documentation; re-running is unnecessary once filetests are migrated.

set -euo pipefail
echo "migrate-file-annotations: no-op (annotations already use explicit targets)." >&2
exit 0
