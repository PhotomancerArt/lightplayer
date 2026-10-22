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
#   scripts/ota/build-image.sh [--chip esp32c6|esp32s3] <out> <version> [features]
#
# `--chip esp32s3` (S3 updates plan, M7 P10) builds target esp32s3-8mb
# (`lp-fw-split build --chip esp32s3`, default features
# esp32s3,server,float-f32) into target/fw-split/esp32s3-8mb and
# target/firmware-parts/esp32s3-8mb. The S3 boots through the ESP-IDF door
# only, so its `merged.bin` IS that door and there is no `merged-lp.bin`;
# `merged-idf.bin` is copied as on the C6. The C6 (no flag) is unchanged.
#
# <version> must be an app version the OTA manifest accepts: a release
# (2026.10.05-3) or a dev version (4-40 lowercase hex, e.g. a0a0a0a0).
# Builds into target/fw-split/esp32c6-4mb (the def's split dir) and
# target/firmware-parts/esp32c6-4mb, so two of these never run at once.
set -euo pipefail

chip=esp32c6
if [[ "${1:-}" == "--chip" ]]; then
    chip="$2"
    shift 2
fi
out="$1"
version="$2"
case "$chip" in
    esp32c6)
        target=esp32c6-4mb
        features="${3:-esp32c6,server}"
        split_args=()
        merged=(merged.bin merged-idf.bin merged-lp.bin)
        ;;
    esp32s3)
        target=esp32s3-8mb
        features="${3:-esp32s3,server,float-f32}"
        split_args=(--chip esp32s3)
        merged=(merged.bin merged-idf.bin)
        ;;
    *)
        echo "build-image: --chip $chip: expected esp32c6 or esp32s3" >&2
        exit 2
        ;;
esac
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

split_dir=target/fw-split/$target
parts_dir=target/firmware-parts/$target

APP_VERSION="$version" cargo run -q -p lp-fw-split --release -- build ${split_args[@]+"${split_args[@]}"} \
    --out "$split_dir" --features "$features" --target "$target"
if [[ "$chip" == esp32s3 ]]; then
    jq -e '.chip == "esp32s3" and .layout == 2' "$split_dir/split.json" >/dev/null || {
        echo "build-image: $split_dir/split.json is not an esp32s3 layout-2 split" >&2
        exit 1
    }
fi
rm -rf "$out"
mkdir -p "$out"
cargo run -q -p lp-cli -- firmware package "$target" --no-build --out "$out/package" >/dev/null
test -f "$parts_dir/ota-manifest.json" || {
    echo "build-image: no OTA files in $parts_dir (a dev build whose commit does not resolve?)" >&2
    exit 1
}
cp -R "$parts_dir" "$out/ota"
for f in "${merged[@]}" split.json core.bin engine.bin; do
    cp "$split_dir/$f" "$out/"
done
echo "build-image: $out ($(jq -r '.buildId' "$out/split.json"), features $features)"
