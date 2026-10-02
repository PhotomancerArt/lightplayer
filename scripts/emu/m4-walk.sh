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
# The hardware walk flashes twice and hosts the board's link in two separate
# processes, one at a time: round 1 flashes and `lp-cli upload`s the project,
# round 2 reflashes so the board boots into it while `lp-cli link capture`
# hosts the link and reads the dump. This walk needs one run on
# either chip: `lp-cli emu run --host-link --upload` (since the image went
# onto lp-link at wire proto 30, and M8 taught the same door the S3) makes
# this process the host on the board's own link, in process — one run
# uploads over the link and keeps hosting it, so the board's log lines — the
# `[OUT] dump` among them — keep leaving the board after the upload is done.
# The project surviving a reboot — the thing the second flash also happens to
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
    OUT_DEFAULT="$REPO/target/lp-emu-c6-walk"
    RV32_IS_THE_GUESTS_CODEGEN=1
    ;;
esp32s3)
    PAD="${LP_WALK_PAD:-9}"
    MERGED_CHIP="esp32s3"
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
        # `lp-cli emu run --chip esp32s3` infers rom-up from `--merged` the
        # same way the C6 does (`run_s3.rs`'s own boot-mode match) — no
        # `--boot-mode` flag here; that belonged to the standalone
        # `lp-emu-esp32s3` binary this walk no longer runs.
        boot_args=(--merged "$OUT/merged.bin")
        ;;
    direct)
        # No flash chip, and no bootloader: the fast path, for bisecting. On
        # the S3 that means no partition table either, so the firmware logs
        # `no lpfs partition in the flashed table … using memory FS` and runs
        # on the memory FS — a different allocator load from the ROM-up boot's
        # and a thing to remember before comparing figures across the two.
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
# will wait for. One binary on both chips now: `lp-cli emu run --host-link`
# hosts the link and drives the upload in process, on the C6 and the S3
# alike (M8 taught `emu run` the second chip; the standalone
# `lp-emu-esp32s3` binary stays the workshop for everything this door
# doesn't cover, but this walk isn't one of those things any more).
echo "==> building lp-cli (release — the emulator's own interpreter loop)"
cargo build --quiet --release -p lp-cli
cli="$REPO/target/release/lp-cli"

# ---------------------------------------------------- the machine, listening
cleanup() {
    [[ -n "${emu_pid:-}" ]] && kill "$emu_pid" 2>/dev/null || true
}
trap cleanup EXIT

# The simulated cable-out/cable-in a hardware walk's fingers do, and nobody
# writes down — the S3's substitute for the old `--control` TCP dance (D2,
# lp2025/2026-09-28-s3-walk-host-link): a scripted `--usb-script` (the same
# control-word grammar `--usb-script` always had, `<ms> <verb>`), because the
# hosted-run door has no live control channel to query mid-run the way the
# now-retired standalone binary did. Detach well past the deferred lit dump
# (30 frames after the first lit one — see the TIMEOUT comment above for the
# render's own timing budget), replug, and reopen, all comfortably inside
# TIMEOUT; what proves the replug worked is read back from the console and
# the frame dump afterward (the COMPARISON section below), not a live query.
cable_script=""
if [[ "$CHIP" == "esp32s3" ]]; then
    # Timed to leave the product's one 5 s-interval heartbeat (measured: link
    # up at ~0.25 s, one heartbeat at 5.00 s uptime, none before or after in
    # an 8 s run) on the FAR side of the replug — the cleanest evidence this
    # walk can read back with no live query: a mid-run unplug, then the
    # regularly-scheduled heartbeat still arriving once the cable is back.
    UNPLUG_MS="${LP_WALK_UNPLUG_MS:-1500}"
    REPLUG_MS="${LP_WALK_REPLUG_MS:-4000}"
    OPEN_MS=$((REPLUG_MS + 300))
    cable_script="$OUT/cable.usb-script"
    cat >"$cable_script" <<EOF
${UNPLUG_MS} detach
${REPLUG_MS} attach
${OPEN_MS} open
EOF
fi

