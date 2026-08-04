#!/usr/bin/env bash
# Hardware walk — a shader compiled and executed on a device's on-chip Xtensa
# JIT, verified against a host render rather than against an LED.
#
# Usage:
#   scripts/m4-hardware-walk.sh [port]                 # ESP32-S3   (M4 gate)
#   scripts/m4-hardware-walk.sh --chip esp32 [port]    # classic ESP32 (M7 gate)
#
# Named for the S3's M4 gate, which it was written for; the classic ESP32's
# M7 FINAL gate asks the identical question ("does the on-device JIT render
# bit-exactly against the host oracle?") of a chip whose JIT installs code by a
# completely different route — the classic heap has no I-bus view, so compiled
# code is walked into a fixed SRAM1 region through a word-mirrored D-bus
# aperture. Two chips, one question, so one script: everything chip-specific
# lives in the `case` block below, and nothing downstream of it branches.
#
# The walk is in two flashes because espflash's `--monitor` HOLDS the port,
# so `lp-cli` cannot open it at the same time. Round 1 flashes and pushes the
# project; round 2 reflashes to watch the board boot, auto-load it, compile the
# shader on device, and dump the frame. The app partition is rewritten by the
# second flash but `lpfs` is not, so the project pushed in round 1 is still
# there.
#
# The other hazard, inherited from M3: SIGTERM/SIGKILL on espflash wedges the
# port until a human physically replugs the board. Always SIGINT.
#
# ⚠️ Classic ESP32 only: a bare `espflash monitor` stub-halts that board. The
# monitor is therefore only ever attached via `flash --monitor` (what the
# `flash-fw-esp32v3` recipe does), under a `script` pty. If you need to watch
# without reflashing, hold the fd open by hand instead:
#   exec 3<> "$port"; stty -f "$port" 921600 raw -echo clocal; cat <&3
#
# Both flashes carry the `frame-dump` feature (see FLASH_FEATURES below): the
# RMT driver drives real LEDs, and an LED cannot be diffed against a host
# render, so the walk needs the build that also prints each transmitted frame to
# serial. A default `just flash-fw-esp32s3` / `just flash-fw-esp32v3` produces
# no `[OUT]` lines at all and this walk would report "nothing rendered".
#
# The gate is the last section: the device's `[OUT] dump` hex must equal the
# oracle's `[ORACLE] rgb` hex, byte for byte. `examples/shader-oracle` is
# clock-free precisely so that comparison needs no time synchronisation.
set -euo pipefail

cd "$(dirname "$0")/.."
LOG_DIR="${TMPDIR:-/tmp}"
PROJECT="${PROJECT:-examples/shader-oracle}"
# The `[OUT]` transcript this walk parses exists only under this feature; see
# lp-fw/fw-esp32s3/src/output/rmt/frame_dump.rs and its byte-for-byte port at
# lp-fw/fw-esp32v3/src/output/rmt/frame_dump.rs — the two print identical line
# shapes so that everything below this point is chip-agnostic.
FLASH_FEATURES="${FLASH_FEATURES:-frame-dump}"

usage() {
    sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'
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

# ------------------------------------------------------------ chip table
# The whole of this script's chip knowledge. Four facts each:
#
#   FLASH_RECIPE    the just recipe that builds+flashes+monitors this chip. It
#                   owns the partition table, the flash size and (on the
#                   classic) `--monitor-baud 921600`; this script must not
#                   duplicate any of them.
#   ENDPOINT_LABEL  the board label the oracle project's output node must name.
#                   `examples/shader-oracle` is authored for the XIAO S3's
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
    *)
        echo "unsupported --chip '$chip' (known: esp32s3, esp32)" >&2
        exit 2
        ;;
esac

echo "==> chip=$chip recipe=just $FLASH_RECIPE endpoint=ws281x:local:$ENDPOINT_LABEL"

# ------------------------------------------------------------ port
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

if [[ "$VERIFY_CHIP" != 0 ]]; then
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
    script -q "$LOG" just "$FLASH_RECIPE" "$port" "$FLASH_FEATURES" >/dev/null 2>&1 &
    local pid=$!
    for _ in $(seq 1 "$secs"); do
        grep -qa "$marker" "$LOG" 2>/dev/null && return 0
        kill -0 "$pid" 2>/dev/null || return 1
        sleep 1
    done
    return 1
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
if ! cargo run -q -p lp-cli -- upload "$UPLOAD_DIR" "serial:$port"; then
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
# native code generator both Xtensa chips JIT, one ISA over. Both are printed
# because which of them a device byte agrees with is the whole triage.
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
device_hex="$(strip_ansi "$LOG" | grep -ao 'rgb=[0-9a-f]*' | tail -1 | cut -d= -f2)"
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
    echo "       offers, in which case the DEVICE section above says so.)" >&2
    exit 1
fi
if [[ "$device_hex" == "$oracle_hex" ]]; then
    echo "PASS: $chip frame is byte-identical to the host oracle (${#device_hex} hex chars)."
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
