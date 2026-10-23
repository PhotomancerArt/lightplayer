#!/usr/bin/env bash
# SPIKE: the fluid spike's sittings. Hold the lease first
# (`board take fixture-s3 --as "s3-fluid-spike" ...`).
#   sitting.sh s3|c6 known-good          # the latest release (first and last)
#   sitting.sh s3|c6 bench <image> <tag> [secs]   # flash, capture the boot bench
#   sitting.sh s3|c6 run <image> <tag> [secs] [project]   # flash, upload, reboot-capture [perf]
set -uo pipefail
W=$(cd "$(dirname "$0")/../.." && pwd)
F=~/.photomancer/planning/lp2025/2026-10-20-2335-s3-radio-and-ota/fluid-spike
CLI=${LP_CLI:-$W/target/release/lp-cli}
chip=$1; shift
case "$chip" in
  s3) MAC=D8:3B:DA:47:29:70; OUT=$F/fs3; TARGET=esp32s3-8mb; ECHIP=esp32s3
      PT="$W/lp-fw/fw-esp32s3/partitions.csv"; FS=8mb ;;
  c6) MAC=14:C1:9F:E6:54:90; OUT=$F/c6; TARGET=esp32c6-4mb; ECHIP=esp32c6
      PT="$W/lp-fw/fw-esp32c6/partitions.csv"; FS=4mb ;;
esac
mkdir -p "$OUT"
export BOARD_HOLDER="s3-fluid-spike"
PORT=${PORT:-$($CLI fwcheck port --mac $MAC 2>/dev/null | tail -1)}
echo "port: $PORT"
flash() {
  espflash flash --chip $ECHIP --partition-table "$PT" --flash-size $FS \
    --after hard-reset --port "$PORT" "$F/images/$1.elf" 2>&1 | tail -1
}
case "$1" in
  known-good)
    $CLI firmware install --release latest --mac $MAC --target $TARGET --yes 2>&1 | tail -3 ;;
  bench)
    img=$2; tag=$3; secs=${4:-240}
    flash "$img"; sleep 3
    python3 "$W/scripts/emu/tty-capture.py" --dev "$PORT" --out "$OUT/$img--$tag.txt" \
      --seconds "$secs" --until "[fluid-bench] done" 2> "$OUT/$img--$tag.cap.log"
    echo "capture rc=$?"; grep -a "fluid-bench" "$OUT/$img--$tag.txt" | cut -c1-200 ;;
  run)
    img=$2; tag=$3; secs=${4:-40}; proj=${5:-}
    flash "$img"; sleep 5
    if [[ -n "$proj" ]]; then timeout 120 $CLI upload "$proj" "serial:$PORT" 2>&1 | tail -1; sleep 2; fi
    timeout $((secs+40)) $CLI link capture "$PORT" --console "$OUT/$img--$tag.txt" \
      --request reboot --seconds "$secs" > "$OUT/$img--$tag.cap.log" 2>&1; echo "capture rc=$?"
    grep -aE "\[perf\] frame=|\[perf\] frames|compilation succeeded|panic|PANIC|Guru|rror" \
      "$OUT/$img--$tag.txt" | grep -v '^M!' | cut -c1-200 | tail -8 ;;
esac
