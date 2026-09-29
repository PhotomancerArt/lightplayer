#!/usr/bin/env bash
# Hardware walk — a shader compiled and executed on a device's on-chip JIT,
# verified against a host render rather than against an LED.
#
# Usage:
#   scripts/m4-hardware-walk.sh [port]                   # ESP32-S3   (M4 gate)
#   scripts/m4-hardware-walk.sh --chip esp32 [port]      # classic ESP32 (M7 gate)
#   scripts/m4-hardware-walk.sh --chip esp32c6 [port]    # ESP32-C6
#   scripts/m4-hardware-walk.sh --chip esp32c6 tcp://127.0.0.1:5591   # TEST HOOK (below)
#
# Named for the S3's M4 gate, which it was written for; the classic ESP32's
# M7 FINAL gate asks the identical question ("does the on-device JIT render
# bit-exactly against the host oracle?") of a chip whose JIT installs code by a
# completely different route — the classic heap has no I-bus view, so compiled
# code is walked into a fixed SRAM1 region through a word-mirrored D-bus
# aperture. The ESP32-C6 asks it a third time of a RISC-V core running the
# native rv32 code generator the host oracle's second engine also runs, which
# is why its `[ORACLE-RV32]` line stops being only a triage aid there and
# becomes the same code path. Three chips, one question, so one script:
# everything chip-specific lives in the `case` block below, and nothing
# downstream of it branches.
#
# The C6 has a fourth caller too: `scripts/emu/m4-walk.sh` runs this walk's
# every step against the `lp-emu-esp32c6` emulator instead of a board, on the
# same image bytes, and compares both its answers — the firmware's `[OUT] dump`
# line AND the WS281x waveform decoded off the emulated pad — against the same
# host oracle. That twin is the reason the C6 grew a `frame-dump` build at all
# (`lp-fw/fw-esp32c6/src/output/rmt/frame_dump.rs`). Keep the two scripts'
# comparison sections saying the same thing.
#
# ## The board's console is an lp-link, so the reader is a link host
#
# Since wire proto 30 (C6, S3) and 32 (the classic, #884) the board's serial
# port is an lp-link (`lp-base/lp-link`): its log lines — the `[OUT] dump`
# this walk reads among them — go into a log ring and leave the board as
# frames on the link's log channel, and ONLY while a host has brought the
# link up. `espflash flash --monitor`, which this walk used to read, is not a
# link host: it holds the port, never answers the board's SYN, and sees the
# boot text before the link task starts and then nothing. (Measured on the
# emulated C6 with the project already in flash and a raw reader on the port:
# 2,078 bytes of console ending at `starting server loop`, zero `[OUT] dump`
# lines.) So the walk reads the board through `lp-cli link capture`, which
# opens the port the way lp-cli's own transports do (no reset dance), hosts
# the link, and writes the DECODED console — raw text, log records, each wire
# message as its `M!{json}` line, `[link] …` notes — to a file.
#
# **One link host on the port at a time.** Each step below opens the port,
# does its one thing and closes it before the next one starts:
#
#   1. flash, with NO monitor (the recipe's `no-monitor` argument), so espflash
#      exits and lets go of the port the moment the write is done;
#   2. `lp-cli upload` — a link host for as long as the upload takes;
#   3. flash again, no monitor — the board reboots, auto-loads the project
#      from `lpfs` (the second flash rewrites the app partition, not `lpfs`),
#      compiles the shader on device and renders;
#   4. `lp-cli link capture` for `LP_WALK_WATCH_SECS` (default 30) — the host
#      that reads the boot, the compile and the frame dumps.
#
# The board's log ring keeps the newest lines while nobody hosts the link, so
# what the board said between the flash's reset and the capture's open is
# delivered when the capture brings the link up, not lost.
#
# Port discipline (from M3, the S3 walks and `scripts/emu/desk-flash-no-
# monitor.sh`): the flash runs in the FOREGROUND, under script(1); a port
# something else holds (`lsof`) is refused rather than taken; and the only
# signal this script ever sends espflash is SIGINT, to the espflash that holds
# OUR port — SIGTERM/SIGKILL on espflash wedges a native-USB port until a human
# replugs the board, and `pkill -f espflash` takes out another lane's flash.
#
# ⚠️ Classic ESP32 only: a bare `espflash monitor` stub-halts that board. This
# walk no longer attaches a monitor at all, and must not grow one. If you need
# to watch without reflashing, host the link:
#   lp-cli link capture /dev/cu.wchusbserialNNNN --console out.txt --seconds 30
# ⚠️ The classic arm needs a board on lp-link firmware (wire proto 32, #884).
# A pre-link classic image prints `M!` lines on a UART the capture cannot
# host; walk it with this script as it was before the lp-link port.
#
# Both flashes carry the `frame-dump` feature (see FLASH_FEATURES below): the
# RMT driver drives real LEDs, and an LED cannot be diffed against a host
# render, so the walk needs the build that also prints each transmitted frame.
# A default `just flash-fw-esp32s3` / `just flash-fw-esp32v3` produces no
# `[OUT]` lines at all and this walk would report "nothing rendered".
#
# The gate is the last section: the device's `[OUT] dump` hex must equal the
# oracle's `[ORACLE] rgb` hex, byte for byte. `projects/test/shader-oracle` is
# clock-free precisely so that comparison needs no time synchronisation.
#
# ## TEST HOOK: `tcp://host:port` in place of a serial port
#
# A port that starts `tcp://` is an EMULATED board's link socket, and the walk
# runs its host side against it: no port resolution, no chip probe, NO FLASH
# (the emulator already booted whatever image it was given — it must be a
# `frame-dump` build), the upload over `serial:tcp://…`, and in place of the
# second flash a `--request reboot` on the capture, which reboots the board
# into the project it persisted. The board must be one that reboots rather
# than ending on the reset and keeps its flash, e.g.:
#
#   lp-cli emu run --merged <frame-dump merged.bin> --link 127.0.0.1:5591 \
#       --reboot-on-reset --timeout 120s --wall-timeout 900
#
# (No `--strict-bus`: the emulated C6 does not perform a software reset, so
# the reboot arrives by the LP watchdog ~8 s later, after a fall-through that
# writes to address 0 — docs/defects/2026-09-29-the-emulated-c6-does-not-
# perform-a-software-reset.md.)
#
# It exists so the host half of this walk — the upload, the capture, the
# parse and the comparison — can be proved with no board. It proves nothing
# about espflash, the port's re-enumeration after a reset, or silicon.
set -euo pipefail

