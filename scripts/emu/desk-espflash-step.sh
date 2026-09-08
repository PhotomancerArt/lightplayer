#!/usr/bin/env bash
# desk-espflash-step.sh <capture> <sentinel> <max-secs> -- <espflash args...>
#
# One desk step: run espflash in the foreground of this shell under script(1)
# (it needs a pty for --monitor), poll the capture for <sentinel> (bounded by
# <max-secs>), then SIGINT ONLY that espflash pid and confirm the port is free.
#
# The python shim matters: a `&` child of a non-interactive bash inherits
# SIGINT = SIG_IGN, and script(1) passes that on, so a bare
# `script -q log espflash … &` produces an espflash that ignores SIGINT and can
# only be freed by TERM/KILL (which wedges a native-USB port). Resetting the
# disposition to SIG_DFL before exec is what `just` does for the hardware walks.
#
# Env: PORT_DEV (default /dev/cu.usbmodem1433201). Never signals by pattern.
set -u
cap="$1"; sentinel="$2"; max="$3"; shift 3
[[ "${1:-}" == "--" ]] && shift
PORT_DEV="${PORT_DEV:-/dev/cu.usbmodem1433201}"
PY="${PY:-/opt/homebrew/bin/python3}"
if lsof -n 2>/dev/null | grep -qE '/dev/(cu|tty)\.usbmodem'; then echo "PRECHECK: a usbmodem port is held"; lsof -n | grep -E '/dev/(cu|tty)\.usbmodem'; exit 9; fi
if pgrep -fl "^espflash" >/dev/null; then echo "PRECHECK: espflash running"; pgrep -fl "^espflash"; exit 9; fi
rm -f "$cap"
script -q "$cap" "$PY" -c 'import signal,os,sys; signal.signal(signal.SIGINT, signal.SIG_DFL); os.execvp(sys.argv[1], sys.argv[1:])' \
    espflash "$@" >/dev/null 2>&1 &
spid=$!
epid=""
for i in $(seq 1 "$max"); do
    if [[ -z "$epid" ]]; then epid="$(pgrep -f "^espflash .*${PORT_DEV}" | head -1)"; fi
    if grep -qa -- "$sentinel" "$cap" 2>/dev/null; then echo "SENTINEL after ${i}s (espflash pid=$epid)"; break; fi
    if ! kill -0 "$spid" 2>/dev/null; then echo "espflash exited on its own after ${i}s"; break; fi
    sleep 1
done
if [[ -n "$epid" ]] && kill -0 "$epid" 2>/dev/null; then
    echo "SIGINT -> $epid"; kill -INT "$epid"
    for _ in $(seq 1 15); do kill -0 "$epid" 2>/dev/null || break; sleep 1; done
    kill -0 "$epid" 2>/dev/null && echo "WARNING: espflash $epid still alive after SIGINT"
fi
wait "$spid" 2>/dev/null; echo "script exit=$?"
sleep 1
echo "POSTCHECK lsof:"; lsof -n 2>/dev/null | grep -E '/dev/(cu|tty)\.usbmodem' || echo "  (port free)"
echo "POSTCHECK pgrep:"; pgrep -fl "^espflash" || echo "  (no espflash)"
