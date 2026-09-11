#!/usr/bin/env bash
# The classic ESP32's frame walk, with the emulator where the board goes.
#
#   scripts/emu/m4-walk-esp32v3.sh          # `just walk-esp32v3-emu-frame`
#   scripts/emu/m4-walk-esp32v3.sh --keep   # leave the artefacts for inspection
#
# `scripts/emu/m4-walk.sh` is this script's C6 twin and was written first; read
# it beside this one. Both ask the same question of a chip — does the shader it
# compiled and executed itself render the same bytes a host render produces? —
# and both ask it the two ways only an emulator can:
#
#   [OUT] dump   the firmware's own record of the frame it handed the WS281x
#                driver, over the serial link (`frame-dump`).
#   the pad      the WS281x waveform the classic RMT model actually produced,
#                decoded back at the datasheet's ±150 ns by a decoder that
#                never spoke to the firmware.
#
# ## What is different here, and why
#
# * **The link is UART0.** The classic has no USB-Serial-JTAG, so the wire the
#   product ships on is UART0 and `--uart0 tcp:<addr>` is the socket `lp-cli
#   … serial:tcp://` talks to.
# * **The runner is the `lp-emu-esp32v3` binary, not `lp-cli emu run`.**
#   `lp-cli emu run --chip` knows one chip (`esp32c6`) today; teaching it the
#   classic is not this phase's, so the walk drives the crate's own binary,
#   which has every door the walk needs (`--uart0 tcp:`, `--console`,
#   `--dump-frames`, `--core-quantum`).
# * **The project is retargeted into a scratch copy.**
#   `projects/test/shader-oracle` names `ws281x:local:D10`, the XIAO S3's pad;
#   the DOM-Z-102 has no D10 and calls its four fused DATA terminals
#   IO18/IO16/IO14/IO2. An output node whose endpoint the board does not have
#   never opens, and the walk would then report a pixel mismatch that is really
#   a mis-addressed pin. `prepare_project` below is
#   `scripts/m4-hardware-walk.sh`'s, by the same mechanism and with the same
#   loud guard — so both chips render provably identical pixels, because the
#   endpoint chooses a wire and never a colour.
#
# ## All three readings, on 2026-09-11 at a0707caaf + this branch
#
# The project loads, the output opens, the shader compiles, and the run reaches
# its own emulated deadline with `unmapped=0`: **2,438 frames on IO18, 2,437 of
# them lit, one distinct lit byte string, and it equals the guest's own
# deferred `[OUT] dump` and `[ORACLE] rgb=` — 384 hex characters, three ways.**
# Exit 0.
#
# Until M4 **P4b** (PR #711) this walk reached only two of the three: loading a
# project killed the guest a few seconds in, in `_WindowUnderflow8` with a null
# `a1`, before the deferred dump 30 frames past the first lit frame could be
# printed. `CALL0`/`CALLX0` zeroing `PS.CALLINC` was the cause; the crate
# README's "The window, across a context save" has the trace. The walk's
# evidence-first structure below is that episode's legacy and stays: a run that
# loses its link must still print the frames the pad already carried.
set -euo pipefail

cd "$(dirname "$0")/../.."
REPO="$PWD"

PROJECT="${PROJECT:-projects/test/shader-oracle}"
# The DOM-Z-102's first fused DATA terminal, and the pad the decoder watches.
ENDPOINT_LABEL="${ENDPOINT_LABEL:-IO18}"
PAD="${LP_WALK_PAD:-18}"
LINK="${LP_WALK_LINK:-127.0.0.1:5607}"
# EMULATED time, and the walk's whole cost. The budget, measured on this
# machine: the direct load reaches `[RECOVERY] boot complete` at ~115 ms, the
# scripted upload lands its project at ~7.3 s (twelve requests, 64 B every
# 30 ms of guest time), the classic compiles the shader, and the deferred
# `[OUT] dump` fires at the guest's frame 31 a further ~70 ms later. 10 s is
# that with a second of margin; it was 30 s while the window-spill defect
# (M4 P4b, PR #711) made a longer run the only hope of a dump, and a longer
# run now costs wall clock and proves nothing more — a clock-free project
# renders the same bytes for ever. Raise it with LP_WALK_TIMEOUT for a soak
# and expect the wall clock to move with it.
TIMEOUT="${LP_WALK_TIMEOUT:-10s}"
# Wall-clock net. The classic runs several times slower than real time while
# it is busy and FASTER than real time while it idles (the idle skip).
WALL="${LP_WALK_WALL_TIMEOUT:-900}"
QUANTUM="${LP_WALK_QUANTUM:-256}"
OUT="${LP_WALK_OUT:-$REPO/target/lp-emu-esp32v3-walk}"

