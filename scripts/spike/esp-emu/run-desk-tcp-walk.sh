#!/usr/bin/env bash
# Desk twin of run-emu-tcp-walk.sh: the same lp-cli command, the same transcribing
# proxy, the same <label>.uart.bin format — but the bytes come from a board on a
# USB-Serial-JTAG port through usb-tcp-bridge.py instead of esp-emu --uart-tcp.
#
#   SCRATCH=.. WORKTREE=.. run-desk-tcp-walk.sh <label> <hold-secs> [--flash <merged.bin>] -- lp-cli args...
#
# lp-cli args may contain the literal TCP, replaced by tcp://127.0.0.1:<proxy port>.
# --flash writes <merged.bin> at 0x0 first (espflash write-bin: the whole 4 MiB,
# so lpfs/nvs/phy_init come back BLANK — the emulator's starting state).
# <hold-secs> keeps the bridge reading after lp-cli exits so heartbeats land in
# the transcript. Env: PORT_DEV, BRIDGE (5581), PROXY (5591), SETTLE (6), OUT, LPCLI.
set -u
S="${SCRATCH:?}"; W="${WORKTREE:?}"
HERE="$(cd "$(dirname "$0")" && pwd)"
PORT_DEV="${PORT_DEV:-/dev/cu.usbmodem1433201}"; BRIDGE="${BRIDGE:-5581}"; PROXY="${PROXY:-5591}"
PY="${PY:-/opt/homebrew/bin/python3}"; LPCLI="${LPCLI:-$W/target/debug/lp-cli}"
OUT="${OUT:-$S/desk}"; mkdir -p "$OUT"
label="$1"; hold="$2"; shift 2
img=""; if [[ "${1:-}" == "--flash" ]]; then img="$2"; shift 2; fi
[[ "${1:-}" == "--" ]] && shift
held() { lsof -n 2>/dev/null | grep -E '/dev/(cu|tty)\.usbmodem'; }
if held; then echo "PRECHECK: a usbmodem port is held"; exit 9; fi
if pgrep -fl "^espflash"; then echo "PRECHECK: espflash running"; exit 9; fi
if [[ -n "$img" ]]; then
    PORT_DEV="$PORT_DEV" "$HERE/desk-espflash-step.sh" "$OUT/$label.flash.cap" 'NEVER' 240 \
        -- write-bin --chip esp32c6 --port "$PORT_DEV" --after hard-reset 0x0 "$img" | grep -E 'exited|SENTINEL|WARN|port'
    grep -q 'successfully written' "$OUT/$label.flash.cap" || { echo "FLASH DID NOT COMPLETE"; exit 8; }
    sleep 2
fi
rm -f "$OUT/$label.uart.bin" "$OUT/$label.uart.bin.times"
"$PY" "$HERE/usb-tcp-bridge.py" --dev "$PORT_DEV" --listen "127.0.0.1:$BRIDGE" > "$OUT/$label.bridge.log" 2>&1 &
bpid=$!
for _ in $(seq 1 50); do grep -q LISTEN "$OUT/$label.bridge.log" 2>/dev/null && break; sleep 0.1; done
t0=$(date +%s)
"$PY" "$HERE/uart-tcp-proxy.py" --target "127.0.0.1:$BRIDGE" --listen "127.0.0.1:$PROXY" --log "$OUT/$label.uart.bin" > "$OUT/$label.proxy.log" 2>&1 &
ppid=$!
for _ in $(seq 1 100); do grep -q CONNECTED "$OUT/$label.proxy.log" 2>/dev/null && break; sleep 0.1; done
echo "bridge=$bpid proxy=$ppid ($(tr '\n' ' ' < "$OUT/$label.bridge.log"))"
sleep "${SETTLE:-6}"
args=("${@/TCP/tcp://127.0.0.1:$PROXY}")
echo "==> lp-cli ${args[*]}"
start=$(date +%s)
( cd "$W" && "$LPCLI" "${args[@]}" ) > "$OUT/$label.cli.log" 2>&1
echo "lp-cli exit=$? after $(( $(date +%s) - start )) s"
sleep "$hold"
# TERM, not INT: these are `&` children of a non-interactive shell (SIGINT ignored).
kill -TERM "$ppid" 2>/dev/null; sleep 1; kill -TERM "$bpid" 2>/dev/null
for _ in $(seq 1 10); do kill -0 "$bpid" 2>/dev/null || break; sleep 1; done
wait "$ppid" 2>/dev/null; wait "$bpid" 2>/dev/null
echo "walk wall=$(( $(date +%s) - t0 )) s; bridge: $(tr '\n' ' ' < "$OUT/$label.bridge.log")"
sleep 1; echo "POSTCHECK:"; held || echo "  port free"; pgrep -fl 'usb-tcp-bridge|uart-tcp-proxy|^espflash' || echo "  no helpers"
