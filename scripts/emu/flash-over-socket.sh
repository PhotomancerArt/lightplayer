#!/usr/bin/env bash
# Flash the emulated C6 with a REAL flasher, over the socket, and boot what
# was written.
#
#   scripts/emu/flash-over-socket.sh [options] <merged.bin>
#
# The whole M3 P2 story in one command: the machine comes up ROM-up with the
# download strap and a WRITABLE chip, a pty appears in front of its byte
# socket, a real flasher writes the merged image through the mask ROM's
# download console, the host performs its reset dance, and the chip boots
# what the flasher just wrote and says hello. Then the bytes are compared.
#
# # Why the chip is `--flash` and not `--merged`
#
# `--merged` is READ-ONLY on purpose: it is the image a gate named, and a run
# that wrote back into it would quietly stop being that image. Here the image
# is the flasher's INPUT, not the chip's contents — the chip starts blank (or
# holding some older image) and the whole question is whether the flasher can
# put the bytes there. So the chip is `--flash <file>`, which is the backing
# that keeps a run's writes, and the machine is told to boot ROM-up with
# `--boot-mode rom-up`.
#
# # Why a pty, and why the reset is on the control channel
#
# Flashers speak to a serial PORT; the machine speaks bytes on a socket.
# `pty-tcp-bridge.py` is the adapter. What a pty cannot carry is DTR and RTS,
# so the flasher's own `--after hard-reset` would toggle lines that go
# nowhere. Every flasher here is therefore run with `--before no-reset
# --after no-reset` and the SAME dance is performed on the machine's
# `--control` channel (`dtr 0`, `rts 1`, `rts 0`), where USB_DEVICE models it
# — a falling RTS with DTR low is the serial bridge's plain chip reset into
# the app strap. Nothing about the chip's side of the reset is faked: it is
# the modelled dance, on the modelled register.
#
# # Clients
#
#   --client esptool   (default) `esptool` / `esptool.py`. Works.
#   --client espflash  espflash's CLI. REFUSED BY ESPFLASH, not by us — see
#                      below; the script says so and exits 20.
#
# `espflash` 3.3.0's CLI resolves `--port` by looking the name up in the
# operating system's serial-port ENUMERATION (`serialport::available_ports()`
# → IOKit on macOS, libudev/`/sys/class/tty` on Linux) and refuses a name
# that is not in the list (`src/cli/serial.rs`, `find_serial_port`). A pty is
# not an enumerated device on either platform, so `espflash --port <pty>`
# fails with `espflash::serial_not_found` before a single byte is sent. That
# is a property of the flasher's port lookup and has nothing to do with the
# emulator. The espflash PROTOCOL is still gated, and by espflash 3.3.0's own
# code: `lp-emu/esp/lp-emu-esp32c6/tests/flash_over_socket.rs` drives
# `espflash::flasher::Flasher` directly — the way this repository's own
# product code flashes a board (`lp-cli/src/commands/fwcheck/flash.rs`,
# `lp-app/lpa-link/.../host_esp32_flash.rs`), which also bypasses the CLI's
# enumeration.
#
# EXIT CODES
#   0   the image was written, the bytes match, and the app said hello
#   10  a port this run needs is already held
#   11  the machine never listened
#   12  the bridge never produced a pty
#   13  the flasher failed (its log carries a failure banner)
#   14  the bytes on the chip are not the image's
#   15  the chip did not say hello after the reset
#   16  the machine faulted, or a strict-bus violation ended it
#   20  the chosen client cannot open a pty (espflash's CLI)
#   127 a tool this run needs is missing
set -uo pipefail

die() { echo "flash-over-socket: $2" >&2; exit "$1"; }

image=""
seed=""
client="esptool"
stub="--no-stub"
port=5711
ctrl=5712
outdir=""
boot=1
wall=600
emu_timeout="3000s"
strict="--strict-bus"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed) seed="${2:?--seed needs an image}"; shift 2 ;;
        --client) client="${2:?--client needs a name}"; shift 2 ;;
        --stub) stub=""; shift ;;
        --no-stub) stub="--no-stub"; shift ;;
        --port) port="${2:?}"; shift 2 ;;
        --control-port) ctrl="${2:?}"; shift 2 ;;
        --out) outdir="${2:?}"; shift 2 ;;
        --no-boot) boot=0; shift ;;
        --wall) wall="${2:?}"; shift 2 ;;
        --emu-timeout) emu_timeout="${2:?}"; shift 2 ;;
        --no-strict) strict=""; shift ;;
        -h|--help) sed -n '2,70p' "$0"; exit 0 ;;
        -*) die 64 "unknown option \`$1\`" ;;
        *) image="$1"; shift ;;
    esac
done