keep=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep) keep=1; shift ;;
        -h|--help) sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

command -v jq >/dev/null 2>&1 || {
    echo "jq not found. Install it (brew install jq) — the decoded frames are JSON." >&2
    exit 2
}

mkdir -p "$OUT"
console="$OUT/walk.console.txt"
frames="$OUT/walk.frames.jsonl"
rm -f "$console" "$frames"

# ------------------------------------------- the project this board can open
#
# `scripts/m4-hardware-walk.sh:244-267`, kept in step with it. The copy keeps
# the directory basename, because that is the project's on-device name.
prepare_project() {
    local authored
    authored="$(sed -n 's/.*"endpoint"[[:space:]]*:[[:space:]]*"ws281x:local:\([^"]*\)".*/\1/p' \
        "$PROJECT/output.json" | head -1)"
    if [[ -z "$authored" ]]; then
        echo "FAIL: no ws281x:local endpoint found in $PROJECT/output.json." >&2
        exit 1
    fi
    if [[ "$authored" == "$ENDPOINT_LABEL" ]]; then
        UPLOAD_DIR="$PROJECT"
        return
    fi
    UPLOAD_DIR="$OUT/project/$(basename "$PROJECT")"
    rm -rf "$UPLOAD_DIR"
    mkdir -p "$UPLOAD_DIR"
    cp "$PROJECT"/* "$UPLOAD_DIR"/
    sed -i.bak "s|\"ws281x:local:$authored\"|\"ws281x:local:$ENDPOINT_LABEL\"|" \
        "$UPLOAD_DIR/output.json"
    rm -f "$UPLOAD_DIR/output.json.bak"
    # A substitution that matched nothing would upload an endpoint this board
    # does not have, and the walk would blame the JIT for a wiring mistake.
    if ! grep -q "\"ws281x:local:$ENDPOINT_LABEL\"" "$UPLOAD_DIR/output.json"; then
        echo "FAIL: could not rewrite the output endpoint to $ENDPOINT_LABEL." >&2
        exit 1
    fi
    echo "==> retargeted $authored -> $ENDPOINT_LABEL in $UPLOAD_DIR"
}

prepare_project

# ------------------------------------------------- the image, and its bytes
#
# The CURRENT tree's shipped feature set plus `frame-dump`, exactly as
# `scripts/m4-hardware-walk.sh` flashes the current tree plus the same
# feature. Built through the justfile recipe because it owns the Xtensa
# toolchain's PATH and the `touch src/main.rs` a feature flip needs on this
# crate.
echo "==> building fw-esp32v3 (esp32,server,float-f32 + frame-dump)"
just build-fw-esp32v3 frame-dump
built="$REPO/target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3"
[[ -f "$built" ]] || { echo "the build reported success but $built is missing" >&2; exit 1; }
# A copy under our own name: EVERY feature set of this crate builds to that one
# path, so whatever is there is whatever was built last, and a walk that read
# it could be reading a build with no readout in it at all.
elf="$OUT/fw-esp32v3-frame-dump"
cp "$built" "$elf"

# The readout has to be IN the image. Without this check a default build would
# render perfectly, print nothing, and the walk would report "nothing
# rendered". `grep -a`, not `grep -qa`: under `pipefail` a `-q` grep closes the
# pipe on its first hit, `strings` dies of SIGPIPE, and the PIPELINE reports
# 141 — so the check would fail loudest exactly when it passed.
if ! strings "$elf" | grep -a '\[OUT\] dump frame=' >/dev/null; then
    echo "FAIL: the built image carries no frame-dump readout." >&2
    echo "      Did the 'frame-dump' feature stop reaching the driver's write path?" >&2
    exit 1
fi

# Release, and the expensive step on a cold cache — minutes, not seconds. It
# has to be: the same binary uploads the project and the emulator's own
# interpreter loop is a release build for the same reason.
echo "==> building lp-cli and lp-emu-esp32v3 (release)"
cargo build --quiet --release -p lp-cli -p lp-emu-esp32v3
cli="$REPO/target/release/lp-cli"
emu="$REPO/target/release/lp-emu-esp32v3"

# ---------------------------------------------------- the machine, listening
cleanup() {
    [[ -n "${emu_pid:-}" ]] && kill "$emu_pid" 2>/dev/null || true
}
trap cleanup EXIT

echo "==> lp-emu-esp32v3 (direct load, ${TIMEOUT} emulated, quantum $QUANTUM) on $LINK"
"$emu" \
    --elf "$elf" \
    --uart0 "tcp:$LINK" \
    --console "$console" \
    --dump-frames "file:$frames" \
    --strict-bus \
    --core-quantum "$QUANTUM" \
    --time-grade t1 \
    --timeout "$TIMEOUT" \
    --wall-timeout "$WALL" \
    >"$OUT/emu.stdout" 2>"$OUT/emu.stderr" &
emu_pid=$!

# The machine listens as soon as it is built, before the hart has run an
# instruction; wait for the PORT rather than for a log line. bash's own
# /dev/tcp rather than `nc`, whose flags differ between the BSD one macOS ships
# and the GNU one a CI runner has.
for _ in $(seq 1 300); do
    (exec 3<>"/dev/tcp/${LINK%%:*}/${LINK##*:}") 2>/dev/null && break
    kill -0 "$emu_pid" 2>/dev/null || break
    sleep 0.1
done
if ! kill -0 "$emu_pid" 2>/dev/null; then
    echo "FAIL: the emulator exited before it served the link." >&2
    tail -20 "$OUT/emu.stderr" >&2
    exit 1
fi

# ---------------------------------------------------------------- the upload
echo
echo "===== UPLOAD ====="
set +e
"$cli" upload "$UPLOAD_DIR" "serial:tcp://$LINK" --wait-timeout "${LP_WALK_CLI_TIMEOUT:-600}" \
    >"$OUT/cli.stdout" 2>"$OUT/cli.stderr"
cli_status=$?
set -e
tail -5 "$OUT/cli.stdout" || true
upload_failed=0
if [[ $cli_status -ne 0 ]]; then
    echo "upload: FAILED (exit $cli_status)" >&2
    tail -30 "$OUT/cli.stderr" >&2
    # …and the walk carries on regardless, deliberately. A guest that dies
    # mid-upload has usually already loaded the project, opened its output and
    # rendered — the run that found the window-spill defect (M4 P4b, PR #711)
    # got two lit frames onto the pad and then lost the link — and a walk that
    # exits here throws away the only evidence it went to all this trouble to
    # collect. The exit code still says FAILED.
    echo "       continuing: the frames the pad already carried are printed below" >&2
    upload_failed=1
else
    echo "upload: OK"
fi

# Let the guest render on past the upload to its deferred lit dump, then let
# the machine reach its own emulated deadline rather than killing it — a killed
# run has no report and no flushed frames.
echo
echo "==> letting the machine run on to its emulated deadline"
wait "$emu_pid" && emu_status=0 || emu_status=$?
emu_pid=
if [[ $emu_status -ne 0 ]]; then
    echo "NOTE: the machine did not end at its deadline (exit $emu_status)." >&2
    grep -m 8 -aE "STRICT BUS STOP|FAULT|pc      =|cycle   =|access  =" "$OUT/emu.stdout" >&2 || true
    echo "      Exit 3 is a strict-bus stop: the machine refused an access the guest" >&2
    echo "      made. The crate README's \"The window, across a context save\" is the" >&2
    echo "      last one of these to be diagnosed (M4 P4b, PR #711) and shows how." >&2
fi

echo
echo "===== DEVICE ====="
# `does not produce` is in the filter on purpose, exactly as in the hardware
# walk: its ABSENCE is the signal that something feeds the output node.
#
# `grep -m 40`, not `| head -40`: under `pipefail` a `head` that closes the pipe
# kills `grep` with SIGPIPE and the whole pipeline reports 141, so the walk
# would die HERE, at a progress print, on a run that was going perfectly.
grep -m 40 -aE "boot:|ESP-ROM|INIT|RECOVERY|Project|compilation|\[OUT\]|ERROR|does not produce" \
    "$console" || true

echo
echo "===== ORACLE ====="
# `set +e` around it deliberately: a failing `cargo test` inside a command
# substitution would kill this script at the ASSIGNMENT under `set -e`, and the
# walk would end with an empty ORACLE section and no reason.
set +e
oracle_out="$(cargo test -q -p lpa-server --test shader_oracle_frame -- --nocapture 2>"$OUT/oracle.stderr")"
oracle_status=$?
set -e
if [[ $oracle_status -ne 0 ]] || ! echo "$oracle_out" | grep -a "\[ORACLE" >/dev/null; then
    echo "FAIL: the host oracle did not run (exit $oracle_status)." >&2
    echo "      It needs the rv32 builtins image — 'just build-rv32-builtins' (or the" >&2
    echo "      whole 'just ci-prereqs'). 'just walk-esp32v3-emu-frame' depends on it; a" >&2
    echo "      bare call to this script does not." >&2
    tail -20 "$OUT/oracle.stderr" >&2
    exit 1
fi
echo "$oracle_out" | grep -a "\[ORACLE"

echo
echo "===== COMPARISON ====="
# `grep -m 1`, never `| head -1`: see the DEVICE section for what a closed pipe
# does to this script.
#
# ⚠️ `|| true` on every one of these, and it is not defensive noise: under
# `pipefail` a `grep` that matches nothing fails the whole pipeline even
# though `cut` succeeded, and an assignment from a failing command
# substitution ends the script under `set -e`. A walk whose guest printed no
# `rgb=` would then die at an *assignment*, with no comparison and no reason —
# which is exactly the run that most needs one.
hex_of() { echo "$1" | grep -m 1 -a "^\[$2\] rgb=" | cut -d= -f2 || true; }
oracle_hex="$(hex_of "$oracle_out" ORACLE)"
rv32_hex="$(hex_of "$oracle_out" ORACLE-RV32)"

# The LAST dump, not the first — the same rule and the same reason as the
# hardware walk. The first frame after a project load is the compile-window
# black fallback (ADR 2026-08-03-memory-pressure-at-compile-safe-points), and
# `frame_dump` dumps it at open before re-arming for the first lit frame.
device_hex="$(grep -ao 'rgb=[0-9a-f]*' "$console" | tail -1 | cut -d= -f2 || true)"
dumps="$(grep -ac '\[OUT\] dump frame=' "$console" || true)"

pad_hex="$(jq -r --argjson pad "$PAD" -s '
    [ .[] | select(.kind == "ws281x-frame" and .pad == $pad and .complete
                   and (.rgb | test("[1-9a-f]"))) ] | .[0].rgb // ""' "$frames")"
pad_frames="$(jq -r --argjson pad "$PAD" -s \
    '[ .[] | select(.kind == "ws281x-frame" and .pad == $pad) ] | length' "$frames")"
lit_frames="$(jq -r --argjson pad "$PAD" -s '
    [ .[] | select(.kind == "ws281x-frame" and .pad == $pad and .complete
                   and (.rgb | test("[1-9a-f]"))) ] | length' "$frames")"
# Every lit frame the same bytes. A clock-free project that changed between
# frames would mean the render is not a function of the project alone, and a
# single-frame comparison would be luck.
distinct_lit="$(jq -r --argjson pad "$PAD" -s '
    [ .[] | select(.kind == "ws281x-frame" and .pad == $pad and .complete
                   and (.rgb | test("[1-9a-f]"))) | .rgb ] | unique | length' "$frames")"

echo "  pad $PAD: $pad_frames frame(s) decoded, $lit_frames lit, $distinct_lit distinct lit frame(s)"

if [[ -z "$rv32_hex" ]]; then
    echo "FAIL: the oracle's rv32 engine produced no frame — run 'just ci-prereqs'" >&2
    echo "      and try again. (A missing builtins image renders black, and a black" >&2
    echo "      host frame is not an oracle.)" >&2
    exit 1
fi
fail=$upload_failed

# ⚠️ **Readings (b) and (c) are compared FIRST, and a missing reading (a) does
# not stop them.** The firmware's own dump is the *deferred* one — 30 frames
# past the first lit frame — so it is the reading a run that ends early loses
# first, and the pad-against-oracle comparison is exactly the evidence such a
# run still holds. An earlier draft exited here on a black-only dump and threw
# that away.
if [[ -z "$pad_hex" ]]; then
    echo "FAIL: no lit frame reached pad $PAD — the render never left the chip."
    echo "      $pad_frames frame(s) were decoded there."
    fail=1
elif [[ "$distinct_lit" != "1" ]]; then
    echo "FAIL: $distinct_lit distinct lit frames on pad $PAD, for a clock-free project."
    echo "      The render is not a function of the project alone; comparing any one of"
    echo "      them to the oracle would be luck."
    fail=1
elif [[ "$pad_hex" != "$oracle_hex" ]]; then
    echo "FAIL: the pad and wasmtime differ."
    echo "  pad $PAD:   $pad_hex"
    echo "  wasmtime: $oracle_hex"
    echo "  rv32-emu: $rv32_hex"
    if [[ "$pad_hex" == "$rv32_hex" ]]; then
        echo "  TRIAGE: the pad agrees with rv32-emu — a DIFFERENT code generator on a"
        echo "          different ISA from the Xtensa one the classic JITs, so this is the"
        echo "          wasmtime side. Start at lpvm-native, not at the machine."
    else
        echo "  TRIAGE: the pad agrees with NEITHER host engine. Both host engines agree"
        echo "          with each other on this project, so this is the MACHINE or the"
        echo "          classic's own code generator — the JIT's install path, the cache,"
        echo "          or a peripheral."
    fi
    fail=1
else
    echo "PASS: pad $PAD == [ORACLE] rgb (${#pad_hex} hex chars), $lit_frames lit frame(s),"
    echo "      all of them the same bytes."
fi

if [[ -z "$device_hex" || ( "$dumps" == "1" && "$device_hex" =~ ^0+$ ) ]]; then
    echo "FAIL: the guest's own lit dump never arrived (reading (a))."
    echo "      The open-time dump is the compile-window black fallback; the deferred lit"
    echo "      dump fires 30 frames after the first LIT one. So either the run was too"
    echo "      short (raise LP_WALK_TIMEOUT) or it ended early — the NOTE above says"
    echo "      which, and the crate README's \"A frame three ways\" says what the three"
    echo "      readings are and how to triage a disagreement."
    fail=1
    device_hex=""
fi
if [[ -n "$device_hex" && "$device_hex" != "$pad_hex" ]]; then
    echo "FAIL: the firmware's dump and the pad disagree."
    echo "  [OUT] dump: $device_hex"
    echo "  pad $PAD:      $pad_hex"
    echo "  TRIAGE: the render is one thing and the wire another — the RMT encode, the"
    echo "          colour order, or the refill path. This is the comparison a board"
    echo "          cannot make, and the reason this walk exists."
    fail=1
fi
if [[ -n "$device_hex" && "$device_hex" != "$oracle_hex" ]]; then
    echo "FAIL: guest and wasmtime frames differ."
    echo "  guest:    $device_hex"
    echo "  wasmtime: $oracle_hex"
    echo "  rv32-emu: $rv32_hex"
    if [[ "$device_hex" == "$rv32_hex" ]]; then
        echo "  TRIAGE: the guest agrees with rv32-emu — a DIFFERENT code generator on a"
        echo "          different ISA from the Xtensa one the classic JITs, so this is the"
        echo "          wasmtime side. Start at lpvm-native, not at the machine."
    else
        echo "  TRIAGE: the guest agrees with NEITHER host engine. Both host engines agree"
        echo "          with each other on this project, so this is the MACHINE or the"
        echo "          classic's own code generator — the JIT's install path, the cache,"
        echo "          or a peripheral."
    fi
    fail=1
fi

if [[ $fail -eq 0 ]]; then
    echo "PASS: the frame is byte-identical on all THREE readings (${#device_hex} hex chars)."
    echo "  [OUT] dump == pad $PAD == [ORACLE] rgb"
    [[ "$device_hex" == "$rv32_hex" ]] || \
        echo "  note: rv32-emu differs from both — investigate."
fi

echo
grep -m 8 -a "^run: " "$OUT/emu.stdout" || true
grep -m 8 -a "^pin gpio" "$OUT/emu.stdout" || true
echo "artefacts: $OUT"
if [[ $keep -eq 0 && $fail -eq 0 ]]; then
    rm -f "$OUT/emu.stdout" "$OUT/cli.stdout" "$OUT/cli.stderr"
fi
exit "$fail"
