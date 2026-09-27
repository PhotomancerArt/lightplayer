#!/usr/bin/env bash
# One comms-lab run through Chromium's Web Serial (headless Brave): lp-link
# in wasm (spikes/link-lab-wasm) against a test_comms_lab board.
#
#   scripts/link/lab-chrome.sh <out-dir> <name> '<query>'
#
# e.g. scripts/link/lab-chrome.sh captures/m3 chrome-30min 'echo=900&stream=900&logs=1000'
#
# soak-chrome.sh's shape (M1b): serves spikes/serial-lab (its /lab page) on a
# bench-block port, which the serial-grant policy covers (`just serial-grant`;
# Brave only), opens the page in headless Brave with a scratch profile, waits
# for the page to post its report, and stops the browser and the server it
# started (by pid). The board must run a test_comms_lab image and be the only
# Espressif port attached; nothing else may hold the port.
#
# Plan lp2025/2026-09-26-1720-reliable-device-link, M3.
set -euo pipefail

out="$1"; name="$2"; query="$3"
root="$(cd "$(dirname "$0")/../.." && pwd)"
browser="${SOAK_BROWSER:-/Applications/Brave Browser.app/Contents/MacOS/Brave Browser}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
[[ -f "$root/spikes/link-lab-wasm/pkg/link_lab_wasm.js" ]] || "$root/spikes/link-lab-wasm/build.sh" >/dev/null

port="$("$root/scripts/dev-port.sh" --bench soak-lab)"
SERIAL_LAB_PORT="$port" SOAK_OUT="$out" python3 "$root/spikes/serial-lab/server.py" \
    >"$out/$name.server.log" 2>&1 &
server_pid=$!
profile="$(mktemp -d "${TMPDIR:-/tmp}/lab-brave-XXXXXX")"
browser_pid=""
cleanup() {
    [[ -n "$browser_pid" ]] && kill "$browser_pid" 2>/dev/null || true
    kill "$server_pid" 2>/dev/null || true
    sleep 0.5
    rm -rf "$profile"
}
trap cleanup EXIT
sleep 0.7

rm -f "$out/$name.json" "$out/$name.txt" "$out/$name.failed.txt"
"$browser" --headless=new --disable-gpu --no-first-run --no-default-browser-check \
    --disable-backgrounding-occluded-windows --disable-renderer-backgrounding \
    --disable-background-timer-throttling --user-data-dir="$profile" \
    "http://127.0.0.1:$port/lab?name=$name&$query" >"$out/$name.browser.log" 2>&1 &
browser_pid=$!

# Long runs: up to three hours.
for _ in $(seq 1 43200); do
    if [[ -f "$out/$name.json" || -f "$out/$name.failed.txt" ]]; then break; fi
    sleep 0.25
done
if [[ -f "$out/$name.failed.txt" ]]; then
    cat "$out/$name.failed.txt" >&2
    exit 1
fi
[[ -f "$out/$name.json" ]] || { echo "lab-chrome: no result" >&2; exit 1; }
sleep 0.5
grep -A20 "done:" "$out/$name.txt" || true
