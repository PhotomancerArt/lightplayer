#!/usr/bin/env bash
# One over-the-air test image of the C6's split build, packaged the way a
# release is: the split image (lp-fw-split, two link passes) built as target
# esp32c6-4mb at app version <version> with <features>, then
# `lp-cli firmware package esp32c6-4mb --no-build`, which writes the package
# AND its OTA directory (ota-manifest.json, core.bin, engine.bin, core.z,
# engine.z) with the one packer. <out> gets:
#
#   merged.bin   the whole chip (the emulator's --rom-up-flash seed), behind
#                our own bootloader; merged-idf.bin the same app.bin behind
#                the ESP-IDF one (`LP_BOOT_DOOR=idf` in the OTA tests)
#   split.json   the split tool's account of it
#   package/     the USB package (manifest.json + the merged image)
#   ota/         the OTA directory `--ota-offer` reads
#
#   scripts/ota/build-image.sh <out> <version> [features]
#
# <version> must be an app version the OTA manifest accepts: a release
# (2026.10.05-3) or a dev version (4-40 lowercase hex, e.g. a0a0a0a0).
# Builds into target/fw-split/<target> (the def's split dir) and
# target/firmware-parts/<target>, so two of these never run at once.
#
# `LP_OTA_TARGET=esp32s3-8mb` builds the S3's (`lp-fw-split build --chip
# esp32s3`; its features default to `esp32s3,server`; the S3 has one front
# door, the IDF's, so its `merged-idf.bin` is its `merged.bin` and there is
# no `merged-lp.bin`). Unset, the C6's, as before.
set -euo pipefail

out="$1"
version="$2"
target="${LP_OTA_TARGET:-esp32c6-4mb}"
case "$target" in
    esp32c6-4mb) chip=esp32c6 ;;
    esp32s3-8mb) chip=esp32s3 ;;
    *)
        echo "build-image: LP_OTA_TARGET must be esp32c6-4mb or esp32s3-8mb, not $target" >&2
        exit 1
        ;;
esac
features="${3:-${chip},server}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
if [[ "$chip" == esp32s3 ]]; then
    gcc_bin="$(just _xt-gcc-dir)"
    if [[ -n "$gcc_bin" ]]; then
        export PATH="$gcc_bin:$PATH"
    fi
fi

split_dir="target/fw-split/$target"
parts_dir="target/firmware-parts/$target"

APP_VERSION="$version" cargo run -q -p lp-fw-split --release -- build \
    --chip "$chip" --out "$split_dir" --features "$features" --target "$target"
rm -rf "$out"
mkdir -p "$out"
cargo run -q -p lp-cli -- firmware package "$target" --no-build --out "$out/package" >/dev/null
test -f "$parts_dir/ota-manifest.json" || {
    echo "build-image: no OTA files in $parts_dir (a dev build whose commit does not resolve?)" >&2
    exit 1
}
cp -R "$parts_dir" "$out/ota"
for file in merged.bin merged-idf.bin merged-lp.bin split.json core.bin engine.bin; do
    if [[ -f "$split_dir/$file" ]]; then
        cp "$split_dir/$file" "$out/"
    fi
done
echo "build-image: $out ($(jq -r '.buildId' "$out/split.json"), features $features)"
