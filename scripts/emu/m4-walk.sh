#!/usr/bin/env bash
# The hardware walk, with the emulator where the board goes.
#
#   scripts/emu/m4-walk.sh                     # `just walk-esp32c6-emu`
#   scripts/emu/m4-walk.sh --chip esp32s3      # `just walk-esp32s3-emu`
#   scripts/emu/m4-walk.sh --keep              # leave the artefacts behind
#
# TWO chips, one script (M6 P10). Both have a native USB-Serial-JTAG link, the
# same generation of RMT, and a board whose `D10` pad is the one
# `projects/test/shader-oracle` already names — so the project is uploaded
# UNMODIFIED on both and the only differences are which crate is built, which
# gpio the pad is, and which runner serves the link. The CLASSIC ESP32 is not
# here: its UART0 link and its CH340's cable verbs made a separate script the
# honest shape (`scripts/emu/m4-walk-esp32v3.sh`, M5 P5 / DD69).
#
# `scripts/m4-hardware-walk.sh` asks one question of a device: does the shader
# it compiled and executed on its own JIT render the same bytes a host render
# produces? This asks the same question of the machine, on the same image
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
# ROM-up from a merged image, the same bytes espflash would write (4 MiB on
# the C6, 8 MiB on the S3 — its partition table does not fit a 4 MB part):
# the hart starts at the reset vector, the real mask ROM finds the ESP-IDF
# second-stage bootloader in flash, and the bootloader hashes and loads the
# app (M7 on the C6, M6 P06 on the S3). `LP_WALK_BOOT=direct` takes the
# faster direct load instead, which both chips proved reaches a byte-equal
# state at app entry — it skips the merged-image build and the bootloader's
# own seconds, and is the path to bisect on. ROM-up is the default because
# this script's whole point is to be the twin of one that flashes and resets
# a board.
#
# ## ⚠️ On the S3 this walk is the milestone's only end-to-end exercise of
# ## D2's alias
#
# The oracle project compiles a shader ON THE DEVICE. On the S3 that shader
# is written through the D-bus and executed through the I-bus — SRAM1 is
# mapped twice, and `AliasRule::Offset` in the machine's board is what makes
# the two views one memory. Every other test on this chip writes and reads
# through one view. If this walk passes, the alias is right; if the alias is
# wrong, this walk is where it shows, as a shader that compiles and then
# renders the wrong bytes or faults.
set -euo pipefail

cd "$(dirname "$0")/../.."
REPO="$PWD"

PROJECT="${PROJECT:-projects/test/shader-oracle}"

CHIP="${LP_WALK_CHIP:-esp32c6}"
keep=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --chip) CHIP="${2:?--chip needs a value: esp32c6 or esp32s3}"; shift 2 ;;
        --chip=*) CHIP="${1#*=}"; shift ;;
        --keep) keep=1; shift ;;
        -h|--help) sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

# ------------------------------------------------------------- the chip table
#
# Everything this script knows about a chip, and nothing it can derive. The
# two rows are deliberately the same shape as `scripts/m4-hardware-walk.sh`'s
# table, which is the script this one is the twin of.
#
#   PAD             the gpio the board's `D10` label is, and the pad the
#                   decoder reads. `projects/test/shader-oracle` names
#                   `ws281x:local:D10` and BOTH boards have that pad — the
#                   XIAO C6's is gpio18, the XIAO S3 Plus's is gpio9 (the
#                   checked-in `seeed/xiao-esp32-s3-plus` profile says so).
#                   ⚠️ No project rewrite on either chip. The classic's walk
#                   copies the project and retargets `D10 -> IO18` because the
#                   DOM-Z-102 has no such pad; a reader coming from that
#                   script will look for the rewrite here and there is none.
#   MERGED_CHIP     what `build-merged-image.sh --chip` is told (it owns the
#                   partition table and the flash size).
#   LINK / CTRL     the byte socket and, on the S3, the cable's own channel.
#                   Different defaults per chip so two walks can run at once.
#   RV32_IS_THE_GUESTS_CODEGEN
#                   whether `[ORACLE-RV32]` is the guest's OWN code generator.
#                   On the C6 it is — same ISA, same backend — so a guest that
#                   agrees with it and not with wasmtime is diagnostic. On the
#                   S3 the guest JITs XTENSA, so rv32-emu is a third opinion
#                   and nothing more. The triage text below says which.
case "$CHIP" in
esp32c6)
    PAD="${LP_WALK_PAD:-18}"
    MERGED_CHIP="esp32c6"
    LINK="${LP_WALK_LINK:-127.0.0.1:5597}"
    OUT_DEFAULT="$REPO/target/lp-emu-c6-walk"
    RV32_IS_THE_GUESTS_CODEGEN=1
    ;;
