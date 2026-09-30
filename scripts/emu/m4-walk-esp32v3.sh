#!/usr/bin/env bash
# The classic ESP32's walk, with the emulator where the board goes.
#
#   scripts/emu/m4-walk-esp32v3.sh          # `just walk-esp32v3-emu`
#   scripts/emu/m4-walk-esp32v3.sh --frame  # `just walk-esp32v3-emu-frame`
#   scripts/emu/m4-walk-esp32v3.sh --keep   # leave the artefacts for inspection
#
# Two front doors, one script, and the difference between them is the boot and
# the cable rather than the question:
#
#   `just walk-esp32v3-emu`        ROM-up from a merged 4 MiB image — the same
#   (M5 P5, the whole walk)        bytes a flasher writes — with the CH340
#                                  cable's own verbs on a scripted schedule
#                                  (`--control-script`): attach, the
#                                  classic-reset dance, open, upload, close,
#                                  detach, and the port proved released at
#                                  the end.
#   `just walk-esp32v3-emu-frame`  the frame half (M4 P4): direct load, no
#                                  cable. The iteration path — it skips the
#                                  merged-image build and one whole boot, and
#                                  asks exactly the same question of the
#                                  frame.
#
# `scripts/emu/m4-walk.sh` is this script's C6 twin and was written first; read
# it beside this one. Both ask the same question of a chip — does the shader it
# compiled and executed itself render the same bytes a host render produces? —
# and both ask it the two ways only an emulator can:
#
#   [OUT] dump   the firmware's own record of the frame it handed the WS281x
#                driver, over the serial link (`frame-dump`). Since wire proto
#                32 that link is lp-link, and the dump is log records on its
#                log channel: the bytes the link itself delivered, decoded by
#                the product's own host end.
#   the pad      the WS281x waveform the classic RMT model actually produced,
#                decoded back at the datasheet's ±150 ns by a decoder that
#                never spoke to the firmware.
#
# ## What is different here, and why
#
# * **The link is UART0.** The classic has no USB-Serial-JTAG, so the wire the
#   product ships on is UART0 — and since wire proto 32 it is an lp-link
#   (plan `classic-uart-on-lp-link`), not `M!` lines.
# * **The runner is `lp-cli emu run --chip esp32v3 --host-link`.** Until proto
#   32 this walk drove the crate's own binary with the link on `--uart0 tcp:`
#   and `lp-cli upload` in a second process. On lp-link that shape loses the
#   walk's first reading: the board's log records — the `[OUT] dump` among
#   them — leave the board only while a host holds the link, and an upload
#   client that leaves takes the host with it. So the walk runs the C6's and
#   the S3's shape: one process boots the image, hosts the link in EMULATED
#   time, uploads the project over it (`--upload`) and keeps hosting to the
#   deadline, writing the decoded console as it goes.
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
# ## The boot path
#
# **ROM-up by default**, because the twin's whole point is to be the twin of
# something that flashes and resets a board: the hart starts at the mask ROM's
# reset vector, the real ROM attaches the flash chip and programs the MMU, the
# real ESP-IDF second-stage bootloader espflash bundled reads the partition
# table and maps the app, and only then does any of our code run. Nothing is
# vendored — the merged image IS the provenance (DD25). It costs ~260 ms more
# emulated time than a direct load (measured: `[RECOVERY] boot complete` at
# 373,644 us ROM-up against ~115,000 us direct) and ~2.4 s of wall clock.
#
# `LP_WALK_BOOT=direct` takes the direct load instead, which `tests/
# rom_up_boot.rs` holds byte-equal to the ROM-up path at the application's
# entry (2,189,961 bytes, `PS`, `a1`, `VECBASE`, the save area and all 256
# flash-MMU entries — with the 32-byte inter-segment DROM gap named and
# excluded). `--frame` implies it.
#
# ## The cable, and why its verbs are in this file
#
# A hardware walk gets four things from the CH340 on the carrier board that a
# reader never sees written down: the cable is in, the board is reset, a tty
# is opened, and at the end the tty is released and the lines are left slack.
# Here they are a scripted schedule in EMULATED time — `--control-script`,
# the deterministic twin of the machine's `--control tcp:` socket, which the
# hosted door has no second socket for — and they are in the script rather
# than in a human's head:
#
#   attach                    the cable goes in (host bookkeeping; the SoC
#                             cannot see a bridge chip, see the crate README's
#                             "The CH340 cable")
#   reset                     {dtr:0,rts:1} then {rts:0} — the classic-reset
#                             dance. The reboot happens on the RELEASE, and
#                             the hosted board is built to reboot on reset,
#                             so that release reboots the machine instead of
#                             ending the run
#   open                      an application opened the tty
#   …the upload…
#   close / detach            and the cable's state at the end of the run
#                             (the run report's `cable:` line, in the `state`
#                             verb's own words) is ASSERTED: `port=closed`,
#                             `cable=absent`, `dtr=0 rts=0`, `reboots=1`
#
# The console then holds TWO boots — the ROM banner the reset cut mid-line and
# the whole boot after it — which is what a reset board's transcript looks
# like and is the twin's own evidence that the cable did something.
#
# ⚠️ **What the reset does NOT buy is a second boot.** A reboot restores the
# power-on snapshot, and on this machine that snapshot includes the flash
# chip, so the boot after the cable reset formats the same blank `lpfs` the
# first one did. On silicon every capture is a second boot because espflash
# hard-resets after WRITING (M5 notes §3.2); the emulated twin of that is a
# two-run recipe, and it is P6's, not this walk's.
#
# ## The upload's pacing
#
# UART0 has no RTS/CTS, so a host that writes faster than the guest drains
# can lose bytes. `UartEngine::poll_source`
# (`lp-emu-esp-common/src/engine/uart.rs`) delivers the host's bytes **at the
# programmed baud** — at 921,600, at most ~92 bytes reach the 128-byte RX FIFO
# per millisecond — and since proto 32 whatever the FIFO does lose is resent
# by the link's ARQ and counted, never silent. The upload is lp-link frames at
# the link's own pace (a window of four), so there is nothing to meter here.
#
# The committed `walks/*.script` files are the pre-lp-link `M!` replays of
# this upload, kept for the tests that check their shape; the lp-link image no
# longer reads them.
#
# ## All three readings, on lp-link (2026-09-29, lp-emu:esp32v3:t1 at 6a4456271)
#
# ROM-up with the cable: the reset reboots the chip once, the link comes up
# and the board says hello at 0.379 s, the upload lands the project at
# 0.798 s, and the run reaches its 4 s deadline with no unmapped access:
# **1,440 frames on IO18, 1,438 of them lit, one distinct lit byte string, and
# it equals the guest's own deferred `[OUT] dump` (delivered over the link's
# log channel) and `[ORACLE] rgb=` — 384 hex characters, three ways.** The
# host link: 123 frames out / 179 in, 0 resent, 0 damaged, 0 resets. The
# direct load (`--frame`): hello at 0.123 s, upload at 0.521 s, 1,562 frames,
# 1,560 lit, the same three-way equality. Exit 0 both. (On the pre-lp-link
# wire, 2026-09-11 at a0707caaf: 2,438 frames on IO18 in a 10 s run whose
# scripted upload landed at ~7.3 s, the same 384 characters.)
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
# EMULATED time, and the walk's whole cost. The budget, measured on this
# machine (2026-09-29, lp-emu:esp32v3:t1, direct load): the link is up and the
# board says hello at 0.123 s, the upload over it lands the project at
# ~0.52 s, the classic compiles the shader, and the deferred `[OUT] dump`
# fires at the guest's frame 31 a moment later. ROM-up adds ~0.26 s. 4 s is
# that with room; it was 10 s while the upload was the `M!` replay's twelve
# requests at 64 B every 30 ms (~7.3 s), and a longer run costs wall clock and
# proves nothing more — a clock-free project renders the same bytes for ever.
# Raise it with LP_WALK_TIMEOUT for a soak and expect the wall clock to move
# with it. Whole seconds: the cable's close/detach are placed from it.
TIMEOUT="${LP_WALK_TIMEOUT:-4s}"
# Wall-clock net. The classic runs several times slower than real time while
# it is busy and FASTER than real time while it idles (the idle skip).
WALL="${LP_WALK_WALL_TIMEOUT:-900}"
OUT="${LP_WALK_OUT:-$REPO/target/lp-emu-esp32v3-walk}"

