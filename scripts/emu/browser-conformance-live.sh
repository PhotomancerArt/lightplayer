#!/usr/bin/env bash
# The live half of M2's conformance suite: the SAME assertions, against a real
# `lp-cli emu serve` holding a real emulated C6, in a real Chrome.
#
# Deliberately not a CI job (emulator plan two PD9, E-cost pre-ruled): Chrome +
# chromedriver + the emulator + a firmware build is far over the ~5-minute
# line. CI runs the scripted half (`just lpa-link-browser-test`); an agent runs
# this one by hand.
#
# The tests that need to reach INSIDE the door — the scripted control log, the
# door dropping a byte channel, a reboot arranged behind the shim's back — sit
# out here and say so; what runs is everything that is a claim about the JS
# layer itself: enumeration, session-id stability, close-vs-release, the flash
# bridge's acquisition, `getInfo()`, openness, `requestPort`, hotplug.
#
# The server is stopped BY PID. Never `pkill -f`: another worktree's `lp-cli`
# is not ours to kill.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
repo="$PWD"

image="${1:-}"
if [ -z "$image" ]; then
    image="${LP_EMU_C6_ELF:-}"
fi
if [ -z "$image" ]; then
    image="$repo/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6"
fi

if [ ! -f "$image" ]; then
    cat >&2 <<EOF
browser-conformance-live: no emulator image at
  $image

Pass one as an argument, set LP_EMU_C6_ELF, or build one first — the same
image `just test-emu-serve` uses:

  LP_EMU_BUILD_FW=1 cargo test -p lp-cli --test emu_serve_door -- --include-ignored

This is a SKIP, not a pass: the live half did not run.
EOF
    exit 3
fi

if ! command -v wasm-bindgen-test-runner >/dev/null 2>&1; then
    echo "browser-conformance-live: wasm-bindgen-test-runner not found (cargo install wasm-bindgen-cli --version 0.2.114)" >&2
    exit 3
fi

state="$(mktemp -d "${TMPDIR:-/tmp}/emu-serve-live.XXXXXX")"
log="$state/serve.log"

echo "browser-conformance-live: building lp-cli"
cargo build -p lp-cli --quiet

"$repo/target/debug/lp-cli" emu serve \
    --board "c6-a=$image" \
    --board "c6-b=$image" \
    --listen 127.0.0.1:0 \
    --state-dir "$state" \
    --console-dir "$state" >"$log" 2>&1 &
serve_pid=$!

cleanup() {
    if kill -0 "$serve_pid" 2>/dev/null; then
        kill "$serve_pid" 2>/dev/null || true
        wait "$serve_pid" 2>/dev/null || true
    fi
    echo "--- emu serve log ---"
    cat "$log" || true
}
trap cleanup EXIT

addr=""
for _ in $(seq 1 100); do
    addr="$(sed -n 's/.*emu serve: listening on http:\/\/\(.*\)$/\1/p' "$log" | head -1 || true)"
    [ -n "$addr" ] && break
    if ! kill -0 "$serve_pid" 2>/dev/null; then
        echo "browser-conformance-live: emu serve exited before it listened" >&2
        exit 1
    fi
    sleep 0.2
done

if [ -z "$addr" ]; then
    echo "browser-conformance-live: emu serve never printed a listen address" >&2
    exit 1
fi

echo "browser-conformance-live: emu serve on http://$addr"
echo "browser-conformance-live: GET /boards says:"
curl -sS "http://$addr/boards" || true
echo

# `LP_EMU_SERVE_URL` is read with `option_env!`, so rustc records it as a
# build dependency and cargo rebuilds when it changes — but the port is
# ephemeral and a stale build would silently test the scripted door instead,
# so the test file is touched to make the rebuild unconditional.
touch "$repo/lp-app/lpa-link/tests/browser_serial_conformance.rs"

LP_EMU_SERVE_URL="http://$addr/" \
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER="$repo/scripts/wasm-serial-test-runner.sh" \
    cargo test -p lpa-link --target wasm32-unknown-unknown \
        --features browser-serial-esp32 --test browser_serial_conformance -- --nocapture
status=$?

# Put the build back on the scripted door so the next `just
# lpa-link-browser-test` is not a stale live build.
touch "$repo/lp-app/lpa-link/tests/browser_serial_conformance.rs"

exit "$status"
