#!/usr/bin/env bash
# SPIKE: the measurement images, copied out of target/ (the hourly prune
# wipes it). Every image carries `core1-render` so it logs the render-samples
# counters; `LP_CORE1_RENDER=0` starts no core 1 (the single-core control).
# Usage: build_images.sh <out_dir>
set -uo pipefail
out=$1; mkdir -p "$out"
cd "$(dirname "$0")/../.."
elf=target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3
build() { # name core1 features (LP_LCD_OVERLAP from the caller's env)
  LP_CORE1_RENDER=$2 just build-fw-esp32s3 "$3" && cp $elf "$out/$1.elf" || echo "FAILED $1"
}
build rmt-1core 0 "psram,core1-render"
build rmt-2core 1 "psram,core1-render"
build lcd-1core 0 "psram,lcd-ws281x,core1-render"
build lcd-2core 1 "psram,lcd-ws281x,core1-render"
LP_LCD_OVERLAP=1 build lcdov-1core 0 "psram,lcd-ws281x,core1-render"
LP_LCD_OVERLAP=1 build lcdov-2core 1 "psram,lcd-ws281x,core1-render"
ls -la "$out"
