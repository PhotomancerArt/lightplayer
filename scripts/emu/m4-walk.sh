#!/usr/bin/env bash
# The hardware walk, with the emulator where the board goes.
#
#   scripts/emu/m4-walk.sh              # `just walk-esp32c6-emu`
#   scripts/emu/m4-walk.sh --keep       # leave the artefacts for inspection
#
# `scripts/m4-hardware-walk.sh` asks one question of a device: does the shader
# it compiled and executed on its own JIT render the same bytes a host render
# produces? This asks the same question of `lp-emu-esp32c6`, on the same image
# bytes, over the same wire protocol, against the same oracle — and it asks it
# TWICE, because the emulator can answer in two independent ways where a board
# has only one:
#
#   [OUT] dump   the firmware's own record of the frame it handed the WS281x
#                driver, printed over the serial link. Byte for byte the line
#                a real C6 prints (`frame-dump`), read by the same parser.
#   the pad      the WS281x waveform the RMT model actually produced, decoded
#                back at the datasheet's ±150 ns by a decoder that never spoke
#                to the firmware (M5).
#
# The first says what the render produced. The second says what left the chip.
# A board can only be asked the first; the walk record's whole claim rests on
# the two agreeing here and both agreeing with the oracle.
#
# ## What this is NOT
#
# It is not a replacement for holding a board. The emulator has no Chromium
# USB stack, no radio traffic, no analog anything, and its clock is a model —
# the walk record (`docs/reports/2026-09-08-esp32c6-emulator-walk.md`) lists
# what it does not cover, and nothing here should be read as covering it.
#
# ## Why one run where the hardware walk needs two flashes
#
# The hardware walk flashes twice because espflash's `--monitor` HOLDS the
# port, so `lp-cli` cannot open it at the same time; round 1 pushes the
# project, round 2 reflashes to watch it render. The emulator's link is a
# socket that the machine itself serves, so one run uploads AND watches. The
# project surviving a reboot — the thing the second flash also happens to
# prove — is `lp-emu/esp/lp-emu-esp32c6/tests/flash_persistence.rs`'s gate,
# not this walk's.
#
# ## The boot path
#
# ROM-up from a merged 4 MiB image, the same bytes espflash would write: the
# hart starts at the reset vector, the real mask ROM finds the ESP-IDF
# second-stage bootloader in flash, and the bootloader hashes and loads the
# app (M7). `LP_WALK_BOOT=direct` takes the faster direct load instead, which
# M7 proved reaches a byte-equal state at app entry. ROM-up is the default
# because this script's whole point is to be the twin of one that flashes and
# resets a board.
set -euo pipefail

cd "$(dirname "$0")/../.."
REPO="$PWD"

PROJECT="${PROJECT:-projects/test/shader-oracle}"
# The C6's `D10` is gpio18 on the XIAO manifest, which is what the oracle
# project already names — so, unlike the classic, no retarget.
PAD="${LP_WALK_PAD:-18}"
LINK="${LP_WALK_LINK:-127.0.0.1:5597}"
# EMULATED time, and the walk's whole cost: every microsecond of it is
# instructions this host has to interpret. The budget, measured: the ROM and
# bootloader reach `[INIT]` at ~0.5 s, the upload lands its project at ~1.4 s,
# the shader compiles one frame later, and `frame_dump` then waits 30 frames
# (~0.2 s at the ~5.5 ms/frame this project renders at) before the deferred
# lit dump it defers on purpose — a dump printed into the post-compile log
# burst is dropped end to end. So everything the gate needs has happened by
# ~2 s, and the rest is the margin that makes "every later frame is the same
# frame" mean something. 8 s leaves ~1,000 frames after the dump and costs
# about a billion emulated instructions; raise it with LP_WALK_TIMEOUT if you
# want a longer soak, and expect the wall clock to move with it.
TIMEOUT="${LP_WALK_TIMEOUT:-8s}"
# Wall-clock net. Emulated time runs several times slower than real time here.
WALL="${LP_WALK_WALL_TIMEOUT:-900}"
BOOT="${LP_WALK_BOOT:-rom-up}"
OUT="${LP_WALK_OUT:-$REPO/target/lp-emu-c6-walk}"

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