[[ -n "$image" ]] || die 64 "usage: flash-over-socket.sh [options] <merged.bin>"
[[ -f "$image" ]] || die 64 "$image does not exist (scripts/emu/build-merged-image.sh builds one)"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo" || die 64 "cannot cd to $repo"
outdir="${outdir:-$repo/target/emu-flash-over-socket}"
mkdir -p "$outdir"
chip="$outdir/chip.bin"
emu_log="$outdir/machine.log"
bridge_log="$outdir/bridge.log"
client_log="$outdir/flasher.log"
wire_log="$outdir/wire.bin"

command -v python3 >/dev/null 2>&1 || die 127 "MISSING TOOL — python3 (the pty bridge is a python script)"

bin="$repo/target/release/lp-emu-esp32c6"
if [[ ! -x "$bin" ]]; then
    echo "flash-over-socket: building the machine (release)…" >&2
    cargo build -p lp-emu-esp32c6 --release --bin lp-emu-esp32c6 >&2 \
        || die 127 "the machine did not build"
fi

case "$client" in
    esptool)
        flasher="$(command -v esptool || command -v esptool.py || true)"
        [[ -n "$flasher" ]] || die 127 "MISSING TOOL — \`esptool\` is not on PATH.
  It is the local-only second client (OQ8): it opens a port by PATH, so it
  can talk to a pty. Install it with \`pipx install esptool\`, or run the
  espflash-library gate instead:
    cargo test -p lp-emu-esp32c6 --test flash_over_socket -- --include-ignored"
        ;;
    espflash)
        die 20 "espflash's CLI cannot open a pty.
  \`espflash --port <path>\` looks the path up in the operating system's
  serial-port enumeration and refuses anything that is not there
  (espflash 3.3.0 src/cli/serial.rs, \`find_serial_port\` →
  \`espflash::serial_not_found\`). A pty is not an enumerated device on macOS
  (IOKit) or on Linux (libudev / /sys/class/tty), so no bridge can make this
  work — the refusal happens before a byte is sent.
  espflash's PROTOCOL is gated through its LIBRARY, the way this repo's own
  product code flashes a board:
    cargo test -p lp-emu-esp32c6 --test flash_over_socket -- --include-ignored"
        ;;
    *) die 64 "--client \`$client\`: expected esptool or espflash" ;;
esac

# Refuse rather than collide: two of these at once would have the second
# machine's listener fail and the second bridge talk to the FIRST machine.
port_held() {
    python3 - "$1" <<'PY'
import socket, sys
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    s.bind(("127.0.0.1", int(sys.argv[1])))
except OSError:
    sys.exit(1)
finally:
    s.close()
sys.exit(0)
PY
}
port_held "$port" || die 10 "127.0.0.1:$port is already held — pass --port"
port_held "$ctrl" || die 10 "127.0.0.1:$ctrl is already held — pass --control-port"

rm -f "$chip" "$emu_log" "$bridge_log" "$client_log" "$wire_log"
if [[ -n "$seed" ]]; then
    [[ -f "$seed" ]] || die 64 "--seed $seed does not exist"
    cp "$seed" "$chip" || die 64 "could not seed the chip from $seed"
    echo "flash-over-socket: the chip starts holding $seed"
else
    echo "flash-over-socket: the chip starts BLANK"
fi

cleanup() {
    [[ -n "${bridge_pid:-}" ]] && kill "$bridge_pid" 2>/dev/null
    [[ -n "${emu_pid:-}" ]] && kill "$emu_pid" 2>/dev/null
    return 0
}
trap cleanup EXIT

# The machine. ROM-up from a writable chip, the download strap, its byte
# socket listening, and its control channel open for the reset dance.
"$bin" \
    --boot-mode rom-up --flash "$chip" --flash-size 4M \
    --strap download --reboot-on-reset --usb-host attached \
    --usb-sj "tcp:127.0.0.1:$port" --usb-sj-drain auto \
    --control "tcp:127.0.0.1:$ctrl" \
    --timeout "$emu_timeout" --wall-timeout "$wall" \
    $strict --exit-on "hello" > "$emu_log" 2>&1 &
emu_pid=$!

for _ in $(seq 1 200); do
    grep -q "usb-sj listening" "$emu_log" 2>/dev/null && break
    kill -0 "$emu_pid" 2>/dev/null || break
    sleep 0.1
done
grep -q "usb-sj listening" "$emu_log" 2>/dev/null \
    || { tail -20 "$emu_log" >&2; die 11 "the machine never listened on 127.0.0.1:$port"; }

python3 "$repo/scripts/emu/pty-tcp-bridge.py" \
    --target "127.0.0.1:$port" --log "$wire_log" > "$bridge_log" 2>&1 &
bridge_pid=$!
pty=""
for _ in $(seq 1 200); do
    pty="$(awk '/^PTY /{print $2; exit}' "$bridge_log" 2>/dev/null)"
    [[ -n "$pty" ]] && break
    kill -0 "$bridge_pid" 2>/dev/null || break
    sleep 0.1
done
[[ -n "$pty" ]] || { cat "$bridge_log" >&2; die 12 "the bridge never produced a pty"; }
echo "flash-over-socket: the flasher's port is $pty"

# The flasher, in the FOREGROUND. Its stdout is the record checked below.
echo "flash-over-socket: $client ${stub:---stub} write 0x0 $image"
"$flasher" --chip esp32c6 --port "$pty" \
    --before no-reset --after no-reset $stub \
    write-flash 0x0 "$image" 2>&1 | tee "$client_log"
client_rc=${PIPESTATUS[0]}

# The exit code is not enough on its own: esptool can report a warning and
# still leave a chip half written, and a `timeout` kill shows up as 124 with
# a truncated log. Both halves are checked.
if [[ $client_rc -ne 0 ]]; then
    die 13 "the flasher exited $client_rc — see $client_log"
fi
if ! grep -q "Hash of data verified" "$client_log"; then
    die 13 "the flasher did not verify the hash it wrote — see $client_log"
fi
for banner in "A fatal error occurred" "Failed to connect" "Timed out" "Invalid head of packet"; do
    if grep -q "$banner" "$client_log"; then
        die 13 "the flasher's log carries \`$banner\` — see $client_log"
    fi
done

if [[ $boot -eq 1 ]]; then
    # The host's reset dance, on the channel that models it. DTR low, RTS
    # high, RTS low: a falling RTS with DTR low is the plain chip reset, and
    # the chip comes back in the APP strap and boots what was just written.
    echo "flash-over-socket: the host's reset dance"
    python3 - "$ctrl" <<'PY' || exit 16
import socket, sys
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=10)
f = s.makefile("rwb")
for line in (b"dtr 0\n", b"rts 1\n", b"rts 0\n"):
    f.write(line)
    f.flush()
    reply = f.readline().decode().strip()
    print("  control:", reply)
    if reply.startswith("err"):
        sys.exit(1)
s.close()
PY
fi

# The machine ends on its own: `--exit-on hello` when it booted, its emulated
# timeout otherwise. The bridge is left alive until then so the hello is in
# the wire log too.
wait "$emu_pid"; emu_rc=$?
# DRAIN, do not kill. The machine stops at the END of the line `--exit-on`
# matched, and those bytes are still in flight on the socket; killing the
# bridge here loses the tail of the very line this run exists to see. The
# bridge notices the machine's socket closing and says `DEVICE CLOSED`.
for _ in $(seq 1 100); do
    grep -q "DEVICE CLOSED\|CLOSED" "$bridge_log" 2>/dev/null && break
    kill -0 "$bridge_pid" 2>/dev/null || break
    sleep 0.1
done
kill "$bridge_pid" 2>/dev/null
wait "$bridge_pid" 2>/dev/null

case $emu_rc in
    0) ;;
    3) tail -20 "$emu_log" >&2; die 16 "a strict-bus violation ended the run — see $emu_log" ;;
    2) tail -20 "$emu_log" >&2; die 16 "the hart faulted — see $emu_log" ;;
    4) [[ $boot -eq 1 ]] && { tail -20 "$emu_log" >&2; die 15 "the wall-clock net fired before the chip said hello"; } ;;
    *) tail -20 "$emu_log" >&2; die 16 "the machine exited $emu_rc — see $emu_log" ;;
esac

grep -q "flash: image written back" "$emu_log" \
    || die 14 "the machine did not write the chip back — see $emu_log"

# The bytes. With --no-boot nothing has run since the flasher, so the WHOLE
# chip must be the image. After a boot the app has mounted (and, on a fresh
# chip, formatted) its own filesystem partition, so the comparison is every
# byte the merged image actually populates — and the script proves that is
# all of them by checking that the rest of the image is erased flash.
lpfs_offset=$((0x310000))
if [[ $boot -eq 0 ]]; then
    cmp "$chip" "$image" \
        || die 14 "the chip is not the image — see $chip"
    echo "flash-over-socket: the whole chip is byte-identical to $image"
else
    cmp -n "$lpfs_offset" "$chip" "$image" \
        || die 14 "the first $lpfs_offset bytes of the chip are not the image's"
    python3 - "$image" "$lpfs_offset" <<'PY' || exit 14
import sys
tail = open(sys.argv[1], "rb").read()[int(sys.argv[2]):]
if tail.strip(b"\xff"):
    print("flash-over-socket: the image has program bytes past the compared "
          "region — the comparison is INCOMPLETE", file=sys.stderr)
    sys.exit(1)
print(f"  the image past 0x{int(sys.argv[2]):x} is {len(tail)} bytes of erased flash")
PY
    echo "flash-over-socket: the chip matches $image over every byte the image populates"
fi

if [[ $boot -eq 1 ]]; then
    if ! grep -aq '"hello"' "$wire_log"; then
        die 15 "the chip never said hello — see $wire_log"
    fi
    echo "flash-over-socket: the chip booted what was written and said hello"
    grep -ao '"commit":"[0-9a-f]*"' "$wire_log" | tail -1
fi

echo "flash-over-socket: done"
echo "  machine  $emu_log"
echo "  flasher  $client_log"
echo "  wire     $wire_log"
echo "  chip     $chip"
