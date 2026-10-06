#!/usr/bin/env bash
# One over-the-air test image of the C6's split build, packaged the way a
# release is: the split image (lp-fw-split, two link passes) built as target
# esp32c6-4mb at app version <version> with <features>, then
# `lp-cli firmware package esp32c6-4mb --no-build`, which writes the package
# AND its OTA directory (ota-manifest.json, core.bin, engine.bin, core.z,
# engine.z) with the one packer. <out> gets:
#
#   merged.bin   the whole chip (the emulator's --rom-up-flash seed)
#   split.json   the split tool's account of it
#   package/     the USB package (manifest.json + the merged image)
#   ota/         the OTA directory `--ota-offer` reads
#
#   scripts/ota/build-image.sh <out> <version> [features]
#
# <version> must be an app version the OTA manifest accepts: a release
# (2026.10.05-3) or a dev version (4-40 lowercase hex, e.g. a0a0a0a0).
# Builds into target/fw-split/esp32c6-4mb (the def's split dir) and
# target/firmware-parts/esp32c6-4mb, so two of these never run at once.
set -euo pipefail

out="$1"
version="$2"
features="${3:-esp32c6,server}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

split_dir=target/fw-split/esp32c6-4mb
parts_dir=target/firmware-parts/esp32c6-4mb

APP_VERSION="$version" cargo run -q -p lp-fw-split --release -- build \
    --out "$split_dir" --features "$features" --target esp32c6-4mb
rm -rf "$out"
mkdir -p "$out"
cargo run -q -p lp-cli -- firmware package esp32c6-4mb --no-build --out "$out/package" >/dev/null
test -f "$parts_dir/ota-manifest.json" || {
    echo "build-image: no OTA files in $parts_dir (a dev build whose commit does not resolve?)" >&2
    exit 1
}
cp -R "$parts_dir" "$out/ota"
cp "$split_dir/merged.bin" "$split_dir/split.json" "$split_dir/core.bin" "$split_dir/engine.bin" "$out/"
echo "build-image: $out ($(jq -r '.buildId' "$out/split.json"), features $features)"