esp32s3)
    PAD="${LP_WALK_PAD:-9}"
    MERGED_CHIP="esp32s3"
    LINK="${LP_WALK_LINK:-127.0.0.1:5598}"
    CTRL="${LP_WALK_CTRL:-127.0.0.1:5618}"
    OUT_DEFAULT="$REPO/target/lp-emu-esp32s3-walk"
    RV32_IS_THE_GUESTS_CODEGEN=0
    ;;
*)
    echo "--chip '$CHIP': this walk knows esp32c6 and esp32s3." >&2
    echo "  The classic ESP32 has its own script: scripts/emu/m4-walk-esp32v3.sh" >&2
    exit 2
    ;;
esac

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
#
# The same 8 s on the S3, and the shape of the budget is the same: ROM +
# bootloader reach the app entry at ~245 ms of guest time (M6 P06's measured
# figure), the upload follows, and `frame_dump`'s deferred lit dump comes 30
# frames after the first lit one. What differs is only the WALL clock — this
# is an Xtensa interpreter, and an S3 second costs more host time than a C6
# second.
TIMEOUT="${LP_WALK_TIMEOUT:-8s}"
# Wall-clock net. Emulated time runs several times slower than real time here.
WALL="${LP_WALK_WALL_TIMEOUT:-900}"
BOOT="${LP_WALK_BOOT:-rom-up}"
OUT="${LP_WALK_OUT:-$OUT_DEFAULT}"

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
case "$CHIP" in
esp32c6)
    echo "==> building fw-esp32c6 (esp32c6,server,radio + frame-dump)"
    ( cd lp-fw/fw-esp32c6 && cargo build --quiet \
        --target riscv32imac-unknown-none-elf --profile release-esp32 \
        --features esp32c6,frame-dump )
    built="$REPO/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6"
    ;;
esp32s3)
    # Through the justfile recipe, and not `cd … && cargo build` like the C6's:
    # that recipe owns the Xtensa GCC linker's PATH (`just _xt-gcc-dir`, an
    # espup install the host toolchain knows nothing about), which this script
    # must not duplicate. `frame-dump` DECORATES this crate's defaults
    # (`esp32s3,server,float-f32`) rather than replacing them — the recipe's
    # own comment says so — so the image is the shipped one plus the readout,
    # which is exactly what `just flash-fw-esp32s3 <port> frame-dump` writes to
    # the board in the hardware walk.
    echo "==> building fw-esp32s3 (defaults + frame-dump)"
    just build-fw-esp32s3 frame-dump
    built="$REPO/target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3"
    ;;
esac
[[ -f "$built" ]] || { echo "the build reported success but $built is missing" >&2; exit 1; }
# A copy under our own name: `target/<triple>/<profile>/fw-<chip>` is where
# EVERY feature set of that crate builds to, so whatever is there is whatever
# was built last, and a walk that read it could be reading a build with no
# readout in it at all.
elf="$OUT/$(basename "$built")"
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
        echo "==> building the merged image for $MERGED_CHIP (the bytes a flasher writes)"
        scripts/emu/build-merged-image.sh --chip "$MERGED_CHIP" "$elf" "$OUT/merged.bin"
        # `lp-cli emu run` (the C6) infers rom-up from `--merged`; the S3
        # machine binary asks for the boot mode by name, and says so if the
        # two disagree.
        if [[ "$CHIP" == "esp32s3" ]]; then
            boot_args=(--boot-mode rom-up --merged "$OUT/merged.bin")
        else
            boot_args=(--merged "$OUT/merged.bin")
        fi
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
# will wait for. On the C6 one binary serves the link and drives the upload;
# on the S3 the machine is its own binary, because `lp-cli emu run` knows one
# chip and teaching it a second is M8's — so that arm builds both.
case "$CHIP" in
esp32c6)
    echo "==> building lp-cli (release — the emulator's own interpreter loop)"
    cargo build --quiet --release -p lp-cli
    ;;
esp32s3)
    echo "==> building lp-cli and lp-emu-esp32s3 (release)"
    cargo build --quiet --release -p lp-cli -p lp-emu-esp32s3
    emu="$REPO/target/release/lp-emu-esp32s3"
    ;;
esac
cli="$REPO/target/release/lp-cli"

