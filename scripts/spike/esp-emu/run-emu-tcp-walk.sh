#!/usr/bin/env bash
# esp-emu walk over TCP: emulator --uart-tcp <-- transcribing proxy <-- lp-cli serial:tcp://
#   SCRATCH=.. WORKTREE=.. run-emu-tcp-walk.sh <merged.bin> <label> <timeout-secs> [-- lp-cli args...]
# lp-cli args may contain the literal TCP, replaced by tcp://127.0.0.1:<proxy port>.
# Env: PORT (emulator, default 5555), PROXY (default PORT+10), RUST_LOG, SETTLE, LPCLI, OUT.
set -euo pipefail
S="${SCRATCH:?}"; W="${WORKTREE:?}"
EMU="$S/esp-emu/esp-emu-0.42.0-aarch64-apple-darwin/esp-emu"
PY=/opt/homebrew/bin/python3
LPCLI="${LPCLI:-$W/target/debug/lp-cli}"
OUT="${OUT:-$S/walks}"; mkdir -p "$OUT"
img="$1"; label="$2"; secs="$3"; shift 3
port="${PORT:-5555}"; proxy="${PROXY:-$((port + 10))}"
cli_args=(); if [[ "${1:-}" == "--" ]]; then shift; cli_args=("$@"); fi

RUST_LOG="${RUST_LOG:-info}" "$EMU" --chip esp32c6 --firmware "$img" \
    --uart-tcp "127.0.0.1:$port" --timeout "${secs}s" --log-color never \
    > "$OUT/$label.emu.stdout" 2> "$OUT/$label.emu.stderr" &
emu=$!
rm -f "$OUT/$label.uart.bin"
"$PY" "$S/uart-tcp-proxy.py" --target "127.0.0.1:$port" --listen "127.0.0.1:$proxy" \
    --log "$OUT/$label.uart.bin" > "$OUT/$label.proxy.log" 2>&1 &
prx=$!
cleanup() { kill -INT "$emu" 2>/dev/null || true; kill "$prx" 2>/dev/null || true; }
trap cleanup EXIT
for _ in $(seq 1 100); do grep -q CONNECTED "$OUT/$label.proxy.log" 2>/dev/null && break; sleep 0.1; done
echo "emu_pid=$emu port=$port proxy=$proxy"

if [[ ${#cli_args[@]} -gt 0 ]]; then
    sleep "${SETTLE:-6}"
    args=("${cli_args[@]/TCP/tcp://127.0.0.1:$proxy}")
    echo "==> lp-cli ${args[*]}"
    start=$(date +%s)
    set +e
    ( cd "$W" && "$LPCLI" "${args[@]}" ) > "$OUT/$label.cli.log" 2>&1
    echo "lp-cli exit=$? after $(( $(date +%s) - start )) s"
    set -e
fi
( sleep $(( secs + 20 )); kill -INT "$emu" 2>/dev/null; sleep 10; kill -KILL "$emu" 2>/dev/null ) & dog=$!
wait "$emu" || true
kill "$dog" 2>/dev/null || true
echo "emulator exited"
kill "$prx" 2>/dev/null || true; wait "$prx" 2>/dev/null || true