keep=0
frame_only=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep) keep=1; shift ;;
        --frame) frame_only=1; shift ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

# The boot path and the cable, after the arguments: `--frame` is the frame
# half and sets both defaults, and an explicit LP_WALK_BOOT still wins so the
# two axes can be crossed by hand when something is being bisected.
if [[ $frame_only -eq 1 ]]; then
    BOOT="${LP_WALK_BOOT:-direct}"
    CABLE="${LP_WALK_CABLE:-0}"
else
    BOOT="${LP_WALK_BOOT:-rom-up}"
    CABLE="${LP_WALK_CABLE:-1}"
fi
case "$BOOT" in
    rom-up | direct) ;;
    *) echo "LP_WALK_BOOT='$BOOT' — expected rom-up or direct" >&2; exit 2 ;;
esac

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
# `scripts/m4-hardware-walk.sh:391-417`, kept in step with it. The copy keeps
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

# The bytes a flasher writes, when the walk is booting the way one does. The
# recipe is `build-merged-image.sh`'s and not this script's: it pins
# espflash 3.3.0, because a different espflash bundles a different
# second-stage bootloader and the boot log would then be comparing two
# programs.
boot_args=()
case "$BOOT" in
    rom-up)
        echo "==> building the merged 4 MiB image (the bytes a flasher writes)"
        scripts/emu/build-merged-image.sh --chip esp32 "$elf" "$OUT/merged.bin"
        boot_args=(--merged "$OUT/merged.bin")
        ;;
    direct)
        boot_args=(--elf "$elf")
        ;;
