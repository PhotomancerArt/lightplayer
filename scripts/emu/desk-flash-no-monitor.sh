#!/usr/bin/env bash
# desk-flash-no-monitor.sh -- <espflash args...>
#
# Flash the board in the foreground and then LET GO OF THE PORT. No
# `--monitor`, no reader, nothing attached: espflash exits the moment the
# write is done and the port goes back to closed.
#
# This is step 1 of the flash-then-open capture (`Capture::FlashThenOpenAfter`
# in `lp-emu-validate`'s payload registry, M6 P1b). Steps 2 and 3 — the wait,
# and `scripts/emu/tty-capture.py` opening a non-resetting reader — are
# deliberately NOT here: the runner emits them as their own plan steps so that
# `validate run … --dry-run` shows the whole protocol before a board is
# plugged in, which is the only reason a desk step is reviewable at all.
#
# Port discipline is `scripts/spike/esp-emu/desk-espflash-step.sh`'s, and for
# the same reasons it learned them:
#
#   * pre-check `lsof`/`pgrep` and refuse if either is dirty — one holder at a
#     time, and Chromium can take minutes to notice it lost a device;
#   * foreground, not `&` — a backgrounded espflash has died silently
#     mid-write;
#   * under script(1) with a SIG_DFL exec shim, so a Ctrl-C reaches espflash
#     rather than being ignored (a `&` child of a non-interactive bash
#     inherits SIGINT = SIG_IGN, and only TERM/KILL would free it, which
#     wedges a native-USB port);
#   * never signal by pattern — with two lanes on the desk, `pkill -f
#     espflash` takes out the wrong one.
#
# The post-check is not decoration here, it is the measurement's precondition:
# the wait that follows only means anything if nothing is holding the port
# during it.
#
# Env: PORT_DEV (default /dev/cu.usbmodem1433201), LOG (default a temp file).
set -u
[[ "${1:-}" == "--" ]] && shift
PORT_DEV="${PORT_DEV:-/dev/cu.usbmodem1433201}"
PY="${PY:-/opt/homebrew/bin/python3}"
LOG="${LOG:-$(mktemp -t desk-flash-no-monitor)}"

if lsof -n 2>/dev/null | grep -qE '/dev/(cu|tty)\.usbmodem'; then
    echo "PRECHECK: a usbmodem port is held"
    lsof -n | grep -E '/dev/(cu|tty)\.usbmodem'
    exit 9
fi
if pgrep -fl "^espflash" >/dev/null; then
    echo "PRECHECK: espflash running"
    pgrep -fl "^espflash"
    exit 9
fi

echo "FLASH (no monitor) -> $PORT_DEV, log $LOG"
script -q "$LOG" "$PY" -c 'import signal,os,sys; signal.signal(signal.SIGINT, signal.SIG_DFL); os.execvp(sys.argv[1], sys.argv[1:])' \
    espflash "$@"
rc=$?
tail -5 "$LOG" 2>/dev/null

# script(1) reports its child's status, but a wedged bootloader shows up as a
# clean exit with a `TG0_WDT_HPSYS` banner in the log, so say so loudly:
# `g3-desk-batch.md` sitting-1's rule is STOP and report "needs a replug", and
# retrying flashes is what makes it worse.
if grep -qa 'TG0_WDT_HPSYS' "$LOG" 2>/dev/null; then
    echo "WEDGED: the log carries rst:0x7 (TG0_WDT_HPSYS) — the board needs a replug."
    echo "        Do not retry the flash. See docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md"
    rc=8
fi

sleep 1
echo "POSTCHECK lsof:"
lsof -n 2>/dev/null | grep -E '/dev/(cu|tty)\.usbmodem' || echo "  (port free)"
echo "POSTCHECK pgrep:"
pgrep -fl "^espflash" || echo "  (no espflash)"
exit "$rc"
