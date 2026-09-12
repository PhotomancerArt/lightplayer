#!/usr/bin/env bash
# Print the absolute path of `jit-host.js` — the JS half of the emulator's JIT
# seam — as it exists in THIS tree.
#
# One resolver, because three callers need the same answer and none of them
# may guess: `scripts/sync-engine-sidecar.sh` (which content-hashes it into a
# served `pkg/`), `scripts/wasm-serial-test-runner.sh` (which stages it beside
# the module for the browser conformance suite) and
# `scripts/emu/tab-smoke.mjs` (which imports it in node).
#
# THE FILE IS M7's AND IS NEVER EDITED OR HAND-COPIED. It is also moving: M7's
# P9 relocates it from `scripts/emu/bench-web/` to `lp-emu/lp-emu-jit/js/` and
# leaves a one-line RE-EXPORT SHIM behind at the old path. A shim is exactly
# what must not be copied — its relative import points nowhere once the copy
# lands in `pkg/` — so this script prefers the new home, falls back to the old
# one, and REFUSES anything that looks like a shim rather than serving a file
# that would 404 its own dependency in a Worker.
#
# Usage: scripts/emu/jit-host-source.sh
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"

# In order of preference. The first that exists is the answer — a shim at the
# preferred path is a refusal, not a reason to fall through: two real files is
# the only shape where the fallback is the right file.
candidates=(
    "${repo}/lp-emu/lp-emu-jit/js/jit-host.js"
    "${repo}/scripts/emu/bench-web/jit-host.js"
)

chosen=""
for candidate in "${candidates[@]}"; do
    if [[ -f "${candidate}" ]]; then
        chosen="${candidate}"
        break
    fi
done

if [[ -z "${chosen}" ]]; then
    echo "jit-host-source: no jit-host.js in this tree; looked at:" >&2
    for candidate in "${candidates[@]}"; do echo "    ${candidate}" >&2; done
    exit 1
fi

# The two shim tells: a file too short to be the real thing (the real one is
# ~220 lines, half of them the header that explains the seam), and a
# re-export of `makeJitHost` from somewhere else.
lines="$(wc -l < "${chosen}" | tr -d '[:space:]')"
if (( lines < 40 )) || grep -qE '^[[:space:]]*export[[:space:]]+(\*|\{)[^;]*from' "${chosen}"; then
    echo "jit-host-source: ${chosen} looks like a re-export shim (${lines} lines)." >&2
    echo "jit-host-source: a shim cannot be copied into pkg/ — its relative import" >&2
    echo "jit-host-source: would point at nothing. M7 P9 moves the real file to" >&2
    echo "jit-host-source: lp-emu/lp-emu-jit/js/jit-host.js; point this at whichever" >&2
    echo "jit-host-source: path holds it." >&2
    exit 1
fi

echo "${chosen}"
