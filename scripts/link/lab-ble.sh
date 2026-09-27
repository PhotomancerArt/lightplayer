#!/usr/bin/env bash
# One comms-lab run over BLE: lp-link in wasm (spikes/link-lab-wasm) in the
# Mac's own Chrome, as the Web Bluetooth central, against a
# test_comms_lab_ble board. No human: spikes/ble-lab/scripts/cdp-central.mjs
# answers the chooser over CDP.
#
#   scripts/link/lab-ble.sh <out-dir> <name> '<query>' [chooser-prefix]
#
# e.g. scripts/link/lab-ble.sh captures/m3 ble-10min 'echo=300&stream=300&max=4096'
#
# Chrome is launched in the BACKGROUND (`open -g`) with a scratch profile and
# a remote-debugging port, never as a foreground window, and quit by its
# profile path at the end. Chrome needs macOS's Bluetooth permission.
#
# Plan lp2025/2026-09-26-1720-reliable-device-link, M3.
set -euo pipefail

out="$1"; name="$2"; query="$3"; prefix="${4:-LP-LAB}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
[[ -f "$root/spikes/link-lab-wasm/pkg/link_lab_wasm.js" ]] || "$root/spikes/link-lab-wasm/build.sh" >/dev/null

port="$("$root/scripts/dev-port.sh" ble-lab)"
debug_port="${LAB_BLE_DEBUG_PORT:-9333}"
BLE_LAB_PORT="$port" LAB_OUT="$out" python3 "$root/spikes/ble-lab/server.py" \
    >"$out/$name.server.log" 2>&1 &
server_pid=$!
profile="$(mktemp -d "${TMPDIR:-/tmp}/lab-chrome-ble-XXXXXX")"
cleanup() {
    pkill -f "user-data-dir=$profile" 2>/dev/null || true
    kill "$server_pid" 2>/dev/null || true
    sleep 0.5
    rm -rf "$profile"
}
trap cleanup EXIT
sleep 0.7

rm -f "$out/$name.json" "$out/$name.txt" "$out/$name.failed.txt"
open -g -n -a "Google Chrome" --args --remote-debugging-port="$debug_port" \
    --user-data-dir="$profile" --no-first-run --no-default-browser-check \
    --disable-backgrounding-occluded-windows --disable-renderer-backgrounding \
    --disable-background-timer-throttling \
    "http://localhost:$port/link?name=$name&$query"
for _ in $(seq 1 60); do
    curl -s "http://127.0.0.1:$debug_port/json" >/dev/null 2>&1 && break
    sleep 0.5
done
sleep 2
node "$root/spikes/ble-lab/scripts/cdp-central.mjs" --debug-port "$debug_port" \
    --page "localhost:$port" join --prefix "$prefix" --timeout-ms 60000 \
    --click-expr 'document.getElementById("btn-join").click(); true' \
    | tee "$out/$name.join.json"

for _ in $(seq 1 43200); do
    if [[ -f "$out/$name.json" || -f "$out/$name.failed.txt" ]]; then break; fi
    sleep 0.25
done
if [[ -f "$out/$name.failed.txt" ]]; then
    cat "$out/$name.failed.txt" >&2
    exit 1
fi
[[ -f "$out/$name.json" ]] || { echo "lab-ble: no result" >&2; exit 1; }
sleep 0.5
grep -A20 "done" "$out/$name.txt" || true