# ---------------------------------------------------- the machine, listening
cleanup() {
    [[ -n "${emu_pid:-}" ]] && kill "$emu_pid" 2>/dev/null || true
}
trap cleanup EXIT

case "$CHIP" in
esp32c6)
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
    ;;
esp32s3)
    # `--usb-host attached`, and it is not a convenience: this chip's console
    # IS the link, and with no host at power-on the first packet commits,
    # `free` never comes back, esp-println latches TIMED_OUT and the console
    # falls silent for the rest of the run. A XIAO S3 is powered THROUGH the
    # cable, so "a board running with no host attached" is not a state the
    # hardware walk can be in either.
    #
    # `--usb-sj-drain manual` decouples the port from the byte socket, which
    # is what `--monitor` buys on the C6 arm and for exactly the same reason:
    # `lp-cli upload` disconnects when it is done, the deferred lit dump comes
    # THIRTY frames later, and a port that closed with the client would hold
    # it. Here the control channel owns `open`/`close`, the walk opens the
    # port itself, and the coupling is asserted below rather than relied on.
    #
    # `--strict-bus` is a second gate for nothing: this run's report says
    # `unmapped=0`, and a guest that started reaching addresses nothing claims
    # would stop and say where instead of rendering something plausible.
    #
    # `--core-quantum 256` is the machine's own default, named here because
    # DD110 is about it: frame-START cycles on this chip move with the quantum
    # (the guest's ISR observes the RMT threshold at slice boundaries) while
    # frame BYTES do not. This walk compares bytes and nothing else (PD9).
    echo "==> lp-emu-esp32s3 ($BOOT boot, ${TIMEOUT} emulated) on $LINK, cable on $CTRL"
    "$emu" \
        "${boot_args[@]}" \
        --usb-sj "tcp:$LINK" \
        --usb-sj-drain manual \
        --usb-host attached \
        --control "tcp:$CTRL" \
        --console "$console" \
        --dump-frames "file:$frames" \
        --strict-bus \
        --core-quantum 256 \
        --time-grade t1 \
        --timeout "$TIMEOUT" \
        --wall-timeout "$WALL" \
        >"$OUT/emu.stdout" 2>"$OUT/emu.stderr" &
    emu_pid=$!
    ;;
esac

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

# ----------------------------------------------------------------- the cable
#
# Only the S3 has one here: `lp-cli emu run --monitor` owns the C6's port from
# the inside. ONE client for the whole run, on fd 4, because the machine
# listens for one control client at a time and a verb-per-connection would
# make the walk's own reconnects part of what it is testing. Every verb is
# answered with exactly one line, so `read` once per verb is the protocol and
# not a guess.
cable_replies="$OUT/walk.cable.txt"
cable_failed=0
cable() {
    local verb="$*" reply=""
    printf '%s\n' "$verb" >&4
    # `|| true`: a read that times out returns non-zero and would take the
    # script with it under `set -e` — at the point where the reply we are
    # about to complain about would have been printed.
    IFS= read -r -t 30 reply <&4 || true
    printf '%-16s %s\n' "$verb" "$reply" | tee -a "$cable_replies"
    CABLE_REPLY="$reply"
}
cable_want() {
    local what="$1"
    shift
    for want in "$@"; do
        if [[ "$CABLE_REPLY" != *"$want"* ]]; then
            echo "FAIL: $what has no '$want':"
            echo "  $CABLE_REPLY"
            cable_failed=1
        fi
    done
}

if [[ "$CHIP" == "esp32s3" ]]; then
    : >"$cable_replies"
    echo
    echo "===== CABLE ====="
    # The probe is a SUBSHELL, and then the real connection is opened in this
    # one: a failing `exec` redirection kills a non-interactive shell outright,
    # so the reachability question is asked where the answer is cheap.
    cable_open=0
    for _ in $(seq 1 300); do
        (exec 4<>"/dev/tcp/${CTRL%%:*}/${CTRL##*:}") 2>/dev/null && { cable_open=1; break; }
        kill -0 "$emu_pid" 2>/dev/null || break
        sleep 0.1
    done
    if [[ $cable_open -eq 0 ]]; then
        echo "FAIL: the machine never served the control socket on $CTRL." >&2
        tail -20 "$OUT/emu.stderr" >&2
        exit 1
    fi
    exec 4<>"/dev/tcp/${CTRL%%:*}/${CTRL##*:}"
    # ⚠️ This is NOT the classic's `attach; reset; open` dance, and the
    # difference is the chip, not a simplification. There is no CH340 and no
    # auto-reset truth table here — this link IS the chip — and the board is
    # powered THROUGH the cable, so a host attached from power-on is the only
    # state a XIAO S3 walk can start in. `--usb-host attached` says that on the
    # command line; this asks the machine to confirm it, which is the one thing
    # a run cannot get wrong silently: with the host absent the console would
    # simply be empty and every failure below would blame the render.
    cable state
    cable_want "the host's state at power-on" "host=attached" "draining=true" "sof=on"