cd "$(dirname "$0")/.."
LOG_DIR="${TMPDIR:-/tmp}"
PROJECT="${PROJECT:-projects/test/shader-oracle}"
# The `[OUT]` transcript this walk parses exists only under this feature; see
# lp-fw/fw-esp32s3/src/output/rmt/frame_dump.rs and its byte-for-byte port at
# lp-fw/fw-esp32v3/src/output/rmt/frame_dump.rs — the two print identical line
# shapes so that everything below this point is chip-agnostic.
FLASH_FEATURES="${FLASH_FEATURES:-frame-dump}"
# How long the capture hosts the link after the second flash. The deferred
# lit dump comes 30 frames after the first lit frame, which follows the boot,
# the project load and the on-device compile; 30 s is several times that on
# every chip the walk has run on.
WATCH_SECS="${LP_WALK_WATCH_SECS:-30}"
# Seconds to let a board's port settle after espflash's hard reset before
# waiting for its device node: a native-USB board drops off the bus and comes
# back under the same name.
SETTLE_SECS="${LP_WALK_SETTLE_SECS:-2}"

usage() {
    sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
}

# ------------------------------------------------------------ arguments
chip="${LP_CHIP:-esp32s3}"
port=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --chip) chip="${2:?--chip needs a value}"; shift 2 ;;
        --chip=*) chip="${1#*=}"; shift ;;
        -h|--help) usage; exit 0 ;;
        --) shift ;;
        -*) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
        *) port="$1"; shift ;;
    esac
done