# ------------------------------------------------- the image, and its bytes
#
# The CURRENT tree's shipped feature set plus `frame-dump`, exactly as the
# hardware walk flashes the current tree plus the same feature. Built from the
# crate directory because its own `.cargo/config.toml` carries the linker
# script.
echo "==> building fw-esp32c6 (esp32c6,server,radio + frame-dump)"
( cd lp-fw/fw-esp32c6 && cargo build --quiet \
    --target riscv32imac-unknown-none-elf --profile release-esp32 \
    --features esp32c6,frame-dump )
built="$REPO/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6"
[[ -f "$built" ]] || { echo "the build reported success but $built is missing" >&2; exit 1; }
# A copy under our own name: `target/<triple>/release-esp32/fw-esp32c6` is
# where EVERY feature set of this crate builds to, so whatever is there is
# whatever was built last, and a walk that read it could be reading a build
# with no readout in it at all.
elf="$OUT/fw-esp32c6"
cp "$built" "$elf"

# The readout has to be IN the image. Without this check a default build would
# render perfectly, print nothing, and the walk would report "nothing
# rendered" — which is the S3 walk's own documented trap, one level earlier.
# `grep -a`, not `grep -qa`: under `pipefail`, a `-q` grep closes the pipe on
# its first hit, `strings` dies of SIGPIPE, and the PIPELINE reports 141 — so
# the check would fail loudest exactly when it passed.
if ! strings "$elf" | grep -a '\[OUT\] dump frame=' >/dev/null; then
    echo "FAIL: the built image carries no frame-dump readout." >&2
    echo "      Did the 'frame-dump' feature stop reaching the driver's write path?" >&2
    exit 1
fi

boot_args=()
case "$BOOT" in
    rom-up)
        echo "==> building the merged 4 MiB image (the bytes a flasher writes)"
        scripts/emu/build-merged-image.sh "$elf" "$OUT/merged.bin"
        boot_args=(--merged "$OUT/merged.bin")
        ;;
    direct)
        boot_args=(--elf "$elf")
        ;;
    *)
        echo "LP_WALK_BOOT='$BOOT' — expected rom-up or direct" >&2
        exit 2
        ;;
esac

# Release, and it is the expensive step of the walk on a cold cache — minutes,
# not seconds. It has to be: the emulator's interpreter loop IS this binary,
# and a debug build of it runs the emulated seconds below at a speed nobody
# will wait for. The same binary serves the link and drives the upload,
# so one build covers both halves of the walk.
echo "==> building lp-cli (release — the emulator's own interpreter loop)"
cargo build --quiet --release -p lp-cli
cli="$REPO/target/release/lp-cli"

# ---------------------------------------------------- the machine, listening
cleanup() {
    [[ -n "${emu_pid:-}" ]] && kill "$emu_pid" 2>/dev/null || true
}
trap cleanup EXIT

echo "==> lp-cli emu run ($BOOT boot, ${TIMEOUT} emulated) on $LINK"
"$cli" emu run \
    "${boot_args[@]}" \
    --link "$LINK" \
    --link-kind usb \
    --monitor \
    --time-grade t1 \
    --timeout "$TIMEOUT" \
    --wall-timeout "$WALL" \
    --console "$console" \
    --dump-frames "$frames" \
    >"$OUT/emu.stdout" 2>"$OUT/emu.stderr" &
emu_pid=$!

# The machine listens as soon as it is built, before the ROM has run a single
# instruction; wait for the port rather than for a log line, so this works the
# same on both boot paths.
# bash's own /dev/tcp rather than `nc`, whose flags differ between the BSD one
# macOS ships and the GNU one a CI runner has.
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
"$cli" upload "$PROJECT" "serial:tcp://$LINK" --wait-timeout "${LP_WALK_CLI_TIMEOUT:-600}" \
    >"$OUT/cli.stdout" 2>"$OUT/cli.stderr"
