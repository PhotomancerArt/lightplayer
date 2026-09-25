#!/usr/bin/env bash
# SPIKE: instructions per reply, static vs learned, on the emulated C6.
# usage: spikes/learned-wire-dict/cpu_run.sh <c6-spike-learned-trace.elf> <out-dir> <N lens reads>
# The image packs static frames as shipped and ALSO encodes each reply against
# its learned table into a scratch buffer, logging `[lwtrace]` with both
# encodes' minstret deltas. Run from the repo root after `cargo build --release -p lp-cli`.
set -u
ELF=$1; OUT=$2; N=$3; M=$(dirname "$0")
mkdir -p "$OUT"
PORT=$(python3 -c "import socket;s=socket.socket();s.bind(('127.0.0.1',0));print(s.getsockname()[1])")
./target/release/lp-cli emu run --elf "$ELF" --link 127.0.0.1:$PORT --time-grade t2 --timeout 1800s --wall-timeout 1800 > "$OUT/emu.log" 2>&1 &
EMU=$!
sleep 5
LP_WIRE_ENCODING=json timeout 600 ./target/release/lp-cli upload --wait-timeout 5 catalog/projects/playful-choker serial:tcp://127.0.0.1:$PORT > "$OUT/upload.log" 2>&1; echo "upload exit $?"
timeout 900 python3 "$M/lens_client.py" 127.0.0.1:$PORT packed "$OUT/cap.bin" "$N" ./target/release/lp-cli
kill $EMU 2>/dev/null; wait $EMU 2>/dev/null
./target/release/lp-cli wire unpack --sizes < "$OUT/cap.bin" > "$OUT/cap.txt" 2> "$OUT/sizes.txt"
grep -a -o '\[lwtrace\][^"\\]*' "$OUT/cap.txt" "$OUT/emu.log" | sed 's/^[^:]*://' > "$OUT/lwtrace.txt"
wc -l "$OUT/lwtrace.txt"