# The test hook (header): an emulated board's socket instead of a board.
emulated=0
[[ "$port" == tcp://* ]] && emulated=1

# ------------------------------------------------------------ chip table
# The whole of this script's chip knowledge. Four facts each:
#
#   FLASH_RECIPE    the just recipe that builds+flashes this chip (called with
#                   `no-monitor`). It owns the partition table and the flash
#                   size; this script must not duplicate either.
#   ENDPOINT_LABEL  the board label the oracle project's output node must name.
#                   `projects/test/shader-oracle` is authored for the XIAO S3's
#                   `D10`; the DOM-Z-102 has no such pad and names its four
#                   data channels IO18/IO16/IO14/IO2. An output node whose
#                   endpoint the board does not have never opens — the device
#                   then renders nothing and the walk reports a mismatch that
#                   is really a mis-addressed pin. `prepare_project` rewrites
#                   the label into a scratch copy rather than forking the
#                   project, so both chips render provably identical pixels
#                   (the endpoint chooses a wire, never a colour).
#   VERIFY_CHIP     whether to confirm chip identity with `espflash board-info`
#                   before flashing. On by default only for the classic: its
#                   CH340K bridge enumerates as `/dev/cu.wchusbserial<N>` with
#                   an N that floats per hub position and a name shared with
#                   every other CH340 board on the desk, and `fwcheck port`
#                   deliberately does NOT probe when only one candidate exists.
#                   Left off for the S3 so this change cannot perturb the walk
#                   that is already passing there.
#   PORT_HINT       what to tell a human who has no board on the bus.
case "$chip" in
    esp32s3)
        FLASH_RECIPE="flash-fw-esp32s3"
        ENDPOINT_LABEL="${ENDPOINT_LABEL:-D10}"
        VERIFY_CHIP="${VERIFY_CHIP:-0}"
        PORT_HINT="/dev/cu.usbmodem*"
        ;;
    esp32|esp32v3)
        chip="esp32"
        FLASH_RECIPE="flash-fw-esp32v3"
        ENDPOINT_LABEL="${ENDPOINT_LABEL:-IO18}"
        VERIFY_CHIP="${VERIFY_CHIP:-1}"
        PORT_HINT="/dev/cu.wchusbserial*"
        ;;
    esp32c6)
        FLASH_RECIPE="flash-fw-esp32c6"
        # The XIAO ESP32-C6 carries the same `D10` pad the oracle project is
        # authored for (it is gpio18 there — see the driver's open line in
        # `lp-emu/esp/lp-emu-esp32c6/tests/shader_oracle_pin.rs`), so unlike
        # the classic this chip needs no retarget and `prepare_project` below
        # uploads the project unmodified.
        ENDPOINT_LABEL="${ENDPOINT_LABEL:-D10}"
        # Off for the same reason it is off for the S3: `board-info` on a
        # USB-Serial-JTAG port resets the board, and the C6's real ambiguity
        # is not "which chip" but "which of two C6s" — which `board-info`
        # cannot answer and `board-port.py` can, without opening anything.
        VERIFY_CHIP="${VERIFY_CHIP:-0}"
        PORT_HINT="/dev/cu.usbmodem* (resolve by MAC: scripts/emu/board-port.py --list)"
        ;;
    *)
        echo "unsupported --chip '$chip' (known: esp32s3, esp32, esp32c6)" >&2
        exit 2
        ;;
esac

if [[ $emulated == 1 ]]; then
    echo "==> chip=$chip, EMULATED board at $port (test hook: no flash)" \
        "endpoint=ws281x:local:$ENDPOINT_LABEL"
else
    echo "==> chip=$chip recipe=just $FLASH_RECIPE endpoint=ws281x:local:$ENDPOINT_LABEL"
fi

# ------------------------------------------------------------ port
#
# `LP_BOARD_MAC` short-circuits the probe entirely. Two ESP32-C6s (or two S3s)
# on one bus are indistinguishable by port name — both enumerate as
# `303a:1001` and macOS names both `/dev/cu.usbmodem14332xx` — and a probe that
# picks "the first one" flashes a board another session is using.
# `board-port.py` walks IOKit for the USB serial number, which IS the MAC: it
# opens nothing, resets nothing, and cannot pick the wrong board.
#
#   LP_BOARD_MAC=A0:F2:62:87:B4:8C scripts/m4-hardware-walk.sh --chip esp32c6
if [[ -z "$port" && -n "${LP_BOARD_MAC:-}" ]]; then
    echo "==> resolving $LP_BOARD_MAC (passively, by USB serial number)"
    port="$(scripts/emu/board-port.py "$LP_BOARD_MAC")" || {
        echo "FAIL: no board with MAC $LP_BOARD_MAC on the bus." >&2
        scripts/emu/board-port.py --list >&2 || true
        exit 1
    }
    echo "    found: $port"
fi
if [[ -z "$port" ]]; then
    echo "==> identifying the $chip"
    # Probes with per-port timeouts (bare `espflash board-info` can hang on a
    # wedged port); busy ports are skipped, not reset under their owner.
    port="$(cargo run -q -p lp-cli -- fwcheck port --chip "$chip")"
    echo "    found: $port"
fi
if [[ -z "$port" ]]; then
    echo "No $chip found (expected something like $PORT_HINT)." >&2
    echo "Is it plugged in? Is another session holding it?" >&2
    exit 1