cli_status=$?
set -e
tail -5 "$OUT/cli.stdout" || true
if [[ $cli_status -ne 0 ]]; then
    echo "upload: FAILED (exit $cli_status)" >&2
    tail -30 "$OUT/cli.stderr" >&2
    exit 1
fi
echo "upload: OK"

# Let the guest render on past the upload to its deferred lit dump, then let
# the machine reach its own emulated deadline rather than killing it — a
# killed run has no report and no flushed frames.
echo
echo "==> letting the machine run on to its emulated deadline"
wait "$emu_pid" && emu_status=0 || emu_status=$?
emu_pid=
if [[ $emu_status -ne 0 ]]; then
    echo "FAIL: the machine did not end cleanly (exit $emu_status)." >&2
    tail -20 "$OUT/emu.stderr" >&2
    exit 1
fi

echo
echo "===== DEVICE ====="
# `does not produce` is in the filter on purpose, exactly as in the hardware
# walk: its ABSENCE is the signal that something feeds the output node.
#
# `grep -m 40`, not `| head -40`: under `pipefail` a `head` that closes the
# pipe kills `grep` with SIGPIPE and the whole pipeline reports 141, so the
# walk would die HERE, at a progress print, on a run that was going perfectly
# — which is exactly what the first end-to-end run did. `-m` stops grep
# itself, so there is no pipe to break.
grep -m 40 -aE "boot:|ESP-ROM|INIT|RECOVERY|Project|compilation|\[OUT\]|ERROR|does not produce" \
    "$console"

echo
echo "===== ORACLE ====="
# The same command the hardware walk runs, and the same two engines. On this
# chip `[ORACLE-RV32]` is not merely a triage aid: it is `lpvm-native`'s rv32
# code generator, which is the code generator the C6 itself JITs, on the C6's
# own ISA.
#
# `set +e` around it deliberately: a failing `cargo test` inside a command
# substitution would kill this script at the ASSIGNMENT under `set -e`, and
# the walk would end with an empty ORACLE section and no reason — which is
# what the first run of this script did. The guard below is the reason, and
# it only gets to speak if the assignment survives.
set +e
oracle_out="$(cargo test -q -p lpa-server --test shader_oracle_frame -- --nocapture 2>"$OUT/oracle.stderr")"
oracle_status=$?
set -e
if [[ $oracle_status -ne 0 ]] || ! echo "$oracle_out" | grep -a "\[ORACLE" >/dev/null; then
    echo "FAIL: the host oracle did not run (exit $oracle_status)." >&2
    echo "      It needs the rv32 builtins image — 'just build-rv32-builtins' (or the" >&2
    echo "      whole 'just ci-prereqs'). 'just walk-esp32c6-emu' depends on it; a bare" >&2
    echo "      call to this script does not." >&2
    tail -20 "$OUT/oracle.stderr" >&2
    exit 1
fi
echo "$oracle_out" | grep -a "\[ORACLE"

echo
echo "===== COMPARISON ====="
# `grep -m 1`, never `| head -1`: see the DEVICE section above for what a
# closed pipe does to this script.
hex_of() { echo "$1" | grep -m 1 -a "^\[$2\] rgb=" | cut -d= -f2; }
oracle_hex="$(hex_of "$oracle_out" ORACLE)"
rv32_hex="$(hex_of "$oracle_out" ORACLE-RV32)"

# The LAST dump, not the first — the same rule and the same reason as the
# hardware walk. The first frame after a project load is the compile-window
# black fallback (ADR 2026-08-03-memory-pressure-at-compile-safe-points), and
# `frame_dump` dumps it at open before re-arming for the first lit frame.
device_hex="$(grep -ao 'rgb=[0-9a-f]*' "$console" | tail -1 | cut -d= -f2)"
dumps="$(grep -ac '\[OUT\] dump frame=' "$console" || true)"

