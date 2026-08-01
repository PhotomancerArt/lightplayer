#!/usr/bin/env bash
# M4 hardware walk — a shader compiled and executed on the ESP32-S3's Xtensa
# JIT, verified against a host render rather than against an LED.
#
# Usage:  scripts/m4-hardware-walk.sh [port]
#
# The walk is in two flashes because USB-Serial-JTAG is an EXCLUSIVE port:
# espflash's `--monitor` holds it, so `lp-cli` cannot open it at the same time.
# Round 1 flashes and pushes the project; round 2 reflashes to watch the board
# boot, auto-load it, compile the shader on device, and dump the frame. The app
# partition is rewritten by the second flash but `lpfs` is not, so the project
# pushed in round 1 is still there.
#
# The other hazard, inherited from M3: SIGTERM/SIGKILL on espflash wedges the
# port until a human physically replugs the board. Always SIGINT.
#
# Both flashes carry the `frame-dump` feature (see FLASH_FEATURES below): the
# RMT driver drives real LEDs, and an LED cannot be diffed against a host
# render, so the walk needs the build that also prints each transmitted frame to
# serial. A default `just flash-fw-esp32s3` produces no `[OUT]` lines at all and
# this walk would report "nothing rendered".
#
# The gate is the last section: the device's `[OUT] dump` hex must equal the
# oracle's `[ORACLE] rgb` hex, byte for byte. `examples/shader-oracle` is
# clock-free precisely so that comparison needs no time synchronisation.
set -euo pipefail

cd "$(dirname "$0")/.."
LOG_DIR="${TMPDIR:-/tmp}"
PROJECT="${PROJECT:-examples/shader-oracle}"
# The `[OUT]` transcript this walk parses exists only under this feature; see
# lp-fw/fw-esp32s3/src/output/rmt/frame_dump.rs.
FLASH_FEATURES="${FLASH_FEATURES:-frame-dump}"

port="${1:-}"
if [[ -z "$port" ]]; then
    echo "==> identifying the ESP32-S3"
    for p in /dev/cu.usbmodem*; do
        [[ -e "$p" ]] || continue
        if espflash board-info --port "$p" 2>/dev/null | grep -q "esp32s3"; then
            port="$p"
            echo "    found: $port"
            break
        fi
        echo "    skipping $p (not an S3, or busy)"
    done
fi
if [[ -z "$port" ]]; then
    echo "No ESP32-S3 found. Is it plugged in? Is another session holding it?" >&2
    exit 1
fi

release_port() {
    pkill -INT -f "espflash flash" 2>/dev/null || true
    for _ in $(seq 1 15); do
        pgrep -f "espflash flash" >/dev/null 2>&1 || return 0
        sleep 1
    done
    echo "WARNING: espflash still holding the port." >&2
}
trap release_port EXIT

# Flash, then watch the monitor until `marker` appears or `secs` elapse.
# Leaves the transcript in $LOG (ANSI-stripped by the readers below).
flash_and_watch() {
    local marker="$1" secs="$2"
    script -q "$LOG" just flash-fw-esp32s3 "$port" "$FLASH_FEATURES" >/dev/null 2>&1 &
    local pid=$!
    for _ in $(seq 1 "$secs"); do
        grep -qa "$marker" "$LOG" 2>/dev/null && return 0
        kill -0 "$pid" 2>/dev/null || return 1
        sleep 1
    done
    return 1
}

strip_ansi() { sed 's/\x1b\[[0-9;]*m//g' "$1"; }

# ------------------------------------------------------- round 1: push
LOG="$LOG_DIR/m4-walk-push-$$.log"
echo "==> flashing $port (features: $FLASH_FEATURES)"
if ! flash_and_watch "starting server loop" 180; then
    echo "Board did not reach the server loop. Tail of the log:" >&2
    strip_ansi "$LOG" | tail -40 >&2
    exit 1
fi

echo
echo "===== BOOT ====="
strip_ansi "$LOG" | grep -aE "INIT|RECOVERY|hardware manifest|proto=|Boot:" | head -20

echo
echo "==> detaching the monitor so lp-cli can open the port"
release_port

echo
echo "===== UPLOAD ====="
if ! cargo run -q -p lp-cli -- upload "$PROJECT" "serial:$port"; then
    echo "upload: FAILED" >&2
    exit 1
fi
echo "upload: OK"

# --------------------------------------------- round 2: watch it render
LOG="$LOG_DIR/m4-walk-render-$$.log"
echo
echo "==> reflashing to watch the device compile and render the pushed project"
trap release_port EXIT
flash_and_watch "\[OUT\] dump" 180 || true
sleep 8
release_port
trap - EXIT

echo
echo "===== DEVICE ====="
# `does not produce` is in the filter on purpose: its ABSENCE is the signal that
# something now feeds the output node. It was M3's every-frame symptom.
strip_ansi "$LOG" \
    | grep -aE "INIT|RECOVERY|Boot:|Project|compilation|\[OUT\]|ERROR|heap|does not produce" \
    | head -40

echo
echo "===== ORACLE ====="
# One run, two engines: `[ORACLE]` is wasmtime, `[ORACLE-RV32]` is the same
# native code generator the S3 JITs, one ISA over. Both are printed because
# which of them a device byte agrees with is the whole triage.
oracle_out="$(cargo test -q -p lpa-server --test shader_oracle_frame -- --nocapture 2>/dev/null)"
echo "$oracle_out" | grep -a "\[ORACLE" || {
    echo "oracle test did not run" >&2
    exit 1
}

echo
echo "===== COMPARISON ====="
hex_of() { echo "$1" | grep -a "^\[$2\] rgb=" | head -1 | cut -d= -f2; }
device_hex="$(strip_ansi "$LOG" | grep -ao 'rgb=[0-9a-f]*' | head -1 | cut -d= -f2)"
oracle_hex="$(hex_of "$oracle_out" ORACLE)"
rv32_hex="$(hex_of "$oracle_out" ORACLE-RV32)"

if [[ -z "$device_hex" ]]; then
    echo "FAIL: the device printed no frame dump — nothing rendered." >&2
    echo "      (Or the image was built without '$FLASH_FEATURES', in which case" >&2
    echo "       it renders fine and simply says nothing about it.)" >&2
    exit 1
fi
if [[ "$device_hex" == "$oracle_hex" ]]; then
    echo "PASS: device frame is byte-identical to the host oracle (${#device_hex} hex chars)."
    [[ "$device_hex" == "$rv32_hex" ]] || echo "  note: rv32-emu differs from both — investigate."
    exit 0
fi

echo "FAIL: device and wasmtime frames differ."
echo "  device:   $device_hex"
echo "  wasmtime: $oracle_hex"
echo "  rv32-emu: $rv32_hex"
if [[ "$device_hex" == "$rv32_hex" ]]; then
    echo "  TRIAGE: the device agrees with rv32-emu, so this is a native-codegen"
    echo "          versus wasmtime difference, NOT an Xtensa-specific one."
else
    echo "  TRIAGE: the device agrees with NEITHER host engine — Xtensa-specific."
fi
exit 1