fi

strip_ansi_stream() { sed 's/\x1b\[[0-9;]*m//g'; }
strip_ansi() { strip_ansi_stream < "$1"; }

if [[ "$VERIFY_CHIP" != 0 && $emulated == 0 ]]; then
    echo "==> confirming $port really is a $chip"
    probed="$(espflash board-info --port "$port" 2>&1 \
        | strip_ansi_stream \
        | sed -n 's/.*Chip type:[[:space:]]*\([A-Za-z0-9-]*\).*/\1/p' \
        | head -1)"
    # Normalised the same way `lp-cli fwcheck` does, so esp32-s3 / ESP32S3 /
    # esp32s3 all compare equal — and, critically, so "esp32" does NOT match
    # "esp32s3" the way a substring grep would.
    norm() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -cd '[:alnum:]'; }
    if [[ -z "$probed" ]]; then
        echo "FAIL: espflash board-info said nothing about $port." >&2
        echo "      A classic ESP32 that will not identify is usually a wedged" >&2
        echo "      port (replug) or a missing WCH CH34x driver." >&2
        exit 1
    fi
    if [[ "$(norm "$probed")" != "$(norm "$chip")" ]]; then
        echo "FAIL: $port is a '$probed', not a '$chip'. Refusing to flash it." >&2
        exit 1
    fi
    echo "    confirmed: $probed"
fi

# ------------------------------------------------------------ lp-cli
#
# Built once, up front, so no compile sits between a flash's reset and the
# upload or capture that follows it. `LP_CLI` names a binary to use instead.
if [[ -z "${LP_CLI:-}" ]]; then
    echo "==> building lp-cli"
    cargo build -q -p lp-cli
fi
lp_cli() {
    if [[ -n "${LP_CLI:-}" ]]; then
        "$LP_CLI" "$@"
    else
        cargo run -q -p lp-cli -- "$@"
    fi
}

# ------------------------------------------------------------ flashing
#
# Interrupt only the espflash that holds OUR port.
#
# The pattern carries `$port` on purpose. `pkill -f "espflash flash"` kills
# every espflash on the machine, which on a desk running two boards means one
# walk SIGINTs the other lane's flash mid-write — and the recipe passes
# `--port` explicitly, so the port name is always on the command line to
# match against. SIGINT, never TERM or KILL: a killed espflash wedges the port
# until someone physically replugs the board (M3's lesson, in the header).
# The flashes below run in the foreground and exit by themselves; this is the
# net for a walk that is itself interrupted mid-flash.
release_port() {
    [[ $emulated == 1 ]] && return 0
    pkill -INT -f "espflash flash.*$port" 2>/dev/null || true
    for _ in $(seq 1 15); do
        pgrep -f "espflash flash.*$port" >/dev/null 2>&1 || return 0
        sleep 1
    done
    echo "WARNING: espflash still holding $port." >&2
}
trap release_port EXIT

# One holder at a time: refuse a port another process has open (a browser's
# Web Serial, another session's capture) rather than flash or read under it.
refuse_if_held() {
    local holders
    holders="$(lsof -n "$port" 2>/dev/null | tail -n +2 || true)"
    if [[ -n "$holders" ]]; then
        echo "FAIL: $port is held by another process; refusing to take it:" >&2
        echo "$holders" >&2
        exit 1
    fi
}

# Build and flash with no monitor, in the foreground, into the transcript
# `$1`; espflash exits when the write is done and the board is reset.
flash_no_monitor() {
    local log="$1"
    refuse_if_held
    if ! script -q "$log" just "$FLASH_RECIPE" "$port" "$FLASH_FEATURES" no-monitor \
        >/dev/null 2>&1; then
        echo "FAIL: the flash did not complete. Tail of $log:" >&2
        strip_ansi "$log" | tail -30 >&2
        exit 1
    fi
    # A wedged C6 bootloader shows up as a clean exit with a TG0_WDT_HPSYS
    # banner (docs/defects/2026-09-06-c6-analog-master-wedges-the-
    # bootloader.md): stop, and do not retry — retrying makes it worse.
    if grep -qa 'TG0_WDT_HPSYS' "$log" 2>/dev/null; then
        echo "WEDGED: the flash log carries rst:0x7 (TG0_WDT_HPSYS) — replug the board." >&2
        exit 1
    fi
}