esac
cable_script="$OUT/walk.cable.script"
if [[ "$CABLE" != 0 ]]; then
    # The cable's schedule in EMULATED milliseconds. The reset lands 1 ms in,
    # before the first boot has brought a link up, so the boot everything
    # below watches is the one AFTER it — as on a desk, where a flasher
    # resets the board before it talks. close/detach land after the upload
    # and the deferred dump, before the deadline.
    if [[ ! "$TIMEOUT" =~ ^[0-9]+s$ ]]; then
        echo "LP_WALK_TIMEOUT='$TIMEOUT' — the cable's schedule needs whole seconds (e.g. 6s)" >&2
        exit 2
    fi
    deadline_ms=$(( ${TIMEOUT%s} * 1000 ))
    cat >"$cable_script" <<CABLE
# The CH340 cable's verbs, in lp-emu-esp32v3's --control-script grammar.
0 attach
1 reset
2 open
$(( deadline_ms - 200 )) close
$(( deadline_ms - 100 )) detach
CABLE
    boot_args+=(--control-script "$cable_script")
fi

# Release, and the expensive step on a cold cache — minutes, not seconds. It
# has to be: the emulator's own interpreter loop is linked into this binary,
# and a debug build of it takes tens of minutes to reach a render.
echo "==> building lp-cli (release)"
cargo build --quiet --release -p lp-cli
cli="$REPO/target/release/lp-cli"

# ------------------------------------------------ the machine, hosted and fed
echo "==> lp-cli emu run --chip esp32v3 ($BOOT boot, cable=$CABLE, ${TIMEOUT} emulated), hosting UART0's link and uploading $UPLOAD_DIR"
set +e
"$cli" emu run \
    --chip esp32v3 \
    "${boot_args[@]}" \
    --host-link \
    --upload "$UPLOAD_DIR" \
    --console "$console" \
    --dump-frames "$frames" \
    --strict-bus \
    --timeout "$TIMEOUT" \
    --wall-timeout "$WALL" \
    >"$OUT/emu.stdout" 2>"$OUT/emu.stderr"
emu_status=$?
set -e

