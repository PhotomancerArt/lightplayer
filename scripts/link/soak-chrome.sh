#!/usr/bin/env bash
# One link soak read through Chromium's Web Serial (headless Brave), then
# checked by `lp-cli link soak-verify`.
#
#   scripts/link/soak-chrome.sh <out-dir> <name> '<query>'
#
# e.g. scripts/link/soak-chrome.sh captures/board brave-255 'bufferSize=255&seconds=30'
#
# Serves spikes/serial-lab (its /soak page) on a bench-block port, which the
# serial-grant policy covers (`just serial-grant`; Brave only), opens the page
# in headless Brave with a scratch profile, waits for the page to post its
# capture, stops the browser and the server it started (by pid), and runs the
# verifier. The board must be running a soak_link image and be the only
# Espressif port attached; nothing else may hold the port.
#
# Investigation tooling (plan lp2025/2026-09-26-1720-reliable-device-link, M1b).
set -euo pipefail

out="$1"; name="$2"; query="$3"
root="$(cd "$(dirname "$0")/../.." && pwd)"
browser="${SOAK_BROWSER:-/Applications/Brave Browser.app/Contents/MacOS/Brave Browser}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

port="$("$root/scripts/dev-port.sh" --bench soak-lab)"
SERIAL_LAB_PORT="$port" SOAK_OUT="$out" python3 "$root/spikes/serial-lab/server.py" \
    >"$out/$name.server.log" 2>&1 &
server_pid=$!
profile="$(mktemp -d "${TMPDIR:-/tmp}/soak-brave-XXXXXX")"
browser_pid=""
cleanup() {
    [[ -n "$browser_pid" ]] && kill "$browser_pid" 2>/dev/null || true
    kill "$server_pid" 2>/dev/null || true
    sleep 0.5
    rm -rf "$profile"
}
trap cleanup EXIT
sleep 0.7

rm -f "$out/$name.bin" "$out/$name.meta.json" "$out/$name.failed.txt"
"$browser" --headless=new --disable-gpu --no-first-run --no-default-browser-check \
    --disable-backgrounding-occluded-windows --disable-renderer-backgrounding \
    --disable-background-timer-throttling --user-data-dir="$profile" \
    "http://127.0.0.1:$port/soak?name=$name&$query" >"$out/$name.browser.log" 2>&1 &
browser_pid=$!

for _ in $(seq 1 1200); do
    if [[ -f "$out/$name.meta.json" || -f "$out/$name.failed.txt" ]]; then break; fi
    sleep 0.25
done
if [[ -f "$out/$name.failed.txt" ]]; then
    cat "$out/$name.failed.txt" >&2
    exit 1
fi
[[ -f "$out/$name.meta.json" ]] || { echo "soak-chrome: no result" >&2; exit 1; }
python3 - "$out/$name.meta.json" <<'EOF'
import json, sys
m = json.load(open(sys.argv[1]))
print(f"reads {m['reads']}, {m['total']} B, errors {m['errors']}, stalls {m['stalls']}")
sizes = m["chunkSizes"]
top = sorted(sizes.items(), key=lambda kv: -kv[1])[:6]
print("read sizes (bytes: count):", ", ".join(f"{k}: {v}" for k, v in top), "max", max(map(int, sizes)) if sizes else 0)
EOF
"$root/target/release/lp-cli" link soak-verify "$out/$name.bin" --out "$out/$name.verify" \
    --label "chromium web serial ($name)"