# After espflash's hard reset: let the port drop and come back, then wait for
# its device node.
wait_for_port() {
    sleep "$SETTLE_SECS"
    for _ in $(seq 1 30); do
        [[ -e "$port" ]] && return 0
        sleep 1
    done
    echo "FAIL: $port did not come back after the flash's reset." >&2
    exit 1
}

# ------------------------------------------- the project this chip can open
#
# Sets UPLOAD_DIR: `$PROJECT` itself when its output node already names this
# board's label, otherwise a scratch copy with the label rewritten. The copy
# keeps the directory basename because that is the project's on-device name.
prepare_project() {
    # The endpoint lives inside the output's `channels` map (project format 3),
    # one line per channel; this walk drives a single-channel output, so take
    # the first.
    local authored
    authored="$(sed -n 's/.*"endpoint"[[:space:]]*:[[:space:]]*"ws281x:local:\([^"]*\)".*/\1/p' \
        "$PROJECT/output.json" | head -1)"
    if [[ -z "$authored" ]]; then
        echo "FAIL: no ws281x:local channel endpoint found in $PROJECT/output.json." >&2
        exit 1
    fi
    if [[ "$authored" == "$ENDPOINT_LABEL" ]]; then
        UPLOAD_DIR="$PROJECT"
        return
    fi
    UPLOAD_DIR="$LOG_DIR/m4-walk-project-$$/$(basename "$PROJECT")"
    mkdir -p "$UPLOAD_DIR"
    cp "$PROJECT"/* "$UPLOAD_DIR"/
    sed -i.bak "s|\"ws281x:local:$authored\"|\"ws281x:local:$ENDPOINT_LABEL\"|" \
        "$UPLOAD_DIR/output.json"
    rm -f "$UPLOAD_DIR/output.json.bak"
    # A silent no-op substitution would upload an endpoint this board does not
    # have, and the walk would then blame the JIT for a wiring mistake.
    if ! grep -q "\"ws281x:local:$ENDPOINT_LABEL\"" "$UPLOAD_DIR/output.json"; then
        echo "FAIL: could not rewrite the output endpoint to $ENDPOINT_LABEL." >&2
        exit 1
    fi
    echo "==> retargeted $authored -> $ENDPOINT_LABEL in $UPLOAD_DIR"
}

prepare_project

# ------------------------------------------------------- round 1: push
if [[ $emulated == 0 ]]; then
    echo "==> flashing $port (features: $FLASH_FEATURES, no monitor)"
    flash_no_monitor "$LOG_DIR/m4-walk-flash1-$$.log"
    wait_for_port
fi

echo
echo "===== UPLOAD ====="
if ! lp_cli upload "$UPLOAD_DIR" "serial:$port"; then
    echo "upload: FAILED" >&2
    exit 1
fi
echo "upload: OK"

# --------------------------------------------- round 2: watch it render
LOG="$LOG_DIR/m4-walk-render-$$.log"
CAPTURE_ERR="$LOG_DIR/m4-walk-capture-$$.stderr"
capture_args=(link capture "$port" --console "$LOG" --seconds "$WATCH_SECS")
echo
if [[ $emulated == 0 ]]; then
    echo "==> reflashing so the device boots, loads the pushed project, compiles and renders"
    flash_no_monitor "$LOG_DIR/m4-walk-flash2-$$.log"
    wait_for_port
else
    # The test hook's stand-in for the second flash: a software reboot, asked
    # over the link the capture is hosting, into the project the board kept.
    echo "==> asking the emulated board to reboot into the pushed project"
    capture_args+=(--request reboot)
fi
echo "==> hosting the link for ${WATCH_SECS} s (lp-cli link capture) → $LOG"
capture_ok=1
lp_cli "${capture_args[@]}" 2>"$CAPTURE_ERR" || capture_ok=0

echo
echo "===== LINK ====="
grep -a "^link capture:" "$CAPTURE_ERR" || true
if [[ $capture_ok == 0 ]]; then
    echo "FAIL: the capture did not complete. Its stderr:" >&2
    tail -20 "$CAPTURE_ERR" >&2
    exit 1
fi

echo
echo "===== DEVICE ====="
# `does not produce` is in the filter on purpose: its ABSENCE is the signal that
# something now feeds the output node. It was M3's every-frame symptom.
strip_ansi "$LOG" \
    | grep -aE "INIT|RECOVERY|Boot:|Project|compilation|\[OUT\]|ERROR|heap|does not produce|^\[link\]" \
    | cut -c1-200 \
    | head -60

echo
echo "===== ORACLE ====="
# One run, two engines: `[ORACLE]` is wasmtime, `[ORACLE-RV32]` is
# `lpvm-native`'s rv32 code generator under `rt_emu`. Both are printed because
# which of them a device byte agrees with is the whole triage — and what that
# triage MEANS is chip-dependent. On the two Xtensa chips the rv32 engine is
# the same code generator one ISA over, so agreeing with it and not with
# wasmtime is a codegen-vs-wasmtime finding. On the C6 it is the same code
# generator on the SAME ISA: a C6 that agrees with `[ORACLE-RV32]` and not
# with `[ORACLE]` has reproduced the host engine exactly, and the difference
# is wasmtime's.
oracle_out="$(cargo test -q -p lpa-server --test shader_oracle_frame -- --nocapture 2>/dev/null)"
echo "$oracle_out" | grep -a "\[ORACLE" || {
    echo "oracle test did not run" >&2
    exit 1
}

echo
echo "===== COMPARISON ====="
hex_of() { echo "$1" | grep -a "^\[$2\] rgb=" | head -1 | cut -d= -f2; }
# The LAST dump, not the first: the first frame after a project load is the
# compile-window black fallback (ADR 2026-08-03-memory-pressure-at-compile-
# safe-points), and `frame_dump` dumps it before re-arming for the first lit
# frame. Note this comparison assumes a single-channel output; a multi-channel
# project prints one dump per wire and must be diffed per slice by hand.
# A dump is several `part=i/n` lines (a log record on an lp-link board is cut
# at 200 bytes); scripts/frame-dump-hex.sh joins the last whole one.
device_hex="$(strip_ansi "$LOG" | "$(dirname "$0")/frame-dump-hex.sh")"
oracle_hex="$(hex_of "$oracle_out" ORACLE)"
rv32_hex="$(hex_of "$oracle_out" ORACLE-RV32)"

if [[ -z "$rv32_hex" ]]; then
    # The rv32 half asserts "every channel rendered black" when the builtins
    # image it links against is missing, which a fresh worktree or a wiped
    # build cache is enough to cause. Without this guard the triage below would
    # blame the device for a host-side prerequisite.
    echo "FAIL: the oracle's rv32 engine produced no frame — run 'just ci-prereqs'" >&2
    echo "      and try again. (A missing builtins image renders black, and a" >&2
    echo "      black host frame is not an oracle.)" >&2
    exit 1
fi
if [[ -z "$device_hex" ]]; then
    echo "FAIL: the device printed no frame dump — nothing rendered." >&2
    echo "      (Or the image was built without '$FLASH_FEATURES', in which case" >&2
    echo "       it renders fine and simply says nothing about it. Or the output" >&2
    echo "       node's endpoint ws281x:local:$ENDPOINT_LABEL is not one this board" >&2
    echo "       offers, in which case the DEVICE section above says so. Or the" >&2
    echo "       link never came up, in which case the LINK section has no 'up'.)" >&2
    exit 1
fi
echo "device:   $device_hex"
echo "wasmtime: $oracle_hex"
echo "rv32-emu: $rv32_hex"
if [[ "$device_hex" == "$oracle_hex" ]]; then
    echo "PASS: $chip frame is byte-identical to the host oracle (${#device_hex} hex chars)."
    [[ "$device_hex" == "$rv32_hex" ]] || echo "  note: rv32-emu differs from both — investigate."
    exit 0
fi

echo "FAIL: device and wasmtime frames differ."
if [[ "$device_hex" == "$rv32_hex" ]]; then
    echo "  TRIAGE: the device agrees with rv32-emu, so this is a native-codegen"
    if [[ "$chip" == "esp32c6" ]]; then
        echo "          versus wasmtime difference. On this chip the two are the SAME"
        echo "          code generator on the SAME ISA, so the device is right and the"
        echo "          finding is wasmtime's — start at lpvm-native, not at the board."
    else
        echo "          versus wasmtime difference, NOT an Xtensa-specific one."
    fi
elif [[ "$chip" == "esp32c6" ]]; then
    echo "  TRIAGE: the device agrees with NEITHER host engine. On this chip that is"
    echo "          the strong case: rv32-emu runs the device's own code generator on"
    echo "          the device's own ISA, so a difference is the CHIP — the JIT's"
    echo "          install path, the cache, or a peripheral — not the compiler."
else
    echo "  TRIAGE: the device agrees with NEITHER host engine — Xtensa-specific."
fi
exit 1