fi

# ---------------------------------------------------------------- the upload
#
# ⚠️ **`--no-wait` on the S3, and it is the open link defect rather than a
# shortcut.** `lp-cli upload`'s post-deploy wait polls `projectRead`, whose
# reply is a multi-frame stream of the whole shape registry — tens of
# kilobytes. On this chip the link model drops one 64-byte packet whenever a
# framed write follows an `esp-println` packet inside the IN drain latency
# (docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-
# stale-serial-in-empty.md, DD103, still open — which side is wrong is
# P09's measurement), so that stream arrives with a frame missing and
# `lp-cli` reports `expected project read frame seq 0, got 1`. The DEPLOY
# itself is unaffected and the console proves it: `Project loaded`,
# `compilation succeeded`, `[OUT] open`.
#
# So this arm asks for the deploy ack and takes its "is it running?" evidence
# from somewhere better than a reply the link is known to mangle: LIT FRAMES
# ON THE PAD, counted below. That is the thing the walk is about, and a run
# where the project did not start has none of them and says so by name.
#
# ⚠️ Do NOT copy this onto the C6 arm, and re-point this at the plain wait the
# day the defect closes. P07's `walks/shader-oracle.script` carries the same
# workaround one layer down (it waits on `Stopped all projects` rather than on
# the reply's bytes) and the same instruction.
upload_args=(--wait-timeout "${LP_WALK_CLI_TIMEOUT:-600}")
[[ "$CHIP" == "esp32s3" ]] && upload_args=(--no-wait)

echo
echo "===== UPLOAD ====="
set +e
"$cli" upload "$PROJECT" "serial:tcp://$LINK" "${upload_args[@]}" \
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

# ------------------------------------------------- the cable, released again
#
# The upload's client is gone. On the C6 that changes nothing, because
# `--monitor` holds the port open from inside the runner; on the S3 the same
# job is `--usb-sj-drain manual`, and the reply below is the evidence that it
# did it — a port that had closed with the client would HOLD the deferred lit
# dump (thirty frames after the first lit one) and this walk would report a
# render that never printed.
#
# Then the cable really does come out, which is the half of a hardware walk a
# human's fingers do and nobody writes down. It is done HERE, once the pad has
# carried well past the deferred dump, rather than at the end: the machine
# ends at its own emulated deadline and there is nobody left to ask. What it
# proves is that the guest renders on with no host at all — which is what an
# installed light does for the rest of its life.
if [[ "$CHIP" == "esp32s3" ]]; then
    echo
    echo "===== CABLE (released) ====="
    cable state
    cable_want "the port after the upload client left" "host=attached" "draining=true"

    # Wait for the pad to carry past the deferred dump before unplugging. The
    # frames file is written as the run goes (a BufWriter, so it arrives in
    # batches); the CONSOLE is not — it is written when the run ends — so this
    # is the only live view of progress the walk has. Bounded, and a run that
    # never lights up falls through to the checks below, which say so by name.
    lit_target="${LP_WALK_LIT_BEFORE_UNPLUG:-70}"
    lit_now=0
    for _ in $(seq 1 600); do
        lit_now="$(jq -r --argjson pad "$PAD" -s '
            [ .[] | select(.kind == "ws281x-frame" and .pad == $pad and .complete
                           and (.rgb | test("[1-9a-f]"))) ] | length' "$frames" 2>/dev/null || echo 0)"
        [[ "$lit_now" -ge "$lit_target" ]] && break
        kill -0 "$emu_pid" 2>/dev/null || break
        sleep 0.5
    done
    echo "  $lit_now lit frame(s) on pad $PAD before the cable comes out (wanted $lit_target)"

    cable close
    cable detach
    cable state
    cable_want "the released cable" "host=absent" "draining=false"
    exec 4<&- || true
    if [[ $cable_failed -eq 0 ]]; then
        echo "PASS: the port stayed open after the client left, and the cable is out."
    fi
