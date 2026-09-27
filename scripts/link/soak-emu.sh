#!/usr/bin/env bash
# One link soak against an emulated ESP32-C6 (lp-emu:esp32c6:t1).
#
#   scripts/link/soak-emu.sh <fw-elf> <out-dir> [--lag <ns>] [--host-rx <spec>] [-- <lp-cli link soak args>]
#
# Starts target/release/lp-emu-esp32c6 on a free loopback port with the USB
# host attached and the port closed (a client's connect is the open), runs
# `lp-cli link soak` against it, stops the emulator it started (by pid, never
# by name), and writes the soak's capture/events/summary plus the emulator's
# stderr and a provenance line (image, lp-emu commit, flags) into <out-dir>.
#
#   --lag <ns>      the model's free-lag hypothesis switch (--usb-in-free-lag)
#   --host-rx <s>   the host receive-buffer model (--usb-host-rx), e.g.
#                   "cap=1500,stall-every=2000ms,stall=300ms"
#
# Build first: cargo build -p lp-emu-esp32c6 --release; cargo build -p lp-cli --release.
# Investigation tooling (plan lp2025/2026-09-26-1720-reliable-device-link, M1b).
set -euo pipefail

elf="$1"; out="$2"; shift 2
lag=0
host_rx=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --lag) lag="$2"; shift 2 ;;
        --host-rx) host_rx="$2"; shift 2 ;;
        --) shift; break ;;
        *) echo "soak-emu: unknown flag $1" >&2; exit 2 ;;
    esac
done

root="$(cd "$(dirname "$0")/../.." && pwd)"
emu="$root/target/release/lp-emu-esp32c6"
cli="$root/target/release/lp-cli"
mkdir -p "$out"

port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
extra=()
if [[ -n "$host_rx" ]]; then
    extra+=(--usb-host-rx "$host_rx")
fi
"$emu" --elf "$elf" --usb-host attached-idle --usb-sj "tcp:127.0.0.1:$port" \
    --usb-in-free-lag "$lag" ${extra[@]+"${extra[@]}"} \
    --timeout 3600s --wall-timeout 1800 >"$out/emu.stdout" 2>"$out/emu.stderr" &
emu_pid=$!
trap 'kill "$emu_pid" 2>/dev/null || true' EXIT

# Wait for the listen line (never probe the socket: a connect is an open).
for _ in $(seq 1 100); do
    if grep -q "usb-sj listening" "$out/emu.stderr" 2>/dev/null; then break; fi
    sleep 0.1
done
# Let the image boot to its server loop before the port opens.
sleep 1

commit="$(git -C "$root" rev-parse --short HEAD)$(git -C "$root" diff --quiet -- lp-emu lp-fw || echo -dirty)"
printf '{"configuration":"lp-emu:esp32c6:t1","lp_emu_commit":"%s","image":"%s","image_sha256":"%s","free_lag_ns":%s,"host_rx":"%s"}\n' \
    "$commit" "$elf" "$(shasum -a 256 "$elf" | cut -d' ' -f1)" "$lag" "$host_rx" >"$out/provenance.json"

"$cli" link soak --port "tcp://127.0.0.1:$port" --out "$out" --label "lp-emu:esp32c6:t1@$commit" "$@"
