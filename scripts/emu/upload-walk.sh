#!/usr/bin/env bash
# The project-upload walk against the C6 emulator, over a real socket.
#
#   scripts/emu/upload-walk.sh <elf> <out-dir> [<project>=examples/basic] [<port>=5591]
#
# Three processes, in this order:
#
#   lp-emu-esp32c6 --uart0 tcp:127.0.0.1:<port>      the machine, listening
#   uart-tcp-proxy.py --target <port> --listen <port+1>   transcribing in between
#   lp-cli upload <project> serial:tcp://127.0.0.1:<port+1>
#
# `WALK_LINK=usb` moves the machine's half onto the link the product actually
# ships: `--usb-sj tcp:` with `--usb-host attached`, which is the same socket
# `lp-cli … serial:tcp://` talks to and the same bytes a reader on a silicon
# port sees. Everything else is unchanged, including the proxy — the walk is
# a conversation whichever wire carries it. This is the live door M6 P5's
# G5-4 asks for, and the capture it produces is what `walk-script.py` turns
# into the deterministic replay.
#
# The proxy is what makes this a *transcript* rather than half of one: the
# emulator's own UART0 log holds only what the device sent, and a walk is a
# conversation. The proxy writes `<out-dir>/walk.uart.bin` with the device's
# bytes as-is and the host's wrapped in `<<HOST … >>` — the same shape the
# spike's desk captures use, so the two can be diffed line for line.
#
# **This walk is not deterministic and is not meant to be.** lp-cli's
# requests land where the host's wall clock puts them against the guest's
# emulated clock, exactly as the spike report §7 and §11.3 describe. Its job
# is to be the evidence for what a real client sends; the deterministic
# replay is `--uart0-script` with `after "<line>"` steps, and
# `scripts/emu/walk-script.py` turns this capture into one.
set -euo pipefail

elf="${1:?usage: upload-walk.sh <elf> <out-dir> [<project>] [<port>]}"
out="${2:?usage: upload-walk.sh <elf> <out-dir> [<project>] [<port>]}"
project="${3:-examples/basic}"
port="${4:-5591}"
proxy_port=$((port + 1))

repo="$(cd "$(dirname "$0")/../.." && pwd)"
emu="$repo/target/release/lp-emu-esp32c6"
cli="$repo/target/release/lp-cli"
for bin in "$emu" "$cli"; do
    [[ -x "$bin" ]] || { echo "upload-walk: $bin is not built" >&2; exit 2; }
done

mkdir -p "$out"
rm -f "$out/walk.uart.bin" "$out/walk.uart.bin.times"
flash="$out/flash.bin"
rm -f "$flash"

cleanup() {
    [[ -n "${proxy_pid:-}" ]] && kill "$proxy_pid" 2>/dev/null || true
    [[ -n "${emu_pid:-}" ]] && kill "$emu_pid" 2>/dev/null || true
}
trap cleanup EXIT

# `WALK_TRACE=NOTHING` is the useful default when something goes wrong: a
# block filter that matches nothing lets only the machine's *notes* through
# (SPIN, RX FIFO overflow, an unmodelled SPI1 command) with no register
# traffic at all.
# `${a[@]+"${a[@]}"}` rather than `"${a[@]}"`: under `set -u`, bash 3.2 — the
# only bash macOS ships — treats an EMPTY array's `[@]` as unbound and dies.
trace_args=()
if [[ -n "${WALK_TRACE:-}" ]]; then
    trace_args=(--trace "$WALK_TRACE" --trace-file "$out/emu.trace")
fi

link_args=(--uart0 "tcp:127.0.0.1:$port")
if [[ "${WALK_LINK:-uart0}" == "usb" ]]; then
    # A cable in and an application reading, from power-on: the walk's first
    # request waits for `[RECOVERY] boot complete`, which a host that
    # attached later would never see.
    link_args=(--usb-sj "tcp:127.0.0.1:$port" --usb-host attached)
fi

"$emu" --elf "$elf" \
    "${link_args[@]}" \
    --flash "$flash" \
    --time-grade t1 \
    --timeout "${WALK_TIMEOUT:-30s}" \
    --wall-timeout "${WALK_WALL_TIMEOUT:-180}" \
    --strict-bus \
    ${trace_args[@]+"${trace_args[@]}"} \
    >"$out/emu.stdout" 2>"$out/emu.stderr" &
emu_pid=$!

# Pacing, in wall-clock milliseconds, because that is the only clock the
# proxy has. The device's RX FIFO is 128 bytes and its reader takes 64 per
# turn of the server loop, so an unpaced host loses bytes mid-request; the
# desk walk this is modelled on went through a bridge board, which paces
# itself. The default is deliberately slack — the machine runs several times
# slower than real time, so 64 B every 25 ms of wall clock is well under the
# guest's appetite in *emulated* time.
python3 "$repo/scripts/emu/uart-tcp-proxy.py" \
    --target "127.0.0.1:$port" \
    --listen "127.0.0.1:$proxy_port" \
    --pace-bytes "${WALK_PACE_BYTES:-64}" \
    --pace-ms "${WALK_PACE_MS:-25}" \
    --log "$out/walk.uart.bin" \
    >"$out/proxy.log" 2>&1 &
proxy_pid=$!

# The proxy connects to the emulator as soon as the listener is up and
# buffers the boot output, so the client may attach whenever it likes.
for _ in $(seq 1 100); do
    grep -q CONNECTED "$out/proxy.log" 2>/dev/null && break
    sleep 0.1
done
grep -q CONNECTED "$out/proxy.log" || { echo "upload-walk: the proxy never reached the emulator" >&2; cat "$out/proxy.log" >&2; exit 3; }

echo "==> lp-cli upload $project serial:tcp://127.0.0.1:$proxy_port (link ${WALK_LINK:-uart0})"
set +e
RUST_LOG="${RUST_LOG:-info}" "$cli" upload "$project" "serial:tcp://127.0.0.1:$proxy_port" \
    --wait-timeout "${WALK_CLI_TIMEOUT:-120}" >"$out/cli.stdout" 2>"$out/cli.stderr"
cli_status=$?
set -e
echo "lp-cli exit=$cli_status"

# Let the guest answer the last request and reach a heartbeat before the
# machine's own emulated timeout ends it.
wait "$emu_pid" || true
emu_pid=

echo "walk: $(wc -c <"$out/walk.uart.bin") bytes → $out/walk.uart.bin"
exit "$cli_status"