# The trap this walk found the first time it ran, and the reason `emu run`
# grew `--monitor`: the deferred lit dump comes THIRTY frames after the first
# lit one, and if the only reader has meanwhile disconnected, the guest's
# output after that point goes nowhere. The console then holds exactly one
# dump — the open-time black frame — and the naive comparison blames the RMT
# for a render that was perfectly correct. Name it instead.
if [[ "$dumps" == "1" && "$device_hex" =~ ^0+$ ]]; then
    echo "FAIL: the only frame dump in the console is the open-time black frame." >&2
    echo "      That is the compile-window fallback; the deferred lit dump fires 30" >&2
    echo "      frames later. Either nothing lit (the DEVICE section says so), or the" >&2
    echo "      console stopped — check that 'lp-cli emu run --monitor' is still on the" >&2
    echo "      command line above, since without it a client disconnecting closes the" >&2
    echo "      port and the guest talks to nobody." >&2
    exit 1
fi

# The pad's answer: the FIRST lit frame, which is the same frame by a
# different route — the deferred dump above is 30 frames later, and the
# project is clock-free, so every lit frame is the same bytes. That identity
# is asserted below rather than assumed.
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
    echo "      and try again. (A missing builtins image renders black, and a" >&2
    echo "      black host frame is not an oracle.)" >&2
    exit 1
fi
if [[ -z "$device_hex" ]]; then
    echo "FAIL: the guest printed no frame dump — nothing rendered." >&2
    echo "      The DEVICE section above says why; the image was checked for the" >&2
    echo "      readout before the run, so this is not a missing feature." >&2
    exit 1
fi
if [[ -z "$pad_hex" ]]; then
    echo "FAIL: no lit frame reached pad $PAD — the render never left the chip." >&2
    echo "      $pad_frames frame(s) were decoded there. The firmware's own dump" >&2
    echo "      says '$device_hex', so this is the RMT or the routing, not the render." >&2
    exit 1
fi
if [[ "$distinct_lit" != "1" ]]; then
    echo "FAIL: $distinct_lit distinct lit frames on pad $PAD, for a clock-free project." >&2
    echo "      The render is not a function of the project alone; comparing any one" >&2
    echo "      of them to the oracle would be luck." >&2
    exit 1
fi

fail=0
if [[ "$device_hex" != "$pad_hex" ]]; then
    echo "FAIL: the firmware's dump and the pad disagree."
    echo "  [OUT] dump: $device_hex"
    echo "  pad $PAD:      $pad_hex"
    echo "  TRIAGE: the render is one thing and the wire another — the RMT encode,"
    echo "          the colour order, or the refill path. This is the comparison a"
    echo "          board cannot make, and the reason this walk exists."
    fail=1
fi
if [[ "$device_hex" != "$oracle_hex" ]]; then
    echo "FAIL: guest and wasmtime frames differ."
    echo "  guest:    $device_hex"
    echo "  wasmtime: $oracle_hex"
    echo "  rv32-emu: $rv32_hex"
    if [[ "$device_hex" == "$rv32_hex" ]]; then
        echo "  TRIAGE: the guest agrees with rv32-emu, which on this chip is the SAME"
        echo "          code generator on the SAME ISA. The guest is right and the"
        echo "          finding is wasmtime's — start at lpvm-native, not at the machine."
    else
        echo "  TRIAGE: the guest agrees with NEITHER host engine. rv32-emu runs the"
        echo "          guest's own code generator on its own ISA, so this is the"
        echo "          MACHINE — the JIT's install path, the cache, or a peripheral."
    fi
    fail=1
fi

if [[ $fail -eq 0 ]]; then
    echo "PASS: the frame is byte-identical on all three readings (${#device_hex} hex chars)."
    echo "  [OUT] dump == pad $PAD == [ORACLE] rgb"
    [[ "$device_hex" == "$rv32_hex" ]] || \
        echo "  note: rv32-emu differs from both — investigate."
fi

echo
grep -m 8 -a "^emu: " "$OUT/emu.stderr" || true
echo "artefacts: $OUT"
if [[ $keep -eq 0 && $fail -eq 0 ]]; then
    rm -f "$OUT/emu.stdout" "$OUT/cli.stdout" "$OUT/cli.stderr"
fi
exit "$fail"