case "$CHIP" in
esp32c6)
    # `--host-link`: this process is the host on the board's USB link (lp-link
    # since wire proto 30) for the whole run, and `--upload` deploys the
    # project over it once the hello arrives, exactly as `lp-cli upload` does
    # (deploy, then wait for the project to be running). A client in another
    # process would take the link's host with it when it left, and the
    # deferred lit dump thirty frames later would never leave the board —
    # the trap `--monitor` used to close on the `M!` wire. The console is
    # the DECODED one: boot text, log lines, and each wire message as `M!`.
    echo "==> lp-cli emu run ($BOOT boot, ${TIMEOUT} emulated), hosting the link and uploading $PROJECT"
    "$cli" emu run \
        "${boot_args[@]}" \
        --host-link \
        --upload "$PROJECT" \
        --time-grade t1 \
        --timeout "$TIMEOUT" \
        --wall-timeout "$WALL" \
        --console "$console" \
        --dump-frames "$frames" \
        >"$OUT/emu.stdout" 2>"$OUT/emu.stderr" &
    emu_pid=$!
    ;;
esp32s3)
    # `--host-link --chip esp32s3`: the C6 arm's shape, one chip over — since
    # M8 taught `emu run` this chip, this process is the host on the S3's
    # USB-Serial-JTAG link too, and it needs no `--usb-host` override: a
    # hosted run's own default (`run_s3.rs`) is attached and draining from
    # power-on, which is the only state a XIAO S3 walk can start in anyway —
    # this board is powered THROUGH the cable, so there is no "nothing
    # plugged in yet" moment to model.
    #
    # `--strict-bus`: a guest that reached an address nothing claims stops
    # and says where, instead of rendering something plausible.
    #
    # `--core-quantum` is left at the machine's own default (256, DD110);
    # `emu run` does not expose it and this walk has never asked for another.
    echo "==> lp-cli emu run --chip esp32s3 ($BOOT boot, ${TIMEOUT} emulated), hosting the link and uploading $PROJECT"
    "$cli" emu run \
        --chip esp32s3 \
        "${boot_args[@]}" \
        --host-link \
        --upload "$PROJECT" \
        --usb-script "$cable_script" \
        --time-grade t1 \
        --timeout "$TIMEOUT" \
        --wall-timeout "$WALL" \
        --console "$console" \
        --dump-frames "$frames" \
        --strict-bus \
        >"$OUT/emu.stdout" 2>"$OUT/emu.stderr" &
    emu_pid=$!
    ;;
esac

# ---------------------------------------------------------------- the upload
#
# One shape on both chips now: the runner hosts its own link, in process, and
# `--upload` (above) waits for the deploy to be acked and the project to
# render before the runner moves on to the rest of its emulated deadline —
# `run_hosted.rs`'s `wait_for_project_running`, 600 s of WALL budget, polling
# every 250 ms. (DD103, the S3 link defect this used to work around with
# `--no-wait` and a tried-byte stream, was fixed in firmware 2026-09-24 —
# docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-
# stale-serial-in-empty.md — five days before this walk's own ticket was
# filed citing it as still open. This run is that fix's confirmation.)
echo
echo "===== UPLOAD ====="
echo "==> the runner uploads over its own link; letting it run to its emulated deadline"
wait "$emu_pid" && emu_status=0 || emu_status=$?
emu_pid=
grep -a -m 1 "uploaded and running" "$OUT/emu.stderr" >"$OUT/cli.stdout" || true
cp "$OUT/emu.stderr" "$OUT/cli.stderr"
tail -5 "$OUT/cli.stdout" || true
upload_failed=0
if ! grep -qa "uploaded and running" "$OUT/emu.stderr"; then
    echo "upload: FAILED" >&2
    tail -30 "$OUT/cli.stderr" >&2
    exit 1
fi
echo "upload: OK"

if [[ $emu_status -ne 0 ]]; then
    echo "FAIL: the machine did not end cleanly (exit $emu_status)." >&2
    tail -20 "$OUT/emu.stderr" >&2
    exit 1
fi

echo
# Everything a reader needs to know about what the MACHINE did, before the
# frame comparison that is what the walk is for. Here rather than at the end
# because the checks below exit on the first missing reading, and a run that
# rendered nothing is exactly the run whose report is worth having.
fail=$upload_failed
echo "===== THE RUN ====="
grep -m 8 -a "^emu: " "$OUT/emu.stderr" || true