fi

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
# The same command the hardware walk runs, and the same two engines.
#
# ⚠️ What `[ORACLE-RV32]` MEANS depends on the chip, and getting this wrong
# sends a triage the wrong way. On the C6 it is not merely a second opinion:
# it is `lpvm-native`'s rv32 code generator, which is the code generator the
# C6 itself JITs, on the C6's own ISA — so a guest that agrees with it and not
# with wasmtime has an engine finding, not a machine finding. On the S3 the
# guest JITs **Xtensa**, so rv32-emu shares neither the ISA nor the backend
# with it and agreement is worth no more than wasmtime's.
# `scripts/m4-hardware-walk.sh:377-390` says the same thing to a human holding
# the board.
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

fail="$cable_failed"
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
    if [[ "$device_hex" == "$rv32_hex" && "$RV32_IS_THE_GUESTS_CODEGEN" == 1 ]]; then
        echo "  TRIAGE: the guest agrees with rv32-emu, which on this chip is the SAME"
        echo "          code generator on the SAME ISA. The guest is right and the"
        echo "          finding is wasmtime's — start at lpvm-native, not at the machine."
    elif [[ "$RV32_IS_THE_GUESTS_CODEGEN" == 1 ]]; then
        echo "  TRIAGE: the guest agrees with NEITHER host engine. rv32-emu runs the"
        echo "          guest's own code generator on its own ISA, so this is the"
        echo "          MACHINE — the JIT's install path, the cache, or a peripheral."
    elif [[ "$device_hex" == "$rv32_hex" ]]; then
        echo "  TRIAGE: the guest agrees with rv32-emu — but on THIS chip the guest"
        echo "          JITs Xtensa, so rv32-emu is neither its ISA nor its backend and"
        echo "          the agreement is only two engines out of three. Two host engines"
        echo "          differing is the q32 last-bit question"
        echo "          (docs/defects/2026-07-30-q32-native-vs-wasmtime-last-bit.md),"
        echo "          not a verdict on the machine. Compare the Xtensa JIT's own"
        echo "          output (lp-xt) before touching the emulator."
    else
        echo "  TRIAGE: the guest agrees with NEITHER host engine, and on this chip"
        echo "          neither of them shares its ISA — so this says 'the device and"
        echo "          the hosts differ' and nothing finer. Start at the Xtensa JIT"
        echo "          (lp-xt) and the shader it compiled on device, then the machine's"
        echo "          I-bus/D-bus alias, which is what this walk is the only"
        echo "          end-to-end exercise of."
    fi
    fail=1
fi

if [[ "$device_hex" == "$pad_hex" && "$device_hex" == "$oracle_hex" ]]; then
    echo "PASS: the frame is byte-identical on all three readings (${#device_hex} hex chars)."
    echo "  [OUT] dump == pad $PAD == [ORACLE] rgb"
    if [[ "$device_hex" != "$rv32_hex" ]]; then
        if [[ "$RV32_IS_THE_GUESTS_CODEGEN" == 1 ]]; then
            echo "  note: rv32-emu differs from both — investigate."
        else
            echo "  note: rv32-emu differs from both. On this chip it is a third engine on a"
            echo "        third ISA, so this is the q32 last-bit question rather than a"
            echo "        finding about the device."
        fi
    fi
fi

echo
echo "===== THE RUN ====="
case "$CHIP" in
esp32c6)
    grep -m 8 -a "^emu: " "$OUT/emu.stderr" || true
    ;;
esp32s3)
    # This machine prints its report on STDOUT, not stderr, and the `run:`
    # line is the one a gate reads.
    grep -m 8 -aE "^(usb-sj|control|flash|uart0|run):" "$OUT/emu.stdout" || true
    # `unmapped=0` is the walk's third gate, beside the bytes: a run that
    # reached an address nothing claims rendered its frame past a hole in the
    # map, and the frame being right anyway is luck rather than evidence.
    # Under `--strict-bus` the machine would have stopped first — this is what
    # says so in the transcript the PR body carries.
    if grep -qa 'unmapped=0 ' "$OUT/emu.stdout"; then
        echo "PASS: unmapped=0 — every access the guest made landed on a block that claims it."
    else
        echo "FAIL: the run summary does not say unmapped=0."
        grep -m 1 -a '^run: ' "$OUT/emu.stdout" || true
        fail=1
    fi
    ;;
esac
echo "artefacts: $OUT"
if [[ $keep -eq 0 && $fail -eq 0 ]]; then
    rm -f "$OUT/emu.stdout" "$OUT/cli.stdout" "$OUT/cli.stderr"
fi
exit "$fail"
