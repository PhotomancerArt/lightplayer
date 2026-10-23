#!/usr/bin/env bash
# SPIKE: one emulated run per (image, project), in parallel.
# Usage: emu_run.sh <images_dir> <out_dir> <seconds> <project_dir>...
set -uo pipefail
im=$1; out=$2; secs=$3; shift 3
cd "$(dirname "$0")/../.."
mkdir -p "$out"
for proj in "$@"; do
  p=$(basename "$proj")
  for elf in "$im"/*.elf; do
    v=$(basename "$elf" .elf)
    ./target/release/lp-cli emu run --chip esp32s3 --host-link --link-nonce 01020304 \
      --elf "$elf" --upload "$proj" --timeout "${secs}s" \
      --console "$out/$v--$p.txt" > "$out/$v--$p.run.txt" 2>&1 &
  done
done
wait