if [[ "$CHIP" == "esp32s3" ]]; then
    echo
    echo "===== CABLE (scripted) ====="
    # What proves the ${UNPLUG_MS}/${REPLUG_MS} ms detach/attach (above) both
    # happened and recovered. NOT a second hello: the wire session lives in
    # `WireLinkPort`, which this process keeps for the whole run, and a cable
    # event at the USB peripheral does not by itself reset it (measured: this
    # image sends exactly one hello and one heartbeat in an 8 s run, cable or
    # no cable — a session reset needs the ARQ layer to say so, e.g. G3-1's
    # C6 case, `lp-cli/tests/emu_usb_link_gates.rs`, where the standalone
    # `--control` dance forced a longer unplug). What this walk CAN read back
    # with no live query: the product's regularly-scheduled heartbeat (every
    # 5 s of guest uptime) still reaching the host once the cable is back —
    # the schedule above is timed so that heartbeat falls on the far side of
    # the replug — and lit frames on the pad on both sides of the unplug
    # window, proving the guest kept rendering with no host attached at all,
    # which is what an installed light does for the rest of its life.
    heartbeat_line="$(grep -m 1 -a '"msg":{"heartbeat":{' "$console" || true)"
    uptime_ms="$(echo "$heartbeat_line" | grep -oE '"uptime_ms":[0-9]+' | cut -d: -f2 || true)"
    if [[ -z "$heartbeat_line" ]]; then
        echo "FAIL: no heartbeat reached the host in this run." >&2
        fail=1
    elif [[ -z "$uptime_ms" ]]; then
        echo "FAIL: a heartbeat reached the host but its uptime_ms did not parse:" >&2
        echo "  $heartbeat_line" >&2
        fail=1
    elif [[ "$uptime_ms" -gt "$REPLUG_MS" ]]; then
        echo "PASS: a heartbeat (uptime_ms=$uptime_ms) reached the host after the scripted" \
             "replug at ${REPLUG_MS} ms."
    else
        echo "FAIL: the heartbeat's uptime_ms=$uptime_ms is not after the scripted replug" >&2
        echo "      at ${REPLUG_MS} ms — it may have been generated before the unplug" >&2
        echo "      rather than surviving it." >&2
        fail=1
    fi
    lit_before="$(jq -r --argjson pad "$PAD" --argjson us "$((UNPLUG_MS * 1000))" -s '
        [ .[] | select(.kind == "ws281x-frame" and .pad == $pad and .complete
                       and (.rgb | test("[1-9a-f]")) and .start_us < $us) ] | length' \
        "$frames" 2>/dev/null || echo 0)"
    lit_after="$(jq -r --argjson pad "$PAD" --argjson us "$((REPLUG_MS * 1000))" -s '
        [ .[] | select(.kind == "ws281x-frame" and .pad == $pad and .complete
                       and (.rgb | test("[1-9a-f]")) and .start_us > $us) ] | length' \
        "$frames" 2>/dev/null || echo 0)"
    echo "  pad $PAD: $lit_before lit frame(s) before the unplug, $lit_after after the replug"
    if [[ "$lit_before" -lt 1 || "$lit_after" -lt 1 ]]; then
        echo "FAIL: no lit frame on one side of the unplug window — the render did not" >&2
        echo "      carry on with the host detached, or never resumed after the replug." >&2
        fail=1
    fi
    link_report="$(grep -a -m 1 "^emu: host link" "$OUT/emu.stderr" || true)"
    echo "  $link_report"
    if [[ "$link_report" != *" 0 link error(s)"* ]]; then
        echo "FAIL: the host link reported errors across the unplug — see the line above." >&2
        fail=1
    fi
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
# `scripts/m4-hardware-walk.sh`'s TRIAGE lines say the same thing to a human holding
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
# A dump is several `part=i/n` lines (a log record on this link is cut at
# 200 bytes); scripts/frame-dump-hex.sh joins the last whole one.
device_hex="$("$REPO/scripts/frame-dump-hex.sh" < "$console")"
dumps="$("$REPO/scripts/frame-dump-hex.sh" --count < "$console")"

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
    echo "      console stopped — check that 'lp-cli emu run --host-link' is still on the" >&2
    echo "      command line above, since without a host on the link the board's log" >&2
    echo "      lines never leave it." >&2
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
echo "===== FRAME RATE (t1 vs t3) ====="
# REPORT ONLY. t1/t2 install no memory-cost model (`TimeGrade::memory_cost`
# is `None` for both) — a cold flash fetch costs what a warm one does, so a
# preempting thread's frame-rate cost is invisible at t1, the grade the walk
# pins above and everywhere else. t3 (`cache.rs`'s `CacheCost`, a 338-cycle
# line fill measured on silicon 2026-09-08) is the grade that charges it, and
# the io-thread spike found it reproduces about half of silicon's frame-rate
# cost on a project with a preempting link thread — see
# docs/defects/2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md.
# `shader-oracle` (this walk's project) has no such thread, so there is no
# reason to expect its t1/t3 gap to echo that number — this prints whatever
# this run measures, never a threshold (AGENTS.md: "never gate on emulated
# microseconds"). It can only fail the walk if the t3 run itself errors or
# renders nothing — never on the fps figures themselves.
if [[ "$CHIP" != "esp32c6" ]]; then
    echo "skipping: the t3 frame-rate leg is C6-only for now; esp32s3 has none here."