# ---------------------------------------------------------------- the upload
echo
echo "===== UPLOAD ====="
grep -a -m 1 "uploaded and running" "$OUT/emu.stderr" || true
upload_failed=0
if ! grep -qa "uploaded and running" "$OUT/emu.stderr"; then
    echo "upload: FAILED" >&2
    tail -30 "$OUT/emu.stderr" >&2
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
if [[ $emu_status -ne 0 ]]; then
    echo "NOTE: the machine did not end at its deadline (exit $emu_status)." >&2
    grep -m 8 -aE "STRICT|Fault|stopped|wall-clock" "$OUT/emu.stderr" >&2 || true
    echo "      A strict-bus stop is the machine refusing an access the guest made. The" >&2
    echo "      crate README's \"The window, across a context save\" is the last one of" >&2
    echo "      these to be diagnosed (M4 P4b, PR #711) and shows how." >&2
fi

echo
echo "===== THE RUN ====="
grep -m 12 -a "^emu: " "$OUT/emu.stderr" || true
# The link's own account: every message the host read came whole, and nothing
# reached the app damaged. Reading (a) below arrived over this link.
link_report="$(grep -a -m 1 "^emu: host link" "$OUT/emu.stderr" || true)"
link_failed=0
if [[ "$link_report" != *" 0 link error(s)"* ]]; then
    echo "FAIL: the host link reported errors — see the line above." >&2
    link_failed=1
fi

# ------------------------------------------------- the cable, released again
#
# The cable's state at the end of the run, in the `state` verb's own words —
# what a hardware walk does with `release_port` and a human's fingers, and the
# half of it a human's fingers do is the half nobody writes down. ASSERTED
# rather than printed and admired.
cable_failed=0
if [[ "$CABLE" != 0 ]]; then
    echo
    echo "===== CABLE (released) ====="
    grep -v '^#' "$cable_script" | sed 's/^/  /' || true
    released="$(grep -a -m 1 "^emu: cable: " "$OUT/emu.stderr" || true)"
    echo "  $released"
    # `cable=absent`, which is the machine's own word for a detached cable
    # (`ControlReply::State`), not "detached".
    for want in "port=closed" "cable=absent" "dtr=0" "rts=0" "reboots=1"; do
        if [[ "$released" != *"$want"* ]]; then
            echo "FAIL: the cable's final state has no '$want':"
            echo "  $released"
            cable_failed=1
        fi
    done
    if [[ $cable_failed -eq 0 ]]; then
        echo "PASS: the port is released, the lines are slack, and the cable"
        echo "      rebooted the chip exactly once."
    fi
fi

echo
echo "===== DEVICE ====="
# `does not produce` is in the filter on purpose, exactly as in the hardware
# walk: its ABSENCE is the signal that something feeds the output node.
#
# `grep -m 40`, not `| head -40`: under `pipefail` a `head` that closes the pipe
# kills `grep` with SIGPIPE and the whole pipeline reports 141, so the walk
# would die HERE, at a progress print, on a run that was going perfectly.
#
# `cut -c1-600` because `Project` is in the filter and the upload's own
# `projectRead` reply is a single ~30,000-character line of wire JSON: without
# it one line buries the section. 600 keeps an `[OUT] dump` whole — its
# prefix is ~100 characters and its `rgb=` is 384 — which is the one line here
# that has to survive intact.
grep -m 40 -aE "boot:|ESP-ROM|INIT|RECOVERY|Project|compilation|\[OUT\]|ERROR|does not produce" \
    "$console" | cut -c1-600 || true

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
# A dump is several `part=i/n` lines (the three chips share one
# `frame_dump`, and on the lp-link chips a log record is cut at 200 bytes);
# scripts/frame-dump-hex.sh joins the last whole one.
device_hex="$("$REPO/scripts/frame-dump-hex.sh" < "$console")"
dumps="$("$REPO/scripts/frame-dump-hex.sh" --count < "$console")"

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
fail=$(( upload_failed | cable_failed | link_failed ))

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
echo "artefacts: $OUT"
if [[ $keep -eq 0 && $fail -eq 0 ]]; then
    rm -f "$OUT/emu.stdout"
fi
exit "$fail"