else
    # The cadence of complete pad frames, dropping the first (it follows the
    # upload's own latency, not the render loop's) so the window is the
    # steady render, not the boot.
    fps_of() {
        jq -r --argjson pad "$PAD" -s '
            ([ .[] | select(.kind == "ws281x-frame" and .pad == $pad and .complete)
                   | .start_us ]) as $all
            | ($all[1:]) as $t
            | if ($t | length) < 2 then
                "0 \($t | length) 0"
              else
                (($t | length - 1) / (($t[-1] - $t[0]) / 1000000)) as $fps
                | "1 \($t | length) \($fps)"
              end
        ' "$1"
    }

    # Modest on purpose (director: "the t3 grade ran ~1.3x real time on a desk
    # in the spike") — this is a second `emu run` on top of the walk's own,
    # not a soak; LP_WALK_T3_TIMEOUT overrides.
    T3_TIMEOUT="${LP_WALK_T3_TIMEOUT:-6s}"
    t3_console="$OUT/walk.t3.console.txt"
    t3_frames="$OUT/walk.t3.frames.jsonl"
    rm -f "$t3_console" "$t3_frames"

    echo "==> lp-cli emu run (--time-grade t3, ${T3_TIMEOUT} emulated), same image and project"
    t3_wall_start=$(date +%s)
    set +e
    "$cli" emu run \
        "${boot_args[@]}" \
        --host-link \
        --upload "$PROJECT" \
        --time-grade t3 \
        --timeout "$T3_TIMEOUT" \
        --wall-timeout "$WALL" \
        --console "$t3_console" \
        --dump-frames "$t3_frames" \
        >"$OUT/emu.t3.stdout" 2>"$OUT/emu.t3.stderr"
    t3_emu_status=$?
    set -e
    t3_wall_secs=$(( $(date +%s) - t3_wall_start ))

    if [[ $t3_emu_status -ne 0 ]] || ! grep -qa "uploaded and running" "$OUT/emu.t3.stderr"; then
        echo "FAIL: the t3 leg's own run did not complete (exit $t3_emu_status)." >&2
        tail -20 "$OUT/emu.t3.stderr" >&2
        exit 1
    fi

    read -r t1_ok t1_n t1_fps <<<"$(fps_of "$frames")"
    read -r t3_ok t3_n t3_fps <<<"$(fps_of "$t3_frames")"

    if [[ "$t1_ok" != "1" || "$t3_ok" != "1" ]]; then
        echo "FAIL: too few decoded pad $PAD frames to compute a frame rate (t1 n=$t1_n, t3 n=$t3_n)." >&2
        exit 1
    fi

    lp_emu_sha="$(git -C "$REPO" rev-parse --short=9 HEAD)"
    printf "frame rate: t1 %.1f fps (n=%s) · t3 %.1f fps (n=%s) (configuration=lp-emu:esp32c6:t{1,3}, lp-emu %s)\n" \
        "$t1_fps" "$t1_n" "$t3_fps" "$t3_n" "$lp_emu_sha"
    echo "  t3 charges the flash-cache line fill (cache.rs CacheCost, 338 cycles/fill) that t1"
    echo "  does not; this project has no preempting thread, so no preemption cost is expected"
    echo "  here — unlike the reference below, where one is the whole point."
    echo "  reference only, a different project: a XIAO C6 lost 9.9% idle frame rate to a"
    echo "  preempting io thread where lp-emu:esp32c6:t3 showed 4.8% and t1/t2 near 0% (2026-10-01"
    echo "  io-thread spike; docs/defects/2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md)."
    echo "  added host time for this leg: ${t3_wall_secs}s"
fi

echo "artefacts: $OUT"
if [[ $keep -eq 0 && $fail -eq 0 ]]; then
    rm -f "$OUT/emu.stdout" "$OUT/cli.stdout" "$OUT/cli.stderr" "$OUT/emu.t3.stdout"
fi
exit "$fail"
